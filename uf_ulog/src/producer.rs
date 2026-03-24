use core::marker::PhantomData;

use crate::{EncodeError, LogLevel, ParameterValue, Record, TopicOf, ULogData, ULogRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    InvalidTopicIndex,
    InvalidMultiId,
    InvalidWireSize,
    Encode(EncodeError),
    RecordTooLarge,
    ParameterNameTooLong,
}

pub struct ULogProducer<
    R: ULogRegistry,
    const RECORD_CAP: usize = 128,
    const MAX_MULTI_IDS: usize = 4,
> {
    _messages: PhantomData<R>,
}

impl<R, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize>
    ULogProducer<R, RECORD_CAP, MAX_MULTI_IDS>
where
    R: ULogRegistry,
{
    pub fn new() -> Self {
        Self {
            _messages: PhantomData,
        }
    }

    pub fn log(&self, level: LogLevel, ts: u64, msg: &str) -> Record<RECORD_CAP> {
        let text = make_text::<RECORD_CAP>(msg);
        Record::new_log(level, None, ts, &text)
    }

    pub fn log_tagged(&self, level: LogLevel, tag: u16, ts: u64, msg: &str) -> Record<RECORD_CAP> {
        let text = make_text::<RECORD_CAP>(msg);
        Record::new_log(level, Some(tag), ts, &text)
    }

    pub fn parameter_i32(&self, name: &str, value: i32) -> Result<Record<RECORD_CAP>, BuildError> {
        self.parameter("int32_t", name, ParameterValue::I32(value))
    }

    pub fn parameter_f32(&self, name: &str, value: f32) -> Result<Record<RECORD_CAP>, BuildError> {
        self.parameter("float", name, ParameterValue::F32(value))
    }

    pub fn data<T>(&self, value: &T) -> Result<Record<RECORD_CAP>, BuildError>
    where
        T: ULogData + TopicOf<R>,
    {
        self.data_instance(value, 0)
    }

    pub fn data_instance<T>(
        &self,
        value: &T,
        instance: u8,
    ) -> Result<Record<RECORD_CAP>, BuildError>
    where
        T: ULogData + TopicOf<R>,
    {
        let topic_index = <T as TopicOf<R>>::TOPIC.id();
        if usize::from(topic_index) >= R::REGISTRY.len() {
            return Err(BuildError::InvalidTopicIndex);
        }

        if usize::from(instance) >= MAX_MULTI_IDS {
            return Err(BuildError::InvalidMultiId);
        }

        let mut encoded = [0; RECORD_CAP];
        let encoded_len = value.encode(&mut encoded).map_err(BuildError::Encode)?;
        if encoded_len != T::WIRE_SIZE {
            return Err(BuildError::InvalidWireSize);
        }
        if encoded_len > RECORD_CAP {
            return Err(BuildError::RecordTooLarge);
        }

        Record::new_data(
            topic_index,
            instance,
            value.timestamp(),
            &encoded[..encoded_len],
        )
        .ok_or(BuildError::RecordTooLarge)
    }

    fn parameter(
        &self,
        ty: &str,
        name: &str,
        value: ParameterValue,
    ) -> Result<Record<RECORD_CAP>, BuildError> {
        let mut key = heapless::String::<RECORD_CAP>::new();
        if key.push_str(ty).is_err()
            || key.push(' ').is_err()
            || key.push_str(name).is_err()
            || key.len() > usize::from(u8::MAX)
        {
            return Err(BuildError::ParameterNameTooLong);
        }

        Record::new_parameter(key.as_bytes(), value).ok_or(BuildError::RecordTooLarge)
    }
}

impl<R, const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> Default
    for ULogProducer<R, RECORD_CAP, MAX_MULTI_IDS>
where
    R: ULogRegistry,
{
    fn default() -> Self {
        Self::new()
    }
}

