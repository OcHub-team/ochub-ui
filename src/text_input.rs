//! A reusable single-line text input entity for GPUI, adapted from
//! `zed/crates/gpui/examples/input.rs`.
//!
//! Provides focus, cursor, selection, editing, clipboard, and IME support.
//! Host views embed it via `cx.new(|cx| TextInput::new(cx, "placeholder"))`,
//! render the entity as a child, and read/write text via [`TextInput::content`]
//! / [`TextInput::set_content`].

#[cfg(target_os = "macos")]
mod macos;

#[derive(Default)]
struct InputBindings {
    #[cfg(target_os = "macos")]
    inputs: Vec<gpui::WeakEntity<TextInput>>,
}
impl gpui::Global for InputBindings {}

/// Idempotent compatibility entry point for applications with raw key routing.
pub fn init(cx: &mut App) {
    bind_keys(cx);
}

/// Give a focused text field its native editing keys before a host routes keys
/// to a terminal or canvas. Returns false when there is no matching field/key.
/// Only weak references are retained, including dynamically-created Find fields.
pub fn dispatch_focused_editing_key(
    event: &gpui::KeyDownEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    #[cfg(target_os = "macos")]
    {
        if !TextInput::is_macos_editing_key(event) || !cx.has_global::<InputBindings>() {
            return false;
        }
        let registry = cx.global_mut::<InputBindings>();
        registry.inputs.retain(|input| input.is_upgradable());
        let inputs: Vec<_> = registry
            .inputs
            .iter()
            .filter_map(gpui::WeakEntity::upgrade)
            .collect();
        for input in inputs {
            if input.read(cx).focus_handle.is_focused(window) {
                return input.update(cx, |input, cx| {
                    input.handle_macos_editing_key(event, window, cx)
                });
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (event, window, cx);
    false
}

use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollHandle,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div,
    fill, point, prelude::*, px, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::i18n::{k, raw, t};
use crate::icons::{IconName, icon};
use crate::theme;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToBeginningOfLine,
        DeleteToEndOfLine,
        KillToEndOfLine,
        Yank,
        Newline,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        Find,
        FindNext,
        FindPrevious,
        CloseFind,
    ]
);

pub enum TextInputEvent {
    Changed,
}

const INPUT_CONTEXT: Option<&str> = Some("TextInput || SearchInput || CodeEditor");

/// Cocoa's Control-K/Control-Y use an app-local kill buffer, independently of
/// the system clipboard used by Copy/Cut/Paste.
#[derive(Default)]
pub(crate) struct KillBuffer(pub SharedString);

impl gpui::Global for KillBuffer {}

/// Register the text-input key bindings. Call once from `main.rs`.
pub fn bind_keys(cx: &mut App) {
    if cx.has_global::<InputBindings>() {
        return;
    }
    cx.set_global(InputBindings::default());
    #[cfg(target_os = "macos")]
    cx.observe_new::<TextInput>(|_, _, cx| {
        let input = cx.entity().downgrade();
        cx.global_mut::<InputBindings>().inputs.push(input);
    })
    .detach();
    cx.bind_keys([
        gpui::KeyBinding::new("backspace", Backspace, INPUT_CONTEXT),
        gpui::KeyBinding::new("delete", Delete, INPUT_CONTEXT),
        gpui::KeyBinding::new("left", Left, INPUT_CONTEXT),
        gpui::KeyBinding::new("right", Right, INPUT_CONTEXT),
        gpui::KeyBinding::new("up", Up, INPUT_CONTEXT),
        gpui::KeyBinding::new("down", Down, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-left", SelectLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-right", SelectRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-up", SelectUp, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-down", SelectDown, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-home", SelectHome, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-end", SelectEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("home", Home, INPUT_CONTEXT),
        gpui::KeyBinding::new("end", End, INPUT_CONTEXT),
        gpui::KeyBinding::new("enter", Newline, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-enter", Newline, INPUT_CONTEXT),
        gpui::KeyBinding::new("enter", FindNext, Some("SearchInput")),
        gpui::KeyBinding::new("shift-enter", FindPrevious, Some("SearchInput")),
        gpui::KeyBinding::new("escape", CloseFind, Some("SearchInput")),
        gpui::KeyBinding::new("f3", FindNext, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-f3", FindPrevious, INPUT_CONTEXT),
    ]);

    #[cfg(target_os = "macos")]
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-a", SelectAll, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-v", Paste, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-v", Paste, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-c", Copy, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-x", Cut, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-z", Undo, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-z", Redo, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-f", Find, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-g", FindNext, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-g", FindPrevious, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, INPUT_CONTEXT),
        gpui::KeyBinding::new("alt-left", WordLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("alt-right", WordRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("alt-shift-left", SelectWordLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("alt-shift-right", SelectWordRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("alt-backspace", DeleteWordLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("alt-delete", DeleteWordRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-left", Home, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-right", End, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-left", SelectHome, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-right", SelectEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-up", DocumentStart, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-down", DocumentEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-up", SelectDocumentStart, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-shift-down", SelectDocumentEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-backspace", DeleteToBeginningOfLine, INPUT_CONTEXT),
        gpui::KeyBinding::new("cmd-delete", DeleteToEndOfLine, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-a", Home, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-e", End, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-a", SelectHome, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-e", SelectEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-b", Left, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-f", Right, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-p", Up, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-n", Down, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-h", Backspace, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-d", Delete, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-k", KillToEndOfLine, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-y", Yank, INPUT_CONTEXT),
    ]);

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    cx.bind_keys([
        gpui::KeyBinding::new("ctrl-a", SelectAll, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-v", Paste, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-c", Copy, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-x", Cut, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-z", Undo, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-y", Redo, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-z", Redo, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-f", Find, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-g", FindNext, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-g", FindPrevious, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-left", WordLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-right", WordRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-left", SelectWordLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-right", SelectWordRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-backspace", DeleteWordLeft, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-delete", DeleteWordRight, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-home", DocumentStart, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-end", DocumentEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-home", SelectDocumentStart, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-shift-end", SelectDocumentEnd, INPUT_CONTEXT),
        gpui::KeyBinding::new("ctrl-insert", Copy, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-insert", Paste, INPUT_CONTEXT),
        gpui::KeyBinding::new("shift-delete", Cut, INPUT_CONTEXT),
    ]);
}

const MAX_UNDO_SNAPSHOTS: usize = 100;
const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const MASK_GLYPH: &str = "•";

fn accessible_value(raw: &str, masked: bool) -> SharedString {
    if masked && !raw.is_empty() {
        MASK_GLYPH.repeat(raw.chars().count()).into()
    } else {
        SharedString::from(raw.to_string())
    }
}

/// Translate an offset in the stored value to the text actually painted by the
/// lightweight input. Masked fields replace every Unicode scalar with one
/// bullet, so raw byte offsets cannot be used directly against the shaped line.
fn display_offset(raw: &str, masked: bool, raw_offset: usize) -> usize {
    let mut offset = raw_offset.min(raw.len());
    while !raw.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    if masked {
        raw[..offset].chars().count() * MASK_GLYPH.len()
    } else {
        offset
    }
}

/// Byte offset into `text` for a UTF-16 offset measured from its start.
/// Saturates at `text.len()` and always lands on a character boundary, so the
/// result is safe to slice with even when the platform hands us an offset that
/// overruns the string or splits a surrogate pair.
pub(crate) fn utf8_offset_from_utf16(text: &str, utf16_offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;
    for ch in text.chars() {
        if utf16_count >= utf16_offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }
    utf8_offset
}

fn raw_offset_from_display(raw: &str, masked: bool, display_offset: usize) -> usize {
    if !masked {
        let mut offset = display_offset.min(raw.len());
        while !raw.is_char_boundary(offset) {
            offset = offset.saturating_sub(1);
        }
        return offset;
    }

    let character = display_offset / MASK_GLYPH.len();
    raw.char_indices()
        .nth(character)
        .map(|(offset, _)| offset)
        .unwrap_or(raw.len())
}

/// Keep the active caret inside a single-line viewport while preserving the
/// user's current scroll position until the caret reaches either edge.
fn horizontal_scroll_for_caret(
    current: Pixels,
    caret_x: Pixels,
    content_width: Pixels,
    viewport_width: Pixels,
    focused: bool,
) -> Pixels {
    if !focused || viewport_width <= px(0.) || content_width <= viewport_width {
        return px(0.);
    }

    let padding = px(4.);
    let max_scroll = (content_width - viewport_width + padding).max(px(0.));
    let current = current.clamp(px(0.), max_scroll);
    let next = if caret_x < current + padding {
        (caret_x - padding).max(px(0.))
    } else if caret_x > current + viewport_width - padding {
        caret_x - viewport_width + padding
    } else {
        current
    };
    next.clamp(px(0.), max_scroll)
}

/// Shared focus/visibility state for every custom caret in the app. Timer
/// owners only need to schedule the returned epoch and call [`Self::tick`].
#[derive(Default)]
pub(crate) struct CaretBlink {
    focused: bool,
    visible: bool,
    epoch: usize,
}

impl CaretBlink {
    pub(crate) fn set_focused(&mut self, focused: bool) -> Option<usize> {
        if self.focused == focused {
            return None;
        }
        self.focused = focused;
        self.visible = focused;
        self.epoch = self.epoch.wrapping_add(1);
        focused.then_some(self.epoch)
    }

    pub(crate) fn reset(&mut self) -> Option<usize> {
        self.visible = true;
        self.epoch = self.epoch.wrapping_add(1);
        self.focused.then_some(self.epoch)
    }

    pub(crate) fn tick(&mut self, epoch: usize) -> bool {
        if !self.focused || self.epoch != epoch {
            return false;
        }
        self.visible = !self.visible;
        true
    }

    pub(crate) fn visible(&self) -> bool {
        self.visible
    }
}

#[derive(Clone, PartialEq)]
struct EditSnapshot {
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    /// Positive amount by which single-line text is shifted left. Unlike a
    /// generic scroll container this follows keyboard/IME caret movement too.
    horizontal_scroll: Pixels,
    is_selecting: bool,
    /// Initial run selected by a double click. While the mouse remains down,
    /// dragging extends from this whole run instead of collapsing to a
    /// character selection.
    word_selection_anchor: Option<Range<usize>>,
    masked: bool,
    multiline: bool,
    /// Code-editor mode: multiline rendering, monospace, line-number gutter.
    code: bool,
    /// Per-line shaped layouts captured during the last paint (code mode), used
    /// for hit-testing and vertical cursor movement. Only the scroll viewport
    /// (plus overdraw) is shaped; `code_first_row` maps buffer rows onto it.
    code_lines: Vec<ShapedLine>,
    /// Buffer row represented by `code_lines[0]`.
    code_first_row: usize,
    /// Cached line split of `content` (code mode). Rebuilt only when the
    /// content changes, so scroll and caret-blink frames never re-split or
    /// re-allocate the whole document.
    code_line_cache: Vec<SharedString>,
    /// The content the line cache was built from; equality-checked to detect
    /// staleness (SharedString compare is a cheap pointer-or-memcmp).
    code_line_cache_source: Option<SharedString>,
    /// Window-space origin of the text column (right of the gutter), last paint.
    code_origin: Option<Point<Pixels>>,
    code_line_height: Pixels,
    scroll_handle: ScrollHandle,
    undo_stack: Vec<EditSnapshot>,
    redo_stack: Vec<EditSnapshot>,
    find_input: Option<Entity<TextInput>>,
    find_visible: bool,
    find_matches: Vec<Range<usize>>,
    find_active: Option<usize>,
    search_field: bool,
    compact: bool,
    caret_blink: CaretBlink,
    /// 单行输入按下 Enter 时的提交回调（多行/代码模式仍插入换行）。
    on_enter: Option<EnterHandler>,
}

impl gpui::EventEmitter<TextInputEvent> for TextInput {}

/// 单行 Enter 提交回调的签名（如翻页条“跳至 X 页”）。
type EnterHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Byte ranges for non-overlapping matches. ASCII matching is case-insensitive;
/// non-ASCII text (including Chinese) remains exact so byte offsets stay stable.
pub(crate) fn find_matches(content: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    if query.is_ascii() {
        let haystack = content.to_ascii_lowercase();
        let needle = query.to_ascii_lowercase();
        haystack
            .match_indices(&needle)
            .map(|(start, matched)| start..start + matched.len())
            .collect()
    } else {
        content
            .match_indices(query)
            .map(|(start, matched)| start..start + matched.len())
            .collect()
    }
}

pub(crate) fn closest_match(matches: &[Range<usize>], cursor: usize) -> Option<usize> {
    if matches.is_empty() {
        None
    } else {
        Some(
            matches
                .iter()
                .position(|range| range.start >= cursor || range.contains(&cursor))
                .unwrap_or(0),
        )
    }
}

/// The same three-way character classification Zed uses for double-click
/// selection. Keeping punctuation separate means `foo.bar` selects one
/// identifier at a time, while whitespace runs remain selectable too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CharKind {
    Whitespace,
    Punctuation,
    Word,
}

fn char_kind(character: char) -> CharKind {
    if character.is_alphanumeric() || character == '_' {
        CharKind::Word
    } else if character.is_whitespace() {
        CharKind::Whitespace
    } else {
        CharKind::Punctuation
    }
}

pub(crate) fn clamped_char_boundary(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Word navigation skips adjoining whitespace, then one word/punctuation run.
/// Work on graphemes so accents and emoji are never split by a deletion.
pub(crate) fn previous_word_boundary(content: &str, offset: usize) -> usize {
    let offset = clamped_char_boundary(content, offset);
    let mut boundary = offset;
    let mut word_kind = None;
    for (index, grapheme) in content[..offset].grapheme_indices(true).rev() {
        let kind = char_kind(grapheme.chars().next().expect("nonempty grapheme"));
        if let Some(previous) = word_kind {
            if previous != kind {
                break;
            }
        } else if kind != CharKind::Whitespace {
            word_kind = Some(kind);
        }
        boundary = index;
    }
    boundary
}

pub(crate) fn next_word_boundary(content: &str, offset: usize) -> usize {
    let offset = clamped_char_boundary(content, offset);
    let mut boundary = offset;
    let mut word_kind = None;
    for (index, grapheme) in content[offset..].grapheme_indices(true) {
        let kind = char_kind(grapheme.chars().next().expect("nonempty grapheme"));
        if let Some(previous) = word_kind {
            if previous != kind {
                break;
            }
        } else if kind != CharKind::Whitespace {
            word_kind = Some(kind);
        }
        boundary = offset + index + grapheme.len();
    }
    boundary
}

pub(crate) fn line_start(content: &str, offset: usize) -> usize {
    let offset = clamped_char_boundary(content, offset);
    content[..offset].rfind('\n').map_or(0, |index| index + 1)
}

pub(crate) fn line_end(content: &str, offset: usize) -> usize {
    let offset = clamped_char_boundary(content, offset);
    content[offset..]
        .find('\n')
        .map_or(content.len(), |index| offset + index)
}

pub(crate) fn kill_line_end(content: &str, offset: usize) -> usize {
    let end = line_end(content, offset);
    if end == offset && end < content.len() {
        end + 1
    } else {
        end
    }
}

fn surrounding_run(content: &str, offset: usize) -> (Range<usize>, Option<CharKind>) {
    let offset = clamped_char_boundary(content, offset);
    let mut start = offset;
    let mut end = offset;
    let mut previous = content[..offset].chars().rev().take(128).peekable();
    let mut next = content[offset..].chars().take(128).peekable();
    let kind = std::cmp::max(
        previous.peek().copied().map(char_kind),
        next.peek().copied().map(char_kind),
    );

    for character in previous {
        if character != '\n' && Some(char_kind(character)) == kind {
            start -= character.len_utf8();
        } else {
            break;
        }
    }
    for character in next {
        if character != '\n' && Some(char_kind(character)) == kind {
            end += character.len_utf8();
        } else {
            break;
        }
    }

    (start..end, kind)
}

/// Returns the Zed-style run selected by a double click at `offset`.
///
/// At a boundary the higher-priority adjacent class wins (`Word` before
/// punctuation before whitespace), so double-clicking the gap immediately
/// after a word still selects that word. Newlines always stop the range.
pub(crate) fn surrounding_word_range(content: &str, offset: usize) -> Range<usize> {
    surrounding_run(content, offset).0
}

/// Extend a double-click selection while the pointer is dragged. Word runs
/// snap as a unit; punctuation and whitespace outside the original selection
/// retain character-precise dragging, matching Zed's `SelectMode::Word`.
pub(crate) fn extend_word_selection(
    content: &str,
    original: &Range<usize>,
    offset: usize,
) -> (Range<usize>, bool) {
    let offset = clamped_char_boundary(content, offset);
    let (run, kind) = surrounding_run(content, offset);
    let head = if kind == Some(CharKind::Word) || original.contains(&offset) {
        if run.start < original.start {
            run.start
        } else {
            run.end
        }
    } else {
        offset
    };

    if head <= original.start {
        (head..original.end, true)
    } else {
        (original.start..head, false)
    }
}

/// Shared inline find chrome used by both text controls. Buttons dispatch
/// actions back through the focused editor, preserving normal GPUI routing.
pub(crate) fn render_find_bar(
    input: Entity<TextInput>,
    active: Option<usize>,
    total: usize,
) -> gpui::Div {
    let id = input.entity_id().as_u64();
    let counter = if total == 0 {
        "0 / 0".to_string()
    } else {
        format!("{} / {total}", active.unwrap_or(0) + 1)
    };
    let control = |element_id, label: &'static str, icon_name: IconName| {
        crate::keyboard::interactive(div().id((element_id, id)))
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex()
            .items_center()
            .justify_center()
            .w(px(26.))
            .h(px(26.))
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(theme::surface_hover()))
            .child(icon(icon_name, theme::muted(), 12.))
    };

    div()
        .absolute()
        .top(px(8.))
        .right(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .w(px(330.))
        .p_1()
        .rounded_lg()
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::overlay())
        .shadow(theme::shadow_popover())
        .occlude()
        .child(div().flex_1().min_w(px(0.)).child(input))
        .child(
            div()
                .w(px(48.))
                .text_center()
                .text_xs()
                .text_color(theme::muted())
                .child(SharedString::from(counter)),
        )
        .child(
            control(
                "find-previous",
                raw(k::COMMON_FIND_PREVIOUS),
                IconName::ChevronLeft,
            )
            .on_click(|_event, window, cx| {
                window.dispatch_action(Box::new(FindPrevious), cx);
            }),
        )
        .child(
            control(
                "find-next",
                raw(k::COMMON_FIND_NEXT),
                IconName::ChevronRight,
            )
            .on_click(|_event, window, cx| {
                window.dispatch_action(Box::new(FindNext), cx);
            }),
        )
        .child(
            control("find-close", raw(k::COMMON_FIND_CLOSE), IconName::Close).on_click(
                |_event, window, cx| {
                    window.dispatch_action(Box::new(CloseFind), cx);
                },
            ),
        )
}

impl TextInput {
    pub fn new(cx: &mut Context<Self>, placeholder: impl Into<SharedString>) -> Self {
        Self {
            // Explicit tracked handles own their tab-stop flag; the rendered
            // Div's .tab_stop(true) only applies to automatically made handles.
            focus_handle: cx.focus_handle().tab_stop(true),
            content: SharedString::default(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            horizontal_scroll: px(0.),
            is_selecting: false,
            word_selection_anchor: None,
            masked: false,
            multiline: false,
            code: false,
            code_lines: Vec::new(),
            code_first_row: 0,
            code_line_cache: Vec::new(),
            code_line_cache_source: None,
            code_origin: None,
            code_line_height: px(20.),
            scroll_handle: ScrollHandle::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            find_input: None,
            find_visible: false,
            find_matches: Vec::new(),
            find_active: None,
            search_field: false,
            compact: false,
            caret_blink: CaretBlink::default(),
            on_enter: None,
        }
    }

    /// Seed a newly-created input without emitting a change event or scheduling
    /// an extra repaint. Runtime replacements should continue to use
    /// [`Self::set_content`].
    pub fn with_content(mut self, content: impl Into<SharedString>) -> Self {
        self.content = content.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self
    }

    /// Keep caret animation owned by the input entity instead of whichever
    /// host happens to render it. This makes nested inputs (such as find bars)
    /// behave exactly like standalone fields and stops timers on blur.
    fn set_caret_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
        if let Some(epoch) = self.caret_blink.set_focused(focused) {
            self.schedule_caret_blink(epoch, cx);
        }
    }

    fn reset_caret_blink(&mut self, cx: &mut Context<Self>) {
        if let Some(epoch) = self.caret_blink.reset() {
            self.schedule_caret_blink(epoch, cx);
        }
    }

    fn schedule_caret_blink(&self, epoch: usize, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CARET_BLINK_INTERVAL).await;
            this.update(cx, |this, cx| {
                if this.caret_blink.tick(epoch) {
                    cx.notify();
                    this.schedule_caret_blink(epoch, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Compact text field used by the editor's non-modal find bar.
    /// Apply the canonical compact search-field treatment.
    pub fn search_field(mut self) -> Self {
        self.search_field = true;
        self
    }

    /// Compact single-line field for pagination and dense inline controls.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// 注册单行 Enter 提交回调（如翻页条的“跳至 X 页”）。
    pub fn set_on_enter(&mut self, callback: impl Fn(&mut Window, &mut App) + 'static) {
        self.on_enter = Some(Rc::new(callback));
    }

    /// Render as a multiline code editor (monospace, line numbers, real line
    /// breaks). Implies [`Self::multiline`].
    pub fn code(mut self, code: bool) -> Self {
        self.code = code;
        if code {
            self.multiline = true;
        }
        self
    }

    /// Byte offset of the start of each logical line.
    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0usize];
        for (idx, byte) in self.content.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(idx + 1);
            }
        }
        starts
    }

    /// Length (bytes) of logical line `line`, excluding its trailing newline.
    fn line_len(&self, line: usize, starts: &[usize]) -> usize {
        let start = starts[line];
        let end = if line + 1 < starts.len() {
            starts[line + 1].saturating_sub(1)
        } else {
            self.content.len()
        };
        end.saturating_sub(start)
    }

    /// Map a byte offset to (line index, byte offset within that line).
    fn line_index_for_offset(&self, offset: usize) -> (usize, usize) {
        let starts = self.line_starts();
        let mut line = 0;
        for (idx, &start) in starts.iter().enumerate() {
            if start <= offset {
                line = idx;
            } else {
                break;
            }
        }
        (line, offset.saturating_sub(starts[line]))
    }

    /// Rebuild the cached per-line split when the content changed. Code-mode
    /// rendering reads from this cache every frame.
    fn ensure_code_line_cache(&mut self) {
        if self.code_line_cache_source.as_ref() == Some(&self.content) {
            return;
        }
        self.code_line_cache = self
            .content
            .split('\n')
            .map(|line| SharedString::from(line.to_string()))
            .collect();
        self.code_line_cache_source = Some(self.content.clone());
    }

    /// Shaped layout for a buffer row, if it falls inside the shaped window.
    fn code_line(&self, row: usize) -> Option<&ShapedLine> {
        self.code_lines.get(row.checked_sub(self.code_first_row)?)
    }

    /// Hit-test a window-space point to a byte offset (code mode).
    fn code_offset_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(origin) = self.code_origin else {
            return self.cursor_offset();
        };
        if self.code_lines.is_empty() {
            return 0;
        }
        let line_height = f32::from(self.code_line_height).max(1.0);
        let rel_y = f32::from(position.y - origin.y);
        let mut line_idx = (rel_y / line_height).floor() as isize;
        if line_idx < 0 {
            line_idx = 0;
        }
        let starts = self.line_starts();
        let line_idx = (line_idx as usize).min(starts.len().saturating_sub(1));
        let local_x = position.x - origin.x;
        // Rows outside the shaped window (only reachable mid-drag before the
        // follow-scroll catches up) fall back to the line start.
        let col = self
            .code_line(line_idx)
            .map(|line| line.closest_index_for_x(local_x))
            .unwrap_or(0);
        let line_len = self.line_len(line_idx, &starts);
        starts[line_idx] + col.min(line_len)
    }

    fn vertical_offset(&self, delta: isize) -> usize {
        let cursor = self.cursor_offset();
        if !self.multiline {
            return if delta < 0 { 0 } else { self.content.len() };
        }
        let (line, col) = self.line_index_for_offset(cursor);
        let starts = self.line_starts();
        let target = line as isize + delta;
        if target < 0 {
            return 0;
        }
        if target as usize >= starts.len() {
            return self.content.len();
        }
        let target = target as usize;
        let source_text = &self.content[starts[line]..cursor];
        let target_text =
            &self.content[starts[target]..starts[target] + self.line_len(target, &starts)];
        let fallback_col = target_text
            .grapheme_indices(true)
            .nth(source_text.graphemes(true).count())
            .map_or(target_text.len(), |(index, _)| index);
        let target_col = self
            .code_line(line)
            .zip(self.code_line(target))
            .filter(|_| self.code)
            .map_or(fallback_col, |(source, target)| {
                target.closest_index_for_x(source.x_for_index(col))
            });
        starts[target] + clamped_char_boundary(target_text, target_col)
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical_offset(-1), cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical_offset(1), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_offset(-1), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_offset(1), cx);
    }

    /// Render the masked variant (secrets shown as bullets).
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// Toggle masking at runtime (reveal/hide a secret in place).
    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        if self.masked != masked {
            self.masked = masked;
            cx.notify();
        }
    }

    /// Allow Enter and pasted text to preserve newlines. Rendering still uses
    /// the lightweight GPUI input element, but the saved value remains faithful.
    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    /// Current text content.
    pub fn content(&self) -> SharedString {
        self.content.clone()
    }

    pub(crate) fn select_all_content(&mut self, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.reset_caret_blink(cx);
        cx.notify();
    }

    /// Replace the full content, resetting the cursor to the end.
    /// Replace the placeholder without disturbing the field's content, cursor
    /// or focus.
    ///
    /// Needed because placeholders are captured when a view is constructed, and
    /// every top-level view is built once at startup and lives for the whole
    /// process — so after a locale switch their placeholders would otherwise
    /// stay in the old language until the app restarted. Rebuilding the entity
    /// instead would drop whatever the user had typed.
    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn set_content(&mut self, content: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = content.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.refresh_find_matches(cx);
        self.reset_caret_blink(cx);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn snapshot(&self) -> EditSnapshot {
        EditSnapshot {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
            selection_reversed: self.selection_reversed,
        }
    }

    fn record_undo(&mut self) {
        let snapshot = self.snapshot();
        if self.undo_stack.last() != Some(&snapshot) {
            if self.undo_stack.len() == MAX_UNDO_SNAPSHOTS {
                self.undo_stack.remove(0);
            }
            self.undo_stack.push(snapshot);
        }
        self.redo_stack.clear();
    }

    fn restore_snapshot(&mut self, snapshot: EditSnapshot, cx: &mut Context<Self>) {
        self.content = snapshot.content;
        self.selected_range = snapshot.selected_range;
        self.selection_reversed = snapshot.selection_reversed;
        self.marked_range = None;
        self.refresh_find_matches(cx);
        self.scroll_selection_into_view();
        self.reset_caret_blink(cx);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(snapshot) = self.undo_stack.pop() else {
            window.play_system_bell();
            return;
        };
        self.redo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot, cx);
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(snapshot) = self.redo_stack.pop() else {
            window.play_system_bell();
            return;
        };
        self.undo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot, cx);
    }

    fn refresh_find_matches(&mut self, cx: &mut Context<Self>) {
        let query = self
            .find_input
            .as_ref()
            .map(|input| input.read(cx).content.to_string())
            .unwrap_or_default();
        self.update_find_matches(&query, cx);
    }

    fn update_find_matches(&mut self, query: &str, cx: &mut Context<Self>) {
        if !self.find_visible {
            self.find_matches.clear();
            self.find_active = None;
            cx.notify();
            return;
        }
        self.find_matches = find_matches(&self.content, query);
        self.find_active = closest_match(&self.find_matches, self.cursor_offset());
        self.select_active_match();
        self.scroll_selection_into_view();
        cx.notify();
    }

    fn select_active_match(&mut self) {
        let Some(range) = self
            .find_active
            .and_then(|index| self.find_matches.get(index))
            .cloned()
        else {
            return;
        };
        self.selected_range = range;
        self.selection_reversed = false;
    }

    fn scroll_selection_into_view(&self) {
        if !self.code {
            return;
        }
        let (line, _) = self.line_index_for_offset(self.cursor_offset());
        let row_top = self.code_line_height * line as f32;
        let row_bottom = row_top + self.code_line_height;
        let viewport_height = self.scroll_handle.bounds().size.height;
        if viewport_height <= px(0.) {
            return;
        }
        let current = self.scroll_handle.offset();
        let current_top = -current.y;
        let target_top = if row_top < current_top {
            row_top
        } else if row_bottom > current_top + viewport_height {
            row_bottom - viewport_height
        } else {
            current_top
        }
        .clamp(px(0.), self.scroll_handle.max_offset().y);
        self.scroll_handle.set_offset(point(current.x, -target_top));
    }

    fn open_find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        if self.masked || self.search_field {
            window.play_system_bell();
            return;
        }
        if self.find_input.is_none() {
            let selected = if !self.selected_range.is_empty()
                && self.selected_range.len() <= 200
                && !self.content[self.selected_range.clone()].contains(['\r', '\n'])
            {
                self.content[self.selected_range.clone()].to_string()
            } else {
                String::new()
            };
            let input =
                cx.new(|cx| TextInput::new(cx, t(k::COMMON_FIND_PLACEHOLDER)).search_field());
            if !selected.is_empty() {
                input.update(cx, |input, cx| input.set_content(selected, cx));
            }
            cx.subscribe(&input, |this, input, _: &TextInputEvent, cx| {
                let query = input.read(cx).content.to_string();
                this.update_find_matches(&query, cx);
            })
            .detach();
            self.find_input = Some(input);
        }
        self.find_visible = true;
        self.refresh_find_matches(cx);
        if let Some(input) = &self.find_input {
            input.update(cx, |input, cx| input.select_all_content(cx));
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn find_next(&mut self, _: &FindNext, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_visible {
            self.open_find(&Find, window, cx);
            return;
        }
        if self.find_matches.is_empty() {
            window.play_system_bell();
            return;
        }
        self.find_active = Some(
            self.find_active
                .map(|index| (index + 1) % self.find_matches.len())
                .unwrap_or(0),
        );
        self.select_active_match();
        self.scroll_selection_into_view();
        cx.notify();
    }

    fn find_previous(&mut self, _: &FindPrevious, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_visible {
            self.open_find(&Find, window, cx);
            return;
        }
        if self.find_matches.is_empty() {
            window.play_system_bell();
            return;
        }
        self.find_active = Some(
            self.find_active
                .map(|index| {
                    if index == 0 {
                        self.find_matches.len() - 1
                    } else {
                        index - 1
                    }
                })
                .unwrap_or(self.find_matches.len() - 1),
        );
        self.select_active_match();
        self.scroll_selection_into_view();
        cx.notify();
    }

    fn close_find(&mut self, _: &CloseFind, window: &mut Window, cx: &mut Context<Self>) {
        self.find_visible = false;
        self.find_matches.clear();
        self.find_active = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(line_start(&self.content, self.cursor_offset()), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(line_end(&self.content, self.cursor_offset()), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(line_start(&self.content, self.cursor_offset()), cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(line_end(&self.content, self.cursor_offset()), cx);
    }

    fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_document_start(
        &mut self,
        _: &SelectDocumentStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    fn select_document_end(
        &mut self,
        _: &SelectDocumentEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.content.len(), cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(
            previous_word_boundary(&self.content, self.cursor_offset()),
            cx,
        );
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(next_word_boundary(&self.content, self.cursor_offset()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(
            previous_word_boundary(&self.content, self.cursor_offset()),
            cx,
        );
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(next_word_boundary(&self.content, self.cursor_offset()), cx);
    }

    fn delete_to(
        &mut self,
        offset: usize,
        kill: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.cursor_offset();
        let range = if self.selected_range.is_empty() {
            cursor.min(offset)..cursor.max(offset)
        } else {
            self.selected_range.clone()
        };
        if range.is_empty() {
            window.play_system_bell();
            return;
        }
        if kill {
            cx.set_global(KillBuffer(self.content[range.clone()].to_string().into()));
        }
        self.replace_text_in_range(Some(self.range_to_utf16(&range)), "", window, cx);
    }

    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_to(
            previous_word_boundary(&self.content, self.cursor_offset()),
            false,
            window,
            cx,
        );
    }

    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_to(
            next_word_boundary(&self.content, self.cursor_offset()),
            false,
            window,
            cx,
        );
    }

    fn delete_to_beginning_of_line(
        &mut self,
        _: &DeleteToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_to(
            line_start(&self.content, self.cursor_offset()),
            false,
            window,
            cx,
        );
    }

    fn delete_to_end_of_line(
        &mut self,
        _: &DeleteToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_to(
            line_end(&self.content, self.cursor_offset()),
            false,
            window,
            cx,
        );
    }

    fn kill_to_end_of_line(
        &mut self,
        _: &KillToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_to(
            kill_line_end(&self.content, self.cursor_offset()),
            true,
            window,
            cx,
        );
    }

    fn yank(&mut self, _: &Yank, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.try_global::<KillBuffer>().map(|buffer| buffer.0.clone()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                window.play_system_bell();
                return;
            }
            self.select_to(prev, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                window.play_system_bell();
                return;
            }
            self.select_to(next, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        self.word_selection_anchor = None;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            if event.click_count == 2 {
                let range = surrounding_word_range(&self.content, offset);
                let anchor = if self.selection_reversed {
                    self.selected_range.end
                } else {
                    self.selected_range.start
                };
                let reversed = range.start < anchor;
                self.set_selection(range.start.min(anchor)..range.end.max(anchor), reversed, cx);
            } else {
                self.select_to(offset, cx);
            }
        } else if event.click_count == 2 {
            let range = surrounding_word_range(&self.content, offset);
            self.word_selection_anchor = Some(range.clone());
            self.set_selection(range, false, cx);
        } else {
            self.move_to(offset, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
        self.word_selection_anchor = None;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.index_for_mouse_position(event.position);
            if let Some(original) = self.word_selection_anchor.clone() {
                let (range, reversed) = extend_word_selection(&self.content, &original, offset);
                self.set_selection(range, reversed, cx);
            } else {
                self.select_to(offset, cx);
            }
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline {
                text
            } else {
                text.replace('\n', " ")
            };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        } else if let Some(on_enter) = self.on_enter.clone() {
            // `newline` runs while this TextInput entity owns its mutable GPUI
            // lease. Hosts commonly read the submitted input in their Enter
            // handler, so invoking it synchronously would try to lease this
            // entity again and panic. Release the current update first.
            window.defer(cx, move |window, cx| on_enter(window, cx));
        } else {
            window.play_system_bell();
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.scroll_selection_into_view();
        self.reset_caret_blink(cx);
        cx.notify()
    }

    fn set_selection(&mut self, range: Range<usize>, reversed: bool, cx: &mut Context<Self>) {
        self.selected_range = range;
        self.selection_reversed = reversed;
        self.scroll_selection_into_view();
        self.reset_caret_blink(cx);
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        if self.code {
            return self.code_offset_for_position(position);
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let display = line.closest_index_for_x(position.x - bounds.left() + self.horizontal_scroll);
        raw_offset_from_display(&self.content, self.masked, display)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if offset == self.cursor_offset() {
            return;
        }
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.scroll_selection_into_view();
        self.reset_caret_blink(cx);
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf8_offset_from_utf16(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let completes_composition = self.marked_range.is_some();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        if !completes_composition {
            self.record_undo();
        }
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.selection_reversed = false;
        self.refresh_find_matches(cx);
        self.scroll_selection_into_view();
        self.reset_caret_blink(cx);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let starts_composition = self.marked_range.is_none();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        if starts_composition {
            self.record_undo();
        }
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        // The platform reports the composition caret relative to the marked
        // text, not to the whole field, so both endpoints are measured inside
        // `new_text` and rebased onto `range.start`. Converting them against
        // `self.content` (or adding `range.end` to the end offset) yields
        // offsets past the end of the value — or in the middle of a multi-byte
        // character — which survive an unmark and panic the next time the
        // selection is sliced.
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| {
                range.start + utf8_offset_from_utf16(new_text, range_utf16.start)
                    ..range.start + utf8_offset_from_utf16(new_text, range_utf16.end)
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        self.selection_reversed = false;
        self.refresh_find_matches(cx);
        self.scroll_selection_into_view();
        self.reset_caret_blink(cx);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = display_offset(&self.content, self.masked, range.start);
        let end = display_offset(&self.content, self.masked, range.end);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(start) - self.horizontal_scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(end) - self.horizontal_scroll,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let display_index = last_layout.index_for_x(line_point.x + self.horizontal_scroll)?;
        let raw_index = raw_offset_from_display(&self.content, self.masked, display_index);
        Some(self.offset_to_utf16(raw_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    search_highlights: Vec<PaintQuad>,
    text_origin: Point<Pixels>,
    horizontal_scroll: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let raw_content = input.content.clone();
        let masked = input.masked;
        let selected_range = display_offset(&raw_content, masked, input.selected_range.start)
            ..display_offset(&raw_content, masked, input.selected_range.end);
        let cursor = display_offset(&raw_content, masked, input.cursor_offset());
        let marked_range = input.marked_range.as_ref().map(|range| {
            display_offset(&raw_content, masked, range.start)
                ..display_offset(&raw_content, masked, range.end)
        });
        let cursor_visible = input.caret_blink.visible();
        let focused = input.focus_handle.is_focused(window);
        let style = window.text_style();

        let display_content: SharedString = if masked {
            accessible_value(&raw_content, true)
        } else if input.multiline && (raw_content.contains('\n') || raw_content.contains('\r')) {
            raw_content.replace(['\r', '\n'], " ").into()
        } else {
            raw_content.clone()
        };

        let (display_text, text_color) = if display_content.is_empty() {
            (
                input.placeholder.clone(),
                theme::muted().opacity(0.78).into(),
            )
        } else {
            (display_content.clone(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text.clone(), font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let content_width = line.x_for_index(display_text.len());
        let horizontal_scroll = horizontal_scroll_for_caret(
            input.horizontal_scroll,
            cursor_pos,
            content_width,
            bounds.size.width,
            focused && !display_content.is_empty(),
        );
        let text_origin = point(bounds.left() - horizontal_scroll, bounds.top());
        let search_highlights = if display_content.is_empty() {
            Vec::new()
        } else {
            input
                .find_matches
                .iter()
                .map(|range| {
                    display_offset(&raw_content, masked, range.start)
                        ..display_offset(&raw_content, masked, range.end)
                })
                .filter(|range| range.end <= display_text.len() && range.start < range.end)
                .map(|range| {
                    fill(
                        Bounds::from_corners(
                            point(text_origin.x + line.x_for_index(range.start), bounds.top()),
                            point(text_origin.x + line.x_for_index(range.end), bounds.bottom()),
                        ),
                        theme::yellow_soft(),
                    )
                })
                .collect()
        };
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                cursor_visible.then(|| {
                    fill(
                        Bounds::new(
                            point(text_origin.x + cursor_pos, bounds.top()),
                            size(px(2.), bounds.bottom() - bounds.top()),
                        ),
                        theme::accent(),
                    )
                }),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            text_origin.x + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            text_origin.x + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    theme::selection(),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
            search_highlights,
            text_origin,
            horizontal_scroll,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for highlight in prepaint.search_highlights.drain(..) {
            window.paint_quad(highlight);
        }
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }
        let line = prepaint.line.take().unwrap();
        line.paint(
            prepaint.text_origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.horizontal_scroll = prepaint.horizontal_scroll;
        });
    }
}

// Platform text services own an active preedit. Bound editing actions must
// obey the same guard as raw terminal-host dispatch.
fn when_not_composing<A>(
    edit: fn(&mut TextInput, &A, &mut Window, &mut Context<TextInput>),
) -> impl Fn(&mut TextInput, &A, &mut Window, &mut Context<TextInput>) {
    move |input, action, window, cx| {
        if input.marked_range.is_none() {
            edit(input, action, window, cx);
        }
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        self.set_caret_focus(focused, cx);
        let find_bar = self
            .find_visible
            .then(|| self.find_input.clone())
            .flatten()
            .map(|input| render_find_bar(input, self.find_active, self.find_matches.len()));
        let aria_value = accessible_value(&self.content, self.masked);
        let base = div()
            .id("text-input")
            .role(gpui::Role::TextInput)
            .aria_label(self.placeholder.clone())
            .aria_placeholder(self.placeholder.clone())
            .aria_value(aria_value)
            .focusable()
            .tab_stop(true)
            .flex()
            .relative()
            .key_context(if self.search_field {
                "SearchInput"
            } else {
                "TextInput"
            })
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(when_not_composing(Self::backspace)))
            .on_action(cx.listener(when_not_composing(Self::delete)))
            .on_action(cx.listener(when_not_composing(Self::left)))
            .on_action(cx.listener(when_not_composing(Self::right)))
            .on_action(cx.listener(when_not_composing(Self::up)))
            .on_action(cx.listener(when_not_composing(Self::down)))
            .on_action(cx.listener(when_not_composing(Self::select_left)))
            .on_action(cx.listener(when_not_composing(Self::select_right)))
            .on_action(cx.listener(when_not_composing(Self::select_up)))
            .on_action(cx.listener(when_not_composing(Self::select_down)))
            .on_action(cx.listener(when_not_composing(Self::word_left)))
            .on_action(cx.listener(when_not_composing(Self::word_right)))
            .on_action(cx.listener(when_not_composing(Self::select_word_left)))
            .on_action(cx.listener(when_not_composing(Self::select_word_right)))
            .on_action(cx.listener(when_not_composing(Self::select_home)))
            .on_action(cx.listener(when_not_composing(Self::select_end)))
            .on_action(cx.listener(when_not_composing(Self::document_start)))
            .on_action(cx.listener(when_not_composing(Self::document_end)))
            .on_action(cx.listener(when_not_composing(Self::select_document_start)))
            .on_action(cx.listener(when_not_composing(Self::select_document_end)))
            .on_action(cx.listener(when_not_composing(Self::delete_word_left)))
            .on_action(cx.listener(when_not_composing(Self::delete_word_right)))
            .on_action(cx.listener(when_not_composing(Self::delete_to_beginning_of_line)))
            .on_action(cx.listener(when_not_composing(Self::delete_to_end_of_line)))
            .on_action(cx.listener(when_not_composing(Self::kill_to_end_of_line)))
            .on_action(cx.listener(when_not_composing(Self::yank)))
            .on_action(cx.listener(when_not_composing(Self::select_all)))
            .on_action(cx.listener(when_not_composing(Self::home)))
            .on_action(cx.listener(when_not_composing(Self::end)))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(when_not_composing(Self::undo)))
            .on_action(cx.listener(when_not_composing(Self::redo)))
            .when(
                (self.multiline || self.code) && !self.search_field,
                |element| {
                    element
                        .on_action(cx.listener(Self::open_find))
                        .on_action(cx.listener(Self::find_next))
                        .on_action(cx.listener(Self::find_previous))
                        .on_action(cx.listener(Self::close_find))
                },
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .min_w_0()
            // Custom-shaped text can be wider than its layout node. Every
            // input owns the clipping boundary so secrets and long IDs can
            // never paint into adjacent fields.
            .overflow_hidden()
            .rounded_md()
            .bg(if self.search_field {
                if focused {
                    theme::surface_hover()
                } else {
                    theme::inset()
                }
            } else {
                theme::surface()
            })
            .border_1()
            .border_color(if focused && (self.multiline || self.code) {
                // Editors already expose focus through the caret and selection.
                // Keep their frame neutral so opening Find never leaves a large
                // accent-blue rectangle around the editing surface.
                theme::border_strong()
            } else if self.search_field {
                theme::border()
            } else if focused {
                theme::accent()
            } else {
                theme::border()
            })
            .text_color(theme::text())
            .text_size(px(crate::tokens::TextRole::Control.metrics().0))
            .line_height(px(crate::tokens::TextRole::Control.metrics().1))
            .when(self.search_field || self.compact, |element| {
                element.px_2().py(px(3.))
            })
            .when(!self.search_field && !self.compact, |element| {
                element.px_3().py(px(5.))
            })
            .when(!self.multiline && !self.code, |element| {
                element.h(px(if self.search_field || self.compact {
                    crate::tokens::control::COMPACT
                } else {
                    crate::tokens::control::DEFAULT
                }))
            });
        #[cfg(target_os = "macos")]
        let base = base.on_key_down(cx.listener(|this, event, window, cx| {
            if this.handle_macos_editing_key(event, window, cx) {
                cx.stop_propagation();
            }
        }));
        if self.code {
            base.font_family(crate::assets::monospace_font_family())
                .relative()
                .overflow_hidden()
                .h(px(380.))
                .items_start()
                // Stopping propagation (below) only outranks ancestors that
                // registered their wheel handler before this editor painted.
                // `gpui::list` registers its own *after* painting its items, so
                // in the reverse-ordered bubble phase the list scrolls the page
                // before this editor is ever asked — which is how a form field
                // ends up dragging the whole page. Ancestors gate on
                // `should_handle_scroll`, i.e. on the hit test, so take their
                // hitboxes out of it while the pointer is inside this editor.
                // Only once it really can scroll: a short snippet must still let
                // the wheel move the page instead of dead-ending here.
                .when(self.scroll_handle.max_offset().y > px(0.), |element| {
                    element.occlude()
                })
                .child(
                    div()
                        .id(("text-input-scroll", cx.entity_id().as_u64()))
                        .w_full()
                        .h_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle)
                        // Ancestors that are plain scroll containers (painted
                        // before their children) are still stopped the ordinary
                        // way. Once this editor can scroll, the gesture belongs
                        // to it alone.
                        .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                            if this.scroll_handle.max_offset().y > px(0.) {
                                cx.stop_propagation();
                            }
                        }))
                        .child(div().w_full().child(CodeElement { input: cx.entity() })),
                )
                .child(crate::scrollbar::VerticalScrollbar::new(
                    ("text-input-scrollbar", cx.entity_id().as_u64()),
                    self.scroll_handle.clone(),
                ))
                .when_some(find_bar, |element, bar| element.child(bar))
        } else {
            base.when(self.multiline, |s| s.h(px(132.)).items_start())
                .child(div().w_full().child(TextElement { input: cx.entity() }))
                .when_some(find_bar, |element, bar| element.child(bar))
        }
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Multiline code-editor element: shapes each logical line, paints a
/// line-number gutter, and supports cross-line cursor/selection. Shares the
/// `TextInput` entity (content, selection, IME) with the single-line element.
struct CodeElement {
    input: Entity<TextInput>,
}

struct CodePrepaintState {
    /// Shaped layouts for `first_row..first_row + lines.len()` only.
    lines: Vec<ShapedLine>,
    first_row: usize,
    numbers: Vec<(ShapedLine, Pixels)>,
    selections: Vec<PaintQuad>,
    search_highlights: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    gutter: Pixels,
    line_height: Pixels,
    text_origin: Point<Pixels>,
}

/// Buffer rows worth shaping/painting: the scroll viewport plus a small
/// overdraw. Falls back to a bounded prefix before the first layout pass.
fn code_visible_rows(
    content_bounds: Bounds<Pixels>,
    viewport_bounds: Bounds<Pixels>,
    line_height: Pixels,
    row_count: usize,
) -> Range<usize> {
    if row_count == 0 {
        return 0..0;
    }
    if viewport_bounds.size.height <= px(0.) || line_height <= px(0.) {
        return 0..row_count.min(64);
    }
    const OVERDRAW: isize = 3;
    let line_height = f32::from(line_height);
    let first = (f32::from(viewport_bounds.top() - content_bounds.top()) / line_height).floor()
        as isize
        - OVERDRAW;
    let last = (f32::from(viewport_bounds.bottom() - content_bounds.top()) / line_height).ceil()
        as isize
        + OVERDRAW;
    let first = first.max(0) as usize;
    let last = last.max(first as isize).min(row_count as isize) as usize;
    first.min(row_count)..last
}

impl IntoElement for CodeElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CodeElement {
    type RequestLayoutState = ();
    type PrepaintState = CodePrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let line_count = {
            let input = self.input.read(cx);
            input.content.split('\n').count().max(1)
        };
        let line_height = window.line_height();
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = (line_height * line_count as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.input
            .update(cx, |input, _cx| input.ensure_code_line_cache());
        let input = self.input.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let text_color = style.color;
        let muted: gpui::Hsla = theme::muted().opacity(0.85).into();

        let empty = input.content.is_empty();
        let placeholder_line = [input.placeholder.clone()];
        let line_texts: &[SharedString] = if empty {
            &placeholder_line
        } else {
            &input.code_line_cache
        };
        let line_count = line_texts.len();
        let digits = line_count.to_string().len().max(2);
        let gutter = px(18. + digits as f32 * 8.5);
        let text_origin = point(bounds.left() + gutter, bounds.top());

        // Shape only the rows the scroll viewport can show: shaping every row
        // of a large document each frame is what made these editors janky.
        let row_range = code_visible_rows(
            bounds,
            input.scroll_handle.bounds(),
            line_height,
            line_count,
        );
        let first_row = row_range.start;

        let mut lines = Vec::with_capacity(row_range.len());
        let mut numbers = Vec::with_capacity(row_range.len());
        for i in row_range.clone() {
            let text = &line_texts[i];
            let color = if empty { muted } else { text_color };
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            lines.push(
                window
                    .text_system()
                    .shape_line(text.clone(), font_size, &[run], None),
            );

            let num_text = SharedString::from((i + 1).to_string());
            let num_run = TextRun {
                len: num_text.len(),
                font: style.font(),
                color: theme::muted().opacity(0.7).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let num_shaped =
                window
                    .text_system()
                    .shape_line(num_text.clone(), font_size, &[num_run], None);
            let num_width = num_shaped.x_for_index(num_text.len());
            let num_x = bounds.left() + gutter - px(10.) - num_width;
            numbers.push((num_shaped, num_x));
        }

        let mut selections = Vec::new();
        let mut search_highlights = Vec::new();
        if !empty {
            let starts = input.line_starts();
            for matched in &input.find_matches {
                for (ix, line) in lines.iter().enumerate() {
                    let i = first_row + ix;
                    let line_start = starts[i];
                    let line_end = line_start + input.line_len(i, &starts);
                    if matched.start > line_end || matched.end < line_start {
                        continue;
                    }
                    let seg_start = matched.start.max(line_start);
                    let seg_end = matched.end.min(line_end);
                    if seg_end <= seg_start {
                        continue;
                    }
                    let y = bounds.top() + line_height * i as f32;
                    search_highlights.push(fill(
                        Bounds::from_corners(
                            point(text_origin.x + line.x_for_index(seg_start - line_start), y),
                            point(
                                text_origin.x + line.x_for_index(seg_end - line_start),
                                y + line_height,
                            ),
                        ),
                        theme::yellow_soft(),
                    ));
                }
            }
        }
        let selection = input.selected_range.clone();
        if !selection.is_empty() && !empty {
            let starts = input.line_starts();
            for (ix, line) in lines.iter().enumerate() {
                let i = first_row + ix;
                let line_start = starts[i];
                let line_end = line_start + input.line_len(i, &starts);
                if selection.start > line_end || selection.end < line_start {
                    continue;
                }
                let seg_start = selection.start.max(line_start);
                let seg_end = selection.end.min(line_end);
                let x0 = line.x_for_index(seg_start - line_start);
                let mut x1 = line.x_for_index(seg_end - line_start);
                let includes_newline = selection.end > line_end;
                if includes_newline {
                    x1 += px(6.);
                }
                if x1 > x0 {
                    let y = bounds.top() + line_height * i as f32;
                    selections.push(fill(
                        Bounds::from_corners(
                            point(text_origin.x + x0, y),
                            point(text_origin.x + x1, y + line_height),
                        ),
                        theme::selection(),
                    ));
                }
            }
        }

        let cursor = if !input.caret_blink.visible() || !input.selected_range.is_empty() {
            None
        } else if empty {
            Some(fill(
                Bounds::new(
                    point(text_origin.x, bounds.top()),
                    size(px(2.), line_height),
                ),
                theme::accent(),
            ))
        } else {
            let (line, col) = input.line_index_for_offset(input.cursor_offset());
            let cursor_x = line
                .checked_sub(first_row)
                .and_then(|ix| lines.get(ix))
                .map(|l| l.x_for_index(col))
                .unwrap_or(px(0.));
            let y = bounds.top() + line_height * line as f32;
            Some(fill(
                Bounds::new(
                    point(text_origin.x + cursor_x, y),
                    size(px(2.), line_height),
                ),
                theme::accent(),
            ))
        };

        CodePrepaintState {
            lines,
            first_row,
            numbers,
            selections,
            search_highlights,
            cursor,
            gutter,
            line_height,
            text_origin,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        // Gutter background.
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.left(), bounds.top()),
                size(prepaint.gutter, bounds.size.height),
            ),
            theme::inset(),
        ));

        for highlight in prepaint.search_highlights.drain(..) {
            window.paint_quad(highlight);
        }
        for selection in prepaint.selections.drain(..) {
            window.paint_quad(selection);
        }

        let line_height = prepaint.line_height;
        let text_x = prepaint.text_origin.x;
        for (ix, line) in prepaint.lines.iter().enumerate() {
            let y = bounds.top() + line_height * (prepaint.first_row + ix) as f32;
            if let Some((number, num_x)) = prepaint.numbers.get(ix) {
                number
                    .paint(
                        point(*num_x, y),
                        line_height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
            line.paint(
                point(text_x, y),
                line_height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        let lines = std::mem::take(&mut prepaint.lines);
        let first_row = prepaint.first_row;
        let text_origin = prepaint.text_origin;
        self.input.update(cx, |input, _cx| {
            input.code_lines = lines;
            input.code_first_row = first_row;
            input.code_origin = Some(text_origin);
            input.code_line_height = line_height;
            input.last_bounds = Some(bounds);
        });
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        Bounds, ClipboardItem, Context, Entity, EntityInputHandler, Focusable, IntoElement,
        KeyUpEvent, Keystroke, Render, TestAppContext, VisualTestContext, div, point, prelude::*,
        px, size,
    };

    use super::{
        CaretBlink, TextInput, accessible_value, bind_keys, closest_match, code_visible_rows,
        display_offset, extend_word_selection, find_matches, horizontal_scroll_for_caret,
        next_word_boundary, previous_word_boundary, raw_offset_from_display,
        surrounding_word_range, utf8_offset_from_utf16,
    };

    fn press(cx: &mut VisualTestContext, keys: &str) {
        for key in keys.split_whitespace() {
            cx.simulate_keystrokes(key);
            cx.simulate_event(KeyUpEvent {
                keystroke: Keystroke::parse(key).unwrap(),
            });
        }
    }

    fn primary(key: &str) -> String {
        format!(
            "{}-{key}",
            if cfg!(target_os = "macos") {
                "cmd"
            } else {
                "ctrl"
            }
        )
    }

    #[gpui::test]
    fn console_input_sizes_keep_a_full_line_box_for_cjk_and_latin(cx: &mut TestAppContext) {
        for compact in [false, true] {
            let (input, window_cx) = cx.add_window_view(|_, cx| {
                let input = TextInput::new(cx, "").with_content("中文输入 · 日本語 · Latin gjpq");
                if compact { input.compact() } else { input }
            });
            window_cx.update(|window, _| window.activate_window());
            input.read_with(window_cx, |input, _| {
                let bounds = input.last_bounds.expect("text was laid out");
                assert!(bounds.size.height >= px(20.), "{bounds:?}");
                assert!(
                    input
                        .last_layout
                        .as_ref()
                        .is_some_and(|line| line.width > px(0.))
                );
            });
        }
    }

    #[gpui::test]
    fn clipboard_and_history_shortcuts_reach_a_focused_masked_input(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let (input, cx) = cx.add_window_view(|window, cx| {
            let input = TextInput::new(cx, "secret")
                .masked(true)
                .with_content("synthetic-你好🙂");
            input.focus_handle.focus(window, cx);
            input
        });

        press(cx, &format!("{} {}", primary("a"), primary("c")));
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some("synthetic-你好🙂")
            );
        });
        press(cx, &primary("x"));
        input.read_with(cx, |input, _| assert!(input.content.is_empty()));
        press(cx, &primary("z"));
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "synthetic-你好🙂")
        });
        press(cx, &primary("shift-z"));
        input.read_with(cx, |input, _| assert!(input.content.is_empty()));
        press(cx, &primary("v"));
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "synthetic-你好🙂");
            assert_eq!(
                input.selected_range,
                input.content.len()..input.content.len()
            );
        });
    }

    #[gpui::test]
    fn word_selection_and_deletion_preserve_unicode_and_undo_selection(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let (input, cx) = cx.add_window_view(|window, cx| {
            let input = TextInput::new(cx, "name").with_content("alpha cafe\u{301} 变量🙂");
            input.focus_handle.focus(window, cx);
            input
        });
        let modifier = if cfg!(target_os = "macos") {
            "alt"
        } else {
            "ctrl"
        };
        press(cx, &format!("{modifier}-backspace"));
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "alpha cafe\u{301} 变量")
        });
        press(cx, &format!("{modifier}-shift-left {}", primary("c")));
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some("变量")
            );
        });
        press(cx, &format!("{modifier}-backspace"));
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "alpha cafe\u{301} ")
        });
        press(cx, &primary("z"));
        input.read_with(cx, |input, _| {
            assert_eq!(&input.content[input.selected_range.clone()], "变量");
            assert!(input.selection_reversed);
        });
        press(cx, &format!("right {modifier}-left {modifier}-backspace"));
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "alpha 变量")
        });
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn mac_line_document_and_control_editing_shortcuts_work(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let (input, cx) = cx.add_window_view(|window, cx| {
            let input = TextInput::new(cx, "config")
                .code(true)
                .with_content("first line\nsecond line");
            input.focus_handle.focus(window, cx);
            input
        });
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("keep clipboard".into()))
        });
        press(cx, "cmd-left ctrl-k");
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "first line\n")
        });
        press(cx, "ctrl-y cmd-up ctrl-e");
        input.read_with(cx, |input, _| assert_eq!(input.selected_range, 10..10));
        // At a line end, Control-K removes the newline and Control-Y restores it.
        press(cx, "ctrl-k ctrl-y cmd-up ctrl-a cmd-shift-down");
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "first line\nsecond line");
            assert_eq!(input.selected_range, 0..input.content.len());
        });
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some("keep clipboard")
            );
        });
        press(cx, "cmd-down cmd-backspace");
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "first line\n")
        });
        press(cx, "cmd-z cmd-shift-left");
        input.read_with(cx, |input, _| {
            assert_eq!(&input.content[input.selected_range.clone()], "second line")
        });
    }

    #[gpui::test]
    fn completed_composition_remains_editable_with_shortcuts(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let (input, cx) = cx.add_window_view(|window, cx| {
            let input = TextInput::new(cx, "name").with_content("prefix ");
            input.focus_handle.focus(window, cx);
            input
        });
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "你好🙂", Some(4..4), window, cx);
                input.replace_text_in_range(None, "你好🙂", window, cx);
            })
        });
        press(cx, &format!("shift-left {} backspace", primary("c")));
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), "prefix 你好")
        });
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some("🙂")
            );
        });
        press(cx, &format!("{} {}", primary("z"), primary("z")));
        input.read_with(cx, |input, _| assert_eq!(input.content.as_ref(), "prefix "));
    }

    #[gpui::test]
    fn extending_code_selection_keeps_the_active_end_inside_the_viewport(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let content = (0..80)
            .map(|row| format!("row {row:02} α🙂"))
            .collect::<Vec<_>>()
            .join("\n");
        let (input, cx) = cx.add_window_view(|window, cx| {
            let mut input = TextInput::new(cx, "config")
                .code(true)
                .with_content(content.clone());
            input.selected_range = 0..0;
            input.focus_handle.focus(window, cx);
            input
        });
        cx.simulate_resize(size(px(240.), px(440.)));
        let (start, select_end, select_start) = if cfg!(target_os = "macos") {
            ("cmd-up", "cmd-shift-down", "cmd-shift-up")
        } else {
            ("ctrl-home", "ctrl-shift-end", "ctrl-shift-home")
        };
        press(cx, start);
        input.read_with(cx, |input, _| {
            assert!(input.scroll_handle.max_offset().y > px(0.));
            assert_eq!(input.scroll_handle.offset().y, px(0.));
        });
        press(cx, select_end);
        input.read_with(cx, |input, _| {
            assert_eq!(input.content.as_ref(), content);
            assert_eq!(input.selected_range, 0..content.len());
            let viewport_top = -input.scroll_handle.offset().y;
            let viewport_bottom = viewport_top + input.scroll_handle.bounds().size.height;
            let row_top = input.code_line_height * 79.;
            assert!(viewport_top > px(0.), "extension must scroll the document");
            assert!(row_top >= viewport_top - px(1.));
            assert!(row_top + input.code_line_height <= viewport_bottom + px(1.));
            assert!(
                input.code_first_row > 0,
                "the bottom rows must actually be painted"
            );
        });
        press(cx, select_start);
        input.read_with(cx, |input, _| {
            assert_eq!(input.selected_range, 0..0);
            assert_eq!(input.scroll_handle.offset().y, px(0.));
            assert_eq!(input.content.as_ref(), content);
        });
    }

    #[test]
    fn word_boundaries_keep_combining_marks_and_emoji_intact() {
        let content = "cafe\u{301} 👨‍👩‍👧‍👦 变量";
        let emoji_start = content.find('👨').unwrap();
        let chinese_start = content.find('变').unwrap();
        assert_eq!(previous_word_boundary(content, emoji_start), 0);
        assert_eq!(next_word_boundary(content, 0), emoji_start - 1);
        assert_eq!(next_word_boundary(content, emoji_start), chinese_start - 1);
        assert_eq!(previous_word_boundary(content, chinese_start), emoji_start);
        assert_eq!(
            previous_word_boundary(content, content.len()),
            chinese_start
        );
    }

    struct EnterProbe {
        input: Entity<TextInput>,
        submitted: bool,
    }

    impl Render for EnterProbe {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(self.input.clone())
        }
    }

    #[gpui::test]
    fn enter_handler_can_read_the_input_after_its_update(cx: &mut TestAppContext) {
        cx.update(bind_keys);
        let (probe, cx) = cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| TextInput::new(cx, "name").with_content("alpha"));
            let weak_probe = cx.weak_entity();
            input.update(cx, |input, _| {
                input.set_on_enter(move |_window, cx| {
                    weak_probe
                        .update(cx, |probe: &mut EnterProbe, cx| {
                            assert_eq!(probe.input.read(cx).content().as_ref(), "alpha");
                            probe.submitted = true;
                        })
                        .expect("enter probe remains alive");
                });
            });
            input.read(cx).focus_handle(cx).focus(window, cx);
            EnterProbe {
                input,
                submitted: false,
            }
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        probe.read_with(cx, |probe, _| assert!(probe.submitted));
    }

    /// The composition caret arrives as a UTF-16 offset into the marked text.
    /// Measuring it there (rather than against the whole field) is what keeps
    /// the resulting selection inside the value after an IME switch.
    #[test]
    fn composition_caret_stays_inside_the_marked_text() {
        assert_eq!(utf8_offset_from_utf16("ni", 2), 2);
        assert_eq!(utf8_offset_from_utf16("你好", 1), 3);
        // Emoji outside the BMP: a caret between the surrogates snaps to the
        // character boundary instead of splitting it.
        assert_eq!(utf8_offset_from_utf16("😀", 1), 4);
        assert_eq!(utf8_offset_from_utf16("😀", 2), 4);
        // An offset past the end saturates rather than overrunning the string.
        assert_eq!(utf8_offset_from_utf16("ni", 9), 2);
        assert_eq!(utf8_offset_from_utf16("", 3), 0);
    }

    #[test]
    fn code_mode_shapes_only_viewport_rows_with_small_overdraw() {
        let content = Bounds::new(point(px(10.), px(100.)), size(px(600.), px(20_000.)));
        let viewport = Bounds::new(point(px(10.), px(300.)), size(px(600.), px(200.)));
        assert_eq!(code_visible_rows(content, viewport, px(20.), 1_000), 7..23);
    }

    #[test]
    fn code_mode_caps_first_layout_before_scroll_bounds_are_known() {
        let content = Bounds::new(point(px(0.), px(0.)), size(px(600.), px(20_000.)));
        assert_eq!(
            code_visible_rows(content, Bounds::default(), px(20.), 1_000),
            0..64
        );
        assert_eq!(
            code_visible_rows(content, Bounds::default(), px(20.), 0),
            0..0
        );
    }

    #[test]
    fn find_is_ascii_case_insensitive_and_preserves_utf8_offsets() {
        assert_eq!(find_matches("你好 FoO foo", "foo"), vec![7..10, 11..14]);
    }

    #[test]
    fn find_supports_exact_non_ascii_queries() {
        assert_eq!(
            find_matches("网关测试，网关正常", "网关"),
            vec![0..6, 15..21]
        );
        assert!(find_matches("anything", "").is_empty());
    }

    #[test]
    fn closest_match_wraps_to_the_first_result() {
        let matches = vec![2..5, 10..13];
        assert_eq!(closest_match(&matches, 4), Some(0));
        assert_eq!(closest_match(&matches, 7), Some(1));
        assert_eq!(closest_match(&matches, 99), Some(0));
        assert_eq!(closest_match(&[], 0), None);
    }

    #[test]
    fn double_click_range_matches_zed_character_classes() {
        let content = "foo.bar  baz";
        assert_eq!(surrounding_word_range(content, 1), 0..3);
        assert_eq!(surrounding_word_range(content, 3), 0..3);
        assert_eq!(surrounding_word_range(content, 4), 4..7);
        assert_eq!(surrounding_word_range(content, 8), 7..9);
        assert_eq!(surrounding_word_range(content, 9), 9..12);

        let punctuation = "foo::bar";
        assert_eq!(surrounding_word_range(punctuation, 4), 3..5);
    }

    #[test]
    fn double_click_range_is_utf8_safe_and_stops_at_newlines() {
        let unicode = "变量_1.δοκιμή";
        let separator = unicode.find('.').expect("separator");
        assert_eq!(surrounding_word_range(unicode, 1), 0..separator);
        assert_eq!(
            surrounding_word_range(unicode, separator + 2),
            separator + 1..unicode.len()
        );

        assert_eq!(surrounding_word_range("foo\nbar", 3), 0..3);
        assert_eq!(surrounding_word_range("foo\nbar", 4), 4..7);
    }

    #[test]
    fn dragging_after_double_click_extends_by_whole_words() {
        let content = "one two three";
        let original = 4..7;
        assert_eq!(extend_word_selection(content, &original, 1), (0..7, true));
        assert_eq!(extend_word_selection(content, &original, 9), (4..13, false));
    }

    #[test]
    fn caret_blink_restarts_and_rejects_stale_timers() {
        let mut caret = CaretBlink::default();
        assert!(!caret.visible());

        let first_epoch = caret.set_focused(true).expect("focus starts timer");
        assert!(caret.visible());
        assert!(caret.tick(first_epoch));
        assert!(!caret.visible());

        let fresh_epoch = caret.reset().expect("interaction restarts timer");
        assert!(caret.visible());
        assert!(!caret.tick(first_epoch));
        assert!(caret.visible());
        assert!(caret.tick(fresh_epoch));
        assert!(!caret.visible());

        assert_eq!(caret.set_focused(false), None);
        assert!(!caret.tick(fresh_epoch));
        assert!(!caret.visible());

        assert!(caret.set_focused(true).is_some());
        assert!(caret.visible());
    }

    #[test]
    fn masked_offsets_follow_unicode_characters_instead_of_raw_bytes() {
        let raw = "a好🙂";
        assert_eq!(display_offset(raw, true, 0), 0);
        assert_eq!(display_offset(raw, true, 1), 3);
        assert_eq!(display_offset(raw, true, 4), 6);
        assert_eq!(display_offset(raw, true, raw.len()), 9);

        assert_eq!(raw_offset_from_display(raw, true, 0), 0);
        assert_eq!(raw_offset_from_display(raw, true, 3), 1);
        assert_eq!(raw_offset_from_display(raw, true, 6), 4);
        assert_eq!(raw_offset_from_display(raw, true, 9), raw.len());
    }

    #[test]
    fn masked_accessibility_value_never_exposes_the_secret() {
        assert_eq!(accessible_value("sk-secret", true).as_ref(), "•••••••••");
        assert_eq!(accessible_value("sk-secret", false).as_ref(), "sk-secret");
        assert_eq!(accessible_value("", true).as_ref(), "");
    }

    #[test]
    fn horizontal_scroll_keeps_the_caret_inside_the_input_viewport() {
        assert_eq!(
            horizontal_scroll_for_caret(px(0.), px(150.), px(160.), px(100.), true),
            px(54.)
        );
        assert_eq!(
            horizontal_scroll_for_caret(px(54.), px(20.), px(160.), px(100.), true),
            px(16.)
        );
        assert_eq!(
            horizontal_scroll_for_caret(px(54.), px(150.), px(160.), px(100.), false),
            px(0.)
        );
        assert_eq!(
            horizontal_scroll_for_caret(px(20.), px(80.), px(90.), px(100.), true),
            px(0.)
        );
    }
}
