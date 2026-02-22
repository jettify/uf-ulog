use crate::{Record, RecordSink, RecordSource, TrySendError, ULogCfg};

pub struct QueueTx<'a, const N: usize, C: ULogCfg> {
    queue: &'a heapless::mpmc::Queue<Record<C>, N>,
}

impl<'a, const N: usize, C: ULogCfg> QueueTx<'a, N, C> {
    pub fn new(queue: &'a heapless::mpmc::Queue<Record<C>, N>) -> Self {
        Self { queue }
    }
}

impl<'a, const N: usize, C: ULogCfg> RecordSink<C> for QueueTx<'a, N, C> {
    fn try_send(&mut self, item: Record<C>) -> Result<(), TrySendError> {
        self.queue.enqueue(item).map_err(|_| TrySendError::Full)
    }
}

#[derive(Clone, Copy)]
pub struct QueueRx<'a, const N: usize, C: ULogCfg> {
    queue: &'a heapless::mpmc::Queue<Record<C>, N>,
}

impl<'a, const N: usize, C: ULogCfg> QueueRx<'a, N, C> {
    pub fn new(queue: &'a heapless::mpmc::Queue<Record<C>, N>) -> Self {
        Self { queue }
    }
}

impl<'a, const N: usize, C: ULogCfg> RecordSource<C> for QueueRx<'a, N, C> {
    fn try_recv(&mut self) -> Option<Record<C>> {
        self.queue.dequeue()
    }
}

pub fn bind<'a, const N: usize, C: ULogCfg>(
    queue: &'a heapless::mpmc::Queue<Record<C>, N>,
) -> (QueueTx<'a, N, C>, QueueRx<'a, N, C>) {
    (QueueTx::new(queue), QueueRx::new(queue))
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
    fn enqueue_and_dequeue_round_trip() {
        #[expect(deprecated)]
        let queue = heapless::mpmc::Queue::<Record<TestCfg>, 2>::new();
        let (mut tx, mut rx) = bind(&queue);
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
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn full_mapping() {
        #[expect(deprecated)]
        let queue = heapless::mpmc::Queue::<Record<TestCfg>, 2>::new();
        let (mut tx, _rx) = bind(&queue);
        let mut text = heapless::String::<16>::new();
        let _ = text.push_str("x");
        let record = Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 0,
            text,
        };

        tx.try_send(record.clone()).unwrap();
        tx.try_send(record.clone()).unwrap();
        assert_eq!(tx.try_send(record), Err(TrySendError::Full));
    }

    #[test]
    fn multiple_producers() {
        #[expect(deprecated)]
        let queue = heapless::mpmc::Queue::<Record<TestCfg>, 4>::new();
        let (mut tx1, mut rx) = bind(&queue);
        let mut tx2 = QueueTx::new(&queue);

        let mut a = heapless::String::<16>::new();
        let _ = a.push_str("a");
        let mut b = heapless::String::<16>::new();
        let _ = b.push_str("b");

        tx1.try_send(Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 1,
            text: a,
        })
        .unwrap();
        tx2.try_send(Record::LoggedString {
            level: LogLevel::Info,
            tag: None,
            ts: 2,
            text: b,
        })
        .unwrap();

        assert!(rx.try_recv().is_some());
        assert!(rx.try_recv().is_some());
    }
}