fn make_text<const RECORD_CAP: usize>(msg: &str) -> heapless::Vec<u8, RECORD_CAP> {
    debug_assert!(msg.is_ascii(), "ulog string records must be ASCII");
    let mut text = heapless::Vec::new();
    let end = core::cmp::min(msg.len(), RECORD_CAP);
    let _ = text.extend_from_slice(&msg.as_bytes()[..end]);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: usize = 16;
    const MI: usize = 8;

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

    enum TestMessages {}

    impl crate::ULogRegistry for TestMessages {
        const REGISTRY: crate::Registry = crate::Registry::new(&[crate::MessageMeta {
            name: SampleData::NAME,
            format: SampleData::FORMAT,
            wire_size: SampleData::WIRE_SIZE,
        }]);
    }

    impl crate::TopicOf<TestMessages> for SampleData {
        const TOPIC: crate::Topic<Self> = crate::Topic::new(0);
    }

    #[test]
    fn build_log_record() {
        let producer = ULogProducer::<TestMessages, CAP, MI>::new();
        let record = producer.log(LogLevel::Info, 42, "boot");

        assert_eq!(record.kind(), crate::RecordKind::LoggedString);
    }

    #[test]
    fn rejects_too_large_instance() {
        let producer = ULogProducer::<TestMessages, CAP, 1>::new();
        let sample = SampleData;
        let status = producer.data_instance(&sample, 1);

        assert_eq!(status, Err(BuildError::InvalidMultiId));
    }

    #[test]
    fn writes_parameter() {
        let producer = ULogProducer::<TestMessages, CAP, MI>::new();
        let rec = producer.parameter_i32("P", 1).unwrap();

        assert_eq!(rec.kind(), crate::RecordKind::Parameter);
    }

    #[derive(Default)]
    struct BadSize;

    impl ULogData for BadSize {
        const FORMAT: &'static str = "uint64_t timestamp;";
        const NAME: &'static str = "bad_size";
        const WIRE_SIZE: usize = 8;

        fn encode(&self, buf: &mut [u8]) -> Result<usize, EncodeError> {
            if buf.len() < 8 {
                return Err(EncodeError::BufferOverflow);
            }
            buf[..4].copy_from_slice(&[1, 2, 3, 4]);
            Ok(4)
        }

        fn timestamp(&self) -> u64 {
            0
        }
    }

    enum MismatchMessages {}

    impl crate::ULogRegistry for MismatchMessages {
        const REGISTRY: crate::Registry = crate::Registry::new(&[crate::MessageMeta {
            name: BadSize::NAME,
            format: BadSize::FORMAT,
            wire_size: BadSize::WIRE_SIZE,
        }]);
    }

    impl crate::TopicOf<MismatchMessages> for BadSize {
        const TOPIC: crate::Topic<Self> = crate::Topic::new(0);
    }

    #[test]
    fn data_wire_size_mismatch_is_rejected() {
        let producer = ULogProducer::<MismatchMessages, CAP, MI>::new();
        let sample = BadSize;
        let status = producer.data_instance(&sample, 0);

        assert_eq!(status, Err(BuildError::InvalidWireSize));
    }

    enum TestMessagesDefault {}

    impl crate::ULogRegistry for TestMessagesDefault {
        const REGISTRY: crate::Registry = crate::Registry::new(&[crate::MessageMeta {
            name: SampleData::NAME,
            format: SampleData::FORMAT,
            wire_size: SampleData::WIRE_SIZE,
        }]);
    }

    impl crate::TopicOf<TestMessagesDefault> for SampleData {
        const TOPIC: crate::Topic<Self> = crate::Topic::new(0);
    }

    #[test]
    fn default_max_multi_ids_accepts_instance_three() {
        let producer = ULogProducer::<TestMessagesDefault, CAP>::new();
        let sample = SampleData;
        let status = producer.data_instance(&sample, 3);

        assert!(status.is_ok());
    }

    #[test]
    fn default_max_multi_ids_rejects_instance_four() {
        let producer = ULogProducer::<TestMessagesDefault, CAP>::new();
        let sample = SampleData;
        let status = producer.data_instance(&sample, 4);

        assert_eq!(status, Err(BuildError::InvalidMultiId));
    }
}
