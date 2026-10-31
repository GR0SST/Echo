//! A sidebar tab dragged out of the window shows its page under the pointer
//! and, when released there, opens in a window of its own (Arc's tear-off).
//! Released over another such window, it joins that window instead.
use crate::{tab_drag::TabDrag, window::{BrowserWindow, Overlay, OverlayKind}};
use browser_core::TabId;
use gpui::{div, prelude::*, px, AnyElement, App, Bounds, Context, Pixels, Point, RenderImage, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};
use std::sync::Arc;
use std::time::SystemTime;

/// Width of the page picture that follows the pointer outside the window.
const THUMB_WIDTH: f32 = 140.;

pub(crate) struct TearOff {
    pub(crate) source: TabId,
    /// The pointer in screen coordinates.
    pub(crate) pointer: Point<Pixels>,
    /// The display the passive overlay covers.
    display: Bounds<Pixels>,
    overlay: Option<WindowHandle<Overlay>>,
    /// The window size the new window inherits.
    size: gpui::Size<Pixels>,
}

impl TearOff {
    pub(crate) fn overlay_handle(&self) -> Option<WindowHandle<Overlay>> { self.overlay }
}

impl BrowserWindow {
    /// Standalone pages can leave; Split Views and the last page of a
    /// standalone window stay.
    pub(crate) fn can_tear_off(&self, drag: &TabDrag) -> bool {
        self.accepts_tab_drag(drag) && !drag.is_split() && self.browser.split_for(drag.id).is_none()
            && !self.browser.is_private()
            && !(self.browser.is_blank() && self.browser.all_tabs().count() <= 1)
            && self.browser.all_tabs().any(|tab| tab.id == drag.id && !browser_core::omnibox::is_internal_url(&tab.url))
    }

    pub(crate) fn update_tear_off(&mut self, drag: &TabDrag, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        let outside = position.x < px(0.) || position.y < px(0.) || position.x >= viewport.width || position.y >= viewport.height;
        if !outside || !self.can_tear_off(drag) { self.close_tear_off(cx); return; }
        let bounds = window.bounds();
        let pointer = bounds.origin + position;
        let display = cx.displays().into_iter().map(|display| display.bounds()).find(|display| display.contains(&pointer))
            .or_else(|| window.display(cx).map(|display| display.bounds()))
            .unwrap_or(bounds);
        if self.tear_off.is_none() { self.request_tear_off_snapshot(drag.focus_page()); }
        let overlay = match self.tear_off.take() {
            Some(old) if old.display == display && old.source == drag.id => old.overlay,
            Some(old) => { if let Some(overlay) = old.overlay { remove_overlay(overlay, cx); } None }
            None => None,
        };
        self.tear_off = Some(TearOff { source: drag.id, pointer, display, overlay, size: bounds.size });
        self.ensure_tear_off_overlay(cx);
        cx.notify();
    }

    fn request_tear_off_snapshot(&self, tab: TabId) {
        let Some(pages) = &self.pages else { return; };
        let Ok(mut pages) = pages.try_borrow_mut() else { return; };
        if pages.supports_snapshots() && pages.has_page(tab) { pages.request_snapshot(tab); }
    }

