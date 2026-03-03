use std::convert::Infallible;

use uf_ulog::LogLevel;
use uf_ulog::ULogCoreExporter;
use uf_ulog::ULogData;
use uf_ulog::ULogProducer;
use uf_ulog::ULogRegistry;

#[derive(ULogData, Debug)]
struct Acc {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(ULogData, Debug)]
struct Gyro {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(ULogRegistry)]
pub enum UlogDataMessages {
    Gyro,
    Acc,
}

#[derive(Default)]
struct PrintWriter;

impl embedded_io::ErrorType for PrintWriter {
    type Error = Infallible;
}

impl embedded_io::Write for PrintWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        println!("{:?}", buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() {
    let timestamp = 1772079727637;
    let producer = ULogProducer::<UlogDataMessages>::new();
    let mut exporter = ULogCoreExporter::<_, UlogDataMessages>::new(PrintWriter)
        .start(timestamp)
        .unwrap();

    let g = Gyro {
        timestamp,
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let a = Acc {
        timestamp,
        x: 1.0,
        y: 2.0,
        z: 9.0,
    };

    let record_data_gyro = producer.data::<Gyro>(&g).unwrap();
    let record_data_acc_instance = producer.data_instance::<Acc>(&a, 1).unwrap();
    let record_log_info = producer.log(LogLevel::Info, 43, "info log");
    let record_log_info_tagged = producer.log_tagged(LogLevel::Info, 1, 43, "info log");

    exporter.accept(record_data_gyro).unwrap();
    exporter.accept(record_data_acc_instance).unwrap();
    exporter.accept(record_log_info).unwrap();
    exporter.accept(record_log_info_tagged).unwrap();
    println!("done")
}
