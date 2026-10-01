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
pub mod loader;
pub mod localization;
pub mod management;
pub mod metadata;
pub mod png;
pub mod render;
pub mod runtime;
pub mod settings;
pub mod text_layout;
pub mod theme;
#[cfg_attr(not(target_arch = "powerpc"), allow(dead_code))]
mod title_path;
pub mod wiiload;
pub mod wiiload_zip;

pub mod platform;