    fn ensure_tear_off_overlay(&mut self, cx: &mut Context<Self>) {
        let Some(tear) = &self.tear_off else { return; };
        if tear.overlay.is_some() { return; }
        let display = tear.display;
        let source = tear.source;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.tear_off.as_ref().is_some_and(|tear| tear.overlay.is_none() && tear.source == source && tear.display == display)).unwrap_or(false) { return; }
            let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(display)), titlebar: None, focus: false,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default() };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    crate::native::set_overlay_passthrough(child);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::TearOff, window, cx))
            }) {
                let keep = browser.update(cx, |this, _| match &mut this.tear_off {
                    Some(tear) if tear.source == source && tear.display == display => { tear.overlay = Some(overlay); true }
                    _ => false,
                }).unwrap_or(false);
                if !keep { let _ = overlay.update(cx, |_, window, _| window.remove_window()); }
            }
        });
    }

    pub(crate) fn close_tear_off(&mut self, cx: &mut Context<Self>) {
        if let Some(tear) = self.tear_off.take() {
            if let Some(overlay) = tear.overlay { remove_overlay(overlay, cx); }
            cx.notify();
        }
    }

    fn tear_off_image(&self, tab: TabId) -> Option<Arc<RenderImage>> {
        let pages = self.pages.as_ref()?.try_borrow().ok()?;
        let (_, png) = pages.snapshot(tab)?;
        self.snapshot_images.borrow_mut().image(tab, &png)
    }

    pub(crate) fn render_tear_off(&self, window: &mut Window) -> AnyElement {
        let Some(tear) = &self.tear_off else { return div().into_any_element(); };
        let Some(tab) = self.browser.all_tabs().find(|tab| tab.id == tear.source) else { return div().into_any_element(); };
        let focus = self.browser.split_for(tab.id).map_or(tab.id, |split| split.active);
        let image = self.tear_off_image(focus);
        // The snapshot arrives asynchronously; keep polling until it does.
        if image.is_none() { window.request_animation_frame(); }
        let local = tear.pointer - tear.display.origin;
        let shadow = vec![gpui::BoxShadow { color: gpui::hsla(0., 0., 0., 0.35), offset: gpui::point(px(0.), px(6.)), blur_radius: px(18.), spread_radius: px(0.) }];
        let card = if let Some(image) = image {
            let size = image.size(0);
            let height = (THUMB_WIDTH * size.height.0 as f32 / size.width.0.max(1) as f32).clamp(60., 160.);
            div().w(px(THUMB_WIDTH)).h(px(height)).rounded(px(8.)).overflow_hidden().shadow(shadow)
                .child(gpui::img(image).size_full().object_fit(gpui::ObjectFit::Cover))
        } else {
            // Unloaded pages have no picture: show the row's icon and title.
            div().w(px(THUMB_WIDTH)).h(px(90.)).rounded(px(8.)).shadow(shadow).bg(gpui::rgba(0x000000e6))
                .flex().flex_col().items_center().justify_center().gap(px(10.)).px(px(10.))
                .text_color(gpui::white()).text_size(px(12.)).font_weight(gpui::FontWeight::BOLD)
                .child(self.render_favicon_on(tab, self.browser.space().hue, true))
                .child(div().w_full().truncate().text_center().child(self.browser.item_title(tab.id)))
        };
        div().size_full().child(div().debug_selector(|| "tear-off-card".into()).absolute()
            .left(local.x - px(THUMB_WIDTH / 2.)).top(local.y + px(8.))
            .child(card))
            .into_any_element()
    }

    /// Called when a tab drag ends for any reason. Only a release outside
    /// the window, with the tear-off still armed, moves the page.
    pub(crate) fn finish_tear_off(&mut self, drag: &TabDrag, cx: &mut Context<Self>) {
        let Some(tear) = self.tear_off.take() else { return; };
        if let Some(overlay) = tear.overlay { remove_overlay(overlay, cx); }
        if !self.can_tear_off(drag) { return; }
        let tab = drag.id;
        let pointer = tear.pointer;
        let size = tear.size;
        let source = cx.entity().downgrade();
        let me = self.main_window;
        cx.defer(move |cx| {
            let target = standalone_window_at(pointer, me, cx);
            match target {
                Some(target) => { move_into(source, target, tab, cx); }
                None => open_torn_window(source, tab, pointer, size, cx),
            }
        });
    }
}

fn remove_overlay(overlay: WindowHandle<Overlay>, cx: &mut Context<BrowserWindow>) {
    cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); });
}

/// Another standalone window of the same workspace under the pointer.
fn standalone_window_at(pointer: Point<Pixels>, source: gpui::AnyWindowHandle, cx: &mut App) -> Option<WindowHandle<BrowserWindow>> {
    cx.windows().into_iter().filter(|handle| *handle != source).filter_map(|handle| handle.downcast::<BrowserWindow>())
        .find(|handle| handle.update(cx, |view, window, _| view.browser.is_blank() && window.bounds().contains(&pointer)).unwrap_or(false))
}

