use core::marker::PhantomData;

use crate::exporter::{FormatsPending, StreamingReady};
use crate::wire;
use crate::{ExportError, ParameterValue, Record, RecordMeta, ULogRegistry};

pub struct ULogAsyncCoreExporter<
    W,
    R: ULogRegistry,
    State = FormatsPending,
    const RECORD_CAP: usize = 128,
    const MAX_MULTI_IDS: usize = 4,
    const MAX_STREAMS: usize = 128,
> {
    writer: W,
    subscribed: [u8; MAX_STREAMS],
    dropped_streams: u32,
    _messages: PhantomData<R>,
    _state: PhantomData<State>,
}

impl<W, R, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize, const MAX_STREAMS: usize>
    ULogAsyncCoreExporter<W, R, FormatsPending, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>
where
    W: embedded_io_async::Write,
    R: ULogRegistry,
{
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            subscribed: [0; MAX_STREAMS],
            dropped_streams: 0,
            _messages: PhantomData,
            _state: PhantomData,
        }
    }

    pub async fn start(
        mut self,
        timestamp_micros: u64,
    ) -> Result<
        ULogAsyncCoreExporter<W, R, StreamingReady, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>,
        ExportError<<W as embedded_io_async::ErrorType>::Error>,
    > {
        self.emit_startup(timestamp_micros).await?;
        Ok(self.into_streaming())
    }

    async fn emit_startup(
        &mut self,
        timestamp_micros: u64,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        self.write_header(timestamp_micros).await?;
        self.write_flag_bits().await?;
        for meta in R::REGISTRY.entries {
            self.write_format(meta.name, meta.format).await?;
        }

        Ok(())
    }

    fn into_streaming(
        self,
    ) -> ULogAsyncCoreExporter<W, R, StreamingReady, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS> {
        ULogAsyncCoreExporter {
            writer: self.writer,
            subscribed: self.subscribed,
            dropped_streams: self.dropped_streams,
            _messages: PhantomData,
            _state: PhantomData,
        }
    }
}

impl<
        W,
        R,
        State,
        const RECORD_CAP: usize,
        const MAX_MULTI_IDS: usize,
        const MAX_STREAMS: usize,
    > ULogAsyncCoreExporter<W, R, State, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>
