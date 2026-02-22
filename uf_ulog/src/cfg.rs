pub trait TextBuf: Clone + core::fmt::Debug + PartialEq {
    const CAPACITY: usize;

    fn new() -> Self;
    fn push_str(&mut self, s: &str);
    fn as_bytes(&self) -> &[u8];
}

impl<const N: usize> TextBuf for heapless::String<N> {
    const CAPACITY: usize = N;

    fn new() -> Self {
        Self::new()
    }

    fn push_str(&mut self, s: &str) {
        let _ = heapless::String::<N>::push_str(self, s);
    }

    fn as_bytes(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

pub trait PayloadBuf: Clone + core::fmt::Debug + PartialEq {
    const CAPACITY: usize;

    fn zeroed() -> Self;
    fn as_slice(&self) -> &[u8];
    fn as_mut_slice(&mut self) -> &mut [u8];
}

impl<const N: usize> PayloadBuf for [u8; N] {
    const CAPACITY: usize = N;

    fn zeroed() -> Self {
        [0; N]
    }

    fn as_slice(&self) -> &[u8] {
        self
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        self
    }
}

pub trait StreamState {
    const MAX_STREAMS: usize;

    fn zeroed() -> Self;
    fn is_subscribed(&self, slot: usize) -> bool;
    fn mark_subscribed(&mut self, slot: usize);
}

impl<const N: usize> StreamState for [u8; N] {
    const MAX_STREAMS: usize = N;

    fn zeroed() -> Self {
        [0; N]
    }

    fn is_subscribed(&self, slot: usize) -> bool {
        self[slot] != 0
    }

    fn mark_subscribed(&mut self, slot: usize) {
        self[slot] = 1;
    }
}

pub trait ULogCfg {
    type Text: TextBuf;
    type Payload: PayloadBuf;
    type Streams: StreamState;

    const MAX_MULTI_IDS: usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultCfg;

impl ULogCfg for DefaultCfg {
    type Text = heapless::String<128>;
    type Payload = [u8; 256];
    type Streams = [u8; 1024];

    const MAX_MULTI_IDS: usize = 8;
}