/// What the destination needs to adopt a page from its source window.
struct Departure {
    tab: browser_core::Tab,
    protected: bool,
    profile: browser_core::UserProfileId,
    favicon: Option<Vec<u8>>,
    pages: Option<std::rc::Rc<std::cell::RefCell<crate::pages::Pages>>>,
    space: browser_core::Space,
}

fn departure(source: &gpui::WeakEntity<BrowserWindow>, tab: TabId, cx: &mut App) -> Option<Departure> {
    source.read_with(cx, |this, _| {
        let page = this.browser.all_tabs().find(|entry| entry.id == tab)?.clone();
        let profile = this.browser.user_profile_id_for_tab(tab)?;
        let protected = this.browser.is_favorite(tab) || this.browser.spaces.iter().any(|space| space.pinned.iter().any(|entry| entry.id == tab));
        let space = this.browser.spaces.iter().find(|space| space.pinned.iter().chain(&space.today).any(|entry| entry.id == tab))
            .unwrap_or(this.browser.space()).clone();
        Some(Departure { favicon: this.browser.favicon(profile, &page.url).map(Vec::from), tab: page, protected, profile, pages: this.pages.clone(), space })
    }).ok().flatten()
}

/// Adopts the page into `target`, carrying its live document, then removes
/// it from the source. Pinned pages and Favorites leave a copy behind
/// unchanged, so the destination loads its own document.
fn move_into(source: gpui::WeakEntity<BrowserWindow>, target: WindowHandle<BrowserWindow>, tab: TabId, cx: &mut App) -> bool {
    let Some(departure) = departure(&source, tab, cx) else { return false; };
    let adopted = target.update(cx, |view, window, cx| {
        if view.browser.active_user_profile_id() != departure.profile || view.browser.is_private() { return false; }
        let pages = view.pages.clone();
        let source_pages = departure.pages.clone();
        let protected = departure.protected;
        let adopted = view.edit_browser(|browser| browser.adopt_lookup_tab(departure.tab.clone(), SystemTime::now(), |browser, id| {
            protected || crate::transient::transfer_lookup_document(source_pages.as_ref(), tab, pages.as_ref(), id, browser)
        }).is_some());
        if !adopted {
            if let Some(pages) = &pages { pages.borrow_mut().reconcile_inventory(&view.browser); }
            return false;
        }
        if let Some(png) = &departure.favicon { view.edit_browser(|browser| browser.cache_favicon(departure.profile, &departure.tab.url, png.clone())); }
        window.activate_window();
        view.focus(window);
        cx.notify();
        true
    }).unwrap_or(false);
    if adopted && !departure.protected {
        let _ = source.update(cx, |this, cx| {
            if this.edit_browser(|browser| browser.detach_today_tab(tab)).is_some() {
                this.recent_items.forget(&[tab]);
                if let Some(pages) = &this.pages { pages.borrow_mut().sync(&this.browser, this.page_content_visible()); }
                this.after_tab_removal(cx);
            }
            cx.notify();
        });
    }
    adopted
}

/// Opens a standalone window for the Space the page came from, at the
/// release point, and moves the page into it.
fn open_torn_window(source: gpui::WeakEntity<BrowserWindow>, tab: TabId, pointer: Point<Pixels>, size: gpui::Size<Pixels>, cx: &mut App) {
    let Some(departure) = departure(&source, tab, cx) else { return; };
    let size = gpui::size(size.width.min(px(1400.)).max(px(700.)), size.height.min(px(900.)).max(px(500.)));
    // Hold the new window by its sidebar row, where the page left the old one.
    let origin = pointer - gpui::point(px(110.), px(110.));
    // Keep the whole window on the display it was dropped on.
    let display = cx.displays().into_iter().map(|display| display.bounds()).find(|display| display.contains(&pointer));
    let origin = display.map_or(origin, |display| gpui::point(
        origin.x.clamp(display.left(), (display.right() - size.width).max(display.left())),
        origin.y.clamp(display.top(), (display.bottom() - size.height).max(display.top()))));
    let Some(target) = crate::blank_window::open_torn(departure.profile, &departure.space, Bounds::new(origin, size), cx) else { return; };
    if !move_into(source, target, tab, cx) {
        let _ = target.update(cx, |view, _, cx| view.close_window(cx));
    }
}
