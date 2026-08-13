use gpui::{AnyElement, ParentElement, Styled, div};

/// The production usage-summary grid is two columns. Keeping that fact in the
/// shared screen layer prevents browser demos from inventing a desktop-only
/// four-column arrangement.
pub fn summary_grid(cards: impl IntoIterator<Item = AnyElement>) -> gpui::Div {
    div().grid().grid_cols(2).gap_3().children(cards)
}
