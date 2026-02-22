use core::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageMeta {
    pub name: &'static str,
    pub format: &'static str,
    pub wire_size: usize,
}

pub trait RegistryKey {}

pub trait TopicIndex<R: RegistryKey> {
    const INDEX: u16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registry<R: RegistryKey> {
    pub entries: &'static [MessageMeta],
    marker: PhantomData<fn() -> R>,
}

impl<R: RegistryKey> Registry<R> {
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
        Self {
            entries,
            marker: PhantomData,
        }
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
