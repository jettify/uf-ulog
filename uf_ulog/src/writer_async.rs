use core::marker::PhantomData;

use crate::writer::{ExportError, ExportStep};
use crate::{DefaultCfg, MessageSet, PayloadBuf, Record, StreamState, TextBuf, ULogCfg};

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
                if topic_index_usize >= R::REGISTRY.len() {
                    return Err(ExportError::InvalidTopicIndex);
                }
                if usize::from(instance) >= C::MAX_MULTI_IDS {
                    return Err(ExportError::InvalidMultiId);
                }

                let slot = self.stream_slot(topic_index_usize, usize::from(instance))?;
                let msg_id = self.slot_to_msg_id(slot)?;

                if !self.subscribed.is_subscribed(slot) {
                    let meta = R::REGISTRY.entries[topic_index_usize];
                    self.write_add_subscription(instance, msg_id, meta.name).await?;
                    self.subscribed.mark_subscribed(slot);
                }

                let len = usize::from(payload_len);
                self.write_data(msg_id, &payload.as_slice()[..len]).await
            }
        }
    }

    fn stream_slot(
        &self,
        topic_index: usize,
        instance: usize,
    ) -> Result<usize, ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let Some(slot) = topic_index
            .checked_mul(C::MAX_MULTI_IDS)
            .and_then(|v| v.checked_add(instance))
        else {
            return Err(ExportError::TooManyStreams);
        };
        if slot >= C::Streams::MAX_STREAMS {
            return Err(ExportError::TooManyStreams);
        }
        Ok(slot)
    }

    fn slot_to_msg_id(
        &self,
        slot: usize,
    ) -> Result<u16, ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        u16::try_from(slot).map_err(|_| ExportError::TooManyStreams)
    }

    async fn write_header(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut header = [0u8; 16];
        header[..7].copy_from_slice(&[0x55, 0x4c, 0x6f, 0x67, 0x01, 0x12, 0x35]);
        header[7] = 0x01;
        header[8..16].copy_from_slice(&timestamp_micros.to_le_bytes());
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
        let name_bytes = name.as_bytes();
        let format_bytes = format.as_bytes();
        let total_len = name_bytes
            .len()
            .checked_add(1)
            .and_then(|v| v.checked_add(format_bytes.len()))
            .ok_or(ExportError::MessageTooLarge)?;
        if total_len > payload.len() {
            return Err(ExportError::MessageTooLarge);
        }

        payload[..name_bytes.len()].copy_from_slice(name_bytes);
        payload[name_bytes.len()] = b':';
        payload[name_bytes.len() + 1..total_len].copy_from_slice(format_bytes);
        self.write_message(b'F', &payload[..total_len]).await
    }

    async fn write_add_subscription(
        &mut self,
        multi_id: u8,
        msg_id: u16,
        name: &str,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let name_bytes = name.as_bytes();
        let mut payload = [0u8; 256];
        let total_len = 3usize
            .checked_add(name_bytes.len())
            .ok_or(ExportError::MessageTooLarge)?;
        if total_len > payload.len() {
            return Err(ExportError::MessageTooLarge);
        }

        payload[0] = multi_id;
        payload[1..3].copy_from_slice(&msg_id.to_le_bytes());
        payload[3..total_len].copy_from_slice(name_bytes);
        self.write_message(b'A', &payload[..total_len]).await
    }

    async fn write_data(
        &mut self,
        msg_id: u16,
        data: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 1024];
        let total_len = 2usize
            .checked_add(data.len())
            .ok_or(ExportError::MessageTooLarge)?;
        if total_len > payload.len() {
            return Err(ExportError::MessageTooLarge);
        }

        payload[0..2].copy_from_slice(&msg_id.to_le_bytes());
        payload[2..total_len].copy_from_slice(data);
        self.write_message(b'D', &payload[..total_len]).await
    }

    async fn write_log(
        &mut self,
        level: u8,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len = 9usize
            .checked_add(text.len())
            .ok_or(ExportError::MessageTooLarge)?;
        if total_len > payload.len() {
            return Err(ExportError::MessageTooLarge);
        }

        payload[0] = level;
        payload[1..9].copy_from_slice(&timestamp.to_le_bytes());
        payload[9..total_len].copy_from_slice(text);
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
        let total_len = 11usize
            .checked_add(text.len())
            .ok_or(ExportError::MessageTooLarge)?;
        if total_len > payload.len() {
            return Err(ExportError::MessageTooLarge);
        }

        payload[0] = level;
        payload[1..3].copy_from_slice(&tag.to_le_bytes());
        payload[3..11].copy_from_slice(&timestamp.to_le_bytes());
        payload[11..total_len].copy_from_slice(text);
        self.write_message(b'C', &payload[..total_len]).await
    }

    async fn write_message(
        &mut self,
        msg_type: u8,
        payload: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let size = u16::try_from(payload.len()).map_err(|_| ExportError::MessageTooLarge)?;
        let mut header = [0u8; 3];
        header[0..2].copy_from_slice(&size.to_le_bytes());
        header[2] = msg_type;
        self.write_all(&header).await?;
        self.write_all(payload).await
    }

    async fn write_all(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        self.writer.write_all(bytes).await.map_err(ExportError::Write)
    }
}
