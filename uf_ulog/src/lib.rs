#![no_std]
#![allow(
    clippy::needless_doctest_main,
    reason = "This is readme example, not doctest"
)]
#![doc = include_str!("../../README.md")]
extern crate self as uf_ulog;

#[cfg(feature = "derive")]
pub use uf_ulog_macro::ULogData;

pub trait ULogData {
    const FORMAT: &'static str;
    const NAME: &'static str;
    const WIRE_SIZE: usize;

    fn encode(&self, buf: &mut [u8]) -> Result<(), EncodeError>;
    fn timestamp(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    BufferOverflow,
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

            fn encode(&self, buf: &mut [u8]) -> Result<(), EncodeError> {
                if buf.len() < Self::WIRE_SIZE {
                    return Err(EncodeError::BufferOverflow);
                }

                buf[0..8].copy_from_slice(&self.timestamp().to_le_bytes());
                buf[8..12].copy_from_slice(&self.x.to_le_bytes());
                buf[12..16].copy_from_slice(&self.y.to_le_bytes());
                buf[16..20].copy_from_slice(&self.z.to_le_bytes());
                Ok(())
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
        #[derive(ULogData)]
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
