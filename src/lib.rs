#![cfg_attr(target_arch = "powerpc", no_std)]

//! Native Rust implementation of the HBC channel application.
//!
//! Keep policy and parsing independent from Wii hardware. Put hardware access
//! behind small adapters that use the Rust APIs from `libogc-rs`.

extern crate alloc;

pub mod app;
pub mod archive;
pub mod catalog;
pub mod font;
pub mod home_overlay;
pub mod home_overlay_app;
pub mod home_overlay_host;
pub mod loader;
pub mod localization;
pub mod management;
pub mod metadata;
pub mod network_config;
#[cfg(any(target_arch = "powerpc", test))]
mod network_scan;
mod playtime;
pub mod png;
pub mod remote_control;
pub mod remote_input;
pub mod render;
pub mod runtime;
mod screenshot;
pub mod settings;
pub mod text_layout;
pub mod theme;
#[cfg_attr(not(target_arch = "powerpc"), allow(dead_code))]
mod title_path;
pub mod wiiload;
pub mod wiiload_zip;

pub mod platform;

#[cfg(test)]
mod test_support;

#[cfg(all(target_arch = "powerpc", feature = "emulator-acceptance"))]
pub mod acceptance;
