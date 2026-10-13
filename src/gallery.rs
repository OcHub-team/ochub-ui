//! Dev-only component gallery (`MS_GALLERY=1`): renders every shared
//! component in every state so visual tuning and regression checks don't
//! require clicking through the whole app (GPUI has no hot reload).

use gpui::{Context, ScrollHandle, SharedString, Window, div, prelude::*, px};

use crate::components::{self, BadgeTone, ButtonSize, ButtonTone};
use crate::icons::IconName;
use crate::layout;
use crate::navigation;
use crate::notifications::{NotificationHost, NotificationLevel, NotificationRequest};
use crate::table::{self, Column};
use crate::text_input::TextInput;
use crate::theme;
use crate::tokens::{ConsoleStyle, TextRole};

pub struct GalleryView {
    demo_input: gpui::Entity<TextInput>,
    secret_input: gpui::Entity<TextInput>,
    compact_input: gpui::Entity<TextInput>,
    notifications: gpui::Entity<NotificationHost>,
    scroll_handle: ScrollHandle,
}

impl GalleryView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            demo_input: cx.new(|cx| TextInput::new(cx, "https://example.com")),
            secret_input: cx.new(|cx| TextInput::new(cx, "sk-ant-…")),
            compact_input: cx.new(|cx| TextInput::new(cx, "中日英 · 日本語 · Search").compact()),
            notifications: cx.new(|_| NotificationHost::new()),
            scroll_handle: ScrollHandle::new(),
        }
    }

    fn demo_row(children: Vec<gpui::AnyElement>) -> gpui::Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .flex_wrap()
            .gap_2()
            .children(children)
    }

    fn section(title: &'static str, description: &'static str, body: gpui::Div) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .child(layout::section_header(
                title,
                Some(SharedString::from(description)),
            ))
            .child(body)
    }
}

