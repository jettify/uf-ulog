use uf_ulog::{EncodeError, ULogData};

#[derive(uf_ulog_macro::ULogData)]
struct Gyro {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(uf_ulog_macro::ULogData)]
struct RcInput {
    timestamp: u64,
    values: [u16; 4],
    rssi: i8,
    failsafe: bool,
}

#[test]
fn encode_scalars() {
    let msg = Gyro {
        timestamp: 0x0807_0605_0403_0201,
        x: 1.0,
        y: -2.5,
        z: 0.5,
    };

    let mut buf = [0u8; Gyro::WIRE_SIZE];
    msg.encode(&mut buf).unwrap();
    assert_eq!(
        buf,
        [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 128, 63, 0, 0, 32, 192, 0, 0, 0, 63,]
    );

    let mut short = [0u8; Gyro::WIRE_SIZE - 1];
    assert_eq!(msg.encode(&mut short), Err(EncodeError::BufferOverflow));
}

#[test]
fn encode_arrays_and_single_byte_types() {
    let msg = RcInput {
        timestamp: 0x0807_0605_0403_0201,
        values: [0x1122, 0x3344, 0x5566, 0x7788],
        rssi: -2,
        failsafe: true,
    };

    let mut buf = [0u8; RcInput::WIRE_SIZE];
    msg.encode(&mut buf).unwrap();

    assert_eq!(
        buf,
        [1, 2, 3, 4, 5, 6, 7, 8, 0x22, 0x11, 0x44, 0x33, 0x66, 0x55, 0x88, 0x77, 0xFE, 1,]
    );
}
