//! Single-line text input with IME, selection and clipboard support.
//! Adapted from gpui's `examples/input.rs`.

use std::{borrow::Cow, ops::Range};

use gpui::{
    actions, div, fill, hsla, point, prelude::*, px, relative, size, App, Bounds, ClipboardItem,
    ContentMask, Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, Hsla, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    text_input,
    [
        Newline,
        Indent,
        LineUp,
        LineDown,
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        PasteAndMatchStyle,
        Cut,
        Copy,
        Undo,
        Redo,
    ]
);

mod multiline;
const CONTEXT: &str = "TextInput";

/// Replace each CR/LF separately so UTF-16 offsets supplied by an IME stay
/// valid (including offsets inside CRLF). CRLF intentionally becomes two spaces.
fn single_line_text(text: &str) -> Cow<'_, str> {
    if text.contains(['\r', '\n']) {
        Cow::Owned(text.replace(['\r', '\n'], " "))
    } else {
        Cow::Borrowed(text)
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Newline, Some("CodeInput")),
        KeyBinding::new("tab", Indent, Some("CodeInput")),
        KeyBinding::new("up", LineUp, Some("CodeInput")),
        KeyBinding::new("down", LineDown, Some("CodeInput")),
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("left", Left, Some(CONTEXT)),
        KeyBinding::new("right", Right, Some(CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(CONTEXT)),
        KeyBinding::new("home", Home, Some(CONTEXT)),
        KeyBinding::new("end", End, Some(CONTEXT)),
        KeyBinding::new("cmd-left", Home, Some(CONTEXT)),
        KeyBinding::new("cmd-right", End, Some(CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-v", PasteAndMatchStyle, Some(CONTEXT)),
        KeyBinding::new("cmd-c", Copy, Some(CONTEXT)),
        KeyBinding::new("cmd-x", Cut, Some(CONTEXT)),
        KeyBinding::new("cmd-z", Undo, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(CONTEXT)),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some(CONTEXT)),
    ]);
}

/// Emitted whenever the text changes.
pub struct Changed;
/// Lets an owning scoped field remove its scope using the normal Backspace action.
pub struct EmptyBackspace;

#[derive(Clone)]
struct EditState {
    text: SharedString,
    selection: Range<usize>,
    reversed: bool,
}

pub struct TextInput {
    multiline: bool,
    lines: Vec<(usize, ShapedLine, Bounds<Pixels>)>,
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// Horizontal scroll that keeps the cursor visible in long text.
    scroll_x: Pixels,
    undo_history: Vec<EditState>,
    redo_history: Vec<EditState>,
    /// IME updates replace one provisional range; only its final result is
    /// one undoable edit, never the individual conversion candidates.
    composition_before: Option<EditState>,
    complete_after_edit: bool,
}

