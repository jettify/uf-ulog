use uf_ulog::ULogData;

#[derive(uf_ulog_macro::ULogData)]
struct Gyro {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

fn main() {
    let msg = Gyro {
        timestamp: 1,
        x: 0.1,
        y: 0.2,
        z: 0.3,
    };

    assert_eq!(msg.timestamp(), 1);
    assert_eq!(Gyro::WIRE_SIZE, 20);
    assert_eq!(Gyro::FORMAT, "uint64_t timestamp;float x;float y;float z;");
    assert_eq!(Gyro::NAME, "Gyro");
}
