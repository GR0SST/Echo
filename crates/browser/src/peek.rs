//! Page-local Peek. A transparent child hosts native content inside the card.
use std::time::{Instant, SystemTime};
use browser_core::splits::Placement;
use gpui::{actions, div, point, prelude::*, px, AnyElement, App, Context, KeyBinding,
    WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions};
use crate::{icons::{self, Icon}, native, new_window::NormalProfile, pages::StorageSelection,
    profile::ProfileHandle, transient::Transient, window::BrowserWindow};

actions!(peek, [ExitPeek]);
pub(crate) struct PeekOwner {
    pub(crate) owner: WeakEntity<BrowserWindow>,
    pub(crate) history_token: Option<u64>,
    pub(crate) redo_close: Option<u64>,
    sidebar: f32,
    opened: Instant,
    pub(crate) origin: gpui::Point<gpui::Pixels>,
    closing: Option<Instant>,
    pub(crate) detached: bool,
    close_requested: bool,
    close_image: Option<std::sync::Arc<gpui::RenderImage>>,
}

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", ExitPeek, Some("Peek"))]);
}

impl BrowserWindow {
    /// Clearing site data reloads the covered source. CEF can make its parent
    /// key during that navigation; keep the settings document as the responder.
    pub(crate) fn restore_site_settings_peek_focus(&mut self, cx: &mut Context<Self>) {
        let Some(child) = self.peek_child else { return; };
        if !self.site_settings_page.return_focus.is_some_and(|(pending, until)| pending.window_id() == child.window_id() && until >= cx.background_executor().now()) {
            self.site_settings_page.return_focus = None;
            return;
        }
        let root = self.main_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let settings = child.update(cx, |view, _, _| {
                view.site_settings_is_open() && view.peek_owner.as_ref().is_some_and(|state| !state.close_requested && !state.detached)
            }).unwrap_or(false);
            if !settings { return; }
            let current = root.update(cx, |_, window, cx| {
                if !window.is_window_active() { return false; }
                owner.update(cx, |view, cx| {
                    if !view.peek_child.is_some_and(|current| current.window_id() == child.window_id()) { return false; }
                    if !view.site_settings_page.return_focus.is_some_and(|(pending, until)| pending.window_id() == child.window_id() && until >= cx.background_executor().now()) { return false; }
                    view.site_settings_page.return_focus = None;
                    if let Some(pages) = &view.pages { pages.borrow_mut().request_ui_focus(); }
                    true
                }).unwrap_or(false)
            }).unwrap_or(false);
            if !current { return; }
            let _ = child.update(cx, |view, window, cx| {
                if !view.site_settings_is_open() || !view.peek_owner.as_ref().is_some_and(|state| !state.close_requested && !state.detached) { return; }
                window.activate_window();
                if let Some(pages) = &view.pages { pages.borrow_mut().focus_ui(); }
                view.focus_site_settings(window);
                cx.notify();
            });
        });
    }
    pub(crate) fn open_peek(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(url)=url::Url::parse(&url) else{return;};
        if !matches!(url.scheme(),"http"|"https"){return;}
        self.open_peek_document(url,None,window,cx);
    }
    pub(crate) fn open_site_settings_peek(&mut self,url:String,window:&mut Window,cx:&mut Context<Self>) {
        if !crate::site_settings_page::is_site_url(&url){return;}
        let Ok(url)=url::Url::parse(&url) else{return;};
        self.open_peek_document(url,None,window,cx);
    }
    /// Archive rows may belong to another local website profile. Keep its
    /// cookies/storage without changing the host window's current Space.
    pub(crate) fn open_peek_for_profile(&mut self, profile: browser_core::UserProfileId, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() || self.browser.user_profile(profile).is_none() { return; }
        let Ok(url) = url::Url::parse(&url) else { return; };
        if !matches!(url.scheme(), "http" | "https") { return; }
        self.open_peek_document(url, Some(profile), window, cx);
    }
    fn open_peek_document(&mut self,url:url::Url,profile:Option<browser_core::UserProfileId>,window:&mut Window,cx:&mut Context<Self>) {
        if self.peek_owner.is_some() || self.transient.is_some() || self.peek_child.is_some() || crate::quit::is_quitting(){return;}
        self.close_command_bar(cx);
        let site_settings_source=if crate::site_settings_page::is_site_url(url.as_str()){crate::site_settings_page::DataSource::capture(self,cx)}else{None};
        let source = NormalProfile { profile: self.profile.handle.clone(), pages: self.pages.clone() };
        let space = self.browser.space().id;
        let theme = self.browser.space().theme.clone();
        let hue = self.browser.space().hue;
        let website_profile = profile.unwrap_or_else(|| self.browser.active_user_profile_id());
        let private = self.browser.is_private();
        let share_history = !private && !self.browser.is_blank();
        let storage = if private { StorageSelection::Current } else { StorageSelection::for_profile(website_profile) };
        let bounds = window.bounds();
        let origin = self.native_view.and_then(native::pointer_in_view)
            .map(|(x,y)| point(px(x),px(y))).unwrap_or_else(|| window.mouse_position());
        let sidebar = if self.sidebar_navigation.controller.selected()==crate::sidebar_pages::SidebarPage::Library {0.}else{self.sidebar_width() * self.sidebar_amount()};
        let parent = self.native_view;
        let owner = cx.weak_entity();
        if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
        cx.defer(move |cx| {
            if owner.upgrade().is_none() || crate::quit::is_quitting() { return; }
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            }, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, native::view_of(window)) { native::attach_overlay(parent, child, false); }
                let pages = source.pages.as_ref().and_then(|pages| pages.borrow().independent_in_storage(
                    native::view_of(window).unwrap_or(std::ptr::null_mut()), &storage));
                let mut browser = if private { browser_core::Browser::new_private() } else { source.profile.blank_browser(website_profile) };
                browser.spaces[0].hue = hue;
                browser.spaces[0].theme = theme;
                let tab=browser.new_tab(url.as_str(), SystemTime::now());
                if crate::site_settings_page::is_site_url(url.as_str()){browser.set_title(tab,"Settings");}
                let profile = if share_history { ProfileHandle::new_with_shared_history(browser, &source.profile, cx) } else { ProfileHandle::new(browser, cx) };
                let selection = profile.selection();
                cx.new(|cx| {
                    let mut view = BrowserWindow::with_profile_pages(profile, Some(selection), pages, window, cx);
                    if let Some(pages) = &view.pages { pages.borrow_mut().peek_mode = true; }
                    view.site_settings_page.source=site_settings_source;
                    view.sidebar_visible = false;
                    view.transient = Some(Transient { source, space, picker: None,last_active:cx.background_executor().now(),last_active_wall:SystemTime::now(),active:true,archive_retained:false,archive_reason:browser_core::ArchiveReason::Manual });
                    view.peek_owner = Some(PeekOwner { owner: owner.clone(), history_token: None, redo_close: None, sidebar, origin, opened: cx.background_executor().now(), closing: None, detached: false, close_requested: false, close_image: None });
                    view.focus(window);
                    view
                })
            });
            if let Ok(handle) = handle {
                let _ = owner.update(cx, |view, cx| { view.peek_child = Some(handle); cx.notify(); });
            }
        });
    }

    pub(crate) fn restore_peek(&mut self, token: u64, window: &mut Window, cx: &mut Context<Self>) {
        let bounds = window.bounds();
        let sidebar = self.sidebar_width() * self.sidebar_amount();
        let parent = self.native_view;
        let owner = cx.weak_entity();
        let history = self.profile.handle.clone();
        if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
        cx.defer(move |cx| {
            if owner.upgrade().is_none() || crate::quit::is_quitting() { crate::peek_history::discard(token, cx); return; }
            let Some(document) = crate::peek_history::take_document(token, cx) else { return; };
            let origin = document.origin;
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            }, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, native::view_of(window)) { native::attach_overlay(parent, child, false); }
                let pages = document.pages.as_ref().map(|pages| {
                    let parent = native::view_of(window).unwrap_or(std::ptr::null_mut());
                    #[cfg(test)]
                    let parent = if parent.is_null() { std::ptr::dangling_mut::<std::ffi::c_void>() } else { parent };
                    let mut pages = pages.borrow().for_host(parent);
                    pages.peek_mode = true;
                    pages
                });
                cx.new(|cx| {
                    let mut view = BrowserWindow::with_profile_pages(document.profile, Some(document.selection), pages, window, cx);
                    view.sidebar_visible = false;
                    view.transient = Some(Transient { source: document.source, space: document.space, picker: None,last_active:cx.background_executor().now(),last_active_wall:SystemTime::now(),active:true,archive_retained:false,archive_reason:browser_core::ArchiveReason::Manual });
                    view.peek_owner = Some(PeekOwner { owner: owner.clone(), history_token: Some(token), redo_close: None,
                        sidebar, origin, opened: cx.background_executor().now(), closing: None, detached: false, close_requested: false, close_image: None });
                    view.focus(window);
                    view
                })
            });
            if let Ok(handle) = handle {
                let child = handle.update(cx, |_, _, cx| cx.weak_entity());
                let _ = owner.update(cx, |view, cx| { view.peek_child = Some(handle); cx.notify(); });
                if let Ok(child) = child { crate::peek_history::restored(token, owner, history, child, cx); }
            } else { history.edit_settings(|browser| browser.discard_peek_history(token)); }
        });
    }

    pub(crate) fn resize_peek(&self, window: &Window, cx: &mut Context<Self>) {
        let Some(child) = self.peek_child else { return; };
        let size = window.viewport_size();
        let sidebar = if self.sidebar_navigation.controller.selected()==crate::sidebar_pages::SidebarPage::Library {0.}else{self.sidebar_width() * self.sidebar_amount()};
        cx.defer(move |cx| {
            let _ = child.update(cx, |view, window, cx| {
                let Some(state) = &mut view.peek_owner else { return; };
                state.sidebar = sidebar;
                if window.viewport_size() != size { window.resize(size); }
                cx.notify();
            });
        });
    }

    pub(crate) fn exit_peek(&mut self, cx: &mut Context<Self>) {
        if self.max.preview.open.is_some() {self.dismiss_max_preview(cx);return;}
        if let Some(state) = &mut self.peek_owner {
            if state.close_requested { return; }
            state.close_requested = true;
            let capture = self.browser.active_tab().and_then(|tab| {
                let pages = self.pages.as_ref()?;
                if !pages.borrow().supports_snapshots() { return None; }
                let request = pages.borrow_mut().request_snapshot(tab.id)?;
                Some((tab.id, request, pages.clone()))
            });
            let child = cx.weak_entity();
            cx.spawn(async move |_, cx| {
                // Keep the live viewport fixed until its photograph arrives. Never
                // shrink a native web view: that reflows the document during exit.
                let mut image = None;
                if let Some((tab, request, pages)) = capture {
                    for _ in 0..10 {
                        let snapshot = pages.borrow().snapshot(tab).filter(|(id, _)| *id == request);
                        if let Some((_, png)) = snapshot {
                            image = child.update(cx, |view, _| view.snapshot_images.borrow_mut().image(tab, &png)).ok().flatten();
                            break;
                        }
                        if !pages.borrow().snapshot_pending(tab, request) { break; }
                        cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
                    }
                }
                if std::env::var_os("BROWSER_SCRIPT").is_some() { eprintln!("[peek] close snapshot ready={}", image.is_some()); }
                let _ = child.update(cx, |view, cx| {
                    if let Some(state) = &mut view.peek_owner {
                        state.close_image = image;
                        state.closing = Some(cx.background_executor().now());
                        cx.notify();
                    }
                });
                cx.background_executor().timer(std::time::Duration::from_millis(180)).await;
                let _ = child.update(cx, |view, cx| { view.retain_closed_peek(cx); view.close_window(cx); });
            }).detach();
        } else if let Some(child) = self.peek_child {
            cx.defer(move |cx| { let _ = child.update(cx, |view, _, cx| view.exit_peek(cx)); });
        }
    }

    pub(crate) fn detach_peek(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.peek_owner.as_mut() else { return; };
        if state.detached { return; }
        state.detached = true;
        if let Some(token) = state.history_token.take() { crate::peek_history::discard(token, cx); }
        let owner = state.owner.clone();
        let handle = self.main_window;
        cx.defer(move |cx| {
            let _ = owner.update(cx, |view, cx| {
                if view.peek_child.is_some_and(|child| child.window_id() == handle.window_id()) {
                    view.peek_child = None;
                    if let Some(pages) = &view.pages { pages.borrow_mut().request_page_focus(); }
                    let root = view.main_window;
                    let focus = view.focus_handle.clone();
                    cx.defer(move |cx| { let _ = root.update(cx, |_, window, _| { window.activate_window(); focus.focus(window); }); });
                    cx.notify();
                }
            });
        });
    }

    fn split_peek(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &self.peek_owner else { return; };
        let Some(tab) = self.browser.active_tab().cloned() else { return; };
        let pages = self.pages.clone();
        let favicon = self.browser.favicon(self.browser.active_user_profile_id(), &tab.url).map(Vec::from);
        let owner = state.owner.clone();
        let child = cx.weak_entity();
        cx.defer(move |cx| {
            let opened = owner.update(cx, |view, cx| {
                let Some(source) = view.browser.space().active_tab else { return false; };
                if !view.can_add_split(source) { return false; }
                view.begin_split(source, Placement::Right, cx);
                let Some(draft) = view.split_draft.as_ref() else { return false; };
                let pane = draft.pane;
                view.edit_browser(|browser| { browser.set_url(pane, tab.url.clone()); browser.set_title(pane, tab.title.clone()); browser.set_tab_muted(pane, tab.muted); });
                if !crate::transient::transfer_lookup_document(pages.as_ref(), tab.id, view.pages.as_ref(), pane, &view.browser) {
                    view.cancel_split_draft();
                    cx.notify();
                    return false;
                }
                if let Some(png) = &favicon {
                    view.edit_browser(|browser| { browser.cache_favicon(browser.active_user_profile_id(), &tab.url, png.clone()); });
                }
                view.split_draft.take();
                cx.notify();
                true
            }).unwrap_or(false);
            if opened { let _ = child.update(cx, |view, cx| view.close_window(cx)); }
        });
    }

    pub(crate) fn render_peek(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let state = self.peek_owner.as_ref().unwrap();
        let now = cx.background_executor().now();
        let elapsed = now.saturating_duration_since(state.opened).as_secs_f32();
        let closing = state.closing.map(|start| now.saturating_duration_since(start).as_secs_f32() / 0.18);
        let progress = if native::prefers_reduced_motion() { 1. } else {
            // A damped spring follows Arc's measured opening overshoot.
            let t = (elapsed / 0.34).min(1.);
            if t >= 1. { 1. } else { 1. - (-8. * t).exp() * ((12. * t).cos() + (8. / 12.) * (12. * t).sin()) }
        };
        // Arc overshoots the settled 618 px card by approximately one pixel.
        let amount = closing.map_or(progress.min(1.009), |t| 1. - t.clamp(0., 1.).powi(2));
        if elapsed < 0.34 || closing.is_some() { cx.on_next_frame(window, |_, _, cx| cx.notify()); }
        let close_image = state.close_image.clone();
        let sidebar = state.sidebar;
        let extent = window.viewport_size();
        let available = (f32::from(extent.width) - sidebar).max(0.);
        let target_width = available * 0.8;
        let (left, top, width, height) = if closing.is_some() {
            let x = f32::from(state.origin.x).clamp(sidebar, f32::from(extent.width));
            let y = f32::from(state.origin.y).clamp(12., f32::from(extent.height) - 12.);
            (x + (sidebar + available * 0.1 - x) * amount, y + (12. - y) * amount,
                target_width * amount, (f32::from(extent.height) - 24.) * amount)
        } else {
            let top = 12. + 40. * (1. - amount);
            (sidebar + available * 0.1 - 8. * (1. - amount), top,
                target_width * (0.82 + 0.18 * amount), (f32::from(extent.height) - top * 2.).max(0.))
        };
        let control = self.command_bar_panel_fill(false);
        let control_ink = if self.theme_is_dark() { gpui::white() } else { gpui::rgb(0x2f39ff).into() };
        let button = |id: &'static str, icon: AnyElement| div().id(id).debug_selector(move || id.into())
            .size(px(34.)).flex_none().rounded_full().bg(control).text_color(control_ink)
            .flex().items_center().justify_center().shadow_md()
            .hover(|style| style.bg(gpui::hsla(control.h, control.s, (control.l + 0.08).min(1.), 1.)))
            .child(icon);
        let content = div().key_context("Peek").track_focus(&self.focus_handle).size_full().relative()
            .child(div().absolute().size_0().track_focus(&self.native_action_focus))
            .child(div().id("peek-backdrop").absolute().top(px(19.)).bottom(px(19.)).left(px(sidebar + 9.)).right(px(19.))
                .rounded(px(8.)).opacity(0.7 * amount.clamp(0., 1.)).child(self.theme_surface(false)).on_click(cx.listener(|view, _, _, cx| view.exit_peek(cx))))
            .child(div().absolute().left(px(left)).top(px(top)).w(px(width)).h(px(height))
                .opacity(amount.clamp(0., 1.)).child(card_shadow()))
            .child(div().debug_selector(|| "peek-card".into()).absolute().left(px(left)).top(px(top))
                .w(px(width)).h(px(height)).rounded(px(8.)).bg(gpui::white())
                .when(self.site_settings_is_open(),|card|card.occlude())
                .when(self.pages.is_none(), |view| view.child(self.browser.active_tab().map(|tab| tab.url.clone()).unwrap_or_default()))
                .overflow_hidden()
                .when_some(close_image.clone(), |card, image| card.child(div().debug_selector(|| "peek-closing-image".into()).w(px(width)).h(px(height)).child(gpui::img(image).w(px(width)).h(px(height)).object_fit(gpui::ObjectFit::Fill))))
                .when(close_image.is_none() && self.site_settings_is_open(), |card|card.child(self.render_site_settings_page(cx)))
                .when(close_image.is_none() && !self.site_settings_is_open(), |card| card.children(self.render_page_viewport_canvas())))
            .child(div().absolute().left(px(left + width + 13.)).top(px(20.)).opacity(amount.clamp(0., 1.)).flex().flex_col().gap(px(9.5))
                .child(button("peek-close", icons::control_sized(Icon::Close, 24.)).on_click(cx.listener(|view, _, _, cx| view.exit_peek(cx))))
                .child(button("peek-expand", expand_icon()).on_click(cx.listener(|view, _, _, cx| view.promote_transient(None, cx))))
                .child(button("peek-split", icons::control(Icon::Split)).on_click(cx.listener(|view, _, _, cx| view.split_peek(cx)))))
            .into_any_element();
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, close_image.is_none()); }
        self.render_overlay_shell(content, None, cx)
    }
}

