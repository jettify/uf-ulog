use std::convert::Infallible;
use std::sync::mpsc::sync_channel;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::mpsc::{TryRecvError as ChannelTryRecvError, TrySendError as ChannelTrySendError};

use uf_ulog::register_messages;
use uf_ulog::ExportStep;
use uf_ulog::LogLevel;
use uf_ulog::Record;
use uf_ulog::RecordSink;
use uf_ulog::RecordSource;
use uf_ulog::TrySendError;
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
            Self::GenericError => "GenericError",
            Self::GyroError => "GyroError",
            Self::AccError => "AccError",
        }
    }
}

#[derive(ULogData, Debug)]
struct ErrorData {
    timestamp: u64,
    code: u16,
}

const MAX_TEXT: usize = 64;
const MAX_PAYLOAD: usize = 128;

pub struct LogsTx<const T: usize, const P: usize> {
    tx: SyncSender<Record<T, P>>,
}

impl<const T: usize, const P: usize> LogsTx<T, P> {
    fn new(tx: SyncSender<Record<T, P>>) -> Self {
        Self { tx }
    }
}

impl<const T: usize, const P: usize> RecordSink<T, P> for LogsTx<T, P> {
    fn try_send(&self, item: Record<T, P>) -> Result<(), TrySendError> {
        match self.tx.try_send(item) {
            Ok(()) => Ok(()),
            Err(ChannelTrySendError::Full(_)) => Err(TrySendError::Full),
            Err(ChannelTrySendError::Disconnected(_)) => Err(TrySendError::Closed),
        }
    }
}

pub struct LogsRx<const T: usize, const P: usize> {
    rx: Receiver<Record<T, P>>,
}

impl<const T: usize, const P: usize> LogsRx<T, P> {
    fn new(rx: Receiver<Record<T, P>>) -> Self {
        Self { rx }
    }
}

impl<const T: usize, const P: usize> RecordSource<T, P> for LogsRx<T, P> {
    fn try_recv(&mut self) -> Option<Record<T, P>> {
        match self.rx.try_recv() {
            Ok(record) => Some(record),
            Err(ChannelTryRecvError::Empty) | Err(ChannelTryRecvError::Disconnected) => None,
        }
    }
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
    let registry = register_messages![Gyro, Acc, ErrorData];
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

    let (tx, rx) = sync_channel::<Record<MAX_TEXT, MAX_PAYLOAD>>(32);

    let tx = LogsTx::<MAX_TEXT, MAX_PAYLOAD>::new(tx);
    let ulog = ULogProducer::<_, _, MAX_TEXT, MAX_PAYLOAD>::new(tx, &registry);

    ulog.data::<Gyro>(&g);
    ulog.data::<Acc>(&a);
    ulog.data::<Acc>(&a);
    ulog.data::<ErrorData>(&err_code);
    ulog.log(LogLevel::Info, 43, "info log");
    ulog.log_tagged(LogLevel::Info, 1, 43, "info log");

    let rx = LogsRx::<MAX_TEXT, MAX_PAYLOAD>::new(rx);
    let mut exporter =
        ULogExporter::<_, _, _, MAX_TEXT, MAX_PAYLOAD>::new(PrintWriter, rx, &registry);
    exporter.emit_startup(0).unwrap();

    loop {
        match exporter.poll_once().unwrap() {
            ExportStep::Progressed => {}
            ExportStep::Idle => break,
        }
    }

    println!("done")
}
