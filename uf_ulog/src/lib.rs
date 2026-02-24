#![allow(
    clippy::needless_doctest_main,
    reason = "This is readme example, not doctest"
)]
#![doc = include_str!("../../README.md")]
extern crate self as uf_ulog;

#[cfg(feature = "derive")]
pub use uf_ulog_macro::ULogData;
pub mod adapters;
mod cfg;
mod registry;
mod types;
mod ulog;
mod writer;
#[cfg(feature = "async")]
mod writer_async;

pub use cfg::{DefaultCfg, PayloadBuf, StreamState, TextBuf, ULogCfg};
pub use registry::{MessageMeta, MessageSet, Registry, TopicIndex};
pub use types::{EncodeError, LogLevel, LoggedString, Subscription, ULogData};
pub use ulog::{EmitStatus, Record, RecordSink, TrySendError, ULogProducer};
pub use writer::{ExportError, ExportStep, RecordSource, ULogExporter};
#[cfg(feature = "async")]
pub use writer_async::{AsyncRecordSource, ULogAsyncExporter};
