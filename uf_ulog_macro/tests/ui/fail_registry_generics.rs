#[derive(uf_ulog_macro::ULogRegistry)]
enum Topics<T> {
    Gyro,
    _Marker(T),
}

fn main() {}
