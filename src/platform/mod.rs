//! Narrow adapters for Wii services provided by `libogc-rs`.

pub mod allocator;
pub mod input;
#[cfg(target_arch = "powerpc")]
pub mod settings;
pub mod storage;
