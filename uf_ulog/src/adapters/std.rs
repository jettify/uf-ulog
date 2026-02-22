use core::marker::PhantomData;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::mpsc::{TryRecvError as ChannelTryRecvError, TrySendError as ChannelTrySendError};

use crate::{Record, RecordSink, RecordSource, TrySendError, ULogCfg};

pub struct ChannelTx<C: ULogCfg> {
    tx: SyncSender<Record<C>>,
}

impl<C: ULogCfg> ChannelTx<C> {
    pub fn new(tx: SyncSender<Record<C>>) -> Self {
        Self { tx }
    }
}

impl<C: ULogCfg> RecordSink<C> for ChannelTx<C> {
    fn try_send(&mut self, item: Record<C>) -> Result<(), TrySendError> {
        match self.tx.try_send(item) {
            Ok(()) => Ok(()),
            Err(ChannelTrySendError::Full(_)) => Err(TrySendError::Full),
            Err(ChannelTrySendError::Disconnected(_)) => Err(TrySendError::Closed),
        }
    }
}

pub struct ChannelRx<C: ULogCfg> {
    rx: Receiver<Record<C>>,
    _cfg: PhantomData<C>,
}

impl<C: ULogCfg> ChannelRx<C> {
    pub fn new(rx: Receiver<Record<C>>) -> Self {
        Self {
            rx,
            _cfg: PhantomData,
        }
    }
}

impl<C: ULogCfg> RecordSource<C> for ChannelRx<C> {
    fn try_recv(&mut self) -> Option<Record<C>> {
        match self.rx.try_recv() {
            Ok(record) => Some(record),
            Err(ChannelTryRecvError::Empty) | Err(ChannelTryRecvError::Disconnected) => None,
        }
    }
}

pub fn channel<C: ULogCfg>(bound: usize) -> (ChannelTx<C>, ChannelRx<C>) {
    let (tx, rx) = sync_channel(bound);
    (ChannelTx::new(tx), ChannelRx::new(rx))
}

#[cfg(test)]
mod tests {
    use crate::LogLevel;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestCfg;

    impl crate::ULogCfg for TestCfg {
        type Text = heapless::String<16>;
        type Payload = [u8; 16];
        type Streams = [u8; 64];

        const MAX_MULTI_IDS: usize = 8;
    }

    #[test]
    fn send_and_recv_round_trip() {
        let (mut tx, mut rx) = channel::<TestCfg>(1);
        let mut text = heapless::String::<16>::new();
        let _ = text.push_str("ok");
        let record = Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 1,
            text,
        };

        tx.try_send(record.clone()).unwrap();
        assert_eq!(rx.try_recv(), Some(record));
    }

    #[test]
    fn full_and_closed_mapping() {
        let (raw_tx, raw_rx) = sync_channel::<Record<TestCfg>>(1);
        let mut tx = ChannelTx::<TestCfg>::new(raw_tx.clone());
        let mut text = heapless::String::<16>::new();
        let _ = text.push_str("x");
        let record = Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 0,
            text,
        };
        tx.try_send(record.clone()).unwrap();
        assert_eq!(tx.try_send(record), Err(TrySendError::Full));

        drop(raw_rx);
        let mut closed_tx = ChannelTx::<TestCfg>::new(raw_tx);
        let mut text = heapless::String::<16>::new();
        let _ = text.push_str("x");
        let closed_record = Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 0,
            text,
        };
        assert_eq!(closed_tx.try_send(closed_record), Err(TrySendError::Closed));
    }
}
