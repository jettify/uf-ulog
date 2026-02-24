#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageMeta {
    pub name: &'static str,
    pub format: &'static str,
    pub wire_size: usize,
}

pub trait MessageSet: Sized {
    const REGISTRY: Registry;
}

pub trait TopicIndex<R: MessageSet> {
    const INDEX: u16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registry {
    pub entries: &'static [MessageMeta],
}

impl Registry {
    pub const fn new(entries: &'static [MessageMeta]) -> Self {
        let mut i = 0;
        while i < entries.len() {
            let mut j = i + 1;
            while j < entries.len() {
                if str_eq(entries[i].name, entries[j].name) {
                    panic!("duplicate ULog message name in registry");
                }
                j += 1;
            }
            i += 1;
        }
        Self { entries }
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub const fn get(&self, index: usize) -> Option<&MessageMeta> {
        if index >= self.entries.len() {
            None
        } else {
            Some(&self.entries[index])
        }
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }

    let mut i = 0;
    while i < a_bytes.len() {
        if a_bytes[i] != b_bytes[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[macro_export]
macro_rules! register_messages {
    ($vis:vis enum $name:ident { $($ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name {}

        $crate::__register_messages_impl_topic_index!($name, 0u16; $($ty),*);

        impl $crate::MessageSet for $name {
            const REGISTRY: $crate::Registry = $crate::Registry::new(&[
                $(
                    $crate::MessageMeta {
                        name: <$ty as $crate::ULogData>::NAME,
                        format: <$ty as $crate::ULogData>::FORMAT,
                        wire_size: <$ty as $crate::ULogData>::WIRE_SIZE,
                    }
                ),*
            ]);
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __register_messages_impl_topic_index {
    ($key:ty, $idx:expr; $head:ty $(, $tail:ty)*) => {
        impl $crate::TopicIndex<$key> for $head {
            const INDEX: u16 = $idx;
        }
        $crate::__register_messages_impl_topic_index!($key, ($idx + 1u16); $($tail),*);
    };
    ($key:ty, $idx:expr;) => {};
}
