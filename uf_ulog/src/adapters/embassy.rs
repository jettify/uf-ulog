use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender, TrySendError as ChannelTrySendError};

use crate::AsyncRecordSource;
use crate::{Record, RecordSink, TrySendError};

pub struct ChannelTx<
    'a,
    M: RawMutex,
    const N: usize,
    const RECORD_CAP: usize = 128,
    const MAX_MULTI_IDS: usize = 4,
> {
    tx: Sender<'a, M, Record<RECORD_CAP, MAX_MULTI_IDS>, N>,
}

impl<'a, M: RawMutex, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    ChannelTx<'a, M, N, RECORD_CAP, MAX_MULTI_IDS>
{
    pub fn new(tx: Sender<'a, M, Record<RECORD_CAP, MAX_MULTI_IDS>, N>) -> Self {
        Self { tx }
    }
}

impl<'a, M: RawMutex, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    RecordSink for ChannelTx<'a, M, N, RECORD_CAP, MAX_MULTI_IDS>
{
    type Rec = Record<RECORD_CAP, MAX_MULTI_IDS>;

    fn try_send(&mut self, item: Self::Rec) -> Result<(), TrySendError> {
        self.tx
            .try_send(item)
            .map_err(|ChannelTrySendError::Full(_)| TrySendError::Full)
    }
}

pub struct ChannelRx<
    'a,
    M: RawMutex,
    const N: usize,
    const RECORD_CAP: usize = 128,
    const MAX_MULTI_IDS: usize = 4,
> {
    rx: Receiver<'a, M, Record<RECORD_CAP, MAX_MULTI_IDS>, N>,
}

impl<'a, M: RawMutex, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    ChannelRx<'a, M, N, RECORD_CAP, MAX_MULTI_IDS>
{
    pub fn new(rx: Receiver<'a, M, Record<RECORD_CAP, MAX_MULTI_IDS>, N>) -> Self {
        Self { rx }
    }
}

impl<'a, M: RawMutex, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    AsyncRecordSource for ChannelRx<'a, M, N, RECORD_CAP, MAX_MULTI_IDS>
{
    type Rec = Record<RECORD_CAP, MAX_MULTI_IDS>;

    async fn recv(&mut self) -> Self::Rec {
        self.rx.receive().await
    }
}

pub fn bind<
    'a,
    M: RawMutex,
    const N: usize,
    const RECORD_CAP: usize,
    const MAX_MULTI_IDS: usize,
>(
    channel: &'a Channel<M, Record<RECORD_CAP, MAX_MULTI_IDS>, N>,
) -> (
    ChannelTx<'a, M, N, RECORD_CAP, MAX_MULTI_IDS>,
    ChannelRx<'a, M, N, RECORD_CAP, MAX_MULTI_IDS>,
) {
    (
        ChannelTx::new(channel.sender()),
        ChannelRx::new(channel.receiver()),
    )
}

#[cfg(test)]
mod tests {
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use embassy_sync::channel::Channel;
    use futures::executor::block_on;

    use crate::LogLevel;

    use super::*;

    const CAP: usize = 16;
    const MI: usize = 8;

    #[test]
    fn send_and_recv_round_trip() {
        block_on(async {
            let channel = Channel::<NoopRawMutex, Record<CAP, MI>, 2>::new();
            let (mut tx, mut rx) = bind(&channel);
            let record = Record::new_log(LogLevel::Info, None, 1, b"ok");

            tx.try_send(record.clone()).unwrap();
            assert_eq!(rx.recv().await, record);
        });
    }

    #[test]
    fn full_mapping() {
        let channel = Channel::<NoopRawMutex, Record<CAP, MI>, 1>::new();
        let (mut tx, _rx) = bind(&channel);
        let record = Record::new_log(LogLevel::Info, None, 0, b"x");

        tx.try_send(record.clone()).unwrap();
        assert_eq!(tx.try_send(record), Err(TrySendError::Full));
    }

    #[test]
    fn multiple_senders() {
        block_on(async {
            let channel = Channel::<NoopRawMutex, Record<CAP, MI>, 4>::new();
            let (mut tx1, mut rx) = bind(&channel);
            let mut tx2 = ChannelTx::new(channel.sender());

            tx1.try_send(Record::new_log(LogLevel::Info, None, 1, b"a"))
                .unwrap();
            tx2.try_send(Record::new_log(LogLevel::Info, None, 2, b"b"))
                .unwrap();

            let first = rx.recv().await;
            let second = rx.recv().await;
            assert_ne!(first, second);
        });
    }
}
