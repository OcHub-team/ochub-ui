//! The client-side title bar OcHub draws when the platform provides none.
//!
//! Which platforms those are is not obvious, because the two knobs involved are
//! read by different backends:
//!
//! - `TitlebarOptions::appears_transparent` (what `main` sets) is honoured by
//!   macOS and Windows only. macOS keeps its traffic lights and merely hides
//!   the title text, so it still has usable chrome. Windows drops the entire
//!   title bar, buttons included. gpui's Linux backends never read the field.
//! - `WindowOptions::window_decorations` is what Linux reads, and OcHub leaves
//!   it at the platform default: X11 defaults to server-side decorations (a
//!   real title bar), while Wayland starts client-side and then negotiates via
//!   `zxdg_toplevel_decoration_v1`. KDE hands back server-side; GNOME only
//!   supports client-side, so there it stays.
//!
//! So the strip is needed on Windows always, and on Linux only when the
//! compositor left us with `Decorations::Client`. macOS and X11 never see it.
//!
//! The two platforms also *drive* their chrome differently, which is why the
//! buttons below are not shared:
//!
//! - **Windows** expects the hit test to lie. gpui collects `WindowControlArea`
//!   hitboxes during paint and answers `WM_NCHITTEST` with `HTCAPTION` /
//!   `HTMINBUTTON` / `HTMAXBUTTON` / `HTCLOSE`, after which `DefWindowProc`
//!   performs the drag, the double-click maximize and the button presses
//!   itself. Those buttons carry no click handlers at all. It also has no
//!   `start_window_move` implementation, so the drag handlers the rest of the
//!   shell uses are silently dead there.
//! - **Linux** ignores `WindowControlArea` entirely — both the X11 and Wayland
//!   backends stub out `on_hit_test_window_control` — so its buttons are
//!   ordinary elements with real click handlers, and dragging goes through
//!   `start_window_move`, which those backends do implement.
//!
//! Closing routes through the host-provided [`CloseWindowHandler`] on Linux
//! rather than `Window::remove_window`, so applications can persist bounds or
//! minimize to a tray. Windows gets that for free: its caption button posts
//! `WM_CLOSE`, which gpui turns into the host's window-close callback.
//!
//! This mirrors how Zed's own `PlatformTitleBar` handles the same split.

use std::rc::Rc;

use gpui::{AnyElement, App, Window};

/// Height of the client title bar, in logical pixels. Page layout must subtract
/// this only when a strip is actually drawn; a Linux server title bar sits
/// outside GPUI's content viewport.
pub const TITLE_BAR_HEIGHT: f32 = 32.;

#[derive(Clone, Copy)]
enum ChromePlatform {
    MacOs,
    Windows,
    Unix,
    Other,
}

impl ChromePlatform {
    const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            Self::Unix
        } else {
            Self::Other
        }
    }
}

const fn title_bar_height_for(platform: ChromePlatform, client_decorated: bool) -> f32 {
    match platform {
        ChromePlatform::Windows => TITLE_BAR_HEIGHT,
        ChromePlatform::Unix if client_decorated => TITLE_BAR_HEIGHT,
        _ => 0.,
    }
}

pub fn title_bar_height(window: &Window) -> gpui::Pixels {
    gpui::px(title_bar_height_for(
        ChromePlatform::current(),
        matches!(
            window.window_decorations(),
            gpui::Decorations::Client { .. }
        ),
    ))
}

/// Only macOS reserves room for traffic lights inside the navigation rail.
pub const fn sidebar_top_inset() -> f32 {
    if cfg!(target_os = "macos") { 44. } else { 8. }
}

fn caption_label(button: gpui::WindowButton, maximized: bool) -> gpui::SharedString {
    use crate::i18n::{k, t};
    t(match button {
        gpui::WindowButton::Minimize => k::COMMON_WINDOW_MINIMIZE,
        gpui::WindowButton::Maximize if maximized => k::COMMON_WINDOW_RESTORE,
        gpui::WindowButton::Maximize => k::COMMON_WINDOW_MAXIMIZE,
        gpui::WindowButton::Close => k::COMMON_WINDOW_CLOSE,
    })
}

/// Host callback used when a client-side close button is activated.
pub type CloseWindowHandler = Rc<dyn Fn(&mut App)>;

/// The title bar strip, or `None` where the platform draws its own chrome.
#[allow(unused_variables)]
pub fn title_bar(
    window: &mut Window,
    cx: &mut App,
    close_window: CloseWindowHandler,
) -> Option<AnyElement> {
    // Refer to the cross-platform labels even on hosts with native controls;
    // generated catalogs intentionally reject keys which are never used.
    let _ = caption_label;
    #[cfg(target_os = "windows")]
    {
        use gpui::IntoElement as _;
        return Some(windows_chrome::title_bar(window.is_maximized()).into_any_element());
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        use gpui::IntoElement as _;
        // A server-decorated window (X11, or Wayland under a compositor that
        // supports SSD) already has a real title bar; drawing a second one
        // inside the content would just duplicate it.
        if title_bar_height(window) == gpui::px(0.) {
            return None;
        }
        return Some(linux_chrome::title_bar(window, cx, close_window).into_any_element());
    }

    #[allow(unreachable_code)]
    None
}

