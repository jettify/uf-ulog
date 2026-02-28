#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageMeta {
    pub name: &'static str,
    pub format: &'static str,
    pub wire_size: usize,
}

pub trait ULogRegistry: Sized {
    const REGISTRY: Registry;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Topic<T> {
    id: u16,
    _marker: core::marker::PhantomData<fn() -> T>,
}

impl<T> Topic<T> {
    pub const fn new(id: u16) -> Self {
        Self {
            id,
            _marker: core::marker::PhantomData,
        }
    }

    pub const fn id(self) -> u16 {
        self.id
    }
}

pub trait TopicOf<R>: Sized {
    const TOPIC: Topic<Self>;
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
        // self.entries.get(index) is not yet stable in const context
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
    // since this is const context a == b does not work
    // we need to do comparison one char at time.
    let mut i = 0;
    while i < a_bytes.len() {
        if a_bytes[i] != b_bytes[i] {
            return false;
        }
        i += 1;
    }
    true
}
