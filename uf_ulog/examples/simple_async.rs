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

    exporter
        .accept(producer.log(LogLevel::Info, 100, "producer boot"))
        .await
        .unwrap();
    exporter
        .accept(producer.data::<Gyro>(&g).unwrap())
        .await
        .unwrap();
    exporter
        .accept(producer.data::<Acc>(&a).unwrap())
        .await
        .unwrap();
    exporter
        .accept(producer.parameter_i32("SYS_LOGGER", 1).unwrap())
        .await
        .unwrap();
    exporter
        .accept(producer.log_tagged(LogLevel::Warning, 7, 102, "warn"))
        .await
        .unwrap();

    println!("done");
}
