use std::convert::Infallible;

use uf_ulog::LogLevel;
use uf_ulog::ULogCoreExporter;
use uf_ulog::ULogData;
use uf_ulog::ULogProducer;
use uf_ulog::ULogRegistry;

const RECORD_CAP: usize = 256;
const MAX_MULTI_IDS: usize = 8;
const MAX_STREAMS: usize = 256;

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
}

#[derive(ULogData, Debug)]
struct ErrorData {
    timestamp: u64,
    code: u16,
}

#[derive(ULogRegistry)]
pub enum UlogDataMessages {
    Gyro,
    Acc,
    ErrorData,
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
    println!(
        "Using tuned 2x defaults: RECORD_CAP={}, MAX_MULTI_IDS={}, MAX_STREAMS={}",
        RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS
    );

    let producer = ULogProducer::<UlogDataMessages, RECORD_CAP, MAX_MULTI_IDS>::new();
    let mut exporter =
        ULogCoreExporter::<_, UlogDataMessages, _, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>::new(
            PrintWriter,
        )
        .start(0)
        .unwrap();

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

    let record_data_gyro = producer.data::<Gyro>(&g).unwrap();
    let record_data_acc = producer.data::<Acc>(&a).unwrap();
    let record_data_acc_instance = producer.data_instance::<Acc>(&a, 1).unwrap();
    let record_data_error = producer.data::<ErrorData>(&err_code).unwrap();
    let record_param_sys_logger = producer.parameter_i32("SYS_LOGGER", 1).unwrap();
    let record_log_info = producer.log(LogLevel::Info, 43, "info log");
    let record_log_info_tagged = producer.log_tagged(LogLevel::Info, 1, 43, "info log");

    exporter.accept(record_data_gyro).unwrap();
    exporter.accept(record_data_acc).unwrap();
    exporter.accept(record_data_acc_instance).unwrap();
    exporter.accept(record_data_error).unwrap();
    exporter.accept(record_param_sys_logger).unwrap();
    exporter.accept(record_log_info).unwrap();
    exporter.accept(record_log_info_tagged).unwrap();

    println!("done")
}
