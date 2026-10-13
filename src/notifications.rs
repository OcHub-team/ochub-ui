//! Short, non-blocking notifications with an in-memory detail center.
//!
//! Application code owns severity, redaction, environment labels and retry
//! policy. This module never writes notification contents to disk and never
//! turns message text into a business action.

gpui::actions!(ochub_ui_notifications, [BlockFormSubmit]);

/// A read-only notification overlay must not submit forms behind it.
pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-s", BlockFormSubmit, Some("OcHubNotificationCenter")),
        gpui::KeyBinding::new("ctrl-s", BlockFormSubmit, Some("OcHubNotificationCenter")),
        gpui::KeyBinding::new(
            "escape",
            crate::modal::Dismiss,
            Some("OcHubNotificationCenter"),
        ),
    ]);
}

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use gpui::{
    Animation, AnimationExt, ClipboardItem, Context, FocusHandle, IntoElement, ScrollHandle,
    SharedString, Window, div, prelude::*, px,
};

use crate::components::{self, ButtonSize, ButtonTone};
use crate::i18n::{k, t};
use crate::icons::{IconName, icon};
use crate::{
    keyboard, modal, theme,
    tokens::{self, ConsoleStyle, TextRole},
};

const MAX_VISIBLE: usize = 2;
const MAX_HISTORY: usize = 64;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(4);
const ERROR_TIMEOUT: Duration = Duration::from_secs(6);
const TOAST_WIDTH: f32 = 360.;
const TOAST_MARGIN: f32 = 16.;
const TOAST_ENTRY: Duration = Duration::from_millis(140);
const LEGACY_TITLE_LIMIT: usize = 80;
const SUMMARY_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationLevel {
    fn accent_icon(self) -> (gpui::Rgba, IconName) {
        match self {
            Self::Info => (theme::muted(), IconName::Message),
            Self::Success => (theme::green(), IconName::Check),
            Self::Warning => (theme::yellow(), IconName::Diamond),
            Self::Error => (theme::red(), IconName::Close),
        }
    }

    fn default_timeout(self) -> Duration {
        if self == Self::Error {
            ERROR_TIMEOUT
        } else {
            DEFAULT_TIMEOUT
        }
    }

    fn short_title(self) -> SharedString {
        t(match self {
            Self::Info => k::NOTIFICATIONS_KIND_INFO,
            Self::Success => k::NOTIFICATIONS_KIND_SUCCESS,
            Self::Warning => k::NOTIFICATIONS_KIND_WARNING,
            Self::Error => k::NOTIFICATIONS_KIND_ERROR,
        })
    }
}

/// The inset is measured from the selected edge of the full content viewport.
/// Hosts can reserve their own title bar or fixed editor footer without the
/// component knowing the application's current page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToastAnchor {
    TopRight { inset: f32 },
    BottomRight { inset: f32 },
}

impl Default for ToastAnchor {
    fn default() -> Self {
        Self::TopRight { inset: 72. }
    }
}

#[derive(Clone)]
pub struct Notification {
    id: u64,
    key: Option<SharedString>,
    level: NotificationLevel,
    title: SharedString,
    summary: Option<SharedString>,
    details: Option<SharedString>,
    source: Option<SharedString>,
    full_text: SharedString,
    updated_at: i64,
    occurrences: u32,
    read: bool,
    remaining: Option<Duration>,
    countdown_started_at: Option<Instant>,
    timer_epoch: u64,
}

impl Notification {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn level(&self) -> NotificationLevel {
        self.level
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
    pub fn full_text(&self) -> &str {
        &self.full_text
    }
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }
    pub fn occurrences(&self) -> u32 {
        self.occurrences
    }
    pub fn is_read(&self) -> bool {
        self.read
    }

    fn hold(&mut self, now: Instant) {
        if let (Some(remaining), Some(started_at)) =
            (self.remaining.as_mut(), self.countdown_started_at.take())
        {
            *remaining = remaining.saturating_sub(now.saturating_duration_since(started_at));
        }
    }

    fn resume(&mut self, now: Instant) -> Option<Duration> {
        let remaining = self.remaining.filter(|remaining| !remaining.is_zero())?;
        self.countdown_started_at = Some(now);
        Some(remaining)
    }
}

pub struct NotificationRequest {
    level: NotificationLevel,
    title: SharedString,
    summary: Option<SharedString>,
    details: Option<SharedString>,
    source: Option<SharedString>,
    key: Option<SharedString>,
    persistent: bool,
    timeout: Option<Duration>,
}

