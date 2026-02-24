use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::{DefaultCfg, LogLevel, MessageSet, PayloadBuf, TextBuf, TopicIndex, ULogCfg, ULogData};

pub trait RecordSink<C: ULogCfg> {
    fn try_send(&mut self, record: Record<C>) -> Result<(), TrySendError>;
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

#[derive(Debug, Clone, PartialEq)]
pub enum Record<C: ULogCfg> {
    LoggedString {
        level: LogLevel,
        tag: Option<u16>,
        ts: u64,
        text: C::Text,
    },
    Data {
        topic_index: u16,
        instance: u8,
        ts: u64,
        payload_len: u16,
        payload: C::Payload,
    },
}

pub struct ULogProducer<Tx, R: MessageSet, C: ULogCfg = DefaultCfg> {
    tx: Tx,
    dropped_total: AtomicU32,
    _cfg: PhantomData<C>,
    _messages: PhantomData<R>,
}

impl<Tx, R, C> ULogProducer<Tx, R, C>
where
    Tx: RecordSink<C>,
    R: MessageSet,
    C: ULogCfg,
{
    pub fn new(tx: Tx) -> Self {
        Self {
            tx,
            dropped_total: AtomicU32::new(0),
            _cfg: PhantomData,
            _messages: PhantomData,
        }
    }

    pub fn log(&mut self, level: LogLevel, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<C>(msg);
        let record = Record::LoggedString {
            level,
            tag: None,
            ts,
            text,
        };
        self.try_emit(record)
    }

    pub fn log_tagged(&mut self, level: LogLevel, tag: u16, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<C>(msg);
        let record = Record::LoggedString {
            level,
            tag: Some(tag),
            ts,
            text,
        };
        self.try_emit(record)
    }

    pub fn data<T>(&mut self, value: &T) -> EmitStatus
    where
        T: ULogData + TopicIndex<R>,
    {
        self.data_instance(value, 0)
    }

    pub fn data_instance<T>(&mut self, value: &T, instance: u8) -> EmitStatus
    where
        T: ULogData + TopicIndex<R>,
    {
        let topic_index = <T as TopicIndex<R>>::INDEX;
        if usize::from(topic_index) >= R::REGISTRY.len() {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return EmitStatus::Dropped;
        }

        let mut payload = C::Payload::zeroed();
        let encoded_len = match value.encode(payload.as_mut_slice()) {
            Ok(encoded_len) => encoded_len,
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };
        if encoded_len > C::Payload::CAPACITY {
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

    fn try_emit(&mut self, record: Record<C>) -> EmitStatus {
        match self.tx.try_send(record) {
            Ok(()) => EmitStatus::Emitted,
            Err(TrySendError::Full | TrySendError::Closed) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                EmitStatus::Dropped
            }
        }
    }
}

fn make_text<C: ULogCfg>(msg: &str) -> C::Text {
    debug_assert!(msg.is_ascii());
    let mut text = C::Text::new();
    let end = core::cmp::min(msg.len(), C::Text::CAPACITY);
    text.push_str(&msg[..end]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncodeError;
    use core::cell::RefCell;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SmallCfg;

    impl crate::ULogCfg for SmallCfg {
        type Text = heapless::String<16>;
        type Payload = [u8; 16];
        type Streams = [u8; 64];

        const MAX_MULTI_IDS: usize = 8;
    }

    struct CaptureTx<C: ULogCfg> {
        records: RefCell<[Option<Record<C>>; 4]>,
        idx: RefCell<usize>,
        fail_after: Option<usize>,
    }

    impl<C: ULogCfg> Default for CaptureTx<C> {
        fn default() -> Self {
            Self {
                records: RefCell::new([None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: None,
            }
        }
    }

    impl<C: ULogCfg> CaptureTx<C> {
        fn with_fail_after(fail_after: usize) -> Self {
            Self {
                records: RefCell::new([None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: Some(fail_after),
            }
        }
    }

    impl<C: ULogCfg> RecordSink<C> for CaptureTx<C> {
        fn try_send(&mut self, item: Record<C>) -> Result<(), TrySendError> {
            let i = *self.idx.borrow();
            if self.fail_after.is_some_and(|n| i >= n) {
                return Err(TrySendError::Full);
            }
            if i >= 4 {
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

    #[test]
    fn logs_and_tracks_drops() {
        let tx = CaptureTx::<SmallCfg>::with_fail_after(1);
        crate::register_messages! {
            enum TestMessages {
                SampleData,
            }
        }
        let mut producer = ULogProducer::<_, TestMessages, SmallCfg>::new(tx);

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
    fn encodes_data_event() {
        let tx = CaptureTx::<SmallCfg>::default();
        crate::register_messages! {
            enum TestMessages {
                SampleData,
            }
        }
        let mut producer = ULogProducer::<_, TestMessages, SmallCfg>::new(tx);
        let sample = SampleData;

        assert_eq!(producer.data(&sample), EmitStatus::Emitted);
    }

    #[test]
    fn emits_default_and_explicit_instance() {
        let tx = CaptureTx::<SmallCfg>::default();
        crate::register_messages! {
            enum TestMessages {
                SampleData,
            }
        }
        let mut producer = ULogProducer::<_, TestMessages, SmallCfg>::new(tx);
        let sample = SampleData;

        assert_eq!(producer.data(&sample), EmitStatus::Emitted);
        assert_eq!(producer.data_instance(&sample, 3), EmitStatus::Emitted);
    }

    #[test]
    fn can_use_default_config() {
        let tx: CaptureTx<crate::DefaultCfg> = CaptureTx::default();
        crate::register_messages! {
            enum TestMessages {
                SampleData,
            }
        }
        let mut producer = ULogProducer::<_, TestMessages>::new(tx);

        assert_eq!(
            producer.log(LogLevel::Info, 42, "boot"),
            EmitStatus::Emitted
        );
    }
}
