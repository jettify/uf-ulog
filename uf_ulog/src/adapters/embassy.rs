use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender, TrySendError as ChannelTrySendError};

use crate::writer_async::AsyncRecordSource;
use crate::{Record, RecordSink, TrySendError, ULogCfg};

pub struct ChannelTx<'a, M: RawMutex, const N: usize, C: ULogCfg> {
    tx: Sender<'a, M, Record<C>, N>,
}

impl<'a, M: RawMutex, const N: usize, C: ULogCfg> ChannelTx<'a, M, N, C> {
    pub fn new(tx: Sender<'a, M, Record<C>, N>) -> Self {
        Self { tx }
    }
}

impl<'a, M: RawMutex, const N: usize, C: ULogCfg> RecordSink<C> for ChannelTx<'a, M, N, C> {
    fn try_send(&mut self, item: Record<C>) -> Result<(), TrySendError> {
        self.tx
            .try_send(item)
            .map_err(|ChannelTrySendError::Full(_)| TrySendError::Full)
    }
}

pub struct ChannelRx<'a, M: RawMutex, const N: usize, C: ULogCfg> {
    rx: Receiver<'a, M, Record<C>, N>,
}

impl<'a, M: RawMutex, const N: usize, C: ULogCfg> ChannelRx<'a, M, N, C> {
    pub fn new(rx: Receiver<'a, M, Record<C>, N>) -> Self {
        Self { rx }
    }
}

impl<'a, M: RawMutex, const N: usize, C: ULogCfg> AsyncRecordSource<C> for ChannelRx<'a, M, N, C> {
    async fn recv(&mut self) -> Record<C> {
        self.rx.receive().await
    }
}

pub fn bind<'a, M: RawMutex, const N: usize, C: ULogCfg>(
    channel: &'a Channel<M, Record<C>, N>,
) -> (ChannelTx<'a, M, N, C>, ChannelRx<'a, M, N, C>) {
    (ChannelTx::new(channel.sender()), ChannelRx::new(channel.receiver()))
}
