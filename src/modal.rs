//! Modal geometry and focus ownership, independent of product actions.
//!
//! Keep one [`ModalFocusState`] per modal in the owning view. Call `sync` on
//! every render, including the render after dismissal, and attach the host's
//! [`Dismiss`] handler to [`overlay`]. Use [`card_in`] when the overlay lives in
//! a pane: its available size is the pane, not the whole application window.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{
    App, Div, ElementId, FocusHandle, IntoElement, KeyBinding, Pixels, SharedString, Size,
    Stateful, WeakFocusHandle, Window, actions, div, prelude::*, px,
};

use crate::{components, keyboard, theme};

const MODAL_CONTEXT: &str = "OcHubModal";
const EDGE_PADDING: f32 = 16.;

actions!(ochub_ui_modal, [Dismiss]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Dismiss, Some(MODAL_CONTEXT))]);
}

/// Focus belongs to the open modal until its owner closes it. The previous
/// focus is weak so keeping this state does not retain a dismissed editor.
pub struct ModalFocusState {
    scope: FocusHandle,
    previous: Option<WeakFocusHandle>,
    open: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    pending_initial: Rc<Cell<bool>>,
    initial: Rc<RefCell<Option<FocusHandle>>>,
}

impl ModalFocusState {
    pub fn new(cx: &App) -> Self {
        Self {
            scope: cx.focus_handle(),
            previous: None,
            open: Rc::new(Cell::new(false)),
            generation: Rc::new(Cell::new(0)),
            pending_initial: Rc::new(Cell::new(false)),
            initial: Rc::new(RefCell::new(None)),
        }
    }

    /// Synchronize with host visibility. An optional initial input is focused
    /// after the new tree is painted; otherwise the first tab stop is chosen.
    /// Closing restores the invoker only if this modal still owns focus, so a
    /// newly opened modal or an explicit host focus change is not overwritten.
    pub fn sync(
        &mut self,
        open: bool,
        initial_focus: Option<&FocusHandle>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.open.replace(open) == open {
            return;
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        if open {
            self.previous = window.focused(cx).map(|focus| focus.downgrade());
            *self.initial.borrow_mut() = initial_focus.cloned();
            self.pending_initial.set(true);
            self.scope.focus(window, cx);
        } else {
            self.pending_initial.set(false);
            self.initial.borrow_mut().take();
            let previous = self.previous.take().and_then(|focus| focus.upgrade());
            if self.scope.contains_focused(window, cx) {
                if let Some(previous) = previous {
                    previous.focus(window, cx);
                } else {
                    window.blur();
                }
            }
        }
    }

    /// The root handle is useful for host assertions and explicit focus moves.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.scope
    }
}

