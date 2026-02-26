use crate::{Record, RecordSink, RecordSource, TrySendError};

pub struct QueueTx<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> {
    queue: &'a heapless::mpmc::Queue<Record<RECORD_CAP, MAX_MULTI_IDS>, N>,
}

impl<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    QueueTx<'a, N, RECORD_CAP, MAX_MULTI_IDS>
{
    pub fn new(queue: &'a heapless::mpmc::Queue<Record<RECORD_CAP, MAX_MULTI_IDS>, N>) -> Self {
        Self { queue }
    }
}

impl<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> RecordSink
    for QueueTx<'a, N, RECORD_CAP, MAX_MULTI_IDS>
{
    type Rec = Record<RECORD_CAP, MAX_MULTI_IDS>;

    fn try_send(&mut self, item: Self::Rec) -> Result<(), TrySendError> {
        self.queue.enqueue(item).map_err(|_| TrySendError::Full)
    }
}

#[derive(Clone, Copy)]
pub struct QueueRx<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> {
    queue: &'a heapless::mpmc::Queue<Record<RECORD_CAP, MAX_MULTI_IDS>, N>,
}

impl<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    QueueRx<'a, N, RECORD_CAP, MAX_MULTI_IDS>
{
    pub fn new(queue: &'a heapless::mpmc::Queue<Record<RECORD_CAP, MAX_MULTI_IDS>, N>) -> Self {
        Self { queue }
    }
}

impl<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> RecordSource
    for QueueRx<'a, N, RECORD_CAP, MAX_MULTI_IDS>
{
    type Rec = Record<RECORD_CAP, MAX_MULTI_IDS>;

    fn try_recv(&mut self) -> Option<Self::Rec> {
        self.queue.dequeue()
    }
}

pub fn bind<'a, const N: usize, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>(
    queue: &'a heapless::mpmc::Queue<Record<RECORD_CAP, MAX_MULTI_IDS>, N>,
) -> (
    QueueTx<'a, N, RECORD_CAP, MAX_MULTI_IDS>,
    QueueRx<'a, N, RECORD_CAP, MAX_MULTI_IDS>,
) {
    (QueueTx::new(queue), QueueRx::new(queue))
}

#[cfg(test)]
mod tests {
    use crate::LogLevel;

    use super::*;

    const CAP: usize = 16;
    const MI: usize = 8;

    #[test]
    fn enqueue_and_dequeue_round_trip() {
        #[expect(deprecated)]
        let queue = heapless::mpmc::Queue::<Record<CAP, MI>, 2>::new();
        let (mut tx, mut rx) = bind(&queue);
        let record = Record::new_log(LogLevel::Info, None, 1, b"ok");

        tx.try_send(record.clone()).unwrap();
        assert_eq!(rx.try_recv(), Some(record));
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn full_mapping() {
        #[expect(deprecated)]
        let queue = heapless::mpmc::Queue::<Record<CAP, MI>, 2>::new();
        let (mut tx, _rx) = bind(&queue);
        let record = Record::new_log(LogLevel::Info, None, 0, b"x");

        tx.try_send(record.clone()).unwrap();
        tx.try_send(record.clone()).unwrap();
        assert_eq!(tx.try_send(record), Err(TrySendError::Full));
    }

    #[test]
    fn multiple_producers() {
        #[expect(deprecated)]
        let queue = heapless::mpmc::Queue::<Record<CAP, MI>, 4>::new();
        let (mut tx1, mut rx) = bind(&queue);
        let mut tx2 = QueueTx::new(&queue);

        tx1.try_send(Record::new_log(LogLevel::Info, None, 1, b"a"))
        .unwrap();
        tx2.try_send(Record::new_log(LogLevel::Info, None, 2, b"b"))
        .unwrap();

        assert!(rx.try_recv().is_some());
        assert!(rx.try_recv().is_some());
    }
}
