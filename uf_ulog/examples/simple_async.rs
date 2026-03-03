use core::convert::Infallible;

use futures::executor::block_on;
use uf_ulog::LogLevel;
use uf_ulog::ULogAsyncCoreExporter;
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
#[allow(dead_code)]
enum UlogDataMessages {
    Gyro,
    Acc,
}

#[derive(Default)]
struct PrintWriter;

impl embedded_io_async::ErrorType for PrintWriter {
    type Error = Infallible;
}

impl embedded_io_async::Write for PrintWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        println!("{:?}", buf);
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() {
    block_on(async_main());
}

async fn async_main() {
    let producer = ULogProducer::<UlogDataMessages>::new();

    let g = Gyro {
        timestamp: 10,
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let a = Acc {
        timestamp: 20,
        x: 9.0,
        y: 8.0,
        z: 7.0,
    };

    let mut exporter = ULogAsyncCoreExporter::<_, UlogDataMessages>::new(PrintWriter)
        .start(0)
        .await
        .unwrap();

    let record_log_boot = producer.log(LogLevel::Info, 100, "producer boot");
    let record_data_gyro = producer.data::<Gyro>(&g).unwrap();
    let record_data_acc = producer.data::<Acc>(&a).unwrap();
    let record_param_sys_logger = producer.parameter_i32("SYS_LOGGER", 1).unwrap();
    let record_log_warn_tagged = producer.log_tagged(LogLevel::Warning, 7, 102, "warn");

    exporter.accept(record_log_boot).await.unwrap();
    exporter.accept(record_data_gyro).await.unwrap();
    exporter.accept(record_data_acc).await.unwrap();
    exporter.accept(record_param_sys_logger).await.unwrap();
    exporter.accept(record_log_warn_tagged).await.unwrap();

    println!("done");
}
