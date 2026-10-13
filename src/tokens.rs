//! Geometry and typography for the desktop console.
//!
//! Colors remain in [`crate::theme`] so existing user themes keep working.
//! These are logical pixels: platform scale factors are applied by GPUI.

use gpui::{FontWeight, Styled, px};

pub mod space {
    pub const XS: f32 = 4.;
    pub const SM: f32 = 8.;
    pub const MD: f32 = 12.;
    pub const LG: f32 = 16.;
    pub const XL: f32 = 24.;
    pub const XXL: f32 = 32.;
}

pub mod control {
    pub const COMPACT: f32 = 28.;
    pub const DEFAULT: f32 = 32.;
    pub const LARGE: f32 = 36.;
    pub const ICON: f32 = 16.;
    pub const ICON_SMALL: f32 = 14.;
}

pub mod radius {
    pub const CONTROL: f32 = 6.;
    pub const PANEL: f32 = 8.;
    pub const OVERLAY: f32 = 10.;
}

pub mod layout {
    pub const NAVIGATION_WIDTH: f32 = 220.;
    pub const TOOLBAR_HEIGHT: f32 = 56.;
    pub const ROW_HEIGHT: f32 = 44.;
    pub const COMPACT_ROW_HEIGHT: f32 = 36.;
    pub const INSPECTOR_WIDTH: f32 = 360.;
    pub const MIN_PRIMARY_WITH_INSPECTOR: f32 = 640.;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    PageTitle,
    SectionTitle,
    Body,
    Control,
    Caption,
}

impl TextRole {
    pub const fn metrics(self) -> (f32, f32, FontWeight) {
        match self {
            Self::PageTitle => (24., 30., FontWeight::SEMIBOLD),
            Self::SectionTitle => (15., 22., FontWeight::SEMIBOLD),
            Self::Body => (13., 20., FontWeight::NORMAL),
            Self::Control => (14., 20., FontWeight::NORMAL),
            Self::Caption => (12., 18., FontWeight::NORMAL),
        }
    }
}

/// Explicit roles keep line boxes stable across platform font metrics.
pub trait ConsoleStyle: Styled {
    fn text_role(self, role: TextRole) -> Self {
        let (size, height, weight) = role.metrics();
        self.text_size(px(size))
            .line_height(px(height))
            .font_weight(weight)
    }
}

impl<T: Styled> ConsoleStyle for T {}
