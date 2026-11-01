//! The page-bottom Zap instructions are browser chrome, never page HTML.
use super::*;
pub(crate) struct PickerBar {
    owner: WeakEntity<BoostEditor>,
    _owner_changed: Subscription,
}
/// A zapped element's own part of its selector, which is what a person
/// recognizes: `#nav`, `p:nth-of-type(2)`.
fn zap_label(selector: &str) -> String {
    let last = selector.rsplit(" > ").next().unwrap_or(selector);
    if last.chars().count() > 28 { format!("{}…", last.chars().take(27).collect::<String>()) } else { last.to_string() }
}
impl Render for PickerBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (code, zaps) = self.owner.read_with(cx, |editor, cx| {
            (editor.code, editor.boost(cx).map(|boost| boost.zap_selectors).unwrap_or_default())
        }).unwrap_or_default();
        let zaps = if code { Vec::new() } else { zaps };
        let zaps_shown = !zaps.is_empty();
        let instruction = if code {
            div().flex().items_center().child("Click an element on the page to ").child(div().font_weight(gpui::FontWeight::BOLD).child("pick its selector"))
        } else {
            div().flex().items_center().child("Click elements on the page to ").child(div().font_weight(gpui::FontWeight::BOLD).child("Zap")).child(" them")
        };
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
            .gap(px(12.))
            // Zapped elements, each one restorable with its ×.
            .when(!zaps.is_empty(), |bar| {
                bar.child(
                    div()
                        .id("boost-zaps")
                        .debug_selector(|| "boost-zaps".into())
                        .flex_1()
                        .min_w_0()
                        .overflow_x_scroll()
                        .flex()
                        .gap(px(6.))
                        .children(zaps.into_iter().enumerate().map(|(index, selector)| {
                            let owner = self.owner.clone();
                            let restore = selector.clone();
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .h(px(26.))
                                .pl(px(9.))
                                .pr(px(4.))
                                .rounded(px(6.))
                                .bg(rgb(0x3a3a3e))
                                .font_family("Menlo")
                                .text_size(px(11.))
                                .child(zap_label(&selector))
                                .child(
                                    div()
                                        .id(gpui::SharedString::from(format!("boost-zap-restore-{index}")))
                                        .debug_selector(move || if index == 0 { "boost-zap-restore".into() } else { format!("boost-zap-restore-{index}") })
                                        .size(px(18.))
                                        .rounded(px(4.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .text_color(rgb(0xb8b7d6))
                                        .hover(|style| style.bg(rgb(0x4a4a50)).text_color(rgb(0xffffff)))
                                        .child("×")
                                        .on_click(move |_, _, cx| {
                                            let (owner, restore) = (owner.clone(), restore.clone());
                                            cx.defer(move |cx| { let _ = owner.update(cx, |editor, cx| editor.restore_zap(restore, cx)); });
                                        }),
                                )
                        })),
                )
            })
            .child(
                div()
                    .when(!zaps_shown, |el| el.flex_1())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(instruction),
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
        let editor = cx.entity();
        // The bar reads the editor as it first renders, so it opens once this
        // update has finished, and only if picking is still on.
        cx.defer(move |cx| {
            if !editor.read(cx).picking || editor.read(cx).picker_bar.is_some() {
                return;
            }
            let observed = editor.clone();
            let Ok(handle) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| PickerBar { owner, _owner_changed: cx.observe(&observed, |_, _, cx| cx.notify()) })
            }) else {
                return;
            };
            editor.update(cx, |editor, _| editor.picker_bar = Some(handle));
        });
    }
}
