//! Shared keyboard behavior for clickable controls and window tab navigation.
//!
//! GPUI synthesizes a click from a matched Enter/Space press and release for a
//! focused element. Keep activation there, rather than adding a second callback
//! which could run an action twice. Hosts can apply [`interactive`] to their own
//! navigation rows without changing their existing `on_click` handlers.

use gpui::{App, BoxShadow, Div, KeyBinding, Stateful, actions, prelude::*, px};

use crate::theme;

actions!(ochub_ui_keyboard, [FocusNext, FocusPrevious]);
const CONTROL_CONTEXT: &str = "OcHubControl";

/// Add a tab stop and a visible keyboard focus ring without changing geometry.
/// Only apply this to enabled controls that have an activation handler.
pub fn interactive(control: Stateful<Div>) -> Stateful<Div> {
    control
        .focusable()
        .tab_stop(true)
        .key_context(CONTROL_CONTEXT)
        .focus_visible(|style| {
            style.shadow(vec![
                BoxShadow::new(px(0.), px(0.), theme::accent().into()).spread_radius(px(2.)),
            ])
        })
}

/// Install a window-wide tab fallback. A modal may handle these actions first,
/// and editors may keep a more specific Tab binding for inserting indentation.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
        // A focused child control owns activation even when its parent is a
        // SettingsRow. Let GPUI deliver the unbound press/release to on_click.
        KeyBinding::new("enter", gpui::NoAction, Some(CONTROL_CONTEXT)),
        KeyBinding::new("space", gpui::NoAction, Some(CONTROL_CONTEXT)),
    ]);
    cx.on_action(|_: &FocusNext, cx| {
        if let Some(window) = cx.active_window() {
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| window.focus_next(cx));
            });
        }
    });
    cx.on_action(|_: &FocusPrevious, cx| {
        if let Some(window) = cx.active_window() {
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| window.focus_prev(cx));
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use gpui::{
        Context, KeyUpEvent, Keystroke, Render, TestAppContext, VisualTestContext, Window, div,
        prelude::*,
    };

    use crate::{components, layout};

    struct ControlsProbe {
        activations: Vec<&'static str>,
        selection: usize,
    }

    fn press(cx: &mut VisualTestContext, keys: &str) {
        for key in keys.split_whitespace() {
            // GPUI's simulate_keystrokes sends only key-down; activation also
            // requires the matching key-up, just as the native platform does.
            cx.simulate_keystrokes(key);
            cx.simulate_event(KeyUpEvent {
                keystroke: Keystroke::parse(key).unwrap(),
            });
        }
    }

    impl Render for ControlsProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .child(
                    components::button(
                        "first",
                        "First",
                        components::ButtonTone::Neutral,
                        components::ButtonSize::Sm,
                    )
                    .on_click(cx.listener(|this, _, _, _| this.activations.push("first"))),
                )
                .child(components::disabled_button(
                    "disabled",
                    "Disabled",
                    components::ButtonTone::Neutral,
                    components::ButtonSize::Sm,
                    true,
                ))
                .child(layout::action_row(
                    "action",
                    "Action row",
                    None,
                    "Run",
                    components::ButtonTone::Neutral,
                    false,
                    {
                        let view = cx.entity();
                        move |_, cx| {
                            view.update(cx, |this, _| this.activations.push("row"));
                        }
                    },
                ))
                .child(layout::select_row(
                    "choice",
                    "Choice",
                    None,
                    &["A", "B"],
                    self.selection,
                    layout::SelectRowState::new(false, false),
                    {
                        let view = cx.entity();
                        move |event, _, cx| {
                            if let layout::SelectRowEvent::Select(index) = event {
                                view.update(cx, |this, cx| {
                                    this.selection = index;
                                    cx.notify();
                                });
                            }
                        }
                    },
                ))
                .child(layout::switch_row(
                    "switch",
                    "Switch",
                    None,
                    false,
                    false,
                    {
                        let view = cx.entity();
                        move |_, cx| {
                            view.update(cx, |this, _| this.activations.push("switch"));
                        }
                    },
                ))
        }
    }

    #[gpui::test]
    fn tab_skips_disabled_and_nested_controls_activate_once(cx: &mut TestAppContext) {
        cx.update(|cx| {
            super::bind_keys(cx);
            crate::text_input::bind_keys(cx);
            layout::bind_keys(cx);
        });
        let (probe, cx) = cx.add_window_view(|_, _| ControlsProbe {
            activations: Vec::new(),
            selection: 1,
        });
        cx.update(|window, _| window.activate_window());

        press(cx, "tab");
        cx.update(|window, cx| {
            assert!(cx.active_window().is_some(), "test window must be active");
            assert!(window.focused(cx).is_some(), "Tab must move focus");
        });
        press(cx, "enter space");
        probe.read_with(cx, |probe, _| {
            assert_eq!(probe.activations, ["first", "first"]);
        });

        // The disabled button is skipped. Both the row and its actual button
        // can activate, but neither action must fire the other's handler too.
        press(cx, "tab enter tab enter");
        probe.read_with(cx, |probe, _| {
            assert_eq!(probe.activations, ["first", "first", "row", "row"]);
        });

        // Skip the select-row container and focus A. The parent's cycle action
        // would choose B here, so this verifies child activation ownership.
        press(cx, "tab tab enter");
        probe.read_with(cx, |probe, _| assert_eq!(probe.selection, 0));
        press(cx, "tab space");
        probe.read_with(cx, |probe, _| assert_eq!(probe.selection, 1));
    }

    #[gpui::test]
    fn settings_rows_keyboard_fallback_does_not_double_activate(cx: &mut TestAppContext) {
        // Deliberately omit layout::bind_keys: consumers may use row builders
        // without the library installer, and the documented fallback must work.
        cx.update(super::bind_keys);
        let (probe, cx) = cx.add_window_view(|_, _| ControlsProbe {
            activations: Vec::new(),
            selection: 1,
        });
        cx.update(|window, _| window.activate_window());
        // Seven stops: First, action row, Run, choice row, A, B, switch.
        press(cx, "tab tab tab tab tab tab tab enter space");
        probe.read_with(cx, |probe, _| {
            assert_eq!(probe.activations, ["switch", "switch"]);
        });
    }
}
