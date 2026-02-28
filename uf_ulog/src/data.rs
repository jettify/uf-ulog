#[derive(Debug)]
pub struct LoggedString {
    pub level: LogLevel,
    pub tag: Option<u16>,
    pub timestamp: u64,
    pub msg: &'static str,
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum LogLevel {
    Emerg = b'0',
    Alert = b'1',
    Crit = b'2',
    Err = b'3',
    Warning = b'4',
    Notice = b'5',
    Info = b'6',
    Debug = b'7',
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub multi_id: u8,
    pub msg_id: u16,
    pub msg_name: &'static str,
}

pub trait ULogData {
    const FORMAT: &'static str;
    const NAME: &'static str;
    const WIRE_SIZE: usize;

    fn encode(&self, buf: &mut [u8]) -> Result<usize, EncodeError>;
    fn timestamp(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    BufferOverflow,
    FrameTooLarge,
    InvalidArgument,
    QueueFull,
    TopicRegistryFull,
    TopicConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrySendError {
    Full,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterValue {
    I32(i32),
    F32(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    LoggedString,
    Data,
    Parameter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordMeta {
    LoggedString {
        level: LogLevel,
        tag: Option<u16>,
        ts: u64,
    },
    Data {
        topic_index: u16,
        instance: u8,
        ts: u64,
    },
    Parameter {
        value: ParameterValue,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> {
    meta: RecordMeta,
    bytes: heapless::Vec<u8, RECORD_CAP>,
}

impl<const RECORD_CAP: usize, const MAX_MULTI_IDS: usize> Record<RECORD_CAP, MAX_MULTI_IDS> {
    pub fn new_log(level: LogLevel, tag: Option<u16>, ts: u64, text: &[u8]) -> Self {
        let mut bytes = heapless::Vec::new();
        let end = core::cmp::min(text.len(), RECORD_CAP);
        let _ = bytes.extend_from_slice(&text[..end]);
        Self {
            meta: RecordMeta::LoggedString { level, tag, ts },
            bytes,
        }
    }

    pub fn new_data(topic_index: u16, instance: u8, ts: u64, payload: &[u8]) -> Option<Self> {
        let bytes = heapless::Vec::from_slice(payload).ok()?;
        Some(Self {
            meta: RecordMeta::Data {
                topic_index,
                instance,
                ts,
            },
            bytes,
        })
    }

    pub fn new_parameter(key: &[u8], value: ParameterValue) -> Option<Self> {
        if key.len() > usize::from(u8::MAX) {
            return None;
        }
        let bytes = heapless::Vec::from_slice(key).ok()?;
        Some(Self {
            meta: RecordMeta::Parameter { value },
            bytes,
        })
    }

    pub fn kind(&self) -> RecordKind {
        match self.meta {
            RecordMeta::LoggedString { .. } => RecordKind::LoggedString,
            RecordMeta::Data { .. } => RecordKind::Data,
            RecordMeta::Parameter { .. } => RecordKind::Parameter,
        }
    }

    pub fn meta(&self) -> RecordMeta {
        self.meta
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_basic() {
        struct Acc {
            pub timestamp: u64,
            pub x: f32,
            pub y: f32,
            pub z: f32,
        }

        impl ULogData for Acc {
            const FORMAT: &'static str = "uint64_t timestamp;float x;float y;float z;";
            const NAME: &'static str = "Acc";
            const WIRE_SIZE: usize = 20;

            fn timestamp(&self) -> u64 {
                self.timestamp
            }

            fn encode(&self, buf: &mut [u8]) -> Result<usize, EncodeError> {
                if buf.len() < Self::WIRE_SIZE {
                    return Err(EncodeError::BufferOverflow);
                }

                buf[0..8].copy_from_slice(&self.timestamp().to_le_bytes());
                buf[8..12].copy_from_slice(&self.x.to_le_bytes());
                buf[12..16].copy_from_slice(&self.y.to_le_bytes());
                buf[16..20].copy_from_slice(&self.z.to_le_bytes());
                Ok(Self::WIRE_SIZE)
            }
        }

        let a = Acc {
            timestamp: 0x0807_0605_0403_0201,
            x: 1.0,
            y: -2.5,
            z: 0.5,
        };
        assert_eq!(a.timestamp(), 0x0807_0605_0403_0201);
        assert_eq!(a.x, 1.0);
        assert_eq!(a.y, -2.5);
        assert_eq!(a.z, 0.5);
        assert_eq!(Acc::FORMAT, "uint64_t timestamp;float x;float y;float z;");
        assert_eq!(Acc::NAME, "Acc");
        assert_eq!(Acc::WIRE_SIZE, 20);

        let mut encoded = [0u8; Acc::WIRE_SIZE];
        a.encode(&mut encoded).unwrap();
        assert_eq!(
            encoded,
            [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 128, 63, 0, 0, 32, 192, 0, 0, 0, 63,]
        );

        let mut short = [0u8; Acc::WIRE_SIZE - 1];
        assert_eq!(a.encode(&mut short), Err(EncodeError::BufferOverflow));
    }

    #[cfg(feature = "derive")]
    #[test]
    fn test_derive() {
        #[derive(crate::ULogData)]
        #[uf_ulog(name = "gyro")]
        struct Gyro {
            pub timestamp: u64,
            pub x: f32,
            pub y: f32,
            pub z: f32,
        }

        let g = Gyro {
            timestamp: 0x0807_0605_0403_0201,
            x: 1.0,
            y: -2.5,
            z: 0.5,
        };
        assert_eq!(g.timestamp(), 0x0807_0605_0403_0201);
        assert_eq!(g.x, 1.0);
        assert_eq!(g.y, -2.5);
        assert_eq!(g.z, 0.5);
        assert_eq!(Gyro::WIRE_SIZE, 20);
        assert_eq!(Gyro::FORMAT, "uint64_t timestamp;float x;float y;float z;");
        assert_eq!(Gyro::NAME, "gyro");

        let mut encoded = [0u8; Gyro::WIRE_SIZE];
        g.encode(&mut encoded).unwrap();
        assert_eq!(
            encoded,
            [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 128, 63, 0, 0, 32, 192, 0, 0, 0, 63,]
        );

        let mut short = [0u8; Gyro::WIRE_SIZE - 1];
        assert_eq!(g.encode(&mut short), Err(EncodeError::BufferOverflow));
    }
}