/// Move through GPUI's real tab order, accepting only descendants of the modal.
/// A visited set (rather than a fixed iteration limit) also covers empty and
/// dynamically changing dialogs. All moves finish before focus effects flush.
fn move_focus_within(scope: &FocusHandle, backwards: bool, window: &mut Window, cx: &mut App) {
    let mut visited = Vec::new();
    loop {
        if backwards {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        let Some(candidate) = window.focused(cx) else {
            break;
        };
        if candidate != *scope && scope.contains(&candidate, window) {
            return;
        }
        if visited.contains(&candidate) {
            break;
        }
        visited.push(candidate);
    }
    scope.focus(window, cx);
}

/// Pane-sized overlay. Its padding and the card's `max_w_full` constrain the
/// dialog even when a caller supplied the full window size to [`card`].
/// The owner attaches `.on_action(...)` for [`Dismiss`], keeping cancellation
/// policy (unsaved changes, busy state, etc.) in the application.
pub fn overlay(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    focus: &ModalFocusState,
    child: impl IntoElement,
) -> Stateful<Div> {
    let next_scope = focus.scope.clone();
    let previous_scope = focus.scope.clone();
    let initial_scope = focus.scope.clone();
    let pending_initial = focus.pending_initial.clone();
    let initial = focus.initial.clone();
    let active = focus.open.clone();
    let current_generation = focus.generation.clone();
    div()
        .on_children_prepainted(move |_, window, cx| {
            if !pending_initial.replace(false) {
                return;
            }
            let window_handle = window.window_handle();
            let scope = initial_scope.clone();
            let initial = initial.borrow().clone();
            let active = active.clone();
            let current_generation = current_generation.clone();
            let generation = current_generation.get();
            // Defer from prepaint, so the just-mounted modal's real focus tree
            // and tab order exist before choosing a child. This also works on
            // platforms which only request another frame when content changes.
            cx.defer(move |cx| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    if !active.get()
                        || current_generation.get() != generation
                        || !scope.is_focused(window)
                    {
                        return;
                    }
                    if let Some(initial) = initial.filter(|handle| scope.contains(handle, window)) {
                        initial.focus(window, cx);
                    } else {
                        move_focus_within(&scope, false, window, cx);
                    }
                });
            });
        })
        .id(id)
        .role(gpui::Role::Dialog)
        .aria_label(label)
        .absolute()
        .inset_0()
        .min_w_0()
        .min_h_0()
        .p(px(EDGE_PADDING))
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::scrim().alpha(if theme::is_dark() { 0.68 } else { 0.45 }))
        .occlude()
        .track_focus(&focus.scope)
        .tab_group()
        .key_context(MODAL_CONTEXT)
        .on_action(move |_: &keyboard::FocusNext, window, cx| {
            move_focus_within(&next_scope, false, window, cx);
        })
        .on_action(move |_: &keyboard::FocusPrevious, window, cx| {
            move_focus_within(&previous_scope, true, window, cx);
        })
        .child(child)
}

/// A bounded card for an overlay spanning this entire window.
pub fn card(window: &Window, preferred_width: Pixels) -> Div {
    card_in(window.viewport_size(), preferred_width)
}

/// A bounded card for a known containing pane. Header/footer stay in flow;
/// [`body`] takes the remaining height and scrolls independently.
pub fn card_in(available: Size<Pixels>, preferred_width: Pixels) -> Div {
    let max_width = (available.width - px(2. * EDGE_PADDING)).max(px(0.));
    let max_height = (available.height - px(2. * EDGE_PADDING)).max(px(0.));
    components::modal_card()
        .w(preferred_width.max(px(0.)).min(max_width))
        .max_w_full()
        .max_h(max_height)
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
}

