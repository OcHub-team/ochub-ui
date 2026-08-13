//! The reusable GPUI design system used by OcHub desktop applications.
//!
//! The crate owns presentation primitives only. It deliberately has no
//! dependency on `ochub-core`, application storage, networking, or product
//! views, so releases can be maintained independently from the OcHub app.

pub mod anim;
pub mod assets;
pub mod chart;
pub mod code_editor;
pub mod components;
pub mod diff_view;
pub mod fold;
pub mod gallery;
pub mod highlight;
pub mod i18n;
pub mod icons;
pub mod layout;
pub mod notifications;
pub mod scrollbar;
pub mod text_input;
pub mod theme;
pub mod window_chrome;

// Consumers should use these re-exports when they need to implement a custom
// element. This guarantees that component and application types come from the
// same pinned Zed revision.
pub use gpui;
pub use zed_text;

/// Install every key binding owned by the component library.
pub fn install(cx: &mut gpui::App) {
    text_input::bind_keys(cx);
    code_editor::bind_keys(cx);
    layout::bind_keys(cx);
}
