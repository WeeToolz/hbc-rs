//! Native Rust implementation of the HBC channel application.
//!
//! Keep policy and parsing independent from Wii hardware. Put hardware access
//! behind small adapters that use the Rust APIs from `libogc-rs`.

pub mod app;
pub mod catalog;
pub mod metadata;
pub mod settings;

pub mod platform;
