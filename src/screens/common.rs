use gpui::{IntoElement, ParentElement, SharedString, Styled};

use crate::layout;

/// Canonical product-page frame. The body may be a native virtual list or a
/// browser demo adapter; the title bar and flex behavior stay identical.
pub fn page(
    title: impl Into<SharedString>,
    actions: impl IntoElement,
    body: impl IntoElement,
) -> gpui::Div {
    layout::page()
        .relative()
        .child(layout::page_header(title, None).child(actions))
        .child(body)
}
