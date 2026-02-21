use core::sync::atomic::{AtomicU32, Ordering};

use crate::{EncodeError, LogLevel, ULogData};
use heapless::String;

pub trait Clock {
    fn now_micros(&self) -> u64;
}

pub trait TrySend<T> {
    fn try_send(&self, item: T) -> Result<(), TrySendError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrySendError {
    Full,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitError {
    ChannelFull,
    ChannelClosed,
    TextTooLong,
    PayloadTooLarge,
    Encode(EncodeError),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> {
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
    Clk,
    const MAX_TEXT: usize = DEFAULT_MAX_TEXT,
    const MAX_PAYLOAD: usize = DEFAULT_MAX_PAYLOAD,
> {
    tx: Tx,
    clock: Clk,
    truncate_text: bool,
    dropped_total: AtomicU32,
}

impl<Tx, Clk, const MAX_TEXT: usize, const MAX_PAYLOAD: usize>
    ULogProducer<Tx, Clk, MAX_TEXT, MAX_PAYLOAD>
where
    Tx: TrySend<Event<MAX_TEXT, MAX_PAYLOAD>>,
    Clk: Clock,
{
    pub fn new(tx: Tx, clock: Clk, truncate_text: bool) -> Self {
        Self {
            tx,
            clock,
            truncate_text,
            dropped_total: AtomicU32::new(0),
        }
    }

    pub fn try_log(&self, level: LogLevel, msg: &str) -> Result<(), EmitError> {
        let text = make_text::<MAX_TEXT>(msg, self.truncate_text)?;
        let event = Event::LoggedString {
            level,
            tag: None,
            ts: self.clock.now_micros(),
            text,
        };
        self.try_emit(event)
    }

    pub fn try_log_tagged(&self, level: LogLevel, tag: u16, msg: &str) -> Result<(), EmitError> {
        let text = make_text::<MAX_TEXT>(msg, self.truncate_text)?;
        let event = Event::LoggedString {
            level,
            tag: Some(tag),
            ts: self.clock.now_micros(),
            text,
        };
        self.try_emit(event)
    }

    pub fn try_data<T: ULogData>(&self, value: &T) -> Result<(), EmitError> {
        let mut payload = [0u8; MAX_PAYLOAD];
        let encoded_len = value.encode(&mut payload).map_err(EmitError::Encode)?;
        let payload_len = u16::try_from(encoded_len).map_err(|_| EmitError::PayloadTooLarge)?;
        if usize::from(payload_len) > MAX_PAYLOAD {
            return Err(EmitError::PayloadTooLarge);
        }

        let event = Event::Data {
            name: T::NAME,
            ts: value.timestamp(),
            payload_len,
            payload,
        };

        self.try_emit(event)
    }

    pub fn dropped_count(&self) -> u32 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    fn try_emit(&self, event: Event<MAX_TEXT, MAX_PAYLOAD>) -> Result<(), EmitError> {
        self.tx.try_send(event).map_err(|err| match err {
            TrySendError::Full => {
                self.dropped_total.fetch_add(1, Ordering::Relaxed);
                EmitError::ChannelFull
            }
            TrySendError::Closed => EmitError::ChannelClosed,
        })
    }
}

fn make_text<const MAX_TEXT: usize>(
    msg: &str,
    truncate: bool,
) -> Result<String<MAX_TEXT>, EmitError> {
    let mut text = String::<MAX_TEXT>::new();

    for ch in msg.chars() {
        if text.push(ch).is_err() {
            if truncate {
                break;
            }
            return Err(EmitError::TextTooLong);
        }
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::RefCell;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_micros(&self) -> u64 {
            42
        }
    }

    struct CaptureTx<T> {
        events: RefCell<[Option<T>; 4]>,
        idx: RefCell<usize>,
        fail_after: Option<usize>,
    }

    impl<T> Default for CaptureTx<T> {
        fn default() -> Self {
            Self {
                events: RefCell::new([None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: None,
            }
        }
    }

    impl<T> CaptureTx<T> {
        fn with_fail_after(fail_after: usize) -> Self {
            Self {
                events: RefCell::new([None, None, None, None]),
                idx: RefCell::new(0),
                fail_after: Some(fail_after),
            }
        }
    }

    impl<T> TrySend<T> for CaptureTx<T> {
        fn try_send(&self, item: T) -> Result<(), TrySendError> {
            let i = *self.idx.borrow();
            if self.fail_after.is_some_and(|n| i >= n) {
                return Err(TrySendError::Full);
            }
            if i >= 4 {
                return Err(TrySendError::Closed);
            }
            self.events.borrow_mut()[i] = Some(item);
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
        let producer = ULogProducer::<_, _, 16, 16>::new(tx, FixedClock, true);

        assert!(producer.try_log(LogLevel::Info, "boot").is_ok());
        assert_eq!(
            producer.try_log(LogLevel::Info, "next"),
            Err(EmitError::ChannelFull)
        );
        assert_eq!(producer.dropped_count(), 1);
    }

    #[test]
    fn encodes_data_event() {
        let tx = CaptureTx::default();
        let producer = ULogProducer::<_, _, 16, 16>::new(tx, FixedClock, true);
        let sample = SampleData;

        assert!(producer.try_data(&sample).is_ok());
    }

    #[test]
    fn can_use_default_capacities() {
        let tx = CaptureTx::default();
        let producer = ULogProducer::new(tx, FixedClock, true);

        assert!(producer.try_log(LogLevel::Info, "boot").is_ok());
    }
}
