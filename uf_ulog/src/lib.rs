#![no_std]
#![allow(
    clippy::needless_doctest_main,
    reason = "This is readme example, not doctest"
)]
#![doc = include_str!("../../README.md")]
extern crate self as uf_ulog;

#[cfg(feature = "derive")]
pub use uf_ulog_macro::ULogData;
mod types;
mod ulog;

pub use types::{EncodeError, LogLevel, LoggedString, Subscription, ULogData};
pub use ulog::{
    Clock, DEFAULT_MAX_PAYLOAD, DEFAULT_MAX_TEXT, EmitError, Event, TrySend, TrySendError,
    ULogProducer,
};