/// Scrollable body with a stable identity, paired with [`card_in`].
pub fn body(id: impl Into<ElementId>) -> Stateful<Div> {
    components::modal_body()
        .id(id)
        .flex_shrink_1()
        .min_h_0()
        .min_w_0()
        .overflow_y_scroll()
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use gpui::{
        Bounds, Context, Entity, KeyUpEvent, Keystroke, Render, TestAppContext, VisualTestContext,
        size,
    };

    use super::*;
    use crate::components::{ButtonSize, ButtonTone};

    struct ModalProbe {
        open: bool,
        explicit_initial: bool,
        focus: ModalFocusState,
        opener: FocusHandle,
        first: FocusHandle,
        second: FocusHandle,
        after: FocusHandle,
        child_bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    }

    fn press(cx: &mut VisualTestContext, keys: &str) {
        for key in keys.split_whitespace() {
            cx.simulate_keystrokes(key);
            cx.simulate_event(KeyUpEvent {
                keystroke: Keystroke::parse(key).unwrap(),
            });
        }
    }

    impl Render for ModalProbe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.focus.sync(
                self.open,
                self.explicit_initial.then_some(&self.second),
                window,
                cx,
            );
            let bounds = self.child_bounds.clone();
            div()
                .relative()
                .size_full()
                .child(
                    components::button("open", "Open", ButtonTone::Neutral, ButtonSize::Sm)
                        .track_focus(&self.opener)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open = true;
                            cx.notify();
                        })),
                )
                .when(self.open, |root| {
                    root.child(
                        overlay(
                            "modal",
                            "Test dialog",
                            &self.focus,
                            card_in(size(px(708.), px(640.)), px(760.))
                                .on_children_prepainted(move |children, _, _| {
                                    *bounds.borrow_mut() = children;
                                })
                                .child(components::modal_header("Title"))
                                .child(body("body").children(
                                    (0..100).map(|_| div().h(px(30.)).flex_none().child("Content")),
                                ))
                                .child(components::modal_footer(vec![
                                    components::button(
                                        "first",
                                        "First",
                                        ButtonTone::Neutral,
                                        ButtonSize::Sm,
                                    )
                                    .track_focus(&self.first)
                                    .on_click(|_, _, _| {})
                                    .into_any_element(),
                                    components::button(
                                        "second",
                                        "Second",
                                        ButtonTone::Primary,
                                        ButtonSize::Sm,
                                    )
                                    .track_focus(&self.second)
                                    .on_click(|_, _, _| {})
                                    .into_any_element(),
                                ])),
                        )
                        .on_action(cx.listener(
                            |this, _: &Dismiss, _, cx| {
                                this.open = false;
                                cx.notify();
                            },
                        )),
                    )
                })
                .child(
                    components::button("after", "After", ButtonTone::Neutral, ButtonSize::Sm)
                        .track_focus(&self.after)
                        .on_click(|_, _, _| {}),
                )
        }
    }

    fn setup(cx: &mut TestAppContext) -> (Entity<ModalProbe>, &mut gpui::VisualTestContext) {
        cx.update(|cx| {
            keyboard::bind_keys(cx);
            bind_keys(cx);
        });
        cx.add_window_view(|window, cx| {
            let opener = cx.focus_handle().tab_stop(true);
            opener.focus(window, cx);
            ModalProbe {
                open: false,
                explicit_initial: true,
                focus: ModalFocusState::new(cx),
                opener,
                first: cx.focus_handle().tab_stop(true),
                second: cx.focus_handle().tab_stop(true),
                after: cx.focus_handle().tab_stop(true),
                child_bounds: Rc::new(RefCell::new(Vec::new())),
            }
        })
    }

    #[gpui::test]
    fn modal_sets_initial_focus_traps_both_directions_and_restores(cx: &mut TestAppContext) {
        let (probe, cx) = setup(cx);
        press(cx, "enter");
        cx.update(|window, cx| {
            assert!(probe.read(cx).second.is_focused(window));
        });
        press(cx, "tab");
        cx.update(|window, cx| {
            assert!(probe.read(cx).first.is_focused(window));
        });
        press(cx, "shift-tab");
        cx.update(|window, cx| {
            assert!(probe.read(cx).second.is_focused(window));
        });
        press(cx, "escape");
        cx.update(|window, cx| {
            let probe = probe.read(cx);
            assert!(!probe.open);
            assert!(probe.opener.is_focused(window));
        });
    }

    #[gpui::test]
    fn modal_defaults_to_first_control_and_preserves_later_focus(cx: &mut TestAppContext) {
        let (probe, cx) = setup(cx);
        probe.update(cx, |probe, _| probe.explicit_initial = false);
        press(cx, "enter");
        cx.update(|window, cx| {
            assert!(probe.read(cx).first.is_focused(window));
        });
        press(cx, "tab");
        probe.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            assert!(probe.read(cx).second.is_focused(window));
        });
    }

    #[gpui::test]
    fn long_modal_body_keeps_header_and_footer_inside_containing_pane(cx: &mut TestAppContext) {
        let (probe, cx) = setup(cx);
        cx.simulate_resize(size(px(708.), px(640.)));
        press(cx, "enter");
        probe.read_with(cx, |probe, _| {
            let bounds = probe.child_bounds.borrow();
            assert_eq!(bounds.len(), 3);
            for child in bounds.iter() {
                assert!(child.left() >= px(16.), "{child:?}");
                assert!(child.right() <= px(692.), "{child:?}");
                assert!(child.top() >= px(16.), "{child:?}");
                assert!(child.bottom() <= px(624.), "{child:?}");
            }
            assert!(bounds[1].size.height > px(0.));
            assert!(bounds[1].size.height < px(3000.));
        });
    }
}
