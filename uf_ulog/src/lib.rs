#![allow(
    clippy::needless_doctest_main,
    reason = "This is readme example, not doctest"
)]
#![doc = include_str!("../../README.md")]
extern crate self as uf_ulog;

#[cfg(feature = "derive")]
pub use uf_ulog_macro::{ULogData, ULogRegistry};
pub mod adapters;
mod data;
mod exporter;
#[cfg(feature = "async")]
mod exporter_async;
mod producer;
mod registry;
mod wire;

pub use data::{
    EncodeError, LogLevel, LoggedString, ParameterValue, Record, RecordKind, RecordMeta,
    Subscription, TrySendError, ULogData,
};
pub use exporter::{RecordSource, ULogExporter};
#[cfg(feature = "async")]
pub use exporter_async::{AsyncRecordSource, ULogAsyncExporter};
pub use producer::{EmitStatus, RecordSink, ULogProducer};
pub use registry::{MessageMeta, Registry, Topic, TopicOf, ULogRegistry};
pub use wire::{ExportError, ExportStep};