impl NotificationRequest {
    pub fn new(level: NotificationLevel, title: impl Into<SharedString>) -> Self {
        Self {
            level,
            title: title.into(),
            summary: None,
            details: None,
            source: None,
            key: None,
            persistent: false,
            timeout: None,
        }
    }

    /// A brief human-readable explanation. Long text remains available in the
    /// center and is shortened only in the transient preview.
    pub fn summary(mut self, summary: impl Into<SharedString>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Detailed, already redacted diagnostic text. Never displayed in a toast.
    pub fn details(mut self, details: impl Into<SharedString>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Compatibility with previous hosts: message text belongs in the center.
    pub fn message(self, message: impl Into<SharedString>) -> Self {
        self.details(message)
    }

    /// A display-only environment/app label supplied by the host.
    pub fn source(mut self, source: impl Into<SharedString>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Replace an earlier record for the same operation. Include environment
    /// identity in keys; unrelated operations should not share one.
    pub fn key(mut self, key: impl Into<SharedString>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

pub struct NotificationHost {
    next_id: u64,
    next_timer_epoch: u64,
    visible: VecDeque<Notification>,
    history: VecDeque<Notification>,
    hovered: bool,
    focused: bool,
    focus_scope: Option<FocusHandle>,
    anchor: ToastAnchor,
    center_open: bool,
    selected: Option<u64>,
    center_focus: Option<modal::ModalFocusState>,
    center_close_focus: Option<FocusHandle>,
    center_refocus: bool,
    center_scroll: ScrollHandle,
}

fn toast_width(viewport_width: f32) -> f32 {
    (viewport_width - TOAST_MARGIN * 2.).clamp(0., TOAST_WIDTH)
}

fn auto_dismiss_timeout(
    level: NotificationLevel,
    persistent: bool,
    timeout: Option<Duration>,
) -> Option<Duration> {
    (!persistent).then(|| timeout.unwrap_or_else(|| level.default_timeout()))
}

fn short_preview(text: &str, limit: usize) -> SharedString {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= limit {
        return flat.into();
    }
    format!(
        "{}…",
        flat.chars()
            .take(limit.saturating_sub(1))
            .collect::<String>()
    )
    .into()
}

fn request_record(request: NotificationRequest, id: u64, held: bool) -> Notification {
    let timeout = auto_dismiss_timeout(request.level, request.persistent, request.timeout);
    let full_text = [
        Some(request.title.as_ref()),
        request.summary.as_deref(),
        request.details.as_deref(),
        request.source.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n")
    .into();
    let legacy_long =
        request.title.chars().count() > LEGACY_TITLE_LIMIT || request.title.contains(['\n', '\r']);
    let title = if legacy_long {
        request.level.short_title()
    } else {
        request.title.clone()
    };
    let details = if legacy_long {
        Some(
            [Some(request.title.as_ref()), request.details.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n\n")
                .into(),
        )
    } else {
        request.details
    };
    Notification {
        id,
        key: request.key,
        level: request.level,
        title,
        summary: request.summary,
        details,
        source: request.source,
        full_text,
        updated_at: chrono::Local::now().timestamp(),
        occurrences: 1,
        read: false,
        remaining: timeout,
        countdown_started_at: timeout.filter(|_| !held).map(|_| Instant::now()),
        timer_epoch: 0,
    }
}

impl NotificationHost {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            next_timer_epoch: 1,
            visible: VecDeque::new(),
            history: VecDeque::new(),
            hovered: false,
            focused: false,
            focus_scope: None,
            anchor: ToastAnchor::default(),
            center_open: false,
            selected: None,
            center_focus: None,
            center_close_focus: None,
            center_refocus: false,
            center_scroll: ScrollHandle::new(),
        }
    }

    pub fn info(&mut self, title: impl Into<SharedString>, cx: &mut Context<Self>) -> u64 {
        self.notify(NotificationRequest::new(NotificationLevel::Info, title), cx)
    }
    pub fn success(&mut self, title: impl Into<SharedString>, cx: &mut Context<Self>) -> u64 {
        self.notify(
            NotificationRequest::new(NotificationLevel::Success, title),
            cx,
        )
    }
    pub fn warning(
        &mut self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.notify(
            NotificationRequest::new(NotificationLevel::Warning, title).details(message),
            cx,
        )
    }
    pub fn error(
        &mut self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.notify(
            NotificationRequest::new(NotificationLevel::Error, title).details(message),
            cx,
        )
    }
    pub fn status_leveled(
        &mut self,
        level: Option<NotificationLevel>,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.notify(
            NotificationRequest::new(level.unwrap_or(NotificationLevel::Info), message),
            cx,
        )
    }

    pub fn set_anchor(&mut self, anchor: ToastAnchor, cx: &mut Context<Self>) {
        self.anchor = anchor;
        cx.notify();
    }
    pub fn unread_count(&self) -> usize {
        self.history.iter().filter(|record| !record.read).count()
    }
    pub fn is_center_open(&self) -> bool {
        self.center_open
    }
    pub fn history(&self) -> impl Iterator<Item = &Notification> {
        self.history.iter()
    }

    pub fn open_center(&mut self, cx: &mut Context<Self>) {
        self.center_open = true;
        self.visible.clear();
        self.hovered = false;
        cx.notify();
    }
    pub fn toggle_center(&mut self, cx: &mut Context<Self>) {
        if self.center_open {
            self.close_center(cx);
        } else {
            self.open_center(cx);
        }
    }
    pub fn close_center(&mut self, cx: &mut Context<Self>) {
        self.center_open = false;
        cx.notify();
    }
    pub fn show_details(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.history.iter().any(|record| record.id == id) {
            return;
        }
        self.selected = Some(id);
        self.center_refocus = self.center_open;
        self.mark_read(id);
        self.center_scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.open_center(cx);
    }
    pub fn mark_all_read(&mut self, cx: &mut Context<Self>) {
        for record in &mut self.history {
            record.read = true;
        }
        cx.notify();
    }
    pub fn clear_read(&mut self, cx: &mut Context<Self>) {
        self.history.retain(|record| !record.read);
        if self
            .selected
            .is_some_and(|id| !self.history.iter().any(|record| record.id == id))
        {
            self.selected = None;
        }
        cx.notify();
    }
    fn mark_read(&mut self, id: u64) {
        if let Some(record) = self.history.iter_mut().find(|record| record.id == id) {
            record.read = true;
        }
    }

    pub fn notify(&mut self, request: NotificationRequest, cx: &mut Context<Self>) -> u64 {
        let mut record = request_record(request, self.next_id, self.interacting());
        if let Some(previous) = record
            .key
            .as_ref()
            .and_then(|key| {
                self.history
                    .iter()
                    .find(|entry| entry.key.as_ref() == Some(key))
            })
            .cloned()
        {
            record.id = previous.id;
            record.occurrences = previous.occurrences.saturating_add(1);
            self.history.retain(|entry| entry.id != record.id);
            self.visible.retain(|entry| entry.id != record.id);
        } else {
            self.next_id += 1;
        }
        // Coalescing retains the record id. A globally unique timer epoch also
        // invalidates timers from earlier updates that were paused/resumed.
        record.timer_epoch = self.next_timer_epoch;
        self.next_timer_epoch = self.next_timer_epoch.wrapping_add(1);
        let id = record.id;
        if !self.center_open {
            if let Some(timeout) = record.remaining
                && !self.interacting()
            {
                Self::schedule_dismiss(id, timeout, record.timer_epoch, cx);
            }
            self.visible.push_front(record.clone());
            while self.visible.len() > MAX_VISIBLE {
                self.visible.pop_back();
            }
        }
        self.history.push_front(record);
        while self.history.len() > MAX_HISTORY {
            self.history.pop_back();
        }
        if self
            .selected
            .is_some_and(|id| !self.history.iter().any(|record| record.id == id))
        {
            self.selected = None;
            self.center_refocus = self.center_open;
        }
        cx.notify();
        id
    }

    fn schedule_dismiss(id: u64, timeout: Duration, epoch: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(timeout).await;
            this.update(cx, |this, cx| {
                if !this.interacting()
                    && this
                        .visible
                        .iter()
                        .any(|record| record.id == id && record.timer_epoch == epoch)
                {
                    this.dismiss(id);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
    fn interacting(&self) -> bool {
        self.hovered || self.focused
    }
    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let was = self.interacting();
        self.hovered = hovered;
        self.update_countdowns(was, cx);
    }
    fn set_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        let was = self.interacting();
        self.focused = focused;
        self.update_countdowns(was, cx);
    }
    fn update_countdowns(&mut self, was: bool, cx: &mut Context<Self>) {
        if was == self.interacting() {
            return;
        }
        let now = Instant::now();
        let interacting = self.interacting();
        let mut resume = Vec::new();
        let mut expired = Vec::new();
        for record in &mut self.visible {
            if record.remaining.is_none() {
                continue;
            }
            let epoch = self.next_timer_epoch;
            self.next_timer_epoch = self.next_timer_epoch.wrapping_add(1);
            record.timer_epoch = epoch;
            if interacting {
                record.hold(now);
            } else if let Some(remaining) = record.resume(now) {
                resume.push((record.id, remaining, epoch));
            } else {
                expired.push(record.id);
            }
        }
        for id in expired {
            self.dismiss(id);
        }
        for (id, timeout, epoch) in resume {
            Self::schedule_dismiss(id, timeout, epoch, cx);
        }
        cx.notify();
    }
    pub fn dismiss(&mut self, id: u64) {
        self.visible.retain(|record| record.id != id);
    }
    /// Dismiss transient previews. History is cleared only by explicit center actions.
    pub fn clear(&mut self) {
        self.visible.clear();
    }

    fn render_toast(
        &self,
        record: Notification,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = record.id;
        let (accent, icon_name) = record.level.accent_icon();
        let entry = Animation::new(TOAST_ENTRY).with_easing(crate::anim::ease_out_quint);
        div()
            .id(("notification", id))
            .role(if record.level == NotificationLevel::Error {
                gpui::Role::Alert
            } else {
                gpui::Role::Status
            })
            .relative()
            .flex()
            .items_start()
            .gap_2()
            .w_full()
            .min_w_0()
            .px_3()
            .py_2()
            .rounded(px(tokens::radius::OVERLAY))
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .shadow(theme::shadow_hover())
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(24.))
                    .flex_none()
                    .child(icon(icon_name, accent, 16.)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_role(TextRole::Body)
                            .text_color(theme::text())
                            .line_clamp(2)
                            .child(record.title),
                    )
                    .when_some(record.summary, |column, summary| {
                        column.child(
                            div()
                                .text_role(TextRole::Caption)
                                .text_color(theme::subtext())
                                .truncate()
                                .child(short_preview(&summary, SUMMARY_LIMIT)),
                        )
                    }),
            )
            .child(
                components::button(
                    ("notification-details", id),
                    t(k::NOTIFICATIONS_DETAILS),
                    ButtonTone::Ghost,
                    ButtonSize::Sm,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.show_details(id, cx))),
            )
            .child(
                components::icon_only_button_tone(
                    ("notification-close", id),
                    t(k::NOTIFICATIONS_DISMISS),
                    IconName::Close,
                    ButtonTone::Ghost,
                    ButtonSize::Sm,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.dismiss(id);
                    cx.notify();
                })),
            )
            .with_animation(("notification-entry", id), entry, |card, delta| {
                card.opacity(delta).left(px((1. - delta) * 6.))
            })
    }

    fn render_center(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let selected = self
            .selected
            .and_then(|id| self.history.iter().find(|record| record.id == id))
            .cloned();
        let mut body = div().flex().flex_col().gap_2().min_w_0();
        let mut detail_actions = None;
        if let Some(record) = selected {
            let copy = record.full_text.clone();
            detail_actions = Some(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .flex_none()
                    .child(
                        components::button(
                            "notification-back",
                            t(k::NOTIFICATIONS_BACK),
                            ButtonTone::Ghost,
                            ButtonSize::Sm,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.selected = None;
                            this.center_refocus = true;
                            this.center_scroll.set_offset(gpui::point(px(0.), px(0.)));
                            cx.notify();
                        })),
                    )
                    .child(
                        components::button(
                            "notification-copy",
                            t(k::NOTIFICATIONS_COPY),
                            ButtonTone::Neutral,
                            ButtonSize::Sm,
                        )
                        .debug_selector(|| "notification-copy".into())
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string()))
                        }),
                    ),
            );
            body = body
                .child(
                    div()
                        .text_role(TextRole::SectionTitle)
                        .text_color(theme::text())
                        .child(record.title),
                )
                .child(
                    div()
                        .text_role(TextRole::Caption)
                        .text_color(theme::subtext())
                        .child(components::format_local_timestamp(record.updated_at, true)),
                )
                .when_some(record.source, |body, source| {
                    body.child(
                        div()
                            .text_role(TextRole::Caption)
                            .text_color(theme::subtext())
                            .child(source),
                    )
                })
                .when_some(record.summary, |body, summary| {
                    body.child(
                        div()
                            .text_role(TextRole::Body)
                            .text_color(theme::text())
                            .whitespace_normal()
                            .child(summary),
                    )
                })
                .when_some(record.details, |body, details| {
                    body.child(
                        div()
                            .text_role(TextRole::Body)
                            .text_color(theme::subtext())
                            .whitespace_normal()
                            .child(details),
                    )
                });
        } else if self.history.is_empty() {
            body = body.child(
                div()
                    .py_8()
                    .text_role(TextRole::Body)
                    .text_color(theme::subtext())
                    .child(t(k::NOTIFICATIONS_EMPTY)),
            );
        } else {
            for record in self.history.iter().cloned() {
                let id = record.id;
                let (accent, icon_name) = record.level.accent_icon();
                let count = (record.occurrences > 1).then(|| format!("×{}", record.occurrences));
                body = body.child(
                    keyboard::interactive(div().id(("notification-record", id)))
                        .role(gpui::Role::Button)
                        .aria_label(record.title.clone())
                        .flex()
                        .flex_col()
                        .flex_none()
                        .min_w_0()
                        .gap_1()
                        .p_3()
                        .rounded(px(tokens::radius::PANEL))
                        .cursor_pointer()
                        .bg(if record.read {
                            theme::surface()
                        } else {
                            theme::panel()
                        })
                        .hover(|style| style.bg(theme::surface_hover()))
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .min_w_0()
                                .gap_2()
                                .child(icon(icon_name, accent, 14.).flex_none())
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_role(TextRole::Body)
                                        .text_color(theme::text())
                                        .line_clamp(2)
                                        .child(record.title),
                                )
                                .when(!record.read, |row| {
                                    row.child(components::status_dot_sized(theme::accent(), 5.))
                                })
                                .when_some(count, |row, count| {
                                    row.child(
                                        div()
                                            .text_role(TextRole::Caption)
                                            .text_color(theme::subtext())
                                            .child(count),
                                    )
                                }),
                        )
                        .when_some(record.summary, |row, summary| {
                            row.child(
                                div()
                                    .text_role(TextRole::Caption)
                                    .text_color(theme::subtext())
                                    .line_clamp(2)
                                    .child(summary),
                            )
                        })
                        .when_some(record.source, |row, source| {
                            row.child(
                                div()
                                    .text_role(TextRole::Caption)
                                    .text_color(theme::subtext())
                                    .truncate()
                                    .child(source),
                            )
                        })
                        .child(
                            div()
                                .text_role(TextRole::Caption)
                                .text_color(theme::subtext())
                                .child(components::format_local_timestamp(record.updated_at, true)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.show_details(id, cx))),
                );
            }
        }
        let max_height = (f32::from(window.viewport_size().height) - 80.).max(0.);
        let card = modal::card(window, px(400.))
            .id("notification-center-card")
            .debug_selector(|| "notification-center-card".into())
            .h(px(max_height))
            .max_h(px(max_height))
            .rounded(px(tokens::radius::OVERLAY))
            .child(
                div()
                    .id("notification-center-header")
                    .debug_selector(|| "notification-center-header".into())
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .flex_none()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_role(TextRole::SectionTitle)
                                    .text_color(theme::text())
                                    .child(t(k::NOTIFICATIONS_CENTER)),
                            )
                            .child(
                                div()
                                    .text_role(TextRole::Caption)
                                    .text_color(theme::subtext())
                                    .child(t(k::NOTIFICATIONS_SESSION)),
                            ),
                    )
                    .child(
                        components::icon_only_button_tone(
                            "notification-center-close",
                            t(k::NOTIFICATIONS_DISMISS),
                            IconName::Close,
                            ButtonTone::Ghost,
                            ButtonSize::Md,
                        )
                        .track_focus(
                            self.center_close_focus
                                .as_ref()
                                .expect("center close focus initialized"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.close_center(cx))),
                    ),
            )
            .when_some(detail_actions, |card, actions| card.child(actions))
            .child(
                modal::body("notification-center-scroll")
                    .flex_1()
                    .track_scroll(&self.center_scroll)
                    .child(body),
            )
            .child(
                div()
                    .id("notification-center-footer")
                    .debug_selector(|| "notification-center-footer".into())
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .flex_none()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(
                        components::button(
                            "notifications-read",
                            t(k::NOTIFICATIONS_MARK_READ),
                            ButtonTone::Ghost,
                            ButtonSize::Sm,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.mark_all_read(cx))),
                    )
                    .child(
                        components::button(
                            "notifications-clear",
                            t(k::NOTIFICATIONS_CLEAR_READ),
                            ButtonTone::Ghost,
                            ButtonSize::Sm,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.clear_read(cx))),
                    ),
            );
        let scroll = self.center_scroll.clone();
        modal::overlay(
            "notification-center",
            t(k::NOTIFICATIONS_CENTER),
            self.center_focus
                .as_ref()
                .expect("center focus initialized"),
            card,
        )
        .key_context("OcHubModal OcHubNotificationCenter")
        .on_action(|_: &BlockFormSubmit, _, _| {})
        .items_start()
        .justify_end()
        .pt(px(64.))
        .pb_4()
        .bg(theme::scrim().alpha(if theme::is_dark() { 0.2 } else { 0.08 }))
        .on_action(cx.listener(|this, _: &modal::Dismiss, _, cx| this.close_center(cx)))
        .on_key_down(move |event, window, cx| {
            let delta = match event.keystroke.key.as_str() {
                "pageup" => Some(240.),
                "pagedown" => Some(-240.),
                "home" => Some(f32::MAX),
                "end" => Some(-f32::MAX),
                _ => None,
            };
            if let Some(delta) = delta {
                let current = f32::from(scroll.offset().y);
                let max = f32::from(scroll.max_offset().y);
                scroll.set_offset(gpui::point(px(0.), px((current + delta).clamp(-max, 0.))));
                window.refresh();
                cx.stop_propagation();
            }
        })
    }
}

