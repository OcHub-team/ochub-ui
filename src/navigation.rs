//! Navigation presentation. Section ids, selection, environment and actions
//! belong to the application; the library owns only geometry and interaction.

use gpui::{ElementId, SharedString, div, prelude::*, px};

use crate::{
    icons::{IconName, icon},
    keyboard, theme,
    tokens::{self, ConsoleStyle, TextRole},
};

pub fn rail() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .h_full()
        .min_h_0()
        .w(px(tokens::layout::NAVIGATION_WIDTH))
        .flex_none()
        .bg(theme::panel())
        .border_r_1()
        .border_color(theme::border())
}

pub fn section_label(label: impl Into<SharedString>) -> gpui::Div {
    div()
        .px_3()
        .pt_4()
        .pb_1()
        .text_role(TextRole::Caption)
        .text_color(theme::subtext())
        .child(label.into())
}

pub fn item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    name: IconName,
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    let foreground = if selected {
        theme::text()
    } else {
        theme::subtext()
    };
    keyboard::interactive(div().id(id))
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .aria_selected(selected)
        .relative()
        .flex()
        .items_center()
        .gap_2()
        .w_full()
        .min_w_0()
        .h(px(tokens::layout::ROW_HEIGHT))
        .px_3()
        .rounded(px(tokens::radius::CONTROL))
        .text_role(TextRole::Body)
        .text_color(foreground)
        .cursor_pointer()
        .when(selected, |item| {
            item.bg(theme::surface_hover())
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(14.))
                        .w(px(2.))
                        .h(px(16.))
                        .rounded_sm()
                        .bg(theme::accent()),
                )
        })
        .hover(|style| style.bg(theme::surface_hover()).text_color(theme::text()))
        .child(icon(name, foreground, tokens::control::ICON).flex_none())
        .child(div().flex_1().min_w_0().truncate().child(label))
}