/// Measured over the reference's flat backdrop: 13% at one pixel, 3.4%
/// at five and below 1% by eight. Keep this narrow falloff independent of
/// the Space palette and of GPUI's much wider preset shadows.
fn card_shadow() -> AnyElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        let alpha = [0.059, 0.046, 0.0345, 0.0255, 0.0155, 0.0115, 0.007, 0.0027, 0.0023];
        for (index, alpha) in alpha.into_iter().enumerate().rev() {
            let spread = index as f32 + 1.;
            window.paint_quad(gpui::outline(bounds.dilate(px(spread)),
                gpui::hsla(0., 0., 0., alpha), Default::default()).corner_radii(px(8. + spread)));
        }
    }).size_full().into_any_element()
}

fn expand_icon() -> AnyElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        for points in [vec![(7.,3.),(3.,3.),(3.,7.)],vec![(3.,3.),(7.,7.)],vec![(11.,15.),(15.,15.),(15.,11.)],vec![(11.,11.),(15.,15.)]] {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (i,(x,y)) in points.into_iter().enumerate() {
                let p = bounds.origin + point(px(x),px(y));
                if i == 0 { path.move_to(p); } else { path.line_to(p); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, window.text_style().color); }
        }
    }).size(px(18.)).into_any_element()
}
