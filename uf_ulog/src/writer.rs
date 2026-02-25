use core::marker::PhantomData;

use crate::writer_common;
use crate::{ExportError, ExportStep, ParameterValue, Record, ULogRegistry};

pub trait RecordSource {
    type Rec;

    fn try_recv(&mut self) -> Option<Self::Rec>;
}

pub struct ULogExporter<
    W,
    Rx,
    R: ULogRegistry,
    const RECORD_CAP: usize = 256,
    const MAX_MULTI_IDS: usize = 8,
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
    ULogExporter<W, Rx, R, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>
where
    W: embedded_io::Write,
    Rx: RecordSource<Rec = Record<RECORD_CAP, MAX_MULTI_IDS>>,
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
        record: Record<RECORD_CAP, MAX_MULTI_IDS>,
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
                if usize::from(instance) >= MAX_MULTI_IDS {
                    return Err(ExportError::InvalidMultiId);
                }

                let meta = writer_common::registry_entry::<R, <W as embedded_io::ErrorType>::Error>(
                    topic_index_usize,
                )?;

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
                    writer_common::slot_msg_id::<<W as embedded_io::ErrorType>::Error>(slot)?;

                if self.subscribed[slot] == 0 {
                    self.write_add_subscription(instance, msg_id, meta.name)?;
                    self.subscribed[slot] = 1;
                }

                let data = writer_common::payload_with_len::<
                    RECORD_CAP,
                    <W as embedded_io::ErrorType>::Error,
                >(&payload, payload_len)?;
                self.write_data(msg_id, data)
            }
            Record::Parameter { key, value } => self.write_parameter(key.as_bytes(), value),
        }
    }

    fn write_header(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let header = writer_common::write_header(timestamp_micros);
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
        let total_len = writer_common::format_payload(&mut payload, name, format)?;
        self.write_message(b'F', &payload[..total_len])
    }

    fn write_add_subscription(
        &mut self,
        multi_id: u8,
        msg_id: u16,
        name: &str,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 256];
        let total_len =
            writer_common::add_subscription_payload(&mut payload, multi_id, msg_id, name)?;
        self.write_message(b'A', &payload[..total_len])
    }

    fn write_data(
        &mut self,
        msg_id: u16,
        data: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 1024];
        let total_len = writer_common::data_payload(&mut payload, msg_id, data)?;
        self.write_message(b'D', &payload[..total_len])
    }

    fn write_log(
        &mut self,
        level: u8,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let total_len = writer_common::log_payload(&mut payload, level, timestamp, text)?;
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
        let total_len =
            writer_common::tagged_log_payload(&mut payload, level, tag, timestamp, text)?;
        self.write_message(b'C', &payload[..total_len])
    }

    fn write_parameter(
        &mut self,
        key: &[u8],
        value: ParameterValue,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload = [0u8; 512];
        let raw = match value {
            ParameterValue::I32(v) => v.to_le_bytes(),
            ParameterValue::F32(v) => v.to_le_bytes(),
        };
        let total_len = writer_common::parameter_payload(&mut payload, key, &raw)?;
        self.write_message(b'P', &payload[..total_len])
    }

    fn write_message(
        &mut self,
        msg_type: u8,
        payload: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let header = writer_common::message_header(payload.len(), msg_type)?;
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

    const CAP: usize = 32;
    const MI: usize = 8;

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

    struct OneRx(Option<Record<CAP, MI>>);

    impl RecordSource for OneRx {
        type Rec = Record<CAP, MI>;

        fn try_recv(&mut self) -> Option<Self::Rec> {
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

    enum TestMessages {}

    impl crate::ULogRegistry for TestMessages {
        const REGISTRY: crate::Registry = crate::Registry::new(&[crate::MessageMeta {
            name: Sample::NAME,
            format: Sample::FORMAT,
            wire_size: Sample::WIRE_SIZE,
        }]);
    }

    enum EmptyMessages {}

    impl crate::ULogRegistry for EmptyMessages {
        const REGISTRY: crate::Registry = crate::Registry::new(&[]);
    }

    #[test]
    fn startup_and_data_subscription() {
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
        let mut exporter = ULogExporter::<_, _, TestMessages, CAP, MI, 64>::new(sink, rx);

        exporter.emit_startup(100).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn logs_string() {
        let sink = VecSink::default();
        let mut text = heapless::String::<CAP>::new();
        let _ = text.push_str("hello");

        let rec = Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 1,
            text,
        };
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, EmptyMessages, CAP, MI, 64>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn dropped_streams_is_counted_when_streams_clamp() {
        let sink = VecSink::default();
        let rec = Record::Data {
            topic_index: 0,
            instance: 0,
            ts: 0,
            payload_len: 4,
            payload: [
                1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0,
            ],
        };
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, TestMessages, CAP, MI, 0>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
        assert_eq!(exporter.dropped_streams(), 1);
    }

    #[test]
    fn writes_parameter_message() {
        let sink = VecSink::default();
        let mut key = heapless::String::<CAP>::new();
        key.push_str("int32_t TEST_P").unwrap();
        let rec = Record::Parameter {
            key,
            value: ParameterValue::I32(10),
        };
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, EmptyMessages, CAP, MI, 64>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }
}
