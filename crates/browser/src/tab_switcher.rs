//! Arc's Recent Items: five window-local previews while Control is held.
use crate::{
    material::{Backdrop, SurfaceStyle},
    native,
    pages::Pages,
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use browser_core::{recent_items::RecentSelection, TabId};
use gpui::{
    div, point, prelude::*, px, rgb, size, AnyElement, Bounds, Context, FocusHandle, Hsla,
    StyledImage, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, SystemTime},
};

gpui::actions!(recent_items, [NextRecentItem, PreviousRecentItem]);
const ITEM_WIDTH: f32 = 182.;
const ITEM_HEIGHT: f32 = 158.;
const PADDING: f32 = 16.;
const PREVIEW_WIDTH: f32 = 158.;
const PREVIEW_HEIGHT: f32 = 105.;
const SHADOW_MARGIN: f32 = 144.;

pub(crate) struct Switcher {
    pub(crate) selection: RecentSelection,
    pub(crate) overlay: Option<WindowHandle<Overlay>>,
    focus: FocusHandle,
    images: PreviewImages,
    ready: bool,
    _snapshots: Option<gpui::Task<()>>,
    _modifiers: Option<gpui::Task<()>>,
}

impl BrowserWindow {
    pub(crate) fn show_recent_items(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(switcher) = &mut self.tab_switcher {
            switcher.selection.advance(if backwards { -1 } else { 1 });
            cx.notify();
            return;
        }
        if crate::quit::is_quitting() || self.transient.is_some() {
            return;
        }
        self.recent_items.record(&self.browser);
        if self.recent_items.visible(&self.browser).len() < 2 {
            return;
        }
        self.suspend_popup_focus_for_utility();
        self.close_command_bar_with_focus(PopupFocus::OnActivation, cx);
        self.recent_items.record(&self.browser);
        let Some(selection) =
            RecentSelection::new(self.recent_items.visible(&self.browser), backwards)
        else {
            return;
        };
        let focus = cx.focus_handle();
        let preview_tabs: Vec<_> = selection
            .items()
            .iter()
            .map(|tab| {
                self.browser
                    .split_for(*tab)
                    .map_or(*tab, |split| split.active)
            })
            .collect();
        let pending = request_previews(self.pages.as_ref(), &preview_tabs);
        let ready = pending.is_empty()
            || self.pages.as_ref().is_some_and(|pages| {
                let pages = pages.borrow();
                pending
                    .iter()
                    .all(|(tab, _)| pages.snapshot(*tab).is_some())
            });
        self.tab_switcher = Some(Switcher {
            selection,
            overlay: None,
            focus: focus.clone(),
            images: Default::default(),
            ready,
            _snapshots: None,
            _modifiers: None,
        });
        if let Some(pages) = &self.pages {
            pages.borrow_mut().request_ui_focus();
        }
        self.poll_recent_previews(pending, focus.clone(), cx);
        cx.notify();
        let owner = cx.weak_entity();
        let main = self.main_window;
        let parent = self.native_view;
        let fallback = window.bounds();
        // The test backend has no global physical-modifier query. Carry its
        // simulated triggering state into the new child, just as the real
        // platform supplies NSEvent/OS modifier state at window creation.
        #[cfg(test)]
        let trigger_modifiers = window.modifiers();
        cx.defer(move |cx| {
            let Some(count) = owner
                .read_with(cx, |view, _| {
                    view.tab_switcher
                        .as_ref()
                        .filter(|s| s.focus == focus)
                        .map(|s| s.selection.items().len())
                })
                .ok()
                .flatten()
            else {
                return;
            };
            let root = main
                .update(cx, |_, window, _| window.bounds())
                .unwrap_or(fallback);
            let bounds = overlay_bounds(root, count);
            let handle = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    kind: WindowKind::Normal,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Recent Items");
                    #[cfg(test)]
                    window.set_modifiers(trigger_modifiers);
                    if let (Some(parent), Some(child)) = (parent, native::view_of(window)) {
                        native::prepare_toast_window(child);
                        // Attachment disables the native shadow; the bounded GPUI
                        // card paints the reference's wider lower shadow.
                        native::attach_overlay(parent, child, false);
                    }
                    focus.focus(window);
                    cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::RecentItems, cx))
                },
            );
            if let Ok(handle) = handle {
                let attached = owner
                    .update(cx, |view, _| {
                        let Some(switcher) =
                            view.tab_switcher.as_mut().filter(|s| s.focus == focus)
                        else {
                            return false;
                        };
                        switcher.overlay = Some(handle);
                        true
                    })
                    .unwrap_or(false);
                if !attached {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    return;
                }
                let (held, poll) = handle
                    .update(cx, |_, window, _| {
                        window.activate_window();
                        let physical = native::physical_control_is_held(window);
                        (
                            physical.unwrap_or(window.modifiers().control),
                            physical.is_some(),
                        )
                    })
                    .unwrap_or((false, false));
                if !held {
                    let _ = owner.update(cx, |view, cx| {
                        view.finish_recent_items(true, PopupFocus::Restore, cx)
                    });
                } else if poll {
                    let _ = owner.update(cx, |view, cx| {
                        // A child created while Control is already held can miss
                        // the native flags-changed transition. Query the shared
                        // platform interface too, so release cannot strand it.
                        let task = cx.spawn(async move |owner, cx| loop {
                            cx.background_executor()
                                .timer(Duration::from_millis(16))
                                .await;
                            let held = main
                                .update(cx, |_, window, _| native::physical_control_is_held(window))
                                .ok()
                                .flatten();
                            let keep = owner
                                .update(cx, |view, cx| {
                                    if view
                                        .tab_switcher
                                        .as_ref()
                                        .is_none_or(|state| state.overlay != Some(handle))
                                    {
                                        return false;
                                    }
                                    if held == Some(false) {
                                        view.finish_recent_items(true, PopupFocus::Restore, cx);
                                        return false;
                                    }
                                    true
                                })
                                .unwrap_or(false);
                            if !keep {
                                break;
                            }
                        });
                        if let Some(state) = &mut view.tab_switcher {
                            state._modifiers = Some(task);
                        }
                    });
                }
            }
        });
    }

    fn poll_recent_previews(
        &mut self,
        pending: Vec<(TabId, u64)>,
        focus: FocusHandle,
        cx: &mut Context<Self>,
    ) {
        let Some(pages) = self.pages.clone().filter(|_| !pending.is_empty()) else {
            return;
        };
        let started = cx.background_executor().now();
        let task = cx.spawn(async move |owner, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let elapsed = cx
                .background_executor()
                .now()
                .saturating_duration_since(started);
            let (available, finished) = {
                let pages = pages.borrow();
                (
                    pending.iter().all(|(tab, request)| {
                        pages.snapshot(*tab).is_some() || !pages.snapshot_pending(*tab, *request)
                    }),
                    pending
                        .iter()
                        .all(|(tab, request)| !pages.snapshot_pending(*tab, *request)),
                )
            };
            let keep = owner
                .update(cx, |view, cx| {
                    let Some(state) = view.tab_switcher.as_mut().filter(|s| s.focus == focus)
                    else {
                        return false;
                    };
                    // This is capture readiness, not a presentation animation. A
                    // failed/slow capture cannot leave the keyboard UI invisible.
                    state.ready |= available || elapsed >= Duration::from_millis(250);
                    cx.notify();
                    !finished && elapsed < Duration::from_secs(2)
                })
                .unwrap_or(false);
            if !keep {
                break;
            }
        });
        if let Some(state) = &mut self.tab_switcher {
            state._snapshots = Some(task);
        }
    }

    pub(crate) fn finish_recent_items(
        &mut self,
        commit: bool,
        focus: PopupFocus,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.tab_switcher.take() else {
            return;
        };
        if commit {
            if let Some(tab) = state
                .selection
                .selected()
                .filter(|id| self.browser.all_tabs().any(|tab| tab.id == *id))
            {
                self.close_find_with_focus(PopupFocus::OnActivation, cx);
                let profile = self.browser.user_profile_id_for_tab(tab);
                let index = self
                    .browser
                    .spaces
                    .iter()
                    .position(|space| {
                        space
                            .pinned
                            .iter()
                            .chain(&space.today)
                            .any(|entry| entry.id == tab)
                    })
                    .or_else(|| {
                        (profile != Some(self.browser.active_user_profile_id()))
                            .then(|| {
                                self.browser
                                    .spaces
                                    .iter()
                                    .position(|space| Some(space.user_profile_id) == profile)
                            })
                            .flatten()
                    });
                self.edit_browser(|browser| {
                    if let Some(index) = index {
                        browser.switch_space(index);
                    }
                    browser.activate_sidebar_item(tab, SystemTime::now());
                });
                if !self.page_content_visible() {
                    self.sidebar_navigation.controller.select(
                        &self.browser,
                        crate::sidebar_pages::SidebarPage::Space(self.browser.space().id),
                    );
                    self.sync_navigation_visibility();
                }
                self.settle_sidebar_pager(cx);
            }
        }
        self.close_popup_overlay(state.overlay, focus, true, cx);
        cx.notify();
    }

    pub(crate) fn render_recent_items(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = &mut self.tab_switcher else {
            return div().into_any_element();
        };
        state.selection.reconcile(&self.browser);
        if state.selection.items().is_empty() {
            let owner = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = owner.update(cx, |view, cx| {
                    view.finish_recent_items(false, PopupFocus::Restore, cx)
                });
            });
            return div().into_any_element();
        }
        let items = state.selection.items().to_vec();
        let selected = state.selection.selected_index();
        let focus = state.focus.clone();
        let ready = state.ready;
        let material = SurfaceStyle {
            fill: rgb(0x1b1b1c).into(),
            foreground: rgb(0xffffff).into(),
            secondary: rgb(0xbdbdc4).into(),
            border: gpui::hsla(0., 0., 1., 0.20),
            hover: gpui::transparent_black(),
            selected: rgb(0x383c87).into(),
        }
        .resolve(Backdrop::Opaque, false);
        let mut root = div()
            .id("recent-items-overlay")
            .debug_selector(|| "recent-items-overlay".into())
            .size_full()
            .track_focus(&focus)
            .key_context("RecentItems")
            .flex()
            .items_center()
            .justify_center()
            .on_modifiers_changed(cx.listener(
                |view, event: &gpui::ModifiersChangedEvent, _, cx| {
                    if !event.modifiers.control {
                        view.finish_recent_items(true, PopupFocus::Restore, cx);
                    }
                },
            ))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        view.finish_recent_items(false, PopupFocus::Restore, cx);
                        cx.stop_propagation();
                    }
                    "enter" => {
                        view.finish_recent_items(true, PopupFocus::Restore, cx);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.finish_recent_items(false, PopupFocus::Restore, cx)
                }),
            );
        if ready {
            let rows = items
                .into_iter()
                .enumerate()
                .map(|(index, tab)| {
                    let active = index == selected;
                    let title = self.browser.item_title(tab);
                    let preview_tab = self
                        .browser
                        .split_for(tab)
                        .map_or(tab, |split| split.active);
                    let snapshot = self
                        .pages
                        .as_ref()
                        .and_then(|pages| pages.borrow().snapshot(preview_tab));
                    let image = snapshot.and_then(|(_, png)| {
                        self.tab_switcher
                            .as_mut()
                            .unwrap()
                            .images
                            .image(preview_tab, &png)
                    });
                    let icon = self.recent_icon(preview_tab, material.foreground);
                    let mut preview = div()
                        .w(px(PREVIEW_WIDTH))
                        .h(px(PREVIEW_HEIGHT))
                        .flex_none()
                        .relative()
                        .overflow_hidden()
                        .rounded(px(5.))
                        .bg(rgb(0x333437));
                    if let Some(image) = image {
                        preview = preview.child(
                            gpui::img(image)
                                .size_full()
                                .rounded(px(5.))
                                .object_fit(gpui::ObjectFit::Fill),
                        );
                    }
                    div()
                        .id(("recent-item", tab.get()))
                        .debug_selector(move || format!("recent-item-{index}"))
                        .w(px(ITEM_WIDTH))
                        .h(px(ITEM_HEIGHT))
                        .flex_none()
                        .p(px(12.))
                        .rounded(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .when(active, |item| item.bg(material.selected))
                        .cursor_pointer()
                        .on_mouse_move(cx.listener(move |view, _, _, cx| {
                            if let Some(state) = &mut view.tab_switcher {
                                state.selection.select(index);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if let Some(state) = &mut view.tab_switcher {
                                state.selection.select(index);
                            }
                            view.finish_recent_items(true, PopupFocus::Restore, cx);
                        }))
                        .child(preview)
                        .child(
                            div()
                                .h(px(17.))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(icon)
                                .child(
                                    div()
                                        .w(px(PREVIEW_WIDTH - 24.))
                                        .flex_none()
                                        .overflow_hidden()
                                        .child(crate::sidebar_title::SidebarTitle::new(title)),
                                ),
                        )
                })
                .collect::<Vec<_>>();
            root = root.child(
                div()
                    .id("recent-items-card")
                    .debug_selector(|| "recent-items-card".into())
                    .relative()
                    .w(px(card_width(rows.len())))
                    .h(px(ITEM_HEIGHT + 2. * PADDING))
                    .p(px(PADDING))
                    .rounded(px(24.))
                    .bg(material.fill)
                    .text_color(material.foreground)
                    .font_family(".SystemUIFont")
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::hsla(0., 0., 0., 0.55),
                        offset: point(px(0.), px(40.)),
                        blur_radius: px(80.),
                        spread_radius: px(10.),
                    }])
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(div().flex().children(rows))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(px(24.))
                            .border_1()
                            .border_color(material.border),
                    ),
            );
        }
        root.into_any_element()
    }

    fn recent_icon(&self, tab: TabId, ink: Hsla) -> AnyElement {
        let model = self.browser.all_tabs().find(|entry| entry.id == tab);
        if let Some(image) = model.and_then(|entry| self.favicon_image_for(entry)) {
            return gpui::img(image)
                .size(px(16.))
                .flex_none()
                .into_any_element();
        }
        if let Some(icon) = model.and_then(|entry| entry.custom_icon.as_ref()) {
            return div()
                .size(px(16.))
                .flex_none()
                .child(crate::icons::glyph(icon))
                .into_any_element();
        }
        div()
            .size(px(16.))
            .flex_none()
            .rounded(px(4.))
            .bg(Hsla { a: 0.15, ..ink })
            .into_any_element()
    }
}

