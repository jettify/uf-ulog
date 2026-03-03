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

#[derive(ULogData, Debug)]
struct TestArr {
    timestamp: u64,
    x12: [u8; 2],
    x13: [i8; 2],
    x14: [u16; 2],
    x15: [i16; 2],
    x16: [u32; 2],
    x17: [i32; 2],
    x18: [u64; 2],
    x19: [i64; 2],
    x20: [f32; 2],
    x21: [f64; 2],
    x22: [bool; 2],
}

#[derive(ULogRegistry)]
pub enum UlogDataMessages {
    Gyro,
    Acc,
    Mag,
    Test,
    TestArr,
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
    let t1 = Test {
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

    let t2 = TestArr {
        timestamp: 1772079727637,
        x12: [8; 2],
        x13: [-8; 2],
        x14: [16; 2],
        x15: [-16; 2],
        x16: [32; 2],
        x17: [-32; 2],
        x18: [64; 2],
        x19: [-64; 2],
        x20: [32.0; 2],
        x21: [64.0; 2],
        x22: [true; 2],
    };

    let timestamp = 1772079727637;
    let writer = FromStd::new(File::create("out.ulg")?);
    let mut exporter = ULogCoreExporter::<_, UlogDataMessages>::new(writer)
        .start(timestamp)
        .map_err(map_export_error)?;

    let record_param_p_initial = producer.parameter_f32("P", 1.5).unwrap();
    let record_param_i_initial = producer.parameter_f32("I", 0.01).unwrap();
    let record_param_d_initial = producer.parameter_f32("D", 2.01).unwrap();

    let record_data_gyro = producer.data::<Gyro>(&g).unwrap();
    let record_data_acc = producer.data::<Acc>(&a).unwrap();
    let record_data_mag = producer.data::<Mag>(&m).unwrap();
    let record_data_test = producer.data::<Test>(&t1).unwrap();
    let record_data_test_arr = producer.data::<TestArr>(&t2).unwrap();
    let record_param_p_updated = producer.parameter_f32("P", 0.5).unwrap();
    let record_param_i_updated = producer.parameter_f32("I", 0.01).unwrap();
    let record_param_d_updated = producer.parameter_f32("D", 2.01).unwrap();
    let record_param_servo_trim = producer.parameter_i32("SERVO_TRIM", 1500).unwrap();

    let record_log_info_tagged = producer.log_tagged(LogLevel::Info, 1, timestamp, "info tagged log");
    let record_log_debug = producer.log(LogLevel::Debug, timestamp, "This is debug log");
    let record_log_emerg = producer.log(LogLevel::Emerg, timestamp, "This is Emerg log");
    let record_log_alert = producer.log(LogLevel::Alert, timestamp, "This is Alert log");
    let record_log_notice = producer.log(LogLevel::Notice, timestamp, "This is Notice log");

    exporter.accept(record_param_p_initial).map_err(map_export_error)?;
    exporter.accept(record_param_i_initial).map_err(map_export_error)?;
    exporter.accept(record_param_d_initial).map_err(map_export_error)?;

    exporter.accept(record_data_gyro).map_err(map_export_error)?;
    exporter.accept(record_data_acc).map_err(map_export_error)?;
    exporter.accept(record_data_mag).map_err(map_export_error)?;
    exporter.accept(record_data_test).map_err(map_export_error)?;
    exporter.accept(record_data_test_arr).map_err(map_export_error)?;
    exporter.accept(record_param_p_updated).map_err(map_export_error)?;
    exporter.accept(record_param_i_updated).map_err(map_export_error)?;
    exporter.accept(record_param_d_updated).map_err(map_export_error)?;
    exporter
        .accept(record_param_servo_trim)
        .map_err(map_export_error)?;

    exporter
        .accept(record_log_info_tagged)
        .map_err(map_export_error)?;
    exporter.accept(record_log_debug).map_err(map_export_error)?;
    exporter.accept(record_log_emerg).map_err(map_export_error)?;
    exporter.accept(record_log_alert).map_err(map_export_error)?;
    exporter.accept(record_log_notice).map_err(map_export_error)?;

    embedded_io::Write::flush(exporter.writer_mut())?;
    Ok(())
}