/// Shared metric, so the two implementations at least agree on the strip's
/// height even though they agree on nothing else.
#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    test
))]
const BAR_HEIGHT: f32 = TITLE_BAR_HEIGHT;

#[cfg(target_os = "windows")]
mod windows_chrome {
    use gpui::{SharedString, WindowControlArea, div, prelude::*, px};

    use super::BAR_HEIGHT;
    use crate::icons::{IconName, icon};
    use crate::theme;

    /// The Windows 11 caption button metric. Matching it is what makes the
    /// strip read as native chrome rather than as a row of buttons.
    const BUTTON_WIDTH: f32 = 46.;

    pub(super) fn title_bar(maximized: bool) -> impl IntoElement {
        div()
            .id("window-title-bar")
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .w_full()
            .h(px(BAR_HEIGHT))
            .flex_shrink_0()
            // No bottom border: the strip separates itself from the content
            // pane by color, the way a native title bar does. A rule here would
            // also cut across the sidebar, which shares this background and is
            // meant to read as one surface with it.
            .bg(theme::sidebar_background())
            // The whole strip drags, buttons included — they take themselves
            // back below. The hit test walks the marked areas in paint order
            // and this container paints first, so it would otherwise swallow
            // every button.
            .window_control_area(WindowControlArea::Drag)
            .child(caption_button(CaptionButton::Minimize))
            .child(caption_button(if maximized {
                CaptionButton::Restore
            } else {
                CaptionButton::Maximize
            }))
            .child(caption_button(CaptionButton::Close))
    }

    #[derive(Clone, Copy)]
    enum CaptionButton {
        Minimize,
        Maximize,
        Restore,
        Close,
    }

    impl CaptionButton {
        fn id(self) -> &'static str {
            match self {
                Self::Minimize => "window-minimize",
                Self::Maximize => "window-maximize",
                Self::Restore => "window-restore",
                Self::Close => "window-close",
            }
        }

        fn icon(self) -> IconName {
            match self {
                Self::Minimize => IconName::WindowMinimize,
                Self::Maximize => IconName::WindowMaximize,
                Self::Restore => IconName::WindowRestore,
                Self::Close => IconName::Close,
            }
        }

