//! Single selection controls own transient focus and keyboard state here.
//! Hosts own the selected value and whether the popup is open.

use crate::{
    icons::{IconName, icon},
    keyboard, theme,
};
use gpui::{
    Anchor, App, Bounds, ElementId, Entity, FocusHandle, FontWeight, IntoElement, KeyBinding,
    Pixels, RenderOnce, ScrollHandle, SharedString, Window, WindowAppearance, actions, anchored,
    deferred, div, point, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

// ── Select dropdown ────────────────────────────────────────────────────────

/// A selection control should stop laying every option out horizontally once
/// either the list itself is long or the labels would make a compact list wrap.
///
/// The weighted character count treats non-ASCII glyphs as roughly two Latin
/// characters, which is close enough for deciding between the two controls
/// without coupling schema data to a particular font or window width.
pub fn select_prefers_dropdown(options: &[&str]) -> bool {
    if options.len() >= 5 {
        return true;
    }
    if options.len() < 3 {
        return false;
    }
    let label_units: usize = options
        .iter()
        .map(|label| {
            label
                .chars()
                .map(|character| if character.is_ascii() { 1 } else { 2 })
                .sum::<usize>()
        })
        .sum();
    label_units >= 32
}

/// The two state changes emitted by [`select_dropdown`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectDropdownEvent {
    Open(bool),
    Select(usize),
}

type SelectDropdownHandler = Rc<dyn Fn(SelectDropdownEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy)]
enum SelectDropdownChrome {
    Field,
    Sidebar(WindowAppearance),
}

/// A compact single-select trigger with a popover list. The owner keeps the
/// open state so virtualized forms can unmount/remount the control without
/// hiding mutable state inside a short-lived element.
pub fn select_dropdown(
    id: impl Into<SharedString>,
    options: &[&str],
    selected: usize,
    open: bool,
    on_event: impl Fn(SelectDropdownEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    select_dropdown_control(
        id.into(),
        options,
        selected,
        open,
        None,
        SelectDropdownChrome::Field,
        Some(Rc::new(on_event)),
    )
}

/// Sidebar counterpart of [`select_dropdown`].
///
/// The trigger deliberately has no field chrome: it rests directly on the
/// sidebar backdrop and only gains the same hover/open highlight as navigation
/// rows. The popover menu remains identical to the regular selector.
pub fn select_dropdown_sidebar(
    id: impl Into<SharedString>,
    options: &[&str],
    selected: usize,
    open: bool,
    appearance: WindowAppearance,
    on_event: impl Fn(SelectDropdownEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    select_dropdown_control(
        id.into(),
        options,
        selected,
        open,
        None,
        SelectDropdownChrome::Sidebar(appearance),
        Some(Rc::new(on_event)),
    )
}

/// [`select_dropdown`] with guidance text for an unselected value.
///
/// The placeholder is shown only in the trigger and is never inserted into the
/// option list, so required selectors cannot accidentally persist a fake
/// "empty" choice.
pub fn select_dropdown_with_placeholder(
    id: impl Into<SharedString>,
    options: &[&str],
    selected: usize,
    open: bool,
    placeholder: impl Into<SharedString>,
    on_event: impl Fn(SelectDropdownEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    select_dropdown_control(
        id.into(),
        options,
        selected,
        open,
        Some(placeholder.into()),
        SelectDropdownChrome::Field,
        Some(Rc::new(on_event)),
    )
}

/// Read-only counterpart used by disabled settings rows.
pub fn select_dropdown_readonly(
    id: impl Into<SharedString>,
    options: &[&str],
    selected: usize,
) -> gpui::Stateful<gpui::Div> {
    select_dropdown_control(
        id.into(),
        options,
        selected,
        false,
        None,
        SelectDropdownChrome::Field,
        None,
    )
}

const SELECT_CONTEXT: &str = "OcHubSelect";
actions!(ochub_ui_select, [Next, Previous, First, Last, Dismiss]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", Next, Some(SELECT_CONTEXT)),
        KeyBinding::new("up", Previous, Some(SELECT_CONTEXT)),
        KeyBinding::new("home", First, Some(SELECT_CONTEXT)),
        KeyBinding::new("end", Last, Some(SELECT_CONTEXT)),
        KeyBinding::new("enter", gpui::NoAction, Some(SELECT_CONTEXT)),
        KeyBinding::new("space", gpui::NoAction, Some(SELECT_CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(SELECT_CONTEXT)),
    ]);
}

struct SelectState {
    focus: FocusHandle,
    was_open: bool,
    active: usize,
    trigger_bounds: Rc<Cell<Bounds<Pixels>>>,
    scroll: ScrollHandle,
}

#[derive(IntoElement)]
struct SelectControl {
    id: SharedString,
    options: Vec<SharedString>,
    selected: usize,
    open: bool,
    placeholder: Option<SharedString>,
    chrome: SelectDropdownChrome,
    on_event: Option<SelectDropdownHandler>,
}

fn select_dropdown_control(
    id: SharedString,
    options: &[&str],
    selected: usize,
    open: bool,
    placeholder: Option<SharedString>,
    chrome: SelectDropdownChrome,
    on_event: Option<SelectDropdownHandler>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.clone())
        .w_full()
        .min_w_0()
        .child(SelectControl {
            id,
            options: options
                .iter()
                .map(|s| SharedString::from((*s).to_owned()))
                .collect(),
            selected,
            open,
            placeholder,
            chrome,
            on_event,
        })
}

impl RenderOnce for SelectControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| SelectState {
            focus: cx.focus_handle().tab_stop(true),
            was_open: false,
            active: 0,
            trigger_bounds: Rc::new(Cell::new(Bounds::default())),
            scroll: ScrollHandle::new(),
        });
        let mut open = self.open;
        state.update(cx, |state, cx| {
            if open && !state.was_open {
                state.active = self.selected.min(self.options.len().saturating_sub(1));
                state.focus.focus(window, cx);
            }
            state.was_open = open;
        });
        // A newly opened dialog or another focused field owns interaction now.
        // Retire a previous popup before its deferred layer can cover that view.
        if open && !state.read(cx).focus.is_focused(window) {
            open = false;
            if let Some(handler) = self.on_event.clone() {
                let handle = window.window_handle();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        handler(SelectDropdownEvent::Open(false), window, cx)
                    });
                });
            }
        }
        render_control(&self, open, state, cx)
    }
}

