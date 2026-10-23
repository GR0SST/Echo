//! Sidebar labels shaped only after their final layout width is known.
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, ShapedLine, SharedString, Style, Window, relative,
};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) struct SidebarTitle(SharedString);

impl SidebarTitle {
    pub(crate) fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl IntoElement for SidebarTitle {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for SidebarTitle {
    type RequestLayoutState = ();
    type PrepaintState = ShapedLine;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // No text measurement during intrinsic/flex probes. Keep inherited typography.
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> ShapedLine {
        shape_title(&self.0, bounds.size.width, window)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        line: &mut ShapedLine,
        window: &mut Window,
        cx: &mut App,
    ) {
        #[cfg(test)]
        PAINTED.with(|records| {
            records.borrow_mut().push(PaintedSidebarTitle {
                original: self.0.to_string(),
                text: line.text.to_string(),
                len: line.len(),
                width: bounds.size.width,
                shaped_width: line.width,
                glyph_indices: line
                    .runs
                    .iter()
                    .flat_map(|run| run.glyphs.iter().map(|g| g.index))
                    .collect(),
            })
        });
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            let _ = line.paint(bounds.origin, bounds.size.height, window, cx);
        });
    }
}

fn shape_title(original: &str, width: Pixels, window: &Window) -> ShapedLine {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    // A page title is a single line, including when supplied by a renamed tab.
    let text = original.replace(['\n', '\r'], " ");
    let shape = |text: String| {
        let run = style.to_run(text.len());
        window
            .text_system()
            .shape_line(text.into(), font_size, &[run], None)
    };
    let full = shape(text.clone());
    if full.width <= width {
        return full;
    }
    let ellipsis = shape("…".into());
    if ellipsis.width > width {
        return shape(String::new());
    }

    // Search grapheme boundaries, preserving emoji/combining sequences. Every
    // candidate gets a fresh run covering exactly its bytes; no probe state leaks.
    let ends = text
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let (mut low, mut high) = (0, ends.len() - 1);
    let mut best = ellipsis;
    while low < high {
        let mid = (low + high + 1) / 2;
        let candidate = shape(format!("{}…", &text[..ends[mid]]));
        if candidate.width <= width {
            best = candidate;
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    best
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct PaintedSidebarTitle {
    pub original: String,
    pub text: String,
    pub len: usize,
    pub width: Pixels,
    pub shaped_width: Pixels,
    pub glyph_indices: Vec<usize>,
}

#[cfg(test)]
thread_local! {
    static PAINTED: std::cell::RefCell<Vec<PaintedSidebarTitle>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(crate) fn take_painted_titles() -> Vec<PaintedSidebarTitle> {
    PAINTED.with(|records| std::mem::take(&mut *records.borrow_mut()))
}