impl gpui::Render for GalleryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buttons = components::card()
            .gap_3()
            .child(Self::demo_row(vec![
                components::button("g-b1", "主要操作", ButtonTone::Primary, ButtonSize::Sm)
                    .into_any_element(),
                components::button("g-b2", "次要操作", ButtonTone::Neutral, ButtonSize::Sm)
                    .into_any_element(),
                components::button("g-b3", "危险操作", ButtonTone::Danger, ButtonSize::Sm)
                    .into_any_element(),
                components::button("g-b4", "幽灵按钮", ButtonTone::Ghost, ButtonSize::Sm)
                    .into_any_element(),
            ]))
            .child(Self::demo_row(vec![
                components::button("g-b5", "中号主要", ButtonTone::Primary, ButtonSize::Md)
                    .into_any_element(),
                components::button("g-b6", "中号次要", ButtonTone::Neutral, ButtonSize::Md)
                    .into_any_element(),
                components::icon_button_tone(
                    "g-b7",
                    "刷新",
                    IconName::Refresh,
                    ButtonTone::Neutral,
                    ButtonSize::Sm,
                )
                .into_any_element(),
                components::icon_button_tone(
                    "g-b8",
                    "新建",
                    IconName::Add,
                    ButtonTone::Primary,
                    ButtonSize::Sm,
                )
                .into_any_element(),
            ]));

        let fields = components::card()
            .gap_4()
            .child(components::field(
                "网站 URL",
                false,
                Some("仅用于展示，不会写入任何配置。".into()),
                self.demo_input.clone(),
            ))
            .child(components::field(
                "API Key",
                true,
                Some("写入所选鉴权变量；留空则保持不变。".into()),
                self.secret_input.clone(),
            ))
            .child(components::field_row(
                "随应用自动启动",
                Some("启动 OcHub 时自动拉起网关（端点地址保持稳定）。".into()),
                layout::toggle(true),
            ))
            .child(Self::demo_row(vec![
                div()
                    .w(px(240.))
                    .child(self.compact_input.clone())
                    .into_any_element(),
                components::button("g-compact", "28px", ButtonTone::Neutral, ButtonSize::Sm)
                    .into_any_element(),
            ]))
            .child(Self::demo_row(vec![
                components::disabled_button(
                    "g-disabled",
                    "不可用",
                    ButtonTone::Neutral,
                    ButtonSize::Md,
                    true,
                )
                .into_any_element(),
                components::busy_button(
                    "g-busy",
                    "处理中",
                    ButtonTone::Neutral,
                    ButtonSize::Md,
                    false,
                )
                .into_any_element(),
            ]));

        let console_columns = [
            Column::flex("应用 / Application", 120., 1.),
            Column::flex("连接 / 接続", 180., 2.),
            Column::fixed("状态", 96.),
        ];
        let console_table = components::card().p_0().child(
            div()
                .id("gallery-column-scroll")
                .w_full()
                .overflow_x_scroll()
                .child(
                    table::frame("gallery-console-table", &console_columns)
                        .child(table::header("gallery-console-header", &console_columns))
                        .child(table::row(
                            "gallery-console-row",
                            &console_columns,
                            vec![
                                div().child("Claude Code").into_any_element(),
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .child("团队主连接 · チーム接続 · Production")
                                    .into_any_element(),
                                div()
                                    .text_color(theme::subtext())
                                    .child("已配置")
                                    .into_any_element(),
                            ],
                            true,
                        )),
                ),
        );
        let console_navigation = components::card()
            .p_2()
            .child(navigation::item(
                "gallery-overview",
                "总览 / Overview",
                IconName::Layers,
                true,
            ))
            .child(navigation::item(
                "gallery-connections",
                "连接 / Connections",
                IconName::Cloud,
                false,
            ));
        let notice_demo = components::card()
            .gap_3()
            .child(
                div()
                    .text_role(TextRole::Body)
                    .text_color(theme::subtext())
                    .child("浮层保持简短，完整诊断保留在本次启动的通知中心。"),
            )
            .child(Self::demo_row(vec![
                components::button(
                    "gallery-notice-success",
                    "简短结果",
                    ButtonTone::Neutral,
                    ButtonSize::Md,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notifications.update(cx, |host, cx| {
                        host.notify(
                            NotificationRequest::new(NotificationLevel::Success, "演示草稿已保存")
                                .summary("这是组件演示，没有写入目标应用。")
                                .source("本机 · 组件画廊")
                                .key("gallery:save"),
                            cx,
                        );
                    });
                }))
                .into_any_element(),
                components::button(
                    "gallery-notice-error",
                    "长诊断详情",
                    ButtonTone::Neutral,
                    ButtonSize::Md,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notifications.update(cx, |host, cx| {
                        host.notify(
                            NotificationRequest::new(NotificationLevel::Error, "演示配置未写入")
                                .summary("诊断内容保留在通知中心。")
                                .details(format!(
                                    "/synthetic/example/{}\n{}",
                                    "long-path-".repeat(25),
                                    "保留完整说明 · 詳細を保持 · Full diagnostic detail.\n"
                                        .repeat(25)
                                ))
                                .source("组件画廊 · 虚构数据")
                                .key("gallery:diagnostic"),
                            cx,
                        );
                    });
                }))
                .into_any_element(),
                components::button(
                    "gallery-notice-center",
                    "通知中心",
                    ButtonTone::Ghost,
                    ButtonSize::Md,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notifications
                        .update(cx, |host, cx| host.toggle_center(cx))
                }))
                .into_any_element(),
            ]));

        let segmented_demo = components::card()
            .gap_3()
            .child(Self::demo_row(vec![
                components::segmented(
                    "g-seg-1",
                    &["AUTH_TOKEN (Bearer)", "API_KEY (x-api-key)"],
                    0,
                    |_, _, _| {},
                )
                .into_any_element(),
            ]))
            .child(Self::demo_row(vec![
                components::segmented(
                    "g-seg-2",
                    &["messages", "chat", "responses"],
                    1,
                    |_, _, _| {},
                )
                .into_any_element(),
            ]));

        let badges = components::card()
            .gap_3()
            .child(Self::demo_row(vec![
                components::badge(BadgeTone::Neutral, "默认").into_any_element(),
                components::badge(BadgeTone::Accent, "当前").into_any_element(),
                components::badge(BadgeTone::Success, "已启用").into_any_element(),
                components::badge(BadgeTone::Warning, "可更新").into_any_element(),
                components::badge(BadgeTone::Danger, "异常").into_any_element(),
                components::badge(BadgeTone::Teal, "claude").into_any_element(),
                components::badge(BadgeTone::Mauve, "opencode").into_any_element(),
                components::badge(BadgeTone::Peach, "gemini").into_any_element(),
            ]))
            .child(Self::demo_row(vec![
                components::status_dot(theme::green()).into_any_element(),
                components::status_dot(theme::yellow()).into_any_element(),
                components::status_dot(theme::red()).into_any_element(),
                components::status_dot(theme::muted()).into_any_element(),
                components::status_dot_sized(theme::accent(), 6.).into_any_element(),
            ]));

        let cards = layout::group(vec![
            layout::row()
                .child(layout::row_label(
                    "要求 API key",
                    Some("推理端点要求本地 key，用量按 key 归因。".into()),
                ))
                .child(layout::toggle(true))
                .into_any_element(),
            layout::row()
                .child(layout::row_label(
                    "随应用自动启动",
                    Some("启动 OcHub 时自动拉起网关（端点地址保持稳定）。".into()),
                ))
                .child(layout::toggle(false))
                .into_any_element(),
        ]);

        let empty = components::card().p_0().child(components::empty_state(
            IconName::Folder,
            "尚无上游",
            "新建上游并选择接口格式（messages / chat / responses）。",
            Some(
                components::button(
                    "g-empty-cta",
                    "新建上游",
                    ButtonTone::Primary,
                    ButtonSize::Sm,
                )
                .into_any_element(),
            ),
        ));

        let modal_demo = components::modal_overlay(
            components::modal_card()
                .child(components::modal_header("删除供应商"))
                .child(
                    components::modal_body().child(
                        div()
                            .text_color(theme::subtext())
                            .text_sm()
                            .child("确定删除 Fox-aws 吗？该操作不可撤销。"),
                    ),
                )
                .child(components::modal_footer(vec![
                    components::button("g-m1", "取消", ButtonTone::Neutral, ButtonSize::Sm)
                        .into_any_element(),
                    components::button("g-m2", "删除", ButtonTone::Danger, ButtonSize::Sm)
                        .into_any_element(),
                ])),
        );
        // Inline (non-overlay) preview of the modal card so the gallery page
        // stays scrollable; the overlay wrapper is demonstrated by the frame.
        let modal_frame = div()
            .relative()
            .h(px(280.))
            .rounded_lg()
            .overflow_hidden()
            .child(modal_demo);

        let disclosures = components::card().gap_2().p_0().py_2().child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .px_4()
                .child(components::disclosure(
                    "g-d1",
                    "网络设置",
                    Some("监听地址、超时与重试策略".into()),
                    true,
                ))
                .child(
                    div()
                        .pl_6()
                        .text_color(theme::muted())
                        .text_xs()
                        .child("展开状态的正文区域。"),
                )
                .child(components::disclosure(
                    "g-d2",
                    "流式健康检测",
                    Some("对SSE 流做周期性探测与自愈".into()),
                    false,
                )),
        );

        let stats = div()
            .grid()
            .grid_cols(4)
            .gap_3()
            .child(components::stat_tile(
                None,
                theme::green(),
                "网关状态",
                "运行中",
                "http://127.0.0.1:4180",
            ))
            .child(components::stat_tile(
                Some(IconName::Message),
                theme::accent(),
                "请求",
                "1,248",
                "成功率 99.1%",
            ))
            .child(components::stat_tile(
                Some(IconName::Diamond),
                theme::peach(),
                "总成本",
                "$3.42",
                "输入 1.2M / 输出 80K",
            ))
            .child(components::stat_tile(
                Some(IconName::Cloud),
                theme::teal(),
                "缓存命中",
                "86.4%",
                "重复输入复用比例",
            ));

        let table = components::card().p_0().child(
            div()
                .flex()
                .flex_col()
                .child(components::table_header(&[
                    "角色",
                    "模型 ID",
                    "显示名",
                    "1M 上下文",
                ]))
                .child(components::table_row(
                    vec![
                        div().text_sm().child("sonnet").into_any_element(),
                        div()
                            .text_color(theme::subtext())
                            .text_sm()
                            .child("claude-sonnet-4-6")
                            .into_any_element(),
                        div()
                            .text_color(theme::subtext())
                            .text_sm()
                            .child("Sonnet 4.6")
                            .into_any_element(),
                        layout::toggle(false).into_any_element(),
                    ],
                    4,
                    false,
                ))
                .child(components::table_row(
                    vec![
                        div().text_sm().child("opus").into_any_element(),
                        div()
                            .text_color(theme::subtext())
                            .text_sm()
                            .child("claude-opus-4-5")
                            .into_any_element(),
                        div()
                            .text_color(theme::subtext())
                            .text_sm()
                            .child("Opus 4.5")
                            .into_any_element(),
                        layout::toggle(true).into_any_element(),
                    ],
                    4,
                    true,
                )),
        );

        let pagination = components::card().child(components::pagination(
            components::button("g-p1", "‹ 上一页", ButtonTone::Neutral, ButtonSize::Sm)
                .into_any_element(),
            "第 3 / 12 页",
            components::button("g-p2", "下一页 ›", ButtonTone::Neutral, ButtonSize::Sm)
                .into_any_element(),
        ));

        let type_ramp = components::card()
            .gap_2()
            .child(
                div()
                    .text_role(TextRole::PageTitle)
                    .text_color(theme::text())
                    .child("Page title · 页面标题 · 24 / 30"),
            )
            .child(
                div()
                    .text_role(TextRole::SectionTitle)
                    .text_color(theme::text())
                    .child("Section title · 区块标题 · 15 / 22"),
            )
            .child(
                div()
                    .text_role(TextRole::Control)
                    .text_color(theme::text())
                    .child("Control · 输入文字 · 14 / 20"),
            )
            .child(
                div()
                    .text_role(TextRole::Body)
                    .text_color(theme::text())
                    .child("Body · 正文 · 本文 · 13 / 20"),
            )
            .child(
                div()
                    .text_role(TextRole::Caption)
                    .text_color(theme::muted())
                    .child("Caption · 辅助文字 · 補足 · 12 / 18"),
            );

        layout::page()
            .relative()
            .child(layout::page_header(
                "组件画廊",
                Some(SharedString::from("MS_GALLERY=1 · 共享组件全状态预览")),
            ))
            .child(layout::scroll_body(
                "gallery-scroll",
                &self.scroll_handle,
                layout::content_column()
                    .child(Self::section(
                        "控制台导航",
                        "选中 / 未选中，业务状态由宿主拥有",
                        console_navigation,
                    ))
                    .child(Self::section(
                        "通知",
                        "短结果 / 长详情 / 键盘 / 本次启动",
                        notice_demo,
                    ))
                    .child(Self::section("按钮", "tone × size,统一悬停与字重", buttons))
                    .child(Self::section(
                        "表单字段",
                        "竖排 field / 必填 / 横排 field_row",
                        fields,
                    ))
                    .child(Self::section(
                        "分段选择器",
                        "替代所有手搓 pill/chip 选择器",
                        segmented_demo,
                    ))
                    .child(Self::section("徽章与状态点", "八种 tone + 圆点", badges))
                    .child(Self::section(
                        "分组卡片",
                        "layout::group + row + toggle",
                        div().child(cards),
                    ))
                    .child(Self::section(
                        "空状态",
                        "图标 + 标题 + 提示 + 可选 CTA",
                        empty,
                    ))
                    .child(Self::section(
                        "模态",
                        "modal_overlay + modal_card + header/body/footer",
                        modal_frame,
                    ))
                    .child(Self::section(
                        "折叠区块",
                        "disclosure 展开/收起",
                        disclosures,
                    ))
                    .child(Self::section(
                        "指标卡",
                        "stat_tile:圆点或图标两种标记",
                        div().child(stats),
                    ))
                    .child(Self::section("表格", "table_header + table_row", table))
                    .child(Self::section(
                        "数据列",
                        "标题与行共享固定 / 弹性列宽",
                        console_table,
                    ))
                    .child(Self::section("分页", "pagination 页脚条", pagination))
                    .child(Self::section(
                        "字体阶梯",
                        "display / title / heading / body / caption",
                        type_ramp,
                    )),
            ))
            .child(self.notifications.clone())
    }
}
