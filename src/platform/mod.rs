//! Narrow adapters for Wii services provided by `libogc-rs`.

pub mod allocator;
pub mod input;
#[cfg(target_arch = "powerpc")]
pub mod renderer;
#[cfg(target_arch = "powerpc")]
pub mod settings;
#[cfg(target_arch = "powerpc")]
pub mod startup;
pub mod storage;
#[cfg(target_arch = "powerpc")]
pub mod video;
