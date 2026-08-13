//! Product-level OcHub screen composition shared by the desktop application
//! and browser documentation.
//!
//! These modules own layout, responsive breakpoints, and the visual structure
//! of a screen. They accept already-resolved labels, view-model values, and
//! action elements so they stay independent from storage and networking.

pub mod common;
pub mod gateway;
pub mod provider_editor;
pub mod providers;
pub mod settings;
pub mod shell;
pub mod themes;
pub mod tools;
pub mod usage;
