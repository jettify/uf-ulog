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

pub use cfg::{DefaultCfg, PayloadBuf, StreamState, TextBuf, ULogCfg};
pub use registry::{MessageMeta, Registry, RegistryKey, TopicIndex};
pub use types::{EncodeError, LogLevel, LoggedString, Subscription, ULogData};
pub use ulog::{EmitStatus, Record, RecordSink, TrySendError, ULogProducer};
pub use writer::{ExportError, ExportStep, RecordSource, ULogExporter};

#[macro_export]
macro_rules! register_messages {
    () => {
        {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            struct __UlogRegistryKey;
            impl $crate::RegistryKey for __UlogRegistryKey {}

            const REGISTRY: $crate::Registry<__UlogRegistryKey> = $crate::Registry::new(&[]);
            REGISTRY
        }
    };
    ($($ty:ty),+ $(,)?) => {
        {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            struct __UlogRegistryKey;
            impl $crate::RegistryKey for __UlogRegistryKey {}

            $crate::__register_messages_impl_topic_index!(__UlogRegistryKey, 0u16; $($ty),+);

            const ENTRIES: &[$crate::MessageMeta] = &[
                $(
                    $crate::MessageMeta {
                        name: <$ty as $crate::ULogData>::NAME,
                        format: <$ty as $crate::ULogData>::FORMAT,
                        wire_size: <$ty as $crate::ULogData>::WIRE_SIZE,
                    }
                ),+
            ];
            const REGISTRY: $crate::Registry<__UlogRegistryKey> = $crate::Registry::new(ENTRIES);
            REGISTRY
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __register_messages_impl_topic_index {
    ($key:ty, $idx:expr; $head:ty $(, $tail:ty)*) => {
        #[allow(non_local_definitions)]
        impl $crate::TopicIndex<$key> for $head {
            const INDEX: u16 = $idx;
        }
        $crate::__register_messages_impl_topic_index!($key, ($idx + 1u16); $($tail),*);
    };
    ($key:ty, $idx:expr;) => {};
}
