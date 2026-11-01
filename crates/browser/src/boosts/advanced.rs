//! The reference's Contrast / Brightness / Original Saturation popover.
use super::*;
use gpui::AnimationExt;
pub(super) struct Advanced {
    owner: WeakEntity<BoostEditor>,
    focus: gpui::FocusHandle,
    drag: Option<usize>,
    bounds: [Rc<Cell<Option<Bounds<gpui::Pixels>>>>; 3],
    _outside: gpui::Subscription,
}
impl BoostEditor {
    pub(super) fn close_advanced(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.advanced.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }
    pub(super) fn toggle_advanced(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.advanced.is_some() {
            self.close_advanced(cx);
            return;
        }
        // The press on this button already closed it as a click outside.
        if self.advanced_closed_at.take().is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(400)) {
            return;
        }
        let at = window.bounds().origin + point(px(-20.), px(272.));
        let parent = crate::native::view_of(window);
        let owner = cx.entity().downgrade();
        let owner_handle = owner.clone();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                at,
                size(px(224.), px(208.)),
            ))),
            titlebar: None,
            kind: WindowKind::Normal,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        // The popover reads the editor while it first renders, so it opens
        // once this update has finished.
        window.defer(cx, move |_, cx| {
            let Ok(handle) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window);
                    // A click anywhere else (the editor, the page, another app)
                    // takes the key window away and closes the popover.
                    let outside = cx.observe_window_activation(window, |this: &mut Advanced, window, cx| {
                        if !window.is_window_active() {
                            let _ = this.owner.update(cx, |editor, _| editor.advanced_closed_at = Some(std::time::Instant::now()));
                            this.close(window, cx);
                        }
                    });
                    Advanced {
                        owner,
                        focus,
                        drag: None,
                        bounds: std::array::from_fn(|_| Rc::new(Cell::new(None))),
                        _outside: outside,
                    }
                })
            }) else {
                return;
            };
            let kept = owner_handle
                .update(cx, |editor, _| {
                    editor.advanced.get_or_insert(handle) == &handle
                })
                .unwrap_or(false);
            if !kept {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
        });
    }
}
impl Advanced {
    fn set(&mut self, index: usize, x: gpui::Pixels, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds[index].get() else {
            return;
        };
        let value = (f32::from(x - bounds.left()) / f32::from(bounds.size.width) * 200.)
            .clamp(0., 200.) as u16;
        let _ = self.owner.update(cx, |editor, cx| {
            editor.edit(cx, |boost| match index {
                0 => boost.appearance.contrast = Some(value),
                1 => boost.appearance.brightness = Some(value),
                _ => boost.appearance.saturation = Some(value),
            })
        });
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.owner.update(cx, |editor, _| editor.advanced = None);
        window.remove_window();
    }
}
impl Render for Advanced {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let values = self
            .owner
            .read_with(cx, |owner, cx| {
                owner.boost(cx).map(|boost| {
                    [
                        boost.appearance.contrast.unwrap_or(100),
                        boost.appearance.brightness.unwrap_or(100),
                        boost.appearance.saturation.unwrap_or(100),
                    ]
                })
            })
            .ok()
            .flatten()
            .unwrap_or([100; 3]);
        let mut root = div()
            .size_full()
            .rounded(px(20.))
            .bg(gpui::hsla(0., 0., 0.94, 0.97))
            .border_1()
            .border_color(gpui::hsla(0., 0., 0.65, 0.7))
            .p(px(18.))
            .pt(px(22.))
            .text_color(rgb(0x333333))
            .text_size(px(12.))
            .font_family(".SystemUIFont")
            .key_context("BoostEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| this.close(window, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag = None),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if let Some(index) = this.drag {
                    this.set(index, event.position.x, cx);
                }
            }));
        for (index, label) in ["Contrast", "Brightness", "Original Saturation"]
            .into_iter()
            .enumerate()
        {
            let measured = self.bounds[index].clone();
            let value = values[index];
            root = root.child(
                div().mb(px(22.)).child(label).child(
                    div()
                        .id(gpui::SharedString::from(format!("boost-adjust-{index}")))
                        .debug_selector(move || format!("boost-adjust-{index}"))
                        .relative()
                        .h(px(16.))
                        .w_full()
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                this.drag = Some(index);
                                this.set(index, event.position.x, cx);
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(9.))
                                .h(px(6.))
                                .w_full()
                                .rounded_full()
                                .bg(rgb(0xffffff)),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(4.))
                                .left(px(f32::from(value) / 200. * 170.))
                                .size(px(16.))
                                .rounded_full()
                                .bg(rgb(0xffffff))
                                .shadow_sm(),
                        )
                        .child(
                            gpui::canvas(
                                move |bounds, _, _| {
                                    measured.set(Some(bounds));
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        ),
                ),
            );
        }
        // Arc fades the popover in as it drops a few points into place.
        if crate::native::prefers_reduced_motion() {
            return root.into_any_element();
        }
        root.relative()
            .with_animation(
                "boost-advanced-in",
                gpui::Animation::new(std::time::Duration::from_millis(200)).with_easing(|t: f32| 1. - (1. - t).powi(3)),
                |root, delta| root.opacity(delta).top(px(-6. * (1. - delta))),
            )
            .into_any_element()
    }
}
