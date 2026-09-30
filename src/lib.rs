#![cfg_attr(target_arch = "powerpc", no_std)]

//! Native Rust implementation of the HBC channel application.
//!
//! Keep policy and parsing independent from Wii hardware. Put hardware access
//! behind small adapters that use the Rust APIs from `libogc-rs`.

extern crate alloc;

pub mod app;
pub mod archive;
pub mod catalog;
pub mod loader;
pub mod management;
pub mod metadata;
pub mod runtime;
pub mod settings;

pub mod platform;
