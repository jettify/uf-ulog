use embedded_io_adapters::std::FromStd;
use std::fs::File;

use uf_ulog::adapters;
use uf_ulog::ExportError;
use uf_ulog::ExportStep;
use uf_ulog::LogLevel;
use uf_ulog::ULogData;
use uf_ulog::ULogExporter;
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

#[derive(ULogData, Debug)]
struct Mag {
    timestamp: u64,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(ULogData, Debug)]
struct Test {
    timestamp: u64,
    x1: u8,
    x2: i8,
    x3: u16,
    x4: i16,
    x5: u32,
    x6: i32,
    x7: u64,
    x8: i64,
    x9: f32,
    x10: f64,
    x11: bool,
}

#[derive(ULogRegistry)]
pub enum UlogDataMessages {
    Gyro,
    Acc,
    Mag,
    Test,
}

const RECORD_CAP: usize = 128;
const MAX_MULTI_IDS: usize = 4;
const MAX_STREAMS: usize = RECORD_CAP * MAX_MULTI_IDS;

fn map_export_error(err: ExportError<std::io::Error>) -> std::io::Error {
    match err {
        ExportError::Write(io_err) => io_err,
        other => std::io::Error::other(format!("ulog export error: {other:?}")),
    }
}

fn main() -> std::io::Result<()> {
    let g = Gyro {
        timestamp: 1772079727637,
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let a = Acc {
        timestamp: 1772079727637,
        x: 1.0,
        y: 2.0,
        z: 9.0,
    };

    let m = Mag {
        timestamp: 1772079727637,
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    let t = Test {
        timestamp: 1772079727637,
        x1: 8,
        x2: -8,
        x3: 16,
        x4: -16,
        x5: 32,
        x6: -32,
        x7: 64,
        x8: -64,
        x9: -32.0,
        x10: 64.5,
        x11: true,
    };

    let (tx, rx) = adapters::std::channel::<RECORD_CAP, MAX_MULTI_IDS>(32);
    let mut ulog = ULogProducer::<_, UlogDataMessages>::new(tx);

    let timestamp = 1772079727637;
    ulog.parameter_f32("P", 1.5);
    ulog.parameter_f32("I", 0.01);
    ulog.parameter_f32("D", 2.01);

    ulog.data::<Gyro>(&g);
    ulog.data::<Acc>(&a);
    ulog.data::<Mag>(&m);
    ulog.data::<Test>(&t);
    ulog.parameter_f32("P", 0.5);
    ulog.parameter_f32("I", 0.01);
    ulog.parameter_f32("D", 2.01);
    ulog.parameter_i32("SERVO_TRIM", 1500);

    ulog.log_tagged(LogLevel::Info, 1, timestamp, "info tagged log");
    ulog.log(LogLevel::Debug, timestamp, "This is debug log");
    ulog.log(LogLevel::Emerg, timestamp, "This is Emerg log");
    ulog.log(LogLevel::Alert, timestamp, "This is Alert log");
    ulog.log(LogLevel::Notice, timestamp, "This is Notice log");

    let writer = FromStd::new(File::create("out.ulg")?);

    let mut exporter =
        ULogExporter::<_, _, UlogDataMessages, RECORD_CAP, MAX_MULTI_IDS, MAX_STREAMS>::new(
            writer, rx,
        );
    exporter.emit_startup(0).map_err(map_export_error)?;
    while let Ok(v) = exporter.poll_once().map_err(map_export_error) {
        match v {
            ExportStep::Progressed => {}
            ExportStep::Idle => break,
        }
    }
    embedded_io::Write::flush(exporter.writer_mut())?;
    Ok(())
}
