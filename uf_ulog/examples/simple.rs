use uf_ulog::LogLevel;
use uf_ulog::Record;
use uf_ulog::RecordSink;
use uf_ulog::TrySendError;
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

pub struct LogsTX {}

impl<const MAX_TEXT: usize, const MAX_PAYLOAD: usize> RecordSink<MAX_TEXT, MAX_PAYLOAD> for LogsTX {
    fn try_send(&self, item: Record<MAX_TEXT, MAX_PAYLOAD>) -> Result<(), TrySendError> {
        println!("Sending item: {:?}", &item);
        Ok(())
    }
}

fn main() {
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

    let tx = LogsTX {};
    let ulog = ULogProducer::<_, 64, 128>::new(tx);

    ulog.data::<Gyro>(&g);
    ulog.data::<Acc>(&a);
    ulog.data::<Acc>(&a);
    ulog.data::<ErrorData>(&err_code);
    ulog.log(LogLevel::Info, 43, "info log");
    ulog.log_tagged(LogLevel::Info, 1, 43, "info log");
    println!("done")
}
