use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::{LogLevel, ParameterValue, Record, TopicOf, TrySendError, ULogData, ULogRegistry};

pub trait RecordSink {
    type Rec;

    fn try_send(&mut self, record: Self::Rec) -> Result<(), TrySendError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitStatus {
    Emitted,
    Dropped,
}

pub struct ULogProducer<
    Tx,
    R: ULogRegistry,
    const RECORD_CAP: usize = 256,
    const MAX_MULTI_IDS: usize = 4,
> {
    tx: Tx,
    dropped_total: AtomicU32,
    _messages: PhantomData<R>,
}

impl<Tx, R, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    ULogProducer<Tx, R, RECORD_CAP, MAX_MULTI_IDS>
where
    Tx: RecordSink<Rec = Record<RECORD_CAP, MAX_MULTI_IDS>>,
    R: ULogRegistry,
{
    pub fn new(tx: Tx) -> Self {
        Self {
            tx,
            dropped_total: AtomicU32::new(0),
            _messages: PhantomData,
        }
    }

    pub fn log(&mut self, level: LogLevel, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<RECORD_CAP>(msg);
        let record = Record::new_log(level, None, ts, &text);
        self.try_emit(record)
    }

    pub fn log_tagged(&mut self, level: LogLevel, tag: u16, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<RECORD_CAP>(msg);
        let record = Record::new_log(level, Some(tag), ts, &text);
        self.try_emit(record)
    }

    pub fn parameter_i32(&mut self, name: &str, value: i32) -> EmitStatus {
        self.parameter("int32_t", name, ParameterValue::I32(value))
    }

    pub fn parameter_f32(&mut self, name: &str, value: f32) -> EmitStatus {
        self.parameter("float", name, ParameterValue::F32(value))
    }

    pub fn data<T>(&mut self, value: &T) -> EmitStatus
    where
        T: ULogData + TopicOf<R>,
    {
        self.data_instance(value, 0)
    }

    pub fn data_instance<T>(&mut self, value: &T, instance: u8) -> EmitStatus
    where
        T: ULogData + TopicOf<R>,
    {
        let topic_index = <T as TopicOf<R>>::TOPIC.id();
        if usize::from(topic_index) >= R::REGISTRY.len() {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return EmitStatus::Dropped;
        }

        if usize::from(instance) >= MAX_MULTI_IDS {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return EmitStatus::Dropped;
        }

        let mut encoded = [0; RECORD_CAP];
        let encoded_len = match value.encode(&mut encoded) {
            Ok(encoded_len) => encoded_len,
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };
        if encoded_len > RECORD_CAP {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return EmitStatus::Dropped;
        }
        let record = match Record::new_data(
            topic_index,
            instance,
            value.timestamp(),
            &encoded[..encoded_len],
        ) {
            Some(record) => record,
            None => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };

        self.try_emit(record)
    }

    pub fn dropped_count(&self) -> u32 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    fn parameter(&mut self, ty: &str, name: &str, value: ParameterValue) -> EmitStatus {
        let mut key = heapless::String::<RECORD_CAP>::new();
        if key.push_str(ty).is_err()
            || key.push(' ').is_err()
            || key.push_str(name).is_err()
            || key.len() > usize::from(u8::MAX)
        {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return EmitStatus::Dropped;
        }

        let record = match Record::new_parameter(key.as_bytes(), value) {
            Some(record) => record,
            None => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };
        self.try_emit(record)
    }

    fn try_emit(&mut self, record: Record<RECORD_CAP, MAX_MULTI_IDS>) -> EmitStatus {
        match self.tx.try_send(record) {
            Ok(()) => EmitStatus::Emitted,
            Err(TrySendError::Full | TrySendError::Closed) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                EmitStatus::Dropped
            }
        }
    }
}

