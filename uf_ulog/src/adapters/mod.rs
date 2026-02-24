pub mod heapless;

#[cfg(feature = "embassy")]
pub mod embassy;

#[cfg(feature = "std")]
pub mod std;
