use uf_ulog::{TopicOf, ULogData, ULogRegistry};

#[derive(uf_ulog_macro::ULogData)]
struct Gyro {
    timestamp: u64,
    x: f32,
}

#[derive(uf_ulog_macro::ULogData)]
struct Acc {
    timestamp: u64,
    y: f32,
}

#[derive(uf_ulog_macro::ULogRegistry)]
enum Topics {
    Gyro,
    Acc,
}

fn main() {
    assert_eq!(Topics::REGISTRY.len(), 2);
    assert_eq!(Topics::REGISTRY.get(0).unwrap().name, Gyro::NAME);
    assert_eq!(Topics::REGISTRY.get(1).unwrap().name, Acc::NAME);
    assert_eq!(<Gyro as TopicOf<Topics>>::TOPIC.id(), 0);
    assert_eq!(<Acc as TopicOf<Topics>>::TOPIC.id(), 1);
}
