//! Multiline code presentation over the shared IME/clipboard/undo text buffer.
use super::*;
pub(super) struct CodeElement {
    pub input: Entity<TextInput>,
}
impl IntoElement for CodeElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
pub(super) struct CodePaint {
    lines: Vec<(usize, ShapedLine, Bounds<Pixels>)>,
    quads: Vec<PaintQuad>,
}
impl Element for CodeElement {
    type RequestLayoutState = ();
    type PrepaintState = CodePaint;
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
    ) -> (LayoutId, ()) {
        let input = self.input.read(cx);
        let mut style = Style::default();
        // As wide as the longest line, so a long line scrolls sideways
        // instead of running past the edge.
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let longest = input.content.split('\n').map(|text| {
            let run = TextRun { len: text.len(), font: text_style.font(), color: text_style.color, background_color: None, underline: None, strikethrough: None };
            window.text_system().shape_line(text.to_string().into(), font_size, &[run], None).width
        }).fold(px(0.), |widest, width| widest.max(width));
        style.size.width = (longest + px(8.)).into();
        style.min_size.width = relative(1.).into();
        style.flex_shrink = 0.;
        // Exactly its lines: extra height would let the host scroll a few
        // points up and down during a sideways swipe.
        style.size.height =
            (window.line_height() * input.content.split('\n').count() as f32).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> CodePaint {
        let input = self.input.read(cx);
        let style = window.text_style();
        let height = window.line_height();
        let mut result = CodePaint {
            lines: Vec::new(),
            quads: Vec::new(),
        };
        let mut offset = 0;
        input.caret.set(None);
        for (index, text) in input.content.split('\n').enumerate() {
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                text.to_string().into(),
                style.font_size.to_pixels(window.rem_size()),
                &[run],
                None,
            );
            let at = point(bounds.left(), bounds.top() + height * index as f32);
            let line_bounds = Bounds::new(at, size(bounds.size.width, height));
            let selected = input.selected_range.start.max(offset)
                ..input.selected_range.end.min(offset + text.len());
            if selected.start < selected.end {
                result.quads.push(fill(
                    Bounds::from_corners(
                        point(at.x + line.x_for_index(selected.start - offset), at.y),
                        point(
                            at.x + line.x_for_index(selected.end - offset),
                            at.y + height,
                        ),
                    ),
                    hsla(0.65, 0.7, 0.7, 0.35),
                ));
            }
            if input.focus_handle.is_focused(window)
                && input.selected_range.is_empty()
                && input.cursor_offset() >= offset
                && input.cursor_offset() <= offset + text.len()
            {
                let caret = Bounds::new(
                    point(
                        at.x + line.x_for_index(input.cursor_offset() - offset),
                        at.y,
                    ),
                    size(px(2.), height),
                );
                window.request_autoscroll(caret);
                input.caret.set(Some(caret));
                result.quads.push(fill(
                    Bounds::new(
                        point(
                            at.x + line.x_for_index(input.cursor_offset() - offset),
                            at.y,
                        ),
                        size(px(1.5), height),
                    ),
                    style.color,
                ));
            }
            result.lines.push((offset, line, line_bounds));
            offset += text.len() + 1;
        }
        result
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut CodePaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for quad in state.quads.drain(..) {
            window.paint_quad(quad);
        }
        for (_, line, bounds) in &state.lines {
            let _ = line.paint(bounds.origin, bounds.size.height, window, cx);
        }
        let lines = std::mem::take(&mut state.lines);
        self.input.update(cx, |input, _| {
            input.lines = lines;
            input.last_bounds = Some(bounds);
        });
    }
}
