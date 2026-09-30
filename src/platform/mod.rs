//! Narrow adapters for Wii services provided by `libogc-rs`.

pub mod allocator;
pub mod input;
pub mod lifecycle;
pub mod nand;
#[cfg(target_arch = "powerpc")]
pub mod renderer;
#[cfg(target_arch = "powerpc")]
pub mod settings;
#[cfg(target_arch = "powerpc")]
pub mod startup;
pub mod storage;
#[cfg(target_arch = "powerpc")]
pub mod theme;
#[cfg(target_arch = "powerpc")]
pub mod video;
