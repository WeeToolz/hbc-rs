//! Narrow adapters for Wii services provided by `libogc-rs`.

#[cfg(target_arch = "powerpc")]
pub mod allocator;
#[cfg(all(feature = "startup-diagnostics", any(target_arch = "powerpc", test)))]
pub mod diagnostics;
#[cfg(any(target_arch = "powerpc", test))]
mod draw_state;
pub mod input;
pub mod lifecycle;
pub mod nand;
#[cfg(target_arch = "powerpc")]
pub mod network_config;
#[cfg(target_arch = "powerpc")]
pub mod network_scan;
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
#[cfg(target_arch = "powerpc")]
pub mod wiiload;
