use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, SharedString, Styled, div};

use crate::components::{self, BadgeTone};
use crate::icons::{IconName, icon};
use crate::{layout, theme};

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

pub struct McpCard {
    pub name: SharedString,
    pub endpoint: SharedString,
    pub description: Option<SharedString>,
    pub apps_label: SharedString,
    pub actions: Vec<AnyElement>,
    pub app_toggles: Vec<AnyElement>,
}

pub fn mcp_card(spec: McpCard) -> gpui::Div {
    components::card()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap_4()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_color(theme::text())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(spec.name),
                        )
                        .child(
                            div()
                                .text_color(theme::muted())
                                .text_xs()
                                .child(spec.endpoint),
                        )
                        .children(spec.description.map(|description| {
                            div()
                                .text_color(theme::subtext())
                                .text_xs()
                                .child(description)
                        }))
                        .child(
                            div()
                                .text_color(theme::teal())
                                .text_xs()
                                .child(spec.apps_label),
                        ),
                )
                .child(div().flex().flex_row().gap_2().children(spec.actions)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .children(spec.app_toggles),
        )
}

pub fn toggle_chip(label: impl Into<SharedString>, enabled: bool) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(layout::toggle(enabled))
        .child(
            div()
                .text_color(theme::subtext())
                .text_xs()
                .child(label.into()),
        )
}

pub struct SkillCard {
    pub name: SharedString,
    pub badge: SharedString,
    pub badge_tone: BadgeTone,
    pub update_badge: Option<SharedString>,
    pub meta: SharedString,
    pub description: Option<SharedString>,
    pub actions: Vec<AnyElement>,
    pub app_toggles: Vec<AnyElement>,
}

pub fn skill_card(spec: SkillCard) -> gpui::Div {
    components::card()
        .gap_3()
        .border_color(if spec.update_badge.is_some() {
            theme::yellow()
        } else {
            theme::border()
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_color(theme::text())
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(spec.name),
                        )
                        .child(components::badge(spec.badge_tone, spec.badge))
                        .children(
                            spec.update_badge
                                .map(|label| components::badge(BadgeTone::Warning, label)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .flex_none()
                        .children(spec.actions),
                ),
        )
        .child(
            div()
                .w_full()
                .text_color(theme::muted())
                .text_xs()
                .truncate()
                .child(spec.meta),
        )
        .children(spec.description.map(|description| {
            div()
                .w_full()
                .text_color(theme::subtext())
                .text_xs()
                .line_clamp(2)
                .child(description)
        }))
        .children(spec.app_toggles)
}

pub struct SessionCard {
    pub provider: SharedString,
    pub title: SharedString,
    pub snippet: Option<SharedString>,
    pub active_time: Option<SharedString>,
    pub actions: Vec<AnyElement>,
}

pub fn session_card(spec: SessionCard) -> gpui::Div {
    components::card()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(components::badge(BadgeTone::Teal, spec.provider))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_color(theme::text())
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(spec.title),
                        ),
                )
                .children(spec.snippet.map(|snippet| {
                    div()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(icon(IconName::Search, theme::accent(), 12.))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_xs()
                                .text_color(theme::subtext())
                                .child(snippet),
                        )
                }))
                .children(spec.active_time.map(|time| {
                    div()
                        .min_w_0()
                        .text_color(theme::muted())
                        .text_xs()
                        .child(time)
                })),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .flex_shrink_0()
                .children(spec.actions),
        )
}