where
    W: embedded_io_async::Write,
    R: ULogRegistry,
{
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn dropped_streams(&self) -> u32 {
        self.dropped_streams
    }

    async fn write_record_inner(
        &mut self,
        record: Record<RECORD_CAP>,
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

                let meta = wire::registry_entry::<R, <W as embedded_io_async::ErrorType>::Error>(
                    topic_index_usize,
                )?;
                if record.bytes().len() != meta.wire_size {
                    return Err(ExportError::InvalidWireSize);
                }

                let Some(slot) =
                    wire::stream_slot::<MAX_MULTI_IDS>(topic_index_usize, usize::from(instance))
                else {
                    self.dropped_streams = self.dropped_streams.saturating_add(1);
                    return Ok(());
                };

                if slot >= MAX_STREAMS {
                    self.dropped_streams = self.dropped_streams.saturating_add(1);
                    return Ok(());
                }

                let msg_id = wire::slot_msg_id::<<W as embedded_io_async::ErrorType>::Error>(slot)?;

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
        let header = wire::write_header(timestamp_micros);
        self.write_all(&header).await
    }

    async fn write_flag_bits(
        &mut self,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let payload = [0u8; 40];
        self.write_message(b'B', &payload).await
    }

    async fn write_sync(
        &mut self,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        self.write_message(b'S', &wire::ULOG_SYNC_MAGIC).await
    }

    async fn write_format(
        &mut self,
        name: &str,
        format: &str,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let _ =
            wire::format_payload_len::<<W as embedded_io_async::ErrorType>::Error>(name, format)?;
        let separator = [b':'];
        let parts = [name.as_bytes(), &separator, format.as_bytes()];
        self.write_message_parts(b'F', &parts).await
    }

    async fn write_add_subscription(
        &mut self,
        multi_id: u8,
        msg_id: u16,
        name: &str,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let _ =
            wire::add_subscription_payload_len::<<W as embedded_io_async::ErrorType>::Error>(name)?;
        let prefix = wire::add_subscription_prefix(multi_id, msg_id);
        let parts = [&prefix[..], name.as_bytes()];
        self.write_message_parts(b'A', &parts).await
    }

    async fn write_data(
        &mut self,
        msg_id: u16,
        data: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let _ = wire::data_payload_len::<<W as embedded_io_async::ErrorType>::Error>(data.len())?;
        let prefix = wire::data_prefix(msg_id);
        let parts = [&prefix[..], data];
        self.write_message_parts(b'D', &parts).await
    }

    async fn write_log(
        &mut self,
        level: u8,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let _ = wire::log_payload_len::<<W as embedded_io_async::ErrorType>::Error>(text.len())?;
        let prefix = wire::log_prefix(level, timestamp);
        let parts = [&prefix[..], text];
        self.write_message_parts(b'L', &parts).await
    }

    async fn write_tagged_log(
        &mut self,
        level: u8,
        tag: u16,
        timestamp: u64,
        text: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let _ =
            wire::tagged_log_payload_len::<<W as embedded_io_async::ErrorType>::Error>(text.len())?;
        let prefix = wire::tagged_log_prefix(level, tag, timestamp);
        let parts = [&prefix[..], text];
        self.write_message_parts(b'C', &parts).await
    }

    async fn write_parameter(
        &mut self,
        key: &[u8],
        value: ParameterValue,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let raw = match value {
            ParameterValue::I32(v) => v.to_le_bytes(),
            ParameterValue::F32(v) => v.to_le_bytes(),
        };
        let _ =
            wire::parameter_payload_len::<<W as embedded_io_async::ErrorType>::Error>(key, &raw)?;
        let key_len = wire::parameter_prefix::<<W as embedded_io_async::ErrorType>::Error>(key)?;
        let parts = [&key_len[..], key, &raw];
        self.write_message_parts(b'P', &parts).await
    }

    async fn write_message(
        &mut self,
        msg_type: u8,
        payload: &[u8],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let header = wire::message_header(payload.len(), msg_type)?;
        self.write_all(&header).await?;
        self.write_all(payload).await
    }

    async fn write_message_parts(
        &mut self,
        msg_type: u8,
        parts: &[&[u8]],
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        let mut payload_len = 0usize;
        for part in parts {
            payload_len = wire::checked_total_len(payload_len, part.len(), usize::MAX)?;
        }
        let header = wire::message_header(payload_len, msg_type)?;
        self.write_all(&header).await?;
        for part in parts {
            self.write_all(part).await?;
        }
        Ok(())
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

impl<W, R, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize, const MAX_STREAMS: usize>
    ULogAsyncCoreExporter<W, R, StreamingReady, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>
where
    W: embedded_io_async::Write,
    R: ULogRegistry,
{
    pub async fn accept(
        &mut self,
        record: Record<RECORD_CAP>,
    ) -> Result<(), ExportError<<W as embedded_io_async::ErrorType>::Error>> {
        self.write_record_inner(record).await
    }

    pub async fn emit_sync(
        &mut self,
    ) -> Result<(), ExportError<<W as embedded_io::ErrorType>::Error>> {
        self.write_sync().await
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

    impl embedded_io_async::ErrorType for VecSink {
        type Error = core::convert::Infallible;
    }

    impl embedded_io_async::Write for VecSink {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
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

    enum MismatchMessages {}

    impl crate::ULogRegistry for MismatchMessages {
        const REGISTRY: crate::Registry = crate::Registry::new(&[crate::MessageMeta {
            name: "sample",
            format: "uint64_t timestamp;",
            wire_size: 8,
        }]);
    }

    #[futures_test::test]
    async fn core_start_then_accept() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 1, 0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();

        let mut core =
            ULogAsyncCoreExporter::<_, TestMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(100)
                .await
                .unwrap();

        core.accept(rec).await.unwrap();
    }

    #[futures_test::test]
    async fn startup_and_data_subscription() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 1, 0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let mut exporter =
            ULogAsyncCoreExporter::<_, TestMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(100)
                .await
                .unwrap();
        exporter.accept(rec).await.unwrap();
    }

    #[futures_test::test]
    async fn logs_string() {
        let sink = VecSink::default();
        let rec = Record::new_log(LogLevel::Info, None, 1, b"hello");
        let mut exporter =
            ULogAsyncCoreExporter::<_, EmptyMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();
        exporter.accept(rec).await.unwrap();
    }

    #[futures_test::test]
    async fn dropped_streams_is_counted_when_streams_clamp() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 0, 0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let mut exporter =
            ULogAsyncCoreExporter::<_, TestMessages, FormatsPending, CAP, MI, 0>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();
        exporter.accept(rec).await.unwrap();
        assert_eq!(exporter.dropped_streams(), 1);
    }

    #[futures_test::test]
    async fn writes_parameter_message() {
        let sink = VecSink::default();
        let rec = Record::new_parameter(b"int32_t TEST_P", ParameterValue::I32(10)).unwrap();
        let mut exporter =
            ULogAsyncCoreExporter::<_, EmptyMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();
        exporter.accept(rec).await.unwrap();
    }

    #[futures_test::test]
    async fn data_wire_size_mismatch_rejected() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 1, 0, &[1, 2, 3, 4]).unwrap();
        let mut exporter =
            ULogAsyncCoreExporter::<_, MismatchMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();

        assert_eq!(
            exporter.accept(rec).await,
            Err(ExportError::InvalidWireSize)
        );
    }

    #[futures_test::test]
    async fn default_max_multi_ids_accepts_instance_three() {
        let sink = VecSink::default();
        let rec = Record::new_data(0, 3, 0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let mut exporter = ULogAsyncCoreExporter::<_, TestMessages, FormatsPending, CAP>::new(sink)
            .start(1_772_079_727_637)
            .await
            .unwrap();
        exporter.accept(rec).await.unwrap();
    }

    #[futures_test::test]
    async fn default_max_multi_ids_rejects_instance_four_after_startup() {
        let sink = VecSink::default();
        let mut exporter = ULogAsyncCoreExporter::<_, TestMessages, FormatsPending, CAP>::new(sink)
            .start(1_772_079_727_637)
            .await
            .unwrap();
        let rec = Record::new_data(0, 4, 0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();

        assert_eq!(exporter.accept(rec).await, Err(ExportError::InvalidMultiId));
    }

    #[futures_test::test]
    async fn log_record_wire_bytes_match() {
        let sink = VecSink::default();
        let mut exporter =
            ULogAsyncCoreExporter::<_, EmptyMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();
        let rec = Record::new_log(LogLevel::Info, None, 0x0102_0304_0506_0708, b"hi");

        exporter.accept(rec).await.unwrap();
        assert!(exporter
            .writer_mut()
            .bytes
            .ends_with(&[11, 0, b'L', b'6', 8, 7, 6, 5, 4, 3, 2, 1, b'h', b'i']));
    }

    #[futures_test::test]
    async fn parameter_record_wire_bytes_match() {
        let sink = VecSink::default();
        let mut exporter =
            ULogAsyncCoreExporter::<_, EmptyMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();
        let rec = Record::new_parameter(b"k", ParameterValue::I32(1)).unwrap();

        exporter.accept(rec).await.unwrap();
        assert!(exporter
            .writer_mut()
            .bytes
            .ends_with(&[6, 0, b'P', 1, b'k', 1, 0, 0, 0]));
    }

    #[futures_test::test]
    async fn test_sync_message() {
        let sink = VecSink::default();
        let rec = Record::new_parameter(b"int32_t TEST_P", ParameterValue::I32(10)).unwrap();
        let mut exporter =
            ULogAsyncCoreExporter::<_, EmptyMessages, FormatsPending, CAP, MI, 64>::new(sink)
                .start(1_772_079_727_637)
                .await
                .unwrap();

        exporter.accept(rec).await.unwrap();
        exporter.emit_sync().await.unwrap();

        let expected = [0x2F, 0x73, 0x13, 0x20, 0x25, 0x0C, 0xBB, 0x12];
        assert!(exporter.writer_mut().bytes.ends_with(&expected));
    }
}
