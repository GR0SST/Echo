//! Media empty presentation from the supplied reference. No media enumeration.
use crate::text_input::{Changed, TextInput};
use gpui::{
    div, prelude::*, px, Context, Entity, EventEmitter, Focusable, Hsla, Render, Subscription,
    Window,
};

/// Measured in the installed Arc at a 1000 × 760 logical-point window.
pub(crate) const MEDIA_REFERENCE_COLUMN_WIDTH: f32 = 305.;
pub(crate) const MEDIA_TITLE: &str = "Nothing here yet!";
pub(crate) const MEDIA_BODY: &str = "Save media to your Documents, Desktop,\nand Download Folders to see them here,\nalongside your Captures in Echo.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MediaIntent {
    SearchChanged(String),
    SearchFocused,
    /// The target is unverified. This event never constructs or opens a URL.
    LearnMoreRequested,
}

/// Inherited semantic surfaces. No global appearance change or hardcoded palette.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct MediaColors {
    pub column: Hsla,
    pub preview: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub control: Hsla,
    pub hover: Hsla,
}

/// Keep one entity per browser window. Switching Library routes should not
/// recreate its input. Search only retains/emits a query; no files are read.
pub(crate) struct MediaView {
    colors: MediaColors,
    hosted_preview: bool,
    input: Entity<TextInput>,
    _input_changes: Subscription,
}
impl EventEmitter<MediaIntent> for MediaView {}
impl MediaView {
    pub(crate) fn new(colors: MediaColors, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("Search Media...", cx));
        let changes = cx.subscribe(&input, |_, input, _: &Changed, cx| {
            cx.emit(MediaIntent::SearchChanged(input.read(cx).text().to_owned()));
        });
        Self {
            colors,
            hosted_preview: false,
            input,
            _input_changes: changes,
        }
    }
    /// Let the host render the preview alongside this retained search entity.
    pub(crate) fn set_hosted_preview(&mut self, cx: &mut Context<Self>) {
        if !self.hosted_preview { self.hosted_preview = true; cx.notify(); }
    }
    pub(crate) fn focus_search(&self, window: &mut Window, cx: &gpui::App) {
        self.input.read(cx).focus_handle(cx).focus(window);
    }
    pub(crate) fn query(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text().to_owned()
    }
    /// A host theme update changes semantic colors without resetting query.
    pub(crate) fn set_colors(&mut self, colors: MediaColors, cx: &mut Context<Self>) {
        if self.colors != colors {
            self.colors = colors;
            cx.notify();
        }
    }
}
impl Render for MediaView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        div()
            .id("library-media")
            .debug_selector(|| "library-media".into())
            .relative()
            .size_full()
            .text_color(colors.text)
            .child(
                div()
                    .debug_selector(|| "library-media-column".into())
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(MEDIA_REFERENCE_COLUMN_WIDTH))
                    .bg(colors.column)
                    .overflow_hidden()
                    .child(
                        div()
                            .debug_selector(|| "library-media-toolbar".into())
                            .absolute()
                            .left_0()
                            .right_0()
                            .top_0()
                            .h(px(64.))
                            .child(
                                div()
                                    .id("library-media-search")
                                    .debug_selector(|| "library-media-search".into())
                                    .absolute()
                                    .left(px(8.))
                                    .right(px(8.))
                                    .top(px(10.))
                                    .h(px(36.))
                                    .rounded(px(10.))
                                    .bg(colors.control)
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .text_size(px(13.))
                                    .line_height(px(18.))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            // Bubble after TextInput's own caret/selection handling.
                                            // Padding/icon hits focus it without synthesizing an
                                            // inner click or resetting an existing selection.
                                            let focus = this.input.read(cx).focus_handle(cx);
                                            if !focus.is_focused(window) {
                                                focus.focus(window);
                                            }
                                            // Keep an enclosing focusable browser root from
                                            // taking focus back on this same mouse-down.
                                            cx.emit(MediaIntent::SearchFocused);
                                            window.prevent_default();
                                        }),
                                    )
                                    .child(crate::icons::control(crate::icons::Icon::Search))
                                    .child(
                                        div()
                                            .debug_selector(|| "library-media-search-input".into())
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(self.input.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "library-media-list-body".into())
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(64.))
                            .bottom(px(10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .debug_selector(|| "library-media-empty".into())
                                    .flex_none()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .child(
                                        div()
                                            .debug_selector(|| "library-media-empty-icon".into())
                                            .flex_none()
                                            .size(px(56.))
                                            .text_color(colors.muted)
                                            .child(super::navigation_icon(
                                                super::LibrarySection::Media,
                                                56. / 24.,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(|| "library-media-empty-title".into())
                                            .mt(px(12.))
                                            .h(px(20.))
                                            .text_size(px(16.))
                                            .line_height(px(20.))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child(MEDIA_TITLE),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(|| "library-media-empty-body".into())
                                            .mt(px(6.))
                                            .w(px(272.))
                                            .h(px(48.))
                                            .text_center()
                                            .text_size(px(14.))
                                            .line_height(px(16.))
                                            .text_color(colors.muted)
                                            .child(MEDIA_BODY),
                                    )
                                    .child(
                                        div()
                                            .id("library-media-learn-more")
                                            .debug_selector(|| "library-media-learn-more".into())
                                            .flex_none()
                                            .mt(px(20.))
                                            .w(px(100.))
                                            .h(px(28.))
                                            .rounded(px(8.))
                                            .bg(colors.control)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(colors.hover))
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.emit(MediaIntent::LearnMoreRequested);
                                            }))
                                            .child("Learn more"),
                                    ),
                            ),
                    ),
            )
            .when(!self.hosted_preview, |this| this.child(render_preview(colors.preview, None)))
    }
}

/// The same measured surface serves the standalone presenter and the live host.
/// The parent clips the original-width page canvas at the window edge.
pub(crate) fn render_preview(color: Hsla, content: Option<gpui::AnyElement>) -> gpui::AnyElement {
    render_preview_at(color, content, MEDIA_REFERENCE_COLUMN_WIDTH)
}

pub(crate) fn render_preview_at(color: Hsla, content: Option<gpui::AnyElement>, left: f32) -> gpui::AnyElement {
    div().debug_selector(|| "library-media-preview".into())
        .absolute().left(px(left)).right(px(10.))
        .top(px(10.)).bottom(px(10.)).rounded(px(6.)).shadow_md().overflow_hidden()
        .bg(color).children(content).into_any_element()
}

#[cfg(test)]
#[path = "library_media_tests.rs"]
mod tests;
