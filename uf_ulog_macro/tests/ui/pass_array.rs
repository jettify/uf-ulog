use uf_ulog::ULogData;

#[derive(uf_ulog_macro::ULogData)]
struct RcInput {
    timestamp: u64,
    values: [u16; 8],
}

fn main() {
    let msg = RcInput {
        timestamp: 1,
        values: [0; 8],
    };

    assert_eq!(msg.timestamp(), 1);
    assert_eq!(RcInput::WIRE_SIZE, 24);
    assert_eq!(RcInput::FORMAT, "uint64_t timestamp;uint16_t[8] values;");
    assert_eq!(RcInput::NAME, "RcInput");
}