fn request_previews(pages: Option<&Rc<RefCell<Pages>>>, items: &[TabId]) -> Vec<(TabId, u64)> {
    let Some(pages) = pages else {
        return Vec::new();
    };
    let mut pages = pages.borrow_mut();
    if !pages.supports_snapshots() {
        return Vec::new();
    }
    items
        .iter()
        .filter_map(|tab| pages.request_snapshot(*tab).map(|request| (*tab, request)))
        .collect()
}
fn card_width(count: usize) -> f32 {
    count as f32 * ITEM_WIDTH + 2. * PADDING
}
fn overlay_bounds(parent: Bounds<gpui::Pixels>, count: usize) -> Bounds<gpui::Pixels> {
    let width = f32::from(parent.size.width).max(card_width(count) + 2. * SHADOW_MARGIN);
    let height = f32::from(parent.size.height).max(ITEM_HEIGHT + 2. * PADDING + 2. * SHADOW_MARGIN);
    Bounds::new(
        parent.origin
            - point(
                px((width - f32::from(parent.size.width)) / 2.),
                px((height - f32::from(parent.size.height)) / 2.),
            ),
        size(px(width), px(height)),
    )
}

// Cache the reference-sized, top-aligned thumbnail rather than asking the GPU
// to sample a full-page capture at one fifth scale. This preserves small text
// and applies the rounded image mask to the visible crop, not the source page.
#[derive(Default)]
struct PreviewImages {
    images: std::collections::HashMap<
        TabId,
        (
            std::sync::Arc<Vec<u8>>,
            Option<std::sync::Arc<gpui::RenderImage>>,
        ),
    >,
}
impl PreviewImages {
    fn image(
        &mut self,
        tab: TabId,
        png: &std::sync::Arc<Vec<u8>>,
    ) -> Option<std::sync::Arc<gpui::RenderImage>> {
        if let Some((_, image)) = self
            .images
            .get(&tab)
            .filter(|(cached, _)| std::sync::Arc::ptr_eq(cached, png))
        {
            return image.clone();
        }
        let decoded = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .ok()
            .map(|source| {
                let source = source.into_rgba8();
                let crop_height = ((source.width() as f32 * PREVIEW_HEIGHT / PREVIEW_WIDTH).ceil()
                    as u32)
                    .min(source.height());
                let crop = image::imageops::crop_imm(&source, 0, 0, source.width(), crop_height)
                    .to_image();
                let height = (crop.height() as f32 * PREVIEW_WIDTH / crop.width().max(1) as f32)
                    .round() as u32;
                let thumb = image::imageops::resize(
                    &crop,
                    PREVIEW_WIDTH as u32,
                    height.max(1),
                    image::imageops::FilterType::Lanczos3,
                );
                let mut canvas = image::RgbaImage::from_pixel(
                    PREVIEW_WIDTH as u32,
                    PREVIEW_HEIGHT as u32,
                    image::Rgba([51, 52, 55, 255]),
                );
                image::imageops::overlay(&mut canvas, &thumb, 0, 0);
                for pixel in canvas.pixels_mut() {
                    pixel.0.swap(0, 2);
                }
                std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(canvas)]))
            });
        self.images.insert(tab, (png.clone(), decoded.clone()));
        decoded
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn thumbnails_keep_the_page_top_and_cache_the_same_capture() {
        use std::sync::Arc;
        let tab =
            browser_core::Browser::new().new_tab("https://preview.test", SystemTime::UNIX_EPOCH);
        let mut source = image::RgbaImage::from_pixel(158, 300, image::Rgba([17, 43, 211, 255]));
        for (_, y, pixel) in source.enumerate_pixels_mut() {
            if y >= 105 {
                *pixel = image::Rgba([255, 0, 0, 255]);
            }
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = Arc::new(png.into_inner());
        let mut cache = PreviewImages::default();
        let first = cache.image(tab, &png).unwrap();
        assert_eq!(first.as_bytes(0).unwrap().len(), 158 * 105 * 4);
        assert!(first
            .as_bytes(0)
            .unwrap()
            .chunks_exact(4)
            .all(|pixel| pixel == [211, 43, 17, 255]));
        assert!(Arc::ptr_eq(&first, &cache.image(tab, &png).unwrap()));
        assert!(cache.image(tab, &Arc::new(vec![0, 1, 2])).is_none());
    }
}