impl EventEmitter<Changed> for TextInput {}
impl EventEmitter<EmptyBackspace> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextInput {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let placeholder = placeholder.into();
        Self {
            multiline: false, lines: Vec::new(),
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            placeholder: single_line_text(&placeholder).into_owned().into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll_x: px(0.),
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            composition_before: None,
            complete_after_edit: false,
        }
    }

    pub(crate) fn code(cx: &mut Context<Self>) -> Self {
        let mut input = Self::new("", cx); input.multiline = true; input
    }
    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) { self.replace_text_in_range(None, "\n", window, cx); }
    fn indent(&mut self, _: &Indent, window: &mut Window, cx: &mut Context<Self>) { self.replace_text_in_range(None, "  ", window, cx); }
    fn line_up(&mut self, _: &LineUp, _: &mut Window, cx: &mut Context<Self>) { self.move_line(-1, cx); }
    fn line_down(&mut self, _: &LineDown, _: &mut Window, cx: &mut Context<Self>) { self.move_line(1, cx); }
    fn move_line(&mut self, delta: isize, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        if let Some(index) = self.lines.iter().rposition(|(start, _, _)| *start <= cursor) {
            let (start, line, _) = &self.lines[index];
            let x = line.x_for_index(cursor - start);
            let target = (index as isize + delta).clamp(0, self.lines.len().saturating_sub(1) as isize) as usize;
            let (start, line, _) = &self.lines[target];
            self.move_to(start + line.closest_index_for_x(x), cx);
        }
    }
    pub fn scroll_x(&self) -> Pixels {
        self.scroll_x
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) {
        let placeholder = placeholder.into();
        self.placeholder = single_line_text(&placeholder).into_owned().into();
        cx.notify();
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// Complete only a fresh insertion at the end, never a deletion, selection,
    /// mid-string edit or provisional IME composition. Select the suffix so the
    /// next character replaces it and Right accepts it using normal input rules.
    pub(crate) fn complete_suffix(&mut self, completion: &str, cx: &mut Context<Self>) {
        let length = self.content.len();
        if !self.complete_after_edit || self.marked_range.is_some()
            || self.selected_range != (length..length) || completion.len() <= length
            || !completion[..length].eq_ignore_ascii_case(&self.content) { return; }
        self.content = format!("{}{}", self.content, &completion[length..]).into();
        self.selected_range = length..self.content.len();
        self.complete_after_edit = false;
        cx.notify();
    }

    pub(crate) fn set_caret_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.set_text(text.to_string(), cx);
        self.move_to(self.content.len(), cx);
    }

    /// Replaces the text and selects all of it, so typing overwrites it.
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let text = text.into();
        self.content = if self.multiline { text } else { single_line_text(&text).into_owned().into() };
        self.complete_after_edit = false;
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.undo_history.clear();
        self.redo_history.clear();
        self.composition_before = None;
        cx.emit(Changed);
        cx.notify();
    }

    /// Initialize a value without a replace-all selection.
    pub(crate) fn set_text_at_end(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.set_text(text, cx);
        self.move_to(self.content.len(), cx);
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

    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.select_all_text(cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.content.is_empty() { cx.emit(EmptyBackspace); return; }
        let range = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())..self.cursor_offset()
        } else { self.selected_range.clone() };
        self.replace_text_in_range(Some(self.range_to_utf16(&range)), "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selected_range.is_empty() {
            self.cursor_offset()..self.next_boundary(self.cursor_offset())
        } else { self.selected_range.clone() };
        self.replace_text_in_range(Some(self.range_to_utf16(&range)), "", window, cx)
    }

    fn edit_state(&self) -> EditState {
        EditState { text: self.content.clone(), selection: self.selected_range.clone(), reversed: self.selection_reversed }
    }

    fn record_edit(&mut self, before: EditState) {
        if before.text != self.content {
            self.undo_history.push(before);
            self.redo_history.clear();
        }
    }

    fn finish_composition(&mut self) {
        if let Some(before) = self.composition_before.take() { self.record_edit(before); }
        self.marked_range = None;
    }

    fn restore_edit(&mut self, state: EditState, cx: &mut Context<Self>) {
        let changed = self.content != state.text;
        self.content = state.text;
        self.selected_range = state.selection;
        self.selection_reversed = state.reversed;
        self.marked_range = None;
        self.is_selecting = false;
        if changed { cx.emit(Changed); }
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_composition();
        let Some(before) = self.undo_history.pop() else { return; };
        self.redo_history.push(self.edit_state());
        self.restore_edit(before, cx);
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.composition_before.is_some() { return; }
        self.finish_composition();
        let Some(after) = self.redo_history.pop() else { return; };
        self.undo_history.push(self.edit_state());
        self.restore_edit(after, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
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
        self.paste_text(window, cx);
    }

    fn paste_and_match_style(&mut self, _: &PasteAndMatchStyle, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI inputs own plain text only; both commands share normalization,
        // selection replacement and the same single undo transaction.
        self.paste_text(window, cx);
    }

    fn paste_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
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
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() { return 0; }
        if self.multiline {
            if let Some((start, line, bounds)) = self.lines.iter().find(|(_, _, bounds)| position.y < bounds.bottom()).or_else(|| self.lines.last()) {
                return start + line.closest_index_for_x(position.x - bounds.left());
            }
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
        line.closest_index_for_x(position.x - bounds.left() + self.scroll_x)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        Self::offset_in_text_from_utf16(&self.content, offset)
    }

    fn offset_in_text_from_utf16(text: &str, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in text.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
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
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_composition();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = if self.multiline { Cow::Borrowed(new_text) } else { single_line_text(new_text) };
        let new_text = new_text.as_ref();
        let old_text = self.content.clone();
        let before = self.composition_before.take().unwrap_or_else(|| self.edit_state());
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.complete_after_edit = !new_text.is_empty();
        self.selection_reversed = false;
        self.marked_range.take();
        self.record_edit(before);
        if old_text != self.content { cx.emit(Changed); }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = if self.multiline { Cow::Borrowed(new_text) } else { single_line_text(new_text) };
        let new_text = new_text.as_ref();
        if self.composition_before.is_none() { self.composition_before = Some(self.edit_state()); }
        let old_text = self.content.clone();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        // The new selection is relative to the inserted text.
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| Self::offset_in_text_from_utf16(new_text, range_utf16.start)
                ..Self::offset_in_text_from_utf16(new_text, range_utf16.end))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;
        if old_text != self.content { cx.emit(Changed); }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        if self.multiline {
            let (start, line, bounds) = self.lines.iter().rev().find(|(start, _, _)| *start <= range.start)?;
            let x = line.x_for_index(range.start - start);
            return Some(Bounds::new(point(bounds.left() + x, bounds.top()), size(px(2.), bounds.size.height)));
        }
        let last_layout = self.last_layout.as_ref()?;
        let left = bounds.left() - self.scroll_x;
        Some(Bounds::from_corners(
            point(left + last_layout.x_for_index(range.start), bounds.top()),
            point(left + last_layout.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if self.multiline { return Some(self.offset_to_utf16(self.index_for_mouse_position(point))); }
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x + self.scroll_x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextInput>,
    cursor_color: Hsla,
}

struct PrepaintState {
    scroll_x: Pixels,
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
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
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), style.color.opacity(0.45))
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match input.marked_range.as_ref() {
            Some(marked) if !input.content.is_empty() => vec![
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            _ => vec![run],
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        // The placeholder is not editable text: keep the cursor at the start.
        let cursor_x = if input.content.is_empty() {
            px(0.)
        } else {
            line.x_for_index(cursor)
        };
        // Scroll just enough to keep the cursor inside the field.
        let cursor_width = px(2.);
        let visible = bounds.size.width - cursor_width;
        let mut scroll_x = input.scroll_x;
        if cursor_x < scroll_x {
            scroll_x = cursor_x;
        } else if cursor_x > scroll_x + visible {
            scroll_x = cursor_x - visible;
        }
        let max_scroll = (line.width - visible).max(px(0.));
        let scroll_x = scroll_x.clamp(px(0.), max_scroll);
        let left = bounds.left() - scroll_x;
        let (selection, cursor) = if selected_range.is_empty() || input.content.is_empty() {
            let cursor = fill(
                Bounds::new(
                    point(left + cursor_x, bounds.top()),
                    size(cursor_width, bounds.bottom() - bounds.top()),
                ),
                self.cursor_color,
            );
            (None, Some(cursor))
        } else {
            let selection = fill(
                Bounds::from_corners(
                    point(left + line.x_for_index(selected_range.start), bounds.top()),
                    point(left + line.x_for_index(selected_range.end), bounds.bottom()),
                ),
                self.cursor_color.opacity(0.25),
            );
            (Some(selection), None)
        };
        PrepaintState {
            scroll_x,
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
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
        let line = prepaint.line.take().unwrap();
        let scroll_x = prepaint.scroll_x;
        // Long text is clipped to the field.
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection)
            }
            let origin = point(bounds.left() - scroll_x, bounds.top());
            line.paint(origin, window.line_height(), window, cx).unwrap();
            if focus_handle.is_focused(window) {
                if let Some(cursor) = prepaint.cursor.take() {
                    window.paint_quad(cursor);
                }
            }
        });

        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_x = scroll_x;
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .key_context(if self.multiline { "TextInput CodeInput" } else { CONTEXT })
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::line_up))
            .on_action(cx.listener(Self::line_down))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::paste_and_match_style))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            // Even an empty editor owns these actions: bubbling could undo
            // an unrelated sidebar operation.
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(if self.multiline { multiline::CodeElement { input: cx.entity() }.into_any_element() } else { TextElement {
                input: cx.entity(), cursor_color: hsla(0.6, 0.8, 0.5, 1.0),
            }.into_any_element() })
    }
}

#[cfg(test)]
#[path = "single_line_input_tests.rs"]
mod single_line_input_tests;
