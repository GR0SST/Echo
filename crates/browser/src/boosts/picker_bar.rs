//! The page-bottom Zap instructions are browser chrome, never page HTML.
use super::*;
pub(crate) struct PickerBar {
    owner: WeakEntity<BoostEditor>,
}
impl Render for PickerBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "boost-picker-bar".into())
            .size_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.))
            .bg(rgb(0x242424))
            .border_t_1()
            .border_color(rgb(0x3b3b3b))
            .text_color(rgb(0xe9e8ff))
            .text_size(px(13.))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Click elements on the page to ")
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("Zap"))
                    .child(" them"),
            )
            .child(
                div()
                    .id("boost-picker-done")
                    .debug_selector(|| "boost-picker-done".into())
                    .w(px(58.))
                    .h(px(39.))
                    .rounded(px(7.))
                    .bg(rgb(0xefeeff))
                    .text_color(rgb(0x29292e))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child("Done")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let owner = this.owner.clone();
                        cx.defer(move |cx| {
                            let _ = owner.update(cx, |editor, cx| {
                                editor.stop_picker(cx);
                                cx.notify();
                            });
                        });
                    })),
            )
    }
}
impl BoostEditor {
    pub(super) fn picker_bar(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(handle) = self.picker_bar.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        if !enabled {
            return;
        }
        let Ok((main, parent, surface)) = self.page_browser.read_with(cx, |page, _| {
            (
                page.main_window,
                page.native_view,
                page.pages
                    .as_ref()
                    .and_then(|pages| pages.borrow().surface_bounds().into_iter().next()),
            )
        }) else {
            return;
        };
        let Some(surface) = surface else {
            return;
        };
        let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else {
            return;
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                bounds.origin
                    + point(
                        px(surface.x as f32),
                        px((surface.y + surface.height - 56.) as f32),
                    ),
                size(px(surface.width as f32), px(56.)),
            ))),
            titlebar: None,
            kind: WindowKind::Normal,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let owner = cx.weak_entity();
        self.picker_bar = cx
            .open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|_| PickerBar { owner })
            })
            .ok();
    }
}
