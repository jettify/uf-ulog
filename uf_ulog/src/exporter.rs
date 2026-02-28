use core::marker::PhantomData;

use crate::wire;
use crate::{ExportError, ExportStep, ParameterValue, Record, RecordMeta, ULogRegistry};

pub trait RecordSource {
    type Rec;

    fn try_recv(&mut self) -> Option<Self::Rec>;
}

pub struct ULogExporter<
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
        match record.meta() {
            RecordMeta::LoggedString { level, tag, ts } => {
                if let Some(tag) = tag {
                    self.write_tagged_log(level as u8, tag, ts, record.bytes())
                } else {
                    self.write_log(level as u8, ts, record.bytes())
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

                let meta = wire::registry_entry::<R, <W as embedded_io::ErrorType>::Error>(
                    topic_index_usize,
                )?;

                let Some(slot) = wire::stream_slot::<MAX_MULTI_IDS>(
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
                    wire::slot_msg_id::<<W as embedded_io::ErrorType>::Error>(slot)?;

                if self.subscribed[slot] == 0 {
                    self.write_add_subscription(instance, msg_id, meta.name)?;
                    self.subscribed[slot] = 1;
                }

                self.write_data(msg_id, record.bytes())
            }
            RecordMeta::Parameter { value } => self.write_parameter(record.bytes(), value),
        }
    }

    fn write_header(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let header = wire::write_header(timestamp_micros);
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
        let _ = wire::format_payload_len::<<W as embedded_io::ErrorType>::Error>(
            name, format,
        )?;
        let separator = [b':'];
        let parts = [name.as_bytes(), &separator, format.as_bytes()];
        self.write_message_parts(b'F', &parts)
    }

    fn write_add_subscription(
        &mut self,
        multi_id: u8,
        msg_id: u16,
        name: &str,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let _ = wire::add_subscription_payload_len::<<W as embedded_io::ErrorType>::Error>(
            name,
        )?;
        let prefix = wire::add_subscription_prefix(multi_id, msg_id);
        let parts = [&prefix[..], name.as_bytes()];
        self.write_message_parts(b'A', &parts)
    }

    fn write_data(
        &mut self,
        msg_id: u16,
        data: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let _ =
            wire::data_payload_len::<<W as embedded_io::ErrorType>::Error>(data.len())?;
        let prefix = wire::data_prefix(msg_id);
        let parts = [&prefix[..], data];
        self.write_message_parts(b'D', &parts)
    }

    fn write_log(
        &mut self,
        level: u8,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let _ = wire::log_payload_len::<<W as embedded_io::ErrorType>::Error>(text.len())?;
        let prefix = wire::log_prefix(level, timestamp);
        let parts = [&prefix[..], text];
        self.write_message_parts(b'L', &parts)
    }

    fn write_tagged_log(
        &mut self,
        level: u8,
        tag: u16,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let _ = wire::tagged_log_payload_len::<<W as embedded_io::ErrorType>::Error>(
            text.len(),
        )?;
        let prefix = wire::tagged_log_prefix(level, tag, timestamp);
        let parts = [&prefix[..], text];
        self.write_message_parts(b'C', &parts)
    }

    fn write_parameter(
        &mut self,
        key: &[u8],
        value: ParameterValue,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let raw = match value {
            ParameterValue::I32(v) => v.to_le_bytes(),
            ParameterValue::F32(v) => v.to_le_bytes(),
        };
        let _ = wire::parameter_payload_len::<<W as embedded_io::ErrorType>::Error>(
            key, &raw,
        )?;
        let key_len = wire::parameter_prefix::<<W as embedded_io::ErrorType>::Error>(key)?;
        let parts = [&key_len[..], key, &raw];
        self.write_message_parts(b'P', &parts)
    }

    fn write_message(
        &mut self,
        msg_type: u8,
        payload: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let header = wire::message_header(payload.len(), msg_type)?;
        self.write_all(&header)?;
        self.write_all(payload)
    }

    fn write_message_parts(
        &mut self,
        msg_type: u8,
        parts: &[&[u8]],
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        let mut payload_len = 0usize;
        for part in parts {
            payload_len = wire::checked_total_len(payload_len, part.len(), usize::MAX)?;
        }
        let header = wire::message_header(payload_len, msg_type)?;
        self.write_all(&header)?;
        for part in parts {
            self.write_all(part)?;
        }
        Ok(())
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

    struct OneRxDefault(Option<Record<CAP, 4>>);

    impl RecordSource for OneRxDefault {
        type Rec = Record<CAP, 4>;

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
        let rec = Record::new_data(0, 1, 0, &[1, 2, 3, 4]).unwrap();
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, TestMessages, CAP, MI, 64>::new(sink, rx);

        exporter.emit_startup(100).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn logs_string() {
        let sink = VecSink::default();
        let rec = Record::new_log(LogLevel::Info, None, 1, b"hello");
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, EmptyMessages, CAP, MI, 64>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn dropped_streams_is_counted_when_streams_clamp() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 0, 0, &[1, 2, 3, 4]).unwrap();
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, TestMessages, CAP, MI, 0>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
        assert_eq!(exporter.dropped_streams(), 1);
    }

    #[test]
    fn writes_parameter_message() {
        let sink = VecSink::default();
        let rec = Record::new_parameter(b"int32_t TEST_P", ParameterValue::I32(10)).unwrap();
        let rx = OneRx(Some(rec));
        let mut exporter = ULogExporter::<_, _, EmptyMessages, CAP, MI, 64>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn default_max_multi_ids_accepts_instance_three() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 3, 0, &[1, 2, 3, 4]).unwrap();
        let rx = OneRxDefault(Some(rec));
        let mut exporter = ULogExporter::<_, _, TestMessages, CAP>::new(sink, rx);

        exporter.emit_startup(0).unwrap();
        assert_eq!(exporter.poll_once().unwrap(), ExportStep::Progressed);
    }

    #[test]
    fn default_max_multi_ids_rejects_instance_four() {
        let sink = VecSink::default();
        let rx = OneRxDefault(None);
        let mut exporter = ULogExporter::<_, _, TestMessages, CAP>::new(sink, rx);
        let rec = Record::new_data(0, 4, 0, &[1, 2, 3, 4]).unwrap();

        assert_eq!(exporter.write_record(rec), Err(ExportError::InvalidMultiId));
    }

    #[test]
    fn log_record_wire_bytes_match() {
        let sink = VecSink::default();
        let rx = OneRx(None);
        let mut exporter = ULogExporter::<_, _, EmptyMessages, CAP, MI, 64>::new(sink, rx);
        let rec = Record::new_log(LogLevel::Info, None, 0x0102_0304_0506_0708, b"hi");

        exporter.write_record(rec).unwrap();
        assert_eq!(
            exporter.writer_mut().bytes,
            [11, 0, b'L', b'6', 8, 7, 6, 5, 4, 3, 2, 1, b'h', b'i']
        );
    }

    #[test]
    fn parameter_record_wire_bytes_match() {
        let sink = VecSink::default();
        let rx = OneRx(None);
        let mut exporter = ULogExporter::<_, _, EmptyMessages, CAP, MI, 64>::new(sink, rx);
        let rec = Record::new_parameter(b"k", ParameterValue::I32(1)).unwrap();

        exporter.write_record(rec).unwrap();
        assert_eq!(
            exporter.writer_mut().bytes,
            [6, 0, b'P', 1, b'k', 1, 0, 0, 0]
        );
    }
}
