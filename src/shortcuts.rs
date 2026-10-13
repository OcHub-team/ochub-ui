//! Platform shortcuts and small interaction state shared by desktop hosts.
//!
//! Applications own command meaning and scope. The library owns matching,
//! labels, key-release pairing, and the motion used to reveal shortcut hints.

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use gpui::{
    Action, App, KeyBinding, KeybindingKeystroke, Keystroke, SharedString, Window, div, prelude::*,
};

use crate::anim::Transition;

/// One command's platform bindings, suitable for both GPUI actions and raw
/// terminal/canvas dispatch. Keep the same definition next to its command.
#[derive(Clone, Copy, Debug)]
pub struct Shortcut {
    macos: &'static str,
    other: &'static str,
}

impl Shortcut {
    pub const fn new(macos: &'static str, other: &'static str) -> Self {
        Self { macos, other }
    }

    pub const fn stroke(self) -> &'static str {
        if cfg!(target_os = "macos") {
            self.macos
        } else {
            self.other
        }
    }

    pub fn binding(self, action: impl Action, context: Option<&str>) -> KeyBinding {
        KeyBinding::new(self.stroke(), action, context)
    }

    pub fn matches(self, event: &Keystroke) -> bool {
        Keystroke::parse(self.stroke())
            .is_ok_and(|key| event.should_match(&KeybindingKeystroke::from_keystroke(key)))
    }

    /// Use GPUI's platform-aware modifier and key labels, including arrows.
    pub fn label(self) -> String {
        Keystroke::parse(self.stroke())
            .map(|key| key.to_string())
            .unwrap_or_else(|_| self.stroke().to_string())
    }
}

/// Decode a numbered-navigation key without mistaking F2 or a longer key name
/// for the digit 2. Some platforms report `digit2`/`numpad2`; IMEs can supply a
/// full-width digit in `key_char`.
pub fn navigation_digit(key: &str, key_char: Option<&str>) -> Option<usize> {
    fn digit(value: &str) -> Option<usize> {
        let value = value
            .strip_prefix("digit")
            .or_else(|| value.strip_prefix("numpad"))
            .unwrap_or(value);
        let mut chars = value.chars();
        let ch = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        if ch.is_ascii_digit() {
            return Some((ch as u8 - b'0') as usize);
        }
        "０１２３４５６７８９".chars().position(|digit| digit == ch)
    }
    digit(key).or_else(|| key_char.and_then(digit))
}

/// Readable help row. Shortcut text stays visible while a long translated
/// command wraps, and both remain available to accessibility clients.
pub fn help_row(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    keys: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    let keys = keys.into();
    div()
        .id(id)
        .role(gpui::Role::Label)
        .aria_label(format!("{label}: {keys}"))
        .flex()
        .items_start()
        .gap_4()
        .min_w_0()
        .py_2()
        .border_b_1()
        .border_color(crate::theme::border())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .text_color(crate::theme::text())
                .child(label),
        )
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(crate::theme::subtext())
                .child(keys),
        )
}

/// Pair consumed key-downs with their own key-ups. Modifier release order can
/// differ, so matching deliberately uses the physical key name, not modifiers.
#[derive(Debug, Default)]
pub struct KeyReleaseTracker {
    consumed: HashSet<String>,
}

impl KeyReleaseTracker {
    pub fn record(&mut self, key: &Keystroke) {
        self.consumed.insert(key.key.clone());
    }

    pub fn consume(&mut self, key: &Keystroke) -> bool {
        self.consumed.remove(&key.key)
    }

    /// Clear when a host loses its input session or focus.
    pub fn clear(&mut self) {
        self.consumed.clear();
    }
}

/// Reversible reveal used by hold-to-discover keyboard hints. The host keeps
/// the hint in reserved or absolute space, so revealing it cannot shift text.
#[derive(Clone, Debug)]
pub struct ShortcutReveal(Transition);

impl Default for ShortcutReveal {
    fn default() -> Self {
        Self(Transition::settled(0., Duration::from_millis(120)))
    }
}

impl ShortcutReveal {
    pub fn opacity(&mut self, visible: bool, window: &mut Window, cx: &App) -> f32 {
        let now = Instant::now();
        let reduced = cx.reduce_motion();
        self.0.retarget(if visible { 1. } else { 0. }, now, reduced);
        if self.0.is_animating(now, reduced) {
            window.request_animation_frame();
        }
        self.0.value(now, reduced)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_stay_paired_with_overlapping_keys_and_repeats() {
        let mut releases = KeyReleaseTracker::default();
        let shortcut = Keystroke::parse("cmd-t").unwrap();
        releases.record(&shortcut);
        releases.record(&shortcut);
        releases.record(&Keystroke::parse("ctrl-b").unwrap());
        assert!(!releases.consume(&Keystroke::parse("x").unwrap()));
        assert!(releases.consume(&Keystroke::parse("t").unwrap()));
        assert!(!releases.consume(&shortcut));
        assert!(releases.consume(&Keystroke::parse("b").unwrap()));
        releases.record(&shortcut);
        releases.clear();
        assert!(!releases.consume(&shortcut));
    }

    #[test]
    fn terminal_shortcuts_do_not_capture_plain_control_keys() {
        let shortcut = Shortcut::new("cmd-w", "ctrl-shift-w");
        assert!(shortcut.matches(&Keystroke::parse(shortcut.stroke()).unwrap()));
        assert!(!shortcut.matches(&Keystroke::parse("ctrl-w").unwrap()));
        assert!(!shortcut.matches(&Keystroke::parse("alt-w").unwrap()));
        assert!(!shortcut.label().is_empty());
    }

    #[test]
    fn numbered_navigation_excludes_function_keys() {
        for value in ["2", "digit2", "numpad2", "２"] {
            assert_eq!(navigation_digit(value, None), Some(2));
        }
        for value in ["f2", "f12", "page2", "12", "numpad12"] {
            assert_eq!(navigation_digit(value, None), None);
        }
        assert_eq!(navigation_digit("unidentified", Some("３")), Some(3));
    }
}