fn make_text<const RECORD_CAP: usize>(msg: &str) -> heapless::Vec<u8, RECORD_CAP> {
    debug_assert!(msg.is_ascii());
    let mut text = heapless::Vec::new();
    let end = core::cmp::min(msg.len(), RECORD_CAP);
    let _ = text.extend_from_slice(&msg.as_bytes()[..end]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EncodeError, LogLevel, Record, TrySendError};
    use core::cell::RefCell;

    const CAP: usize = 16;
    const MI: usize = 8;

    struct CaptureTx {
        records: RefCell<[Option<Record<CAP, MI>>; 6]>,
        idx: RefCell<usize>,
        fail_after: Option<usize>,
    }

    impl Default for CaptureTx {
        fn default() -> Self {
            Self {
                records: RefCell::new([None, None, None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: None,
            }
        }
    }

    impl CaptureTx {
        fn with_fail_after(fail_after: usize) -> Self {
            Self {
                records: RefCell::new([None, None, None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: Some(fail_after),
            }
        }
    }

    impl RecordSink for CaptureTx {
        type Rec = Record<CAP, MI>;

        fn try_send(&mut self, item: Self::Rec) -> Result<(), TrySendError> {
            let i = *self.idx.borrow();
            if self.fail_after.is_some_and(|n| i >= n) {
                return Err(TrySendError::Full);
            }
            if i >= 6 {
                return Err(TrySendError::Closed);
            }
            self.records.borrow_mut()[i] = Some(item);
            *self.idx.borrow_mut() = i + 1;
            Ok(())
        }
    }

    struct SampleData;

    impl ULogData for SampleData {
        const FORMAT: &'static str = "uint64_t timestamp;";
        const NAME: &'static str = "sample";
        const WIRE_SIZE: usize = 8;

        fn encode(&self, buf: &mut [u8]) -> Result<usize, EncodeError> {
            if buf.len() < 8 {
                return Err(EncodeError::BufferOverflow);
            }
            buf[..8].copy_from_slice(&100u64.to_le_bytes());
            Ok(8)
        }

        fn timestamp(&self) -> u64 {
            100
        }
    }

    enum TestMessages {}

    impl crate::ULogRegistry for TestMessages {
        const REGISTRY: crate::Registry = crate::Registry::new(&[crate::MessageMeta {
            name: SampleData::NAME,
            format: SampleData::FORMAT,
            wire_size: SampleData::WIRE_SIZE,
        }]);
    }

    impl crate::TopicOf<TestMessages> for SampleData {
        const TOPIC: crate::Topic<Self> = crate::Topic::new(0);
    }

    #[test]
    fn logs_and_tracks_drops() {
        let tx = CaptureTx::with_fail_after(1);
        let mut producer = ULogProducer::<_, TestMessages, CAP, MI>::new(tx);

        assert_eq!(
            producer.log(LogLevel::Info, 42, "boot"),
            EmitStatus::Emitted
        );
        assert_eq!(
            producer.log(LogLevel::Info, 43, "next"),
            EmitStatus::Dropped
        );
        assert_eq!(producer.dropped_count(), 1);
    }

    #[test]
    fn rejects_too_large_instance() {
        let tx = CaptureTx::default();
        let mut producer = ULogProducer::<_, TestMessages, CAP, MI>::new(tx);

        let status = producer.data_instance(&SampleData, MI as u8);
        assert_eq!(status, EmitStatus::Dropped);
        assert_eq!(producer.dropped_count(), 1);
    }

    #[test]
    fn parameter_is_emitted() {
        let tx = CaptureTx::default();
        let mut producer = ULogProducer::<_, TestMessages, CAP, MI>::new(tx);

        assert_eq!(producer.parameter_i32("P", 1), EmitStatus::Emitted);
    }

    struct CaptureTxDefault {
        records: RefCell<[Option<Record<CAP, 4>>; 2]>,
        idx: RefCell<usize>,
    }

    impl Default for CaptureTxDefault {
        fn default() -> Self {
            Self {
                records: RefCell::new([None, None]),
                idx: RefCell::new(0),
            }
        }
    }

    impl RecordSink for CaptureTxDefault {
        type Rec = Record<CAP, 4>;

        fn try_send(&mut self, item: Self::Rec) -> Result<(), TrySendError> {
            let i = *self.idx.borrow();
            if i >= 2 {
                return Err(TrySendError::Full);
            }
            self.records.borrow_mut()[i] = Some(item);
            *self.idx.borrow_mut() = i + 1;
            Ok(())
        }
    }

    #[test]
    fn default_max_multi_ids_accepts_instance_three() {
        let tx = CaptureTxDefault::default();
        let mut producer = ULogProducer::<_, TestMessages, CAP>::new(tx);

        let status = producer.data_instance(&SampleData, 3);
        assert_eq!(status, EmitStatus::Emitted);
        assert_eq!(producer.dropped_count(), 0);
    }

    #[test]
    fn default_max_multi_ids_rejects_instance_four() {
        let tx = CaptureTxDefault::default();
        let mut producer = ULogProducer::<_, TestMessages, CAP>::new(tx);

        let status = producer.data_instance(&SampleData, 4);
        assert_eq!(status, EmitStatus::Dropped);
        assert_eq!(producer.dropped_count(), 1);
    }
}
