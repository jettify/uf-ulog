use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use futures::executor::block_on;
use futures::join;
use std::fs::File;
use std::io::Write as _;

use uf_ulog::ExportError;
use uf_ulog::ULogAsyncCoreExporter;
use uf_ulog::ULogData;
use uf_ulog::ULogProducer;
use uf_ulog::ULogRegistry;

const RECORD_CAP: usize = 128;
const CHANNEL_CAP: usize = 8;
const RECORDS_PER_PRODUCER: usize = 8;
const TOTAL_RECORDS: usize = RECORDS_PER_PRODUCER * 2;
const START_TIMESTAMP: u64 = 1_772_079_727_637;

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
enum UlogDataMessages {
    Gyro,
    Acc,
}

type MsgRecord = uf_ulog::Record<RECORD_CAP>;

static RECORDS: Channel<CriticalSectionRawMutex, MsgRecord, CHANNEL_CAP> = Channel::new();

struct FileAsyncWriter {
    file: File,
}

impl embedded_io_async::ErrorType for FileAsyncWriter {
    type Error = std::io::Error;
}

impl embedded_io_async::Write for FileAsyncWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.file.write(buf)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.file.flush()
    }
}

fn map_export_error(err: ExportError<std::io::Error>) -> std::io::Error {
    match err {
        ExportError::Write(io_err) => io_err,
        other => std::io::Error::other(format!("ulog export error: {other:?}")),
    }
}

async fn writer_task() {
    let file = File::create("out_embassy.ulg").expect("create out_embassy.ulg");
    let writer = FileAsyncWriter { file };

    let mut exporter = ULogAsyncCoreExporter::<_, UlogDataMessages, _, RECORD_CAP>::new(writer)
        .start(START_TIMESTAMP)
        .await
        .map_err(map_export_error)
        .expect("start exporter");

    for _ in 0..TOTAL_RECORDS {
        let record = RECORDS.receive().await;
        exporter
            .accept(record)
            .await
            .map_err(map_export_error)
            .expect("write ulog record");
    }

    embedded_io_async::Write::flush(exporter.writer_mut())
        .await
        .expect("flush output file");
}

async fn gyro_producer_task() {
    let producer = ULogProducer::<UlogDataMessages, RECORD_CAP>::new();

    for idx in 0..RECORDS_PER_PRODUCER {
        let sample = Gyro {
            timestamp: START_TIMESTAMP + idx as u64,
            x: 1.0 + idx as f32,
            y: 2.0 + idx as f32,
            z: 3.0 + idx as f32,
        };

        let record = producer.data::<Gyro>(&sample).expect("build gyro record");
        RECORDS.send(record).await;
    }
}

async fn acc_producer_task() {
    let producer = ULogProducer::<UlogDataMessages, RECORD_CAP>::new();

    for idx in 0..RECORDS_PER_PRODUCER {
        let sample = Acc {
            timestamp: START_TIMESTAMP + idx as u64,
            x: 9.0 - idx as f32,
            y: 8.0 - idx as f32,
            z: 7.0 - idx as f32,
        };

        let record = producer.data::<Acc>(&sample).expect("build acc record");
        RECORDS.send(record).await;
    }
}

fn main() {
    block_on(async {
        join!(writer_task(), gyro_producer_task(), acc_producer_task());
    });
}
