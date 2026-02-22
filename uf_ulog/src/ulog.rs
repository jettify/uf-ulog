use core::sync::atomic::{AtomicU32, Ordering};

use crate::{LogLevel, ULogData};
use heapless::String;

pub trait RecordSink<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> {
    fn try_send(&self, record: Record<MAX_TEXT, MAX_PAYLOAD>) -> Result<(), TrySendError>;
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
pub enum Record<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> {
    LoggedString {
        level: LogLevel,
        tag: Option<u16>,
        ts: u64,
        text: String<MAX_TEXT>,
    },
    Data {
        name: &'static str,
        ts: u64,
        payload_len: u16,
        payload: [u8; MAX_PAYLOAD],
    },
}

pub const DEFAULT_MAX_TEXT: usize = 128;
pub const DEFAULT_MAX_PAYLOAD: usize = 256;

pub struct ULogProducer<
    Tx,
    const MAX_TEXT: usize = DEFAULT_MAX_TEXT,
    const MAX_PAYLOAD: usize = DEFAULT_MAX_PAYLOAD,
> {
    tx: Tx,
    dropped_total: AtomicU32,
}

impl<Tx, const MAX_TEXT: usize, const MAX_PAYLOAD: usize> ULogProducer<Tx, MAX_TEXT, MAX_PAYLOAD>
where
    Tx: RecordSink<MAX_TEXT, MAX_PAYLOAD>,
{
    pub fn new(tx: Tx) -> Self {
        Self {
            tx,
            dropped_total: AtomicU32::new(0),
        }
    }

    pub fn log(&self, level: LogLevel, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<MAX_TEXT>(msg);
        let record = Record::LoggedString {
            level,
            tag: None,
            ts,
            text,
        };
        self.try_emit(record)
    }

    pub fn log_tagged(&self, level: LogLevel, tag: u16, ts: u64, msg: &str) -> EmitStatus {
        let text = make_text::<MAX_TEXT>(msg);
        let record = Record::LoggedString {
            level,
            tag: Some(tag),
            ts,
            text,
        };
        self.try_emit(record)
    }

    pub fn data<T: ULogData>(&self, value: &T) -> EmitStatus {
        let mut payload = [0u8; MAX_PAYLOAD];
        let encoded_len = match value.encode(&mut payload) {
            Ok(encoded_len) => encoded_len,
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };
        let payload_len = match u16::try_from(encoded_len) {
            Ok(payload_len) => payload_len,
            Err(_) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                return EmitStatus::Dropped;
            }
        };
        if usize::from(payload_len) > MAX_PAYLOAD {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return EmitStatus::Dropped;
        }

        let record = Record::Data {
            name: T::NAME,
            ts: value.timestamp(),
            payload_len,
            payload,
        };

        self.try_emit(record)
    }

    pub fn dropped_count(&self) -> u32 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    fn try_emit(&self, record: Record<MAX_TEXT, MAX_PAYLOAD>) -> EmitStatus {
        match self.tx.try_send(record) {
            Ok(()) => EmitStatus::Emitted,
            Err(TrySendError::Full | TrySendError::Closed) => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                EmitStatus::Dropped
            }
        }
    }
}

fn make_text<const MAX_TEXT: usize>(msg: &str) -> String<MAX_TEXT> {
    debug_assert!(msg.is_ascii());
    let mut text = String::<MAX_TEXT>::new();
    let end = core::cmp::min(msg.len(), MAX_TEXT);
    let _ = text.push_str(&msg[..end]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncodeError;
    use core::cell::RefCell;

    struct CaptureTx<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> {
        records: RefCell<[Option<Record<MAX_TEXT, MAX_PAYLOAD>>; 4]>,
        idx: RefCell<usize>,
        fail_after: Option<usize>,
    }

    impl<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> Default for CaptureTx<MAX_TEXT, MAX_PAYLOAD> {
        fn default() -> Self {
            Self {
                records: RefCell::new([None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: None,
            }
        }
    }

    impl<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> CaptureTx<MAX_TEXT, MAX_PAYLOAD> {
        fn with_fail_after(fail_after: usize) -> Self {
            Self {
                records: RefCell::new([None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: Some(fail_after),
            }
        }
    }

    impl<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> RecordSink<MAX_TEXT, MAX_PAYLOAD>
        for CaptureTx<MAX_TEXT, MAX_PAYLOAD>
    {
        fn try_send(&self, item: Record<MAX_TEXT, MAX_PAYLOAD>) -> Result<(), TrySendError> {
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
        let tx = CaptureTx::with_fail_after(1);
        let producer = ULogProducer::<_, 16, 16>::new(tx);

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
        let tx = CaptureTx::default();
        let producer = ULogProducer::<_, 16, 16>::new(tx);
        let sample = SampleData;

        assert_eq!(producer.data(&sample), EmitStatus::Emitted);
    }

    #[test]
    fn can_use_default_capacities() {
        let tx: CaptureTx<DEFAULT_MAX_TEXT, DEFAULT_MAX_PAYLOAD> = CaptureTx::default();
        let producer = ULogProducer::new(tx);

        assert_eq!(
            producer.log(LogLevel::Info, 42, "boot"),
            EmitStatus::Emitted
        );
    }
}