fn render_control(
    control: &SelectControl,
    open: bool,
    state: Entity<SelectState>,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let id = control.id.clone();
    let options: Vec<&str> = control.options.iter().map(|s| s.as_ref()).collect();
    let selected = control.selected;
    let placeholder = control.placeholder.clone();
    let chrome = control.chrome;
    let on_event = control.on_event.clone();
    let focus = state.read(cx).focus.clone();
    let trigger_bounds = state.read(cx).trigger_bounds.clone();
    let active = state.read(cx).active;
    let scroll = state.read(cx).scroll.clone();
    let has_selection = options.get(selected).is_some();
    let current = options
        .get(selected)
        .map(|option| SharedString::from((*option).to_string()))
        .or(placeholder)
        .unwrap_or_default();
    let mut trigger = div()
        .id(ElementId::Name(format!("{id}-trigger").into()))
        .role(gpui::Role::ComboBox)
        .aria_label(current.clone())
        .aria_expanded(open)
        .debug_selector({
            let id = id.clone();
            move || format!("{id}-trigger")
        })
        .w_full()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .rounded_lg()
        .text_sm()
        .child(div().min_w_0().flex_1().truncate().child(current))
        .child(icon(IconName::ChevronDown, theme::muted(), 13.));
    trigger = match chrome {
        SelectDropdownChrome::Field => trigger
            .h(px(38.))
            .px_3()
            .border_1()
            .border_color(if open {
                theme::accent()
            } else {
                theme::border_strong()
            })
            .bg(theme::surface())
            .text_color(if has_selection {
                theme::text()
            } else {
                theme::muted()
            }),
        SelectDropdownChrome::Sidebar(appearance) => trigger
            .h(px(34.))
            .px_2()
            .text_color(if has_selection {
                theme::sidebar_glass_text(appearance)
            } else {
                theme::sidebar_glass_muted(appearance)
            })
            .when(open, |style| {
                style
                    .bg(theme::accent_soft())
                    .font_weight(FontWeight::MEDIUM)
            }),
    };
    if let Some(handler) = on_event.clone() {
        trigger = keyboard::interactive(trigger)
            .track_focus(&focus)
            .tab_stop(true)
            .key_context(SELECT_CONTEXT)
            .cursor_pointer();
        trigger = match chrome {
            SelectDropdownChrome::Field => {
                trigger.hover(|style| style.border_color(theme::accent()).bg(theme::panel()))
            }
            SelectDropdownChrome::Sidebar(_) => trigger.hover(|style| {
                style
                    .bg(theme::surface_hover())
                    .text_color(theme::sidebar_text())
            }),
        };
        let click_focus = focus.clone();
        let click_state = state.clone();
        let option_count = options.len();
        trigger = trigger.on_click(move |event, window, cx| {
            click_focus.focus(window, cx);
            if open && matches!(event, gpui::ClickEvent::Keyboard(_)) && option_count > 0 {
                handler(
                    SelectDropdownEvent::Select(click_state.read(cx).active.min(option_count - 1)),
                    window,
                    cx,
                );
            } else {
                handler(SelectDropdownEvent::Open(!open), window, cx);
            }
        });
    }

    let mut control = div()
        .on_children_prepainted({
            let bounds = trigger_bounds.clone();
            move |children, _, _| {
                if let Some(first) = children.first() {
                    bounds.set(*first);
                }
            }
        })
        .id(ElementId::Name(format!("{id}-control").into()))
        .relative()
        .w_full()
        .min_w_0()
        .child(trigger);
    let Some(handler) = on_event else {
        return control;
    };
    let count = options.len();
    for (action, target) in [
        (0, 0),
        (1, count.saturating_sub(1)),
        (2, active.saturating_sub(1)),
        (3, (active + 1).min(count.saturating_sub(1))),
    ] {
        let state = state.clone();
        let handler = handler.clone();
        let navigate = move |window: &mut Window, cx: &mut App| {
            if !open {
                handler(SelectDropdownEvent::Open(true), window, cx);
            }
            state.update(cx, |state, cx| {
                state.was_open = true;
                state.active = target;
                state.scroll.scroll_to_item(target);
                cx.notify();
            });
        };
        control = match action {
            0 => control.on_action(move |_: &First, w, c| navigate(w, c)),
            1 => control.on_action(move |_: &Last, w, c| navigate(w, c)),
            2 => control.on_action(move |_: &Previous, w, c| navigate(w, c)),
            _ => control.on_action(move |_: &Next, w, c| navigate(w, c)),
        };
    }
    control = control
        .on_action({
            let handler = handler.clone();
            move |_: &Dismiss, window, cx| {
                if open {
                    handler(SelectDropdownEvent::Open(false), window, cx);
                } else {
                    cx.propagate();
                }
            }
        })
        .on_action({
            let handler = handler.clone();
            move |_: &keyboard::FocusNext, window, cx| {
                if open {
                    handler(SelectDropdownEvent::Open(false), window, cx);
                }
                cx.propagate();
            }
        })
        .on_action({
            let handler = handler.clone();
            move |_: &keyboard::FocusPrevious, window, cx| {
                if open {
                    handler(SelectDropdownEvent::Open(false), window, cx);
                }
                cx.propagate();
            }
        });
    if !open {
        return control;
    }

    let mut menu = div()
        .id(ElementId::Name(format!("{id}-menu").into()))
        .role(gpui::Role::ListBox)
        .track_scroll(&scroll)
        .w_full()
        .min_w(px(220.))
        .max_h(px(280.))
        .overflow_y_scroll()
        .p_1()
        .rounded_lg()
        .border_1()
        .border_color(theme::border())
        .bg(theme::overlay())
        .shadow(theme::shadow_popover())
        .occlude();
    for (index, option) in options.iter().enumerate() {
        let is_selected = index == selected;
        let callback = handler.clone();
        let mut item = div()
            .id(ElementId::Name(format!("{id}-option-{index}").into()))
            .role(gpui::Role::ListBoxOption)
            .debug_selector({
                let id = id.clone();
                move || format!("{id}-option-{index}")
            })
            .aria_label(SharedString::from((*option).to_string()))
            .aria_selected(is_selected)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .w_full()
            .min_h(px(34.))
            .px_3()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(SharedString::from((*option).to_string())),
            );
        if is_selected {
            item = item
                .bg(theme::accent_soft())
                .text_color(theme::accent())
                .font_weight(FontWeight::MEDIUM)
                .child(icon(IconName::Check, theme::accent(), 13.));
        } else {
            item = item
                .text_color(theme::subtext())
                .hover(|style| style.bg(theme::surface_hover()).text_color(theme::text()));
        }
        if index == active {
            item = item.bg(theme::surface_hover());
        }
        let state = state.clone();
        let focus = focus.clone();
        menu = menu.child(
            item.on_hover(move |hovered, _, cx| {
                if *hovered {
                    state.update(cx, |state, cx| {
                        state.active = index;
                        cx.notify();
                    });
                }
            })
            .on_click(move |_event, window, cx| {
                focus.focus(window, cx);
                callback(SelectDropdownEvent::Select(index), window, cx)
            }),
        );
    }
    let dismiss = handler;
    menu = menu.on_mouse_down_out(move |event, window, cx| {
        // The trigger handles its own toggle on click. Closing it on mouse-down
        // would rebuild the handler as an opener before mouse-up arrives.
        if !trigger_bounds.get().contains(&event.position) {
            dismiss(SelectDropdownEvent::Open(false), window, cx);
        }
    });

    control = control.child(
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .offset(point(px(0.), px(4.)))
                .snap_to_window_with_margin(px(8.))
                .child(menu),
        )
        .priority(30),
    );
    control
}
