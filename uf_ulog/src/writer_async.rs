use core::marker::PhantomData;

use crate::writer_common;
use crate::{
    DefaultCfg, ExportError, ExportStep, MessageSet, Record, StreamState, TextBuf, ULogCfg,
};

#[allow(async_fn_in_trait)]
pub trait AsyncRecordSource<C: ULogCfg> {
    async fn recv(&mut self) -> Record<C>;
}

pub struct ULogAsyncExporter<W, Rx, R: MessageSet, C: ULogCfg = DefaultCfg> {
    writer: W,
    rx: Rx,
    started: bool,
    subscribed: C::Streams,
    _cfg: PhantomData<C>,
    _messages: PhantomData<R>,
}

impl<W, Rx, R, C> ULogAsyncExporter<W, Rx, R, C>
where
    W: embedded_io_async::Write,
    Rx: AsyncRecordSource<C>,
    R: MessageSet,
    C: ULogCfg,
{
    pub fn new(writer: W, rx: Rx) -> Self {
        Self {
            writer,
            rx,
            started: false,
            subscribed: C::Streams::zeroed(),
            _cfg: PhantomData,
            _messages: PhantomData,
        }
    }

    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
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
        record: Record<C>,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        match record {
            Record::LoggedString {
                level,
                tag,
                ts,
                text,
            } => {
                if let Some(tag) = tag {
                    self.write_tagged_log(level as u8, tag, ts, text.as_bytes())
                        .await
                } else {
                    self.write_log(level as u8, ts, text.as_bytes()).await
                }
            }
            Record::Data {
                topic_index,
                instance,
                ts: _,
                payload_len,
                payload,
            } => {
                let topic_index_usize = usize::from(topic_index);
                if usize::from(instance) >= C::MAX_MULTI_IDS {
                    return Err(ExportError::InvalidMultiId);
                }

                let meta = writer_common::registry_entry::<
                    R,
                    <W as embedded_io_async::ErrorType>::Error,
                >(topic_index_usize)?;

                let slot = writer_common::stream_slot::<
                    C,
                    <W as embedded_io_async::ErrorType>::Error,
                >(topic_index_usize, usize::from(instance))?;
                let msg_id =
                    writer_common::slot_msg_id::<<W as embedded_io_async::ErrorType>::Error>(slot)?;

                if !self.subscribed.is_subscribed(slot) {
                    self.write_add_subscription(instance, msg_id, meta.name)
                        .await?;
                    self.subscribed.mark_subscribed(slot);
                }

                let data = writer_common::payload_with_len::<
                    C,
                    <W as embedded_io_async::ErrorType>::Error,
                >(&payload, payload_len)?;
                self.write_data(msg_id, data).await
            }
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
