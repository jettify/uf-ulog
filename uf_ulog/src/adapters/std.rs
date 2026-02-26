use core::marker::PhantomData;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::mpsc::{TryRecvError as ChannelTryRecvError, TrySendError as ChannelTrySendError};

use crate::{Record, RecordSink, RecordSource, TrySendError};

pub struct ChannelTx<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> {
    tx: SyncSender<Record<RECORD_CAP, MAX_MULTI_IDS>>,
}

impl<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> ChannelTx<RECORD_CAP, MAX_MULTI_IDS> {
    pub fn new(tx: SyncSender<Record<RECORD_CAP, MAX_MULTI_IDS>>) -> Self {
        Self { tx }
    }
}

impl<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> RecordSink
    for ChannelTx<RECORD_CAP, MAX_MULTI_IDS>
{
    type Rec = Record<RECORD_CAP, MAX_MULTI_IDS>;

    fn try_send(&mut self, item: Self::Rec) -> Result<(), TrySendError> {
        match self.tx.try_send(item) {
            Ok(()) => Ok(()),
            Err(ChannelTrySendError::Full(_)) => Err(TrySendError::Full),
            Err(ChannelTrySendError::Disconnected(_)) => Err(TrySendError::Closed),
        }
    }
}

pub struct ChannelRx<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> {
    rx: Receiver<Record<RECORD_CAP, MAX_MULTI_IDS>>,
    _pd: PhantomData<Record<RECORD_CAP, MAX_MULTI_IDS>>,
}

impl<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> ChannelRx<RECORD_CAP, MAX_MULTI_IDS> {
    pub fn new(rx: Receiver<Record<RECORD_CAP, MAX_MULTI_IDS>>) -> Self {
        Self {
            rx,
            _pd: PhantomData,
        }
    }
}

impl<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> RecordSource
    for ChannelRx<RECORD_CAP, MAX_MULTI_IDS>
{
    type Rec = Record<RECORD_CAP, MAX_MULTI_IDS>;

    fn try_recv(&mut self) -> Option<Self::Rec> {
        match self.rx.try_recv() {
            Ok(record) => Some(record),
            Err(ChannelTryRecvError::Empty) | Err(ChannelTryRecvError::Disconnected) => None,
        }
    }
}

pub fn channel<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>(
    bound: usize,
) -> (
    ChannelTx<RECORD_CAP, MAX_MULTI_IDS>,
    ChannelRx<RECORD_CAP, MAX_MULTI_IDS>,
) {
    let (tx, rx) = sync_channel(bound);
    (ChannelTx::new(tx), ChannelRx::new(rx))
}

#[cfg(test)]
mod tests {
    use crate::LogLevel;

    use super::*;

    const CAP: usize = 16;
    const MI: usize = 8;

    #[test]
    fn send_and_recv_round_trip() {
        let (mut tx, mut rx) = channel::<CAP, MI>(1);
        let record = Record::new_log(LogLevel::Info, None, 1, b"ok");

        tx.try_send(record.clone()).unwrap();
        assert_eq!(rx.try_recv(), Some(record));
    }

    #[test]
    fn full_and_closed_mapping() {
        let (raw_tx, raw_rx) = sync_channel::<Record<CAP, MI>>(1);
        let mut tx = ChannelTx::<CAP, MI>::new(raw_tx.clone());
        let record = Record::new_log(LogLevel::Info, None, 0, b"x");
        tx.try_send(record.clone()).unwrap();
        assert_eq!(tx.try_send(record), Err(TrySendError::Full));

        drop(raw_rx);
        let mut closed_tx = ChannelTx::<CAP, MI>::new(raw_tx);
        let closed_record = Record::new_log(LogLevel::Info, None, 0, b"x");
        assert_eq!(closed_tx.try_send(closed_record), Err(TrySendError::Closed));
    }
}