        fn control_area(self) -> WindowControlArea {
            match self {
                Self::Minimize => WindowControlArea::Min,
                Self::Maximize | Self::Restore => WindowControlArea::Max,
                Self::Close => WindowControlArea::Close,
            }
        }
    }

    fn caption_button(button: CaptionButton) -> impl IntoElement {
        let (hover_background, hover_foreground) = match button {
            CaptionButton::Close => (theme::red(), theme::c(0xffffff)),
            _ => (theme::surface_hover(), theme::text()),
        };
        let group = SharedString::new_static(button.id());
        div()
            .id(button.id())
            .group(group.clone())
            .flex()
            .items_center()
            .justify_center()
            .w(px(BUTTON_WIDTH))
            .h_full()
            .flex_shrink_0()
            // Blocking the mouse is what reclaims this square from the strip's
            // drag area: the hit test stops descending at a blocking hitbox, so
            // the container's `Drag` never reaches the list of areas under the
            // cursor and this button's own area wins.
            .occlude()
            .hover(|style| style.bg(hover_background))
            .window_control_area(button.control_area())
            .child(
                // The glyph carries its own color: gpui skips painting an SVG
                // whose element has no text color of its own, so recoloring on
                // hover has to be a group style here rather than an inherited
                // one on the button.
                icon(button.icon(), theme::text(), 12.)
                    .group_hover(group, move |style| style.text_color(hover_foreground)),
            )
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", test))]
mod linux_chrome {
    use gpui::{
        App, MAX_BUTTONS_PER_SIDE, MouseButton, SharedString, Window, WindowButton,
        WindowButtonLayout, div, prelude::*, px,
    };

    use super::BAR_HEIGHT;
    use super::CloseWindowHandler;
    use crate::icons::{IconName, icon};
    use crate::theme;

    /// Round, inset controls rather than the full-height Windows squares —
    /// that is the shape GNOME and KDE both draw.
    const BUTTON_SIZE: f32 = 24.;

    /// Used when the desktop exposes no button preference. Spelled out rather
    /// than taken from `WindowButtonLayout::linux_default`, which is itself
    /// `cfg`-gated to Linux and so cannot be name-checked from a host build.
    const FALLBACK_LAYOUT: WindowButtonLayout = WindowButtonLayout {
        left: [None; MAX_BUTTONS_PER_SIDE],
        right: [
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize),
            Some(WindowButton::Close),
        ],
    };

    pub(super) fn title_bar(
        window: &mut Window,
        cx: &mut App,
        close_window: CloseWindowHandler,
    ) -> impl IntoElement {
        // The desktop decides which buttons exist and which side they sit on;
        // someone who moved their controls to the left expects ours to follow.
        // Fall back to gpui's built-in layout when the platform has no opinion.
        let layout = cx.button_layout().unwrap_or(FALLBACK_LAYOUT);
        let maximized = window.is_maximized();
        let supported = window.window_controls();

        let side = move |buttons: [Option<WindowButton>; MAX_BUTTONS_PER_SIDE],
                         id: &'static str| {
            let close_window = close_window.clone();
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .flex_shrink_0()
                .gap_2()
                .px_2()
                .children(
                    buttons
                        .into_iter()
                        .flatten()
                        .filter(|button| match button {
                            WindowButton::Minimize => supported.minimize,
                            WindowButton::Maximize => supported.maximize,
                            WindowButton::Close => true,
                        })
                        .map(move |button| control(button, maximized, close_window.clone())),
                )
        };

        div()
            .id("window-title-bar")
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(BAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::sidebar_background())
            .child(side(layout.left, "window-controls-left"))
            .child(
                // Unlike Windows, dragging here is an ordinary mouse handler,
                // so it has to be a sibling of the buttons rather than their
                // parent: a press on a button would otherwise start a window
                // move before the click ever landed.
                div()
                    .id("window-title-bar-drag")
                    .flex_1()
                    .h_full()
                    .on_mouse_down(MouseButton::Left, |event, window, _cx| {
                        if event.click_count == 2 && window.window_controls().maximize {
                            window.zoom_window();
                        } else {
                            window.start_window_move();
                        }
                    }),
            )
            .child(side(layout.right, "window-controls-right"))
    }

    fn control(
        button: WindowButton,
        maximized: bool,
        close_window: CloseWindowHandler,
    ) -> impl IntoElement {
        let name = match button {
            WindowButton::Minimize => IconName::WindowMinimize,
            WindowButton::Maximize if maximized => IconName::WindowRestore,
            WindowButton::Maximize => IconName::WindowMaximize,
            WindowButton::Close => IconName::Close,
        };
        let group = SharedString::new_static(button.id());
        crate::keyboard::interactive(div().id(button.id()))
            .group(group.clone())
            .role(gpui::Role::Button)
            .aria_label(super::caption_label(button, maximized))
            .flex()
            .items_center()
            .justify_center()
            .size(px(BUTTON_SIZE))
            .flex_shrink_0()
            .rounded_full()
            .cursor_pointer()
            .bg(theme::surface())
            .hover(|style| style.bg(theme::surface_hover()))
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();
                match button {
                    WindowButton::Minimize => window.minimize_window(),
                    WindowButton::Maximize => window.zoom_window(),
                    // Not `remove_window`: closing has to persist the window
                    // bounds and respect `minimize_to_tray_on_close`, the same
                    // as Ctrl-W and the File menu item do.
                    WindowButton::Close => close_window(cx),
                }
            })
            .child(
                // See the Windows note: an SVG with no text color of its own is
                // simply not painted, so the glyph carries one and recolors via
                // a group style.
                icon(name, theme::subtext(), 12.)
                    .group_hover(group, |style| style.text_color(theme::text())),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_bar_height_matches_platform_decoration_ownership() {
        for client_decorated in [false, true] {
            assert_eq!(
                title_bar_height_for(ChromePlatform::MacOs, client_decorated),
                0.
            );
            assert_eq!(
                title_bar_height_for(ChromePlatform::Windows, client_decorated),
                32.
            );
            assert_eq!(
                title_bar_height_for(ChromePlatform::Other, client_decorated),
                0.
            );
        }
        assert_eq!(title_bar_height_for(ChromePlatform::Unix, false), 0.);
        assert_eq!(title_bar_height_for(ChromePlatform::Unix, true), 32.);
    }

    #[test]
    fn maximized_caption_uses_restore_label() {
        use crate::i18n::{k, t};
        assert_eq!(
            caption_label(gpui::WindowButton::Maximize, false),
            t(k::COMMON_WINDOW_MAXIMIZE)
        );
        assert_eq!(
            caption_label(gpui::WindowButton::Maximize, true),
            t(k::COMMON_WINDOW_RESTORE)
        );
        // Render code must type-check on every CI host, including macOS where
        // the runtime selects native traffic lights instead of this module.
        let _render = linux_chrome::title_bar;
    }
}
