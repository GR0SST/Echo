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
        style.size.width = relative(1.).into();
        style.size.height =
            (window.line_height() * input.content.split('\n').count().max(25) as f32).into();
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
                window.request_autoscroll(Bounds::new(
                    point(
                        at.x + line.x_for_index(input.cursor_offset() - offset),
                        at.y,
                    ),
                    size(px(2.), height),
                ));
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