impl Default for NotificationHost {
    fn default() -> Self {
        Self::new()
    }
}

pub trait ToastSource {
    fn take_toast(&mut self) -> Option<SharedString>;
    fn take_toast_level(&mut self) -> Option<NotificationLevel> {
        None
    }
}

#[macro_export]
macro_rules! impl_status_toasts_leveled {
    ($view:ty) => {
        impl $crate::notifications::ToastSource for $view {
            fn take_toast(&mut self) -> Option<gpui::SharedString> {
                self.status.take()
            }
            fn take_toast_level(&mut self) -> Option<$crate::notifications::NotificationLevel> {
                self.status_level.take()
            }
        }
    };
}
pub use crate::impl_status_toasts_leveled;

impl Render for NotificationHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_scope.is_none() {
            let scope = cx.focus_handle().tab_stop(false);
            cx.on_focus_in(&scope, window, |this, _, cx| this.set_focused(true, cx))
                .detach();
            cx.on_focus_out(&scope, window, |this, _, _, cx| this.set_focused(false, cx))
                .detach();
            self.focus_scope = Some(scope);
        }
        if self.center_focus.is_none() {
            self.center_focus = Some(modal::ModalFocusState::new(cx));
        }
        if self.center_close_focus.is_none() {
            self.center_close_focus = Some(cx.focus_handle().tab_stop(true));
        }
        self.center_focus
            .as_mut()
            .expect("center focus initialized")
            .sync(self.center_open, None, window, cx);
        if self.center_open && std::mem::take(&mut self.center_refocus) {
            self.center_close_focus
                .as_ref()
                .expect("center close focus initialized")
                .focus(window, cx);
        }
        let root = div().absolute().inset_0();
        if self.center_open {
            return root.child(self.render_center(window, cx));
        }
        let mut toasts = div()
            .id("notification-host")
            .debug_selector(|| "notification-host".into())
            // Block clicks behind the stack while keeping its own hover scope
            // and allowing page scroll beneath the transient notification.
            .block_mouse_except_scroll()
            .absolute()
            .right(px(TOAST_MARGIN))
            .track_focus(self.focus_scope.as_ref().expect("focus initialized"))
            .w(px(toast_width(window.viewport_size().width.into())))
            .flex()
            .flex_col()
            .gap_2()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_hovered(*hovered, cx)));
        toasts = match self.anchor {
            ToastAnchor::TopRight { inset } => toasts.top(px(inset.max(TOAST_MARGIN))),
            ToastAnchor::BottomRight { inset } => toasts.bottom(px(inset.max(TOAST_MARGIN))),
        };
        root.child(
            toasts.children(
                self.visible
                    .iter()
                    .cloned()
                    .map(|record| self.render_toast(record, cx)),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn toast_stack_blocks_clicks_behind_it_without_losing_hover_pause(
        cx: &mut gpui::TestAppContext,
    ) {
        struct Probe {
            host: gpui::Entity<NotificationHost>,
            background_clicks: usize,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .relative()
                    .size_full()
                    .child(div().id("background-action").absolute().inset_0().on_click(
                        cx.listener(|this, _, _, _| {
                            this.background_clicks += 1;
                        }),
                    ))
                    .child(self.host.clone())
            }
        }
        let (probe, cx) = cx.add_window_view(|_, cx| Probe {
            host: cx.new(|cx| {
                let mut host = NotificationHost::new();
                host.info("Saved", cx);
                host
            }),
            background_clicks: 0,
        });
        cx.update(|window, _| window.activate_window());
        let bounds = cx.debug_bounds("notification-host").unwrap();
        let position = gpui::point(bounds.left() + px(30.), bounds.top() + px(12.));
        cx.simulate_mouse_move(position, None, gpui::Modifiers::default());
        cx.simulate_click(position, gpui::Modifiers::default());
        probe.read_with(cx, |probe, cx| {
            assert_eq!(probe.background_clicks, 0);
            let host = probe.host.read(cx);
            assert!(host.hovered);
            assert!(host.visible[0].countdown_started_at.is_none());
        });
        let host = probe.read_with(cx, |probe, _| probe.host.clone());
        host.update(cx, |host, cx| {
            host.clear();
            cx.notify();
        });
        cx.simulate_click(position, gpui::Modifiers::default());
        probe.read_with(cx, |probe, _| assert_eq!(probe.background_clicks, 1));
    }

    #[test]
    fn long_legacy_error_is_preserved_outside_the_short_preview() {
        let text = format!("/private/config/{}: cannot write", "long-path-".repeat(30));
        let record = request_record(
            NotificationRequest::new(NotificationLevel::Error, text.clone()),
            1,
            false,
        );
        assert!(record.title.chars().count() < LEGACY_TITLE_LIMIT);
        assert_eq!(record.details(), Some(text.as_str()));
        assert_eq!(record.full_text(), text);
        assert_eq!(toast_width(300.), 268.);
    }

    #[test]
    fn holding_then_resuming_deducts_only_elapsed_running_time() {
        let start = Instant::now();
        let mut toast = request_record(
            NotificationRequest::new(NotificationLevel::Info, "Saved")
                .timeout(Duration::from_secs(3)),
            1,
            false,
        );
        toast.countdown_started_at = Some(start);
        toast.hold(start + Duration::from_secs(1));
        toast.hold(start + Duration::from_secs(30));
        assert_eq!(toast.remaining, Some(Duration::from_secs(2)));
        assert_eq!(
            toast.resume(start + Duration::from_secs(60)),
            Some(Duration::from_secs(2))
        );
    }

    #[gpui::test]
    fn keys_coalesce_only_the_same_operation_and_dismiss_preserves_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let host = cx.new(|_| NotificationHost::new());
        host.update(cx, |host, cx| {
            let first = host.notify(
                NotificationRequest::new(NotificationLevel::Info, "Checking").key("local:check"),
                cx,
            );
            let second = host.notify(
                NotificationRequest::new(NotificationLevel::Success, "Checked").key("local:check"),
                cx,
            );
            assert_eq!(first, second);
            assert_eq!(host.history.len(), 1);
            assert_eq!(host.history[0].occurrences, 2);
            host.set_hovered(true, cx);
            host.set_hovered(false, cx);
            let resumed_epoch = host.visible[0].timer_epoch;
            host.notify(
                NotificationRequest::new(NotificationLevel::Success, "Updated").key("local:check"),
                cx,
            );
            assert!(
                host.visible[0].timer_epoch > resumed_epoch,
                "an earlier resumed timer must never match a replacement preview"
            );
            host.notify(
                NotificationRequest::new(NotificationLevel::Success, "Checked").key("remote:check"),
                cx,
            );
            assert_eq!(host.history.len(), 2);
            host.dismiss(first);
            assert_eq!(host.history.len(), 2);
            assert_eq!(host.unread_count(), 2);
            host.show_details(first, cx);
            assert!(host.is_center_open());
            assert_eq!(host.unread_count(), 1);
            host.clear_read(cx);
            assert_eq!(host.history.len(), 1);
            assert_eq!(host.selected, None);
        });
    }

    #[gpui::test]
    fn keyboard_opens_detail_copies_full_error_and_closes_with_escape(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{KeyUpEvent, Keystroke};
        cx.update(crate::keyboard::bind_keys);
        cx.update(crate::modal::bind_keys);
        let (host, cx) = cx.add_window_view(|_, cx| {
            let mut host = NotificationHost::new();
            host.error("Could not apply", "Full diagnostic detail", cx);
            host
        });
        cx.update(|window, _| window.activate_window());
        let press = |cx: &mut gpui::VisualTestContext, keys: &str| {
            for key in keys.split_whitespace() {
                cx.simulate_keystrokes(key);
                cx.simulate_event(KeyUpEvent {
                    keystroke: Keystroke::parse(key).unwrap(),
                });
            }
        };
        press(cx, "tab");
        host.read_with(cx, |host, _| {
            assert!(host.focused);
            assert!(host.visible[0].countdown_started_at.is_none());
        });
        press(cx, "enter");
        host.read_with(cx, |host, _| {
            assert!(host.is_center_open());
            assert_eq!(host.unread_count(), 0);
        });
        // Close, back, copy: the center starts at its fixed close button.
        press(cx, "tab tab enter");
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some("Could not apply\nFull diagnostic detail".into())
            )
        });
        press(cx, "escape");
        host.read_with(cx, |host, _| assert!(!host.is_center_open()));
    }

    #[gpui::test]
    fn long_details_scroll_while_copy_and_close_remain_inside_small_windows(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::keyboard::bind_keys);
        cx.update(crate::modal::bind_keys);
        let (host, cx) = cx.add_window_view(|_, cx| {
            let mut host = NotificationHost::new();
            let id = host.error("配置未写入 · 設定を書き込めません", "x".repeat(6000), cx);
            host.show_details(id, cx);
            host
        });
        cx.update(|window, _| window.activate_window());
        for (width, height) in [(960., 640.), (420., 400.)] {
            cx.simulate_resize(gpui::size(px(width), px(height)));
            host.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
            let card = cx.debug_bounds("notification-center-card").unwrap();
            let copy = cx.debug_bounds("notification-copy").unwrap();
            let header = cx.debug_bounds("notification-center-header").unwrap();
            let footer = cx.debug_bounds("notification-center-footer").unwrap();
            assert!(
                card.left() >= px(16.) && card.right() <= px(width - 16.),
                "{card:?}"
            );
            assert!(
                header.top() >= px(64.) && footer.bottom() <= px(height - 16.),
                "{header:?} / {footer:?}"
            );
            assert!(
                copy.top() >= header.bottom() && copy.bottom() < footer.top(),
                "{copy:?}"
            );
            host.read_with(cx, |host, _| {
                assert!(host.center_scroll.max_offset().y > px(0.))
            });
            cx.simulate_keystrokes("pagedown");
            host.read_with(cx, |host, _| {
                assert!(host.center_scroll.offset().y < px(0.))
            });
        }
    }

    #[gpui::test]
    fn detail_and_back_navigation_keep_focus_inside_the_center(cx: &mut gpui::TestAppContext) {
        use gpui::{KeyUpEvent, Keystroke};
        cx.update(crate::keyboard::bind_keys);
        cx.update(crate::modal::bind_keys);
        let (host, cx) = cx.add_window_view(|_, cx| {
            let mut host = NotificationHost::new();
            host.error("Not applied", "Details", cx);
            host.open_center(cx);
            host
        });
        cx.update(|window, _| window.activate_window());
        let press = |cx: &mut gpui::VisualTestContext, key: &str| {
            cx.simulate_keystrokes(key);
            cx.simulate_event(KeyUpEvent {
                keystroke: Keystroke::parse(key).unwrap(),
            });
        };
        press(cx, "tab");
        press(cx, "enter");
        cx.update(|window, cx| {
            let host = host.read(cx);
            assert!(host.selected.is_some());
            assert!(host.center_close_focus.as_ref().unwrap().is_focused(window));
        });
        press(cx, "tab");
        press(cx, "enter");
        cx.update(|window, cx| {
            let host = host.read(cx);
            assert!(host.selected.is_none());
            assert!(host.center_close_focus.as_ref().unwrap().is_focused(window));
        });
        press(cx, "escape");
        host.read_with(cx, |host, _| assert!(!host.center_open));
    }
}
