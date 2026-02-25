use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::{LogLevel, TopicOf, ULogData, ULogRegistry};

pub trait RecordSink {
    type Rec;

    fn try_send(&mut self, record: Self::Rec) -> Result<(), TrySendError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrySendError {
    Full,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitStatus {
    Emitted,
    Dropped,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterValue {
    I32(i32),
    F32(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Record<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> {
    LoggedString {
        level: LogLevel,
        tag: Option<u16>,
        ts: u64,
        text: heapless::String<RECORD_CAP>,
    },
    Data {
        topic_index: u16,
        instance: u8,
        ts: u64,
        payload_len: u16,
        payload: [u8; RECORD_CAP],
    },
    Parameter {
        key: heapless::String<RECORD_CAP>,
        value: ParameterValue,
    },
}

pub struct ULogProducer<
    Tx,
    R: ULogRegistry,
    const RECORD_CAP: usize = 256,
    const MAX_MULTI_IDS: usize = 8,
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
        let record = Record::LoggedString {
            level,
            tag: None,
            ts,
            text,
        };
        self.try_emit(record)
    }

    pub fn log_tagged(&mut self, level: LogLevel, tag: u16, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<RECORD_CAP>(msg);
        let record = Record::LoggedString {
            level,
            tag: Some(tag),
            ts,
            text,
        };
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

        let mut payload = [0; RECORD_CAP];
        let encoded_len = match value.encode(&mut payload) {
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
        let payload_len = match u16::try_from(encoded_len) {
            Ok(payload_len) => payload_len,
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };

        let record = Record::Data {
            topic_index,
            instance,
            ts: value.timestamp(),
            payload_len,
            payload,
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

        self.try_emit(Record::Parameter { key, value })
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

fn make_text<const RECORD_CAP: usize>(msg: &str) -> heapless::String<RECORD_CAP> {
    debug_assert!(msg.is_ascii());
    let mut text = heapless::String::new();
    let end = core::cmp::min(msg.len(), RECORD_CAP);
    let _ = text.push_str(&msg[..end]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncodeError;
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
}
