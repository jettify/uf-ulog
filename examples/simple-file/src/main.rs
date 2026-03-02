use embedded_io_adapters::std::FromStd;
use std::fs::File;

use uf_ulog::ExportError;
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

fn map_export_error(err: ExportError<std::io::Error>) -> std::io::Error {
    match err {
        ExportError::Write(io_err) => io_err,
        other => std::io::Error::other(format!("ulog export error: {other:?}")),
    }
}

fn main() -> std::io::Result<()> {
    let producer = ULogProducer::<UlogDataMessages>::new();

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

    let timestamp = 1772079727637;
    let writer = FromStd::new(File::create("out.ulg")?);
    let mut exporter = ULogCoreExporter::<_, UlogDataMessages>::new(writer)
        .start(0)
        .map_err(map_export_error)?;

    exporter
        .accept(producer.parameter_f32("P", 1.5).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.parameter_f32("I", 0.01).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.parameter_f32("D", 2.01).unwrap())
        .map_err(map_export_error)?;

    exporter
        .accept(producer.data::<Gyro>(&g).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.data::<Acc>(&a).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.data::<Mag>(&m).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.data::<Test>(&t).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.parameter_f32("P", 0.5).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.parameter_f32("I", 0.01).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.parameter_f32("D", 2.01).unwrap())
        .map_err(map_export_error)?;
    exporter
        .accept(producer.parameter_i32("SERVO_TRIM", 1500).unwrap())
        .map_err(map_export_error)?;

    exporter
        .accept(producer.log_tagged(LogLevel::Info, 1, timestamp, "info tagged log"))
        .map_err(map_export_error)?;
    exporter
        .accept(producer.log(LogLevel::Debug, timestamp, "This is debug log"))
        .map_err(map_export_error)?;
    exporter
        .accept(producer.log(LogLevel::Emerg, timestamp, "This is Emerg log"))
        .map_err(map_export_error)?;
    exporter
        .accept(producer.log(LogLevel::Alert, timestamp, "This is Alert log"))
        .map_err(map_export_error)?;
    exporter
        .accept(producer.log(LogLevel::Notice, timestamp, "This is Notice log"))
        .map_err(map_export_error)?;

    embedded_io::Write::flush(exporter.writer_mut())?;
    Ok(())
}
