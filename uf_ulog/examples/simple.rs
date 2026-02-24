use std::convert::Infallible;

use uf_ulog::adapters;
use uf_ulog::register_messages;
use uf_ulog::DefaultCfg;
use uf_ulog::ExportStep;
use uf_ulog::LogLevel;
use uf_ulog::ULogCfg;
use uf_ulog::ULogData;
use uf_ulog::ULogExporter;
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

#[repr(u16)]
pub enum ErrorCodes {
    GenericError = 1,
    GyroError = 2,
    AccError = 3,
}

impl ErrorCodes {
    pub const fn code(self) -> u16 {
        self as u16
    }

    pub const fn message(&self) -> &'static str {
        match self {
            Self::GyroError => "GyroError",
            Self::GenericError => "GenericError",
            Self::AccError => "AccError",
        }
    }
}

#[derive(ULogData, Debug)]
struct ErrorData {
    timestamp: u64,
    code: u16,
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
    register_messages! {
        enum UlogDataMessages {
            Gyro,
            Acc,
            ErrorData,
        }
    }
    let g = Gyro {
        timestamp: 0,
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let a = Acc {
        timestamp: 0,
        x: 1.0,
        y: 2.0,
        z: 9.0,
    };

    let err_code = ErrorData {
        timestamp: 0,
        code: ErrorCodes::GenericError.code(),
    };

    let (tx, rx) = adapters::std::channel::<ExampleCfg>(32);
    let mut ulog = ULogProducer::<_, UlogDataMessages, ExampleCfg>::new(tx);

    ulog.data::<Gyro>(&g);
    ulog.data::<Acc>(&a);
    ulog.data_instance::<Acc>(&a, 1);
    ulog.data::<ErrorData>(&err_code);
    ulog.log(LogLevel::Info, 43, "info log");
    ulog.log_tagged(LogLevel::Info, 1, 43, "info log");

    let mut exporter = ULogExporter::<_, _, UlogDataMessages, ExampleCfg>::new(PrintWriter, rx);
    exporter.emit_startup(0).unwrap();

    loop {
        match exporter.poll_once().unwrap() {
            ExportStep::Progressed => {}
            ExportStep::Idle => break,
        }
    }

    println!("done")
}
