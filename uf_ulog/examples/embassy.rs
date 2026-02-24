use core::convert::Infallible;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use futures::executor::block_on;
use uf_ulog::adapters;
use uf_ulog::register_messages;
use uf_ulog::DefaultCfg;
use uf_ulog::LogLevel;
use uf_ulog::ULogAsyncExporter;
use uf_ulog::ULogCfg;
use uf_ulog::ULogData;
use uf_ulog::ULogProducer;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExampleCfg;

impl ULogCfg for ExampleCfg {
    type Text = heapless::String<64>;
    type Payload = [u8; 128];
    type Streams = <DefaultCfg as ULogCfg>::Streams;

    const MAX_MULTI_IDS: usize = DefaultCfg::MAX_MULTI_IDS;
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
    register_messages! {
        enum UlogDataMessages {
            Gyro,
            Acc,
        }
    }
    let channel: Channel<NoopRawMutex, uf_ulog::Record<ExampleCfg>, 32> = Channel::new();

    let tx_a = adapters::embassy::ChannelTx::new(channel.sender());
    let tx_b = adapters::embassy::ChannelTx::new(channel.sender());
    let rx = adapters::embassy::ChannelRx::new(channel.receiver());

    let mut producer_a = ULogProducer::<_, UlogDataMessages, ExampleCfg>::new(tx_a);
    let mut producer_b = ULogProducer::<_, UlogDataMessages, ExampleCfg>::new(tx_b);

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

    producer_a.log(LogLevel::Info, 100, "producer_a boot");
    producer_b.log(LogLevel::Info, 101, "producer_b boot");
    producer_a.data::<Gyro>(&g);
    producer_b.data::<Acc>(&a);
    producer_a.log_tagged(LogLevel::Warning, 7, 102, "producer_a warn");
    producer_b.log_tagged(LogLevel::Err, 9, 103, "producer_b error");

    let mut exporter =
        ULogAsyncExporter::<_, _, UlogDataMessages, ExampleCfg>::new(PrintWriter, rx);
    exporter.emit_startup(0).await.unwrap();

    for _ in 0..6 {
        exporter.poll_once().await.unwrap();
    }

    println!("done");
}
