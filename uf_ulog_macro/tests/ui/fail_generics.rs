#[derive(uf_ulog_macro::ULogData)]
struct GenericMsg<T> {
    timestamp: u64,
    value: T,
}

fn main() {}
