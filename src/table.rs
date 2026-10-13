//! A shared column contract for data tables. Header and body use the same
//! specification; the owner chooses paging/virtualization and row actions.

use gpui::{AnyElement, ElementId, SharedString, div, prelude::*, px};

use crate::{
    theme,
    tokens::{self, ConsoleStyle, TextRole},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    Fixed(f32),
    Flex { minimum: f32, weight: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    Start,
    End,
}

#[derive(Clone, Debug)]
pub struct Column {
    pub label: SharedString,
    pub width: ColumnWidth,
    pub alignment: Alignment,
}

impl Column {
    pub fn fixed(label: impl Into<SharedString>, width: f32) -> Self {
        Self {
            label: label.into(),
            width: ColumnWidth::Fixed(width.max(0.)),
            alignment: Alignment::Start,
        }
    }

    pub fn flex(label: impl Into<SharedString>, minimum: f32, weight: f32) -> Self {
        Self {
            label: label.into(),
            width: ColumnWidth::Flex {
                minimum: minimum.max(0.),
                weight: weight.max(0.01),
            },
            alignment: Alignment::Start,
        }
    }

    pub fn align_end(mut self) -> Self {
        self.alignment = Alignment::End;
        self
    }
}

pub fn minimum_width(columns: &[Column]) -> f32 {
    columns
        .iter()
        .map(|column| match column.width {
            ColumnWidth::Fixed(width) => width,
            ColumnWidth::Flex { minimum, .. } => minimum,
        })
        .sum::<f32>()
        + tokens::space::LG * 2.
        + tokens::space::MD * columns.len().saturating_sub(1) as f32
}

/// Put the resulting table inside a horizontal scroll viewport when its
/// containing pane is narrower than this minimum.
pub fn frame(id: impl Into<ElementId>, columns: &[Column]) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Table)
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(minimum_width(columns)))
}

fn cell(column: &Column) -> gpui::Div {
    let base = div().flex().items_center().min_w_0().overflow_hidden();
    let base = match column.width {
        ColumnWidth::Fixed(width) => base.w(px(width)).flex_none(),
        ColumnWidth::Flex { minimum, weight } => {
            base.flex_basis(px(0.)).flex_grow(weight).min_w(px(minimum))
        }
    };
    match column.alignment {
        Alignment::Start => base,
        Alignment::End => base.justify_end().text_align(gpui::TextAlign::Right),
    }
}

fn row_frame(id: impl Into<ElementId>) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Row)
        .flex()
        .items_center()
        .w_full()
        .min_h(px(tokens::layout::ROW_HEIGHT))
        .gap(px(tokens::space::MD))
        .px_4()
        .py_2()
}

pub fn header(id: impl Into<ElementId>, columns: &[Column]) -> gpui::Stateful<gpui::Div> {
    row_frame(id)
        .min_h(px(tokens::layout::COMPACT_ROW_HEIGHT))
        .text_role(TextRole::Caption)
        .text_color(theme::muted())
        .border_b_1()
        .border_color(theme::border())
        .children(columns.iter().enumerate().map(|(index, column)| {
            cell(column)
                .id(index)
                .role(gpui::Role::ColumnHeader)
                .child(div().min_w_0().truncate().child(column.label.clone()))
        }))
}

pub fn row(
    id: impl Into<ElementId>,
    columns: &[Column],
    cells: Vec<AnyElement>,
    last: bool,
) -> gpui::Stateful<gpui::Div> {
    debug_assert_eq!(
        columns.len(),
        cells.len(),
        "table rows must share the header column contract"
    );
    row_frame(id)
        .text_role(TextRole::Body)
        .text_color(theme::text())
        .when(!last, |row| row.border_b_1().border_color(theme::border()))
        .children(
            columns
                .iter()
                .zip(cells)
                .enumerate()
                .map(|(index, (column, child))| {
                    cell(column).id(index).role(gpui::Role::Cell).child(child)
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_minimum_includes_fixed_actions_and_gutters() {
        let columns = [
            Column::flex("Name", 180., 2.),
            Column::flex("Endpoint", 200., 3.),
            Column::fixed("Actions", 80.),
        ];
        assert_eq!(minimum_width(&columns), 516.);
        assert_eq!(minimum_width(&[]), 32.);
    }
}
