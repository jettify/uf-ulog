use core::marker::PhantomData;

use crate::writer_common;
use crate::{ExportError, ExportStep, ParameterValue, Record, RecordMeta, ULogRegistry};

#[allow(async_fn_in_trait)]
pub trait AsyncRecordSource {
    type Rec;

    async fn recv(&mut self) -> Self::Rec;
}

pub struct ULogAsyncExporter<
    W,
    Rx,
    R: ULogRegistry,
    const RECORD_CAP: usize = 256,
    const MAX_MULTI_IDS: usize = 4,
    const MAX_STREAMS: usize = 1024,
> {
    writer: W,
    rx: Rx,
    started: bool,
    subscribed: [u8; MAX_STREAMS],
    dropped_streams: u32,
    _messages: PhantomData<R>,
}

impl<W, Rx, R, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize, const MAX_STREAMS: usize>
    ULogAsyncExporter<W, Rx, R, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>
where
    W: embedded_io_async::Write,
    Rx: AsyncRecordSource<Rec = Record<RECORD_CAP, MAX_MULTI_IDS>>,
    R: ULogRegistry,
{
    pub fn new(writer: W, rx: Rx) -> Self {
        Self {
            writer,
            rx,
            started: false,
            subscribed: [0; MAX_STREAMS],
            dropped_streams: 0,
            _messages: PhantomData,
        }
    }

    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn dropped_streams(&self) -> u32 {
        self.dropped_streams
    }

    pub async fn emit_startup(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        if self.started {
            return Ok(());
        }

        self.write_header(timestamp_micros).await?;
        self.write_flag_bits().await?;
        for meta in R::REGISTRY.entries {
            self.write_format(meta.name, meta.format).await?;
        }

        self.started = true;
        Ok(())
    }

    pub async fn poll_once(
        &mut self,
    ) -> Result<ExportStep, ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        if !self.started {
            return Ok(ExportStep::Idle);
        }

        let record = self.rx.recv().await;
        self.write_record(record).await?;
        Ok(ExportStep::Progressed)
    }

    pub async fn run(
        &mut self,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        loop {
            self.poll_once().await?;
        }
    }

    pub async fn write_record(
        &mut self,
        record: Record<RECORD_CAP, MAX_MULTI_IDS>,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        match record.meta() {
            RecordMeta::LoggedString { level, tag, ts } => {
                if let Some(tag) = tag {
                    self.write_tagged_log(level as u8, tag, ts, record.bytes())
                        .await
                } else {
                    self.write_log(level as u8, ts, record.bytes()).await
                }
            }
            RecordMeta::Data {
                topic_index,
                instance,
                ts: _,
            } => {
                let topic_index_usize = usize::from(topic_index);
                if usize::from(instance) >= MAX_MULTI_IDS {
                    return Err(ExportError::InvalidMultiId);
                }

                let meta = writer_common::registry_entry::<
                    R,
                    <W as embedded_io_async::ErrorType>::Error,
                >(topic_index_usize)?;

                let Some(slot) = writer_common::stream_slot::<MAX_MULTI_IDS>(
                    topic_index_usize,
                    usize::from(instance),
                ) else {
                    self.dropped_streams = self.dropped_streams.saturating_add(1);
                    return Ok(());
                };

                if slot >= MAX_STREAMS {
                    self.dropped_streams = self.dropped_streams.saturating_add(1);
                    return Ok(());
                }

                let msg_id =
                    writer_common::slot_msg_id::<<W as embedded_io_async::ErrorType>::Error>(slot)?;

                if self.subscribed[slot] == 0 {
                    self.write_add_subscription(instance, msg_id, meta.name)
                        .await?;
                    self.subscribed[slot] = 1;
                }

                self.write_data(msg_id, record.bytes()).await
            }
            RecordMeta::Parameter { value } => self.write_parameter(record.bytes(), value).await,
        }
    }

    async fn write_header(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let header = writer_common::write_header(timestamp_micros);
        self.write_all(&header).await
    }

    async fn write_flag_bits(
        &mut self,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let payload = [0u8; 40];
        self.write_message(b'B', &payload).await
    }

    async fn write_format(
        &mut self,
        name: &str,
        format: &str,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len = writer_common::format_payload(&mut payload, name, format)?;
        self.write_message(b'F', &payload[..total_len]).await
    }

    async fn write_add_subscription(
        &mut self,
        multi_id: u8,
        msg_id: u16,
        name: &str,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 256];
        let total_len =
            writer_common::add_subscription_payload(&mut payload, multi_id, msg_id, name)?;
        self.write_message(b'A', &payload[..total_len]).await
    }

    async fn write_data(
        &mut self,
        msg_id: u16,
        data: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 1024];
        let total_len = writer_common::data_payload(&mut payload, msg_id, data)?;
        self.write_message(b'D', &payload[..total_len]).await
    }

    async fn write_log(
        &mut self,
        level: u8,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len = writer_common::log_payload(&mut payload, level, timestamp, text)?;
        self.write_message(b'L', &payload[..total_len]).await
    }

    async fn write_tagged_log(
        &mut self,
        level: u8,
        tag: u16,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len =
            writer_common::tagged_log_payload(&mut payload, level, tag, timestamp, text)?;
        self.write_message(b'C', &payload[..total_len]).await
    }

    async fn write_parameter(
        &mut self,
        key: &[u8],
        value: ParameterValue,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let raw = match value {
            ParameterValue::I32(v) => v.to_le_bytes(),
            ParameterValue::F32(v) => v.to_le_bytes(),
        };
        let total_len = writer_common::parameter_payload(&mut payload, key, &raw)?;
        self.write_message(b'P', &payload[..total_len]).await
    }

    async fn write_message(
        &mut self,
        msg_type: u8,
        payload: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let header = writer_common::message_header(payload.len(), msg_type)?;
        self.write_all(&header).await?;
        self.write_all(payload).await
    }

    async fn write_all(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        self.writer
            .write_all(bytes)
            .await
            .map_err(ExportError::Write)
    }
}
