use crate::{components, keyboard, modal};
use gpui::{
    Context, KeyUpEvent, Keystroke, Render, TestAppContext, VisualTestContext, Window, div,
    prelude::*, px,
};

struct Probe {
    open: bool,
    selected: usize,
    modal_open: bool,
    focus: modal::ModalFocusState,
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.focus.sync(self.modal_open, None, window, cx);
        let handler = cx.listener(|this, event: &components::SelectDropdownEvent, _, cx| {
            match event {
                components::SelectDropdownEvent::Open(open) => this.open = *open,
                components::SelectDropdownEvent::Select(index) => {
                    this.selected = *index;
                    this.open = false;
                }
            }
            cx.notify();
        });
        div()
            .relative()
            .size_full()
            .child(div().w(px(240.)).m_4().child(components::select_dropdown(
                "choice",
                &["Alpha", "Beta", "Gamma", "Delta", "Epsilon"],
                self.selected,
                self.open,
                move |event, window, cx| handler(&event, window, cx),
            )))
            .when(self.modal_open, |root| {
                root.child(modal::overlay(
                    "top-dialog",
                    "Top dialog",
                    &self.focus,
                    modal::card(window, px(280.)).child(
                        components::button(
                            "cancel",
                            "Cancel",
                            components::ButtonTone::Neutral,
                            components::ButtonSize::Sm,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.modal_open = false;
                            cx.notify();
                        })),
                    ),
                ))
            })
    }
}

fn press(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).unwrap(),
    });
}

#[gpui::test]
fn repeated_trigger_click_should_close(cx: &mut TestAppContext) {
    cx.update(keyboard::bind_keys);
    let (probe, cx) = cx.add_window_view(|_, cx| Probe {
        open: false,
        selected: 0,
        modal_open: false,
        focus: modal::ModalFocusState::new(cx),
    });
    cx.update(|window, _| window.activate_window());
    let point = cx.debug_bounds("choice-trigger").unwrap().center();
    cx.simulate_click(point, Default::default());
    probe.read_with(cx, |probe, _| assert!(probe.open));
    let point = cx.debug_bounds("choice-trigger").unwrap().center();
    cx.simulate_click(point, Default::default());
    probe.read_with(cx, |probe, _| {
        assert!(!probe.open, "clicking the open trigger should close it")
    });
}

#[gpui::test]
fn open_select_should_support_end_and_enter(cx: &mut TestAppContext) {
    cx.update(keyboard::bind_keys);
    let (probe, cx) = cx.add_window_view(|_, cx| Probe {
        open: false,
        selected: 0,
        modal_open: false,
        focus: modal::ModalFocusState::new(cx),
    });
    cx.update(|window, _| window.activate_window());
    press(cx, "tab");
    press(cx, "space");
    probe.read_with(cx, |probe, _| assert!(probe.open));
    press(cx, "end");
    press(cx, "enter");
    probe.read_with(cx, |probe, _| {
        assert_eq!(
            probe.selected, 4,
            "End + Enter should choose the final option"
        )
    });
}

#[gpui::test]
fn open_select_should_support_arrows_and_enter(cx: &mut TestAppContext) {
    cx.update(keyboard::bind_keys);
    let (probe, cx) = cx.add_window_view(|_, cx| Probe {
        open: false,
        selected: 0,
        modal_open: false,
        focus: modal::ModalFocusState::new(cx),
    });
    cx.update(|window, _| window.activate_window());
    press(cx, "tab");
    press(cx, "space");
    press(cx, "down");
    press(cx, "down");
    press(cx, "enter");
    probe.read_with(cx, |probe, _| {
        assert!(
            probe.selected > 0,
            "arrows should move beyond the initial option"
        )
    });
}

#[gpui::test]
fn older_select_should_be_inert_below_a_new_modal(cx: &mut TestAppContext) {
    cx.update(keyboard::bind_keys);
    cx.update(modal::bind_keys);
    let (probe, cx) = cx.add_window_view(|_, cx| Probe {
        open: true,
        selected: 0,
        modal_open: false,
        focus: modal::ModalFocusState::new(cx),
    });
    cx.update(|window, _| window.activate_window());
    let point = cx.debug_bounds("choice-option-1").unwrap().center();
    probe.update(cx, |probe, cx| {
        probe.modal_open = true;
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click(point, Default::default());
    probe.read_with(cx, |probe, _| {
        assert_eq!(
            probe.selected, 0,
            "a dropdown behind the modal must not receive clicks"
        )
    });
}

gpui::actions!(interaction_audit, [BackgroundSave]);

struct NotificationProbe {
    host: gpui::Entity<crate::notifications::NotificationHost>,
    saves: usize,
}

impl Render for NotificationProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .key_context("AuditRoot")
            .on_action(cx.listener(|this, _: &BackgroundSave, _, _| this.saves += 1))
            .child(self.host.clone())
    }
}

#[gpui::test]
fn notification_center_should_not_forward_parent_save(cx: &mut TestAppContext) {
    cx.update(|cx| {
        keyboard::bind_keys(cx);
        modal::bind_keys(cx);
        crate::notifications::bind_keys(cx);
        cx.bind_keys([gpui::KeyBinding::new(
            "cmd-s",
            BackgroundSave,
            Some("AuditRoot"),
        )]);
    });
    let (probe, cx) = cx.add_window_view(|_, cx| NotificationProbe {
        host: cx.new(|cx| {
            let mut host = crate::notifications::NotificationHost::new();
            host.info("Saved", cx);
            host.open_center(cx);
            host
        }),
        saves: 0,
    });
    cx.update(|window, _| window.activate_window());
    press(cx, "cmd-s");
    probe.read_with(cx, |probe, _| {
        assert_eq!(
            probe.saves, 0,
            "modal center must not activate underlying form save"
        )
    });
}
