#![allow(
    clippy::needless_doctest_main,
    reason = "This is readme example, not doctest"
)]
#![doc = include_str!("../../README.md")]
extern crate self as uf_ulog;

#[cfg(feature = "derive")]
pub use uf_ulog_macro::{ULogData, ULogRegistry};
pub mod adapters;
mod export;
mod registry;
mod types;
mod ulog;
mod writer;
#[cfg(feature = "async")]
mod writer_async;
mod writer_common;

pub use export::{ExportError, ExportStep};
pub use registry::{MessageMeta, Registry, Topic, TopicOf, ULogRegistry};
pub use types::{EncodeError, LogLevel, LoggedString, Subscription, ULogData};
pub use ulog::{
    EmitStatus, ParameterValue, Record, RecordKind, RecordMeta, RecordSink, TrySendError,
    ULogProducer,
};
pub use writer::{RecordSource, ULogExporter};
#[cfg(feature = "async")]
pub use writer_async::{AsyncRecordSource, ULogAsyncExporter};
