use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender, TrySendError as ChannelTrySendError};

use crate::writer_async::AsyncRecordSource;
use crate::{Record, RecordSink, TrySendError};

pub struct ChannelTx<
    'a,
    M: RawMutex,
    const N: usize,
    const RECORD_CAP: usize,
    const MAX_MULTI_IDS: usize,
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
    const RECORD_CAP: usize,
    const MAX_MULTI_IDS: usize,
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
