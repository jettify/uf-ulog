#![no_std]
#![allow(
    clippy::needless_doctest_main,
    reason = "This is readme example, not doctest"
)]
#![doc = include_str!("../../README.md")]
extern crate self as uf_ulog;

#[cfg(feature = "derive")]
pub use uf_ulog_macro::ULogMessage;

pub trait ULogMessage {
    const FORMAT: &'static str;
    const NAME: &'static str;
    const WIRE_SIZE: usize;

    fn encode(&self, buf: &mut [u8]);
    fn timestamp(&self) -> u64;
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

        impl ULogMessage for Acc {
            const FORMAT: &'static str = "uint64_t timestamp;float x;float y;float z;";
            const NAME: &'static str = "Acc";
            const WIRE_SIZE: usize = 20;

            fn timestamp(&self) -> u64 {
                self.timestamp
            }

            fn encode(&self, buf: &mut [u8]) {
                buf[0..8].copy_from_slice(&self.timestamp().to_le_bytes());
                buf[9..13].copy_from_slice(&self.x.to_le_bytes());
                buf[13..15].copy_from_slice(&self.y.to_le_bytes());
                buf[17..21].copy_from_slice(&self.z.to_le_bytes());
            }
        }

        let a = Acc {
            timestamp: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        assert_eq!(a.timestamp(), 0);
        assert_eq!(a.x, 0.0);
        assert_eq!(a.y, 0.0);
        assert_eq!(a.z, 0.0);
        assert_eq!(Acc::FORMAT, "uint64_t timestamp;float x;float y;float z;");
        assert_eq!(Acc::NAME, "Acc");
        assert_eq!(Acc::WIRE_SIZE, 20);
    }

    #[test]
    fn test_derive() {
        #[derive(ULogMessage)]
        struct Gyro {
            pub timestamp: u64,
            pub x: f32,
            pub y: f32,
            pub z: f32,
        }

        let g = Gyro {
            timestamp: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        assert_eq!(g.timestamp(), 0);
        assert_eq!(g.x, 0.0);
        assert_eq!(g.y, 0.0);
        assert_eq!(g.z, 0.0);
        assert_eq!(Gyro::WIRE_SIZE, 20);
        assert_eq!(Gyro::FORMAT, "uint64_t timestamp;float x;float y;float z;");
        assert_eq!(Gyro::NAME, "Gyro");
    }
}
