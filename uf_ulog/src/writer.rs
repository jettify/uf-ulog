use core::marker::PhantomData;

use crate::{DefaultCfg, MessageSet, PayloadBuf, Record, StreamState, TextBuf, ULogCfg};

pub trait RecordSource<C: ULogCfg> {
    fn try_recv(&mut self) -> Option<Record<C>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStep {
    Progressed,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportError<WriteError> {
    Write(WriteError),
    InvalidTopicIndex,
    InvalidMultiId,
    TooManyStreams,
    MessageTooLarge,
}

pub struct ULogExporter<W, Rx, R: MessageSet, C: ULogCfg = DefaultCfg> {
    writer: W,
    rx: Rx,
    started: bool,
    subscribed: C::Streams,
    _messages: PhantomData<R>,
}

impl<W, Rx, R, C> ULogExporter<W, Rx, R, C>
where
    W: embedded_io::Write,
    Rx: RecordSource<C>,
    R: MessageSet,
    C: ULogCfg,
{
    pub fn new(writer: W, rx: Rx) -> Self {
        Self {
            writer,
            rx,
            started: false,
            subscribed: C::Streams::zeroed(),
            _messages: PhantomData,
        }
    }

    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn emit_startup(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        if self.started {
            return Ok(());
        }

        self.write_header(timestamp_micros)?;
        self.write_flag_bits()?;
        for meta in R::REGISTRY.entries {
            self.write_format(meta.name, meta.format)?;
        }

        self.started = true;
        Ok(())
    }

    pub fn poll_once(
        &mut self,
    ) -> Result<ExportStep, ExportError<<W as embedded_io::ErrorType>::Error>> {
        if !self.started {
            return Ok(ExportStep::Idle);
        }

        let Some(record) = self.rx.try_recv() else {
            return Ok(ExportStep::Idle);
        };

        self.write_record(record)?;
        Ok(ExportStep::Progressed)
    }

    pub fn write_record(
        &mut self,
        record: Record<C>,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        match record {
            Record::LoggedString {
                level,
                tag,
                ts,
                text,
            } => {
                if let Some(tag) = tag {
                    self.write_tagged_log(level as u8, tag, ts, text.as_bytes())
                } else {
                    self.write_log(level as u8, ts, text.as_bytes())
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

                let meta = self.registry_entry(topic_index_usize)?;

                let slot = self.stream_slot(topic_index_usize, usize::from(instance))?;
                let msg_id = self.slot_msg_id(slot)?;

                if !self.subscribed.is_subscribed(slot) {
                    self.write_add_subscription(instance, msg_id, meta.name)?;
                    self.subscribed.mark_subscribed(slot);
                }

                let len = usize::from(payload_len);
                self.write_data(msg_id, &payload.as_slice()[..len])
            }
        }
    }

    fn registry_entry(
        &self,
        topic_index: usize,
    ) -> Result<&'static crate::MessageMeta, ExportError<<W as embedded_io::ErrorType>::Error>>
    {
        R::REGISTRY
            .get(topic_index)
            .ok_or(ExportError::InvalidTopicIndex)
    }

    fn stream_slot(
        &self,
        topic_index: usize,
        instance: usize,
    ) -> Result<usize, ExportError<<W as embedded_io::ErrorType>::Error>> {
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

    fn slot_msg_id(
        &self,
        slot: usize,
    ) -> Result<u16, ExportError<<W as embedded_io::ErrorType>::Error>> {
        u16::try_from(slot).map_err(|_| ExportError::TooManyStreams)
    }

    fn checked_total_len(
        &self,
        base: usize,
        extra: usize,
        capacity: usize,
    ) -> Result<usize, ExportError<<W as embedded_io::ErrorType>::Error>> {
        let total_len = base
            .checked_add(extra)
            .ok_or(ExportError::MessageTooLarge)?;
        if total_len > capacity {
            return Err(ExportError::MessageTooLarge);
        }
        Ok(total_len)
    }

    fn write_header(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut header = [0u8; 16];
        header[..7].copy_from_slice(&[0x55, 0x4c, 0x6f, 0x67, 0x01, 0x12, 0x35]);
        header[7] = 0x01;
        header[8..16].copy_from_slice(&timestamp_micros.to_le_bytes());
        self.write_all(&header)
    }

    fn write_flag_bits(&mut self) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let payload = [0u8; 40];
        self.write_message(b'B', &payload)
    }

    fn write_format(
        &mut self,
        name: &str,
        format: &str,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let name_bytes = name.as_bytes();
        let format_bytes = format.as_bytes();
        let format_offset = self.checked_total_len(name_bytes.len(), 1, payload.len())?;
        let total_len = self.checked_total_len(format_offset, format_bytes.len(), payload.len())?;

        payload[..name_bytes.len()].copy_from_slice(name_bytes);
        payload[name_bytes.len()] = b':';
        payload[format_offset..total_len].copy_from_slice(format_bytes);
        self.write_message(b'F', &payload[..total_len])
    }

    fn write_add_subscription(
        &mut self,
        multi_id: u8,
        msg_id: u16,
        name: &str,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let name_bytes = name.as_bytes();
        let mut payload = [0u8; 256];
        let total_len = self.checked_total_len(3, name_bytes.len(), payload.len())?;

        payload[0] = multi_id;
        payload[1..3].copy_from_slice(&msg_id.to_le_bytes());
        payload[3..total_len].copy_from_slice(name_bytes);
        self.write_message(b'A', &payload[..total_len])
    }

    fn write_data(
        &mut self,
        msg_id: u16,
        data: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 1024];
        let total_len = self.checked_total_len(2, data.len(), payload.len())?;

        payload[0..2].copy_from_slice(&msg_id.to_le_bytes());
        payload[2..total_len].copy_from_slice(data);
        self.write_message(b'D', &payload[..total_len])
    }

    fn write_log(
        &mut self,
        level: u8,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len = self.checked_total_len(9, text.len(), payload.len())?;

        payload[0] = level;
        payload[1..9].copy_from_slice(&timestamp.to_le_bytes());
        payload[9..total_len].copy_from_slice(text);
        self.write_message(b'L', &payload[..total_len])
    }

    fn write_tagged_log(
        &mut self,
        level: u8,
        tag: u16,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len = self.checked_total_len(11, text.len(), payload.len())?;

        payload[0] = level;
        payload[1..3].copy_from_slice(&tag.to_le_bytes());
        payload[3..11].copy_from_slice(&timestamp.to_le_bytes());
        payload[11..total_len].copy_from_slice(text);
        self.write_message(b'C', &payload[..total_len])
    }

    fn write_message(
        &mut self,
        msg_type: u8,
        payload: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let size = u16::try_from(payload.len()).map_err(|_| ExportError::MessageTooLarge)?;
        let mut header = [0u8; 3];
        header[0..2].copy_from_slice(&size.to_le_bytes());
        header[2] = msg_type;
        self.write_all(&header)?;
        self.write_all(payload)
    }

    fn write_all(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        self.writer.write_all(bytes).map_err(ExportError::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LogLevel, ULogData};

    #[derive(Default)]
    struct VecSink {
        bytes: std::vec::Vec<u8>,
    }

    impl embedded_io::ErrorType for VecSink {
        type Error = core::convert::Infallible;
    }

    impl embedded_io::Write for VecSink {
        fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    struct EmptyRx;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestCfg;

    impl crate::ULogCfg for TestCfg {
        type Text = heapless::String<32>;
        type Payload = [u8; 32];
        type Streams = [u8; 64];

        const MAX_MULTI_IDS: usize = 8;
    }

    impl RecordSource<TestCfg> for EmptyRx {
        fn try_recv(&mut self) -> Option<Record<TestCfg>> {
            None
        }
    }

    struct OneRx(Option<Record<TestCfg>>);

    impl RecordSource<TestCfg> for OneRx {
        fn try_recv(&mut self) -> Option<Record<TestCfg>> {
            self.0.take()
        }
    }

    struct Sample;

    impl ULogData for Sample {
        const FORMAT: &'static str = "uint64_t timestamp;";
        const NAME: &'static str = "sample";
        const WIRE_SIZE: usize = 8;

        fn encode(&self, _buf: &mut [u8]) -> Result<usize, crate::EncodeError> {
            Ok(8)
        }

        fn timestamp(&self) -> u64 {
            0
        }
    }

    #[test]
    fn startup_and_data_subscription() {
        crate::register_messages! {
            enum TestMessages {
                Sample,
            }
        }
        let sink = VecSink::default();
        let rec = Record::Data {
            topic_index: 0,
            instance: 1,
            ts: 0,
            payload_len: 4,
            payload: [
                1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0,
            ],
        };
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, TestMessages, TestCfg>::new(sink, rx);

        exporter.emit_startup(100).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn logs_string() {
        crate::register_messages! {
            enum TestMessages {}
        }
        let sink = VecSink::default();
        let mut text = heapless::String::<32>::new();
        let _ = text.push_str("boot");
        let rec = Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 55,
            text,
        };
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, TestMessages, TestCfg>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn idle_without_record() {
        crate::register_messages! {
            enum TestMessages {}
        }
        let sink = VecSink::default();
        let rx = EmptyRx;
        let mut exporter = ULogExporter::<_, _, TestMessages, TestCfg>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Idle);
    }
}
