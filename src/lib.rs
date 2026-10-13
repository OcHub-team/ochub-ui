//! Reusable GPUI components used by OcHub applications.
//!
//! The crate owns generic presentation primitives only. Product screens,
//! application navigation, business view models, storage, and networking stay
//! in consuming applications.

pub mod anim;
pub mod assets;
pub mod chart;
#[cfg(not(target_family = "wasm"))]
pub mod code_editor;
pub mod components;
pub mod diff_view;
pub mod fold;
pub mod gallery;
pub mod highlight;
pub mod i18n;
pub mod icons;
pub mod keyboard;
pub mod layout;
pub mod modal;
pub mod navigation;
pub mod notifications;
pub mod scrollbar;
pub mod select;
pub mod shortcuts;
pub mod table;
pub mod text_input;
pub mod theme;
pub mod tokens;
pub mod window_chrome;

// Consumers should use these re-exports when they need to implement a custom
// element. This guarantees that component and application types come from the
// same pinned Zed revision.
pub use gpui;
#[cfg(not(target_family = "wasm"))]
pub use zed_text;

/// Install every key binding owned by the component library.
pub fn install(cx: &mut gpui::App) {
    assets::install_fonts(cx);
    keyboard::bind_keys(cx);
    text_input::bind_keys(cx);
    #[cfg(not(target_family = "wasm"))]
    code_editor::bind_keys(cx);
    layout::bind_keys(cx);
    modal::bind_keys(cx);
    notifications::bind_keys(cx);
}

#[cfg(test)]
mod interaction_tests;
