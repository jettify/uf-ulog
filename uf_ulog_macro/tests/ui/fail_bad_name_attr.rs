#[derive(uf_ulog_macro::ULogData)]
#[uf_ulog(name = 10)]
struct BadNameAttr {
    timestamp: u64,
}

fn main() {}
