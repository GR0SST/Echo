//! Lookup popups keep their originating document and captured website profile.
use crate::{pages::{InputOrigin, PageIdentity}, peek::Source, window::BrowserWindow};
use gpui::{Context, WindowHandle};

impl BrowserWindow {
    pub(crate) fn capture_peek_source(&self) -> Option<Source> {
        Some(Source { space: self.browser.space().id, tab: self.browser.space().active_tab?,
            profile: self.browser.active_user_profile_id() })
    }
    fn source_exists(&self, source: Source) -> bool {
        self.browser.spaces.iter().any(|s| s.id == source.space && s.user_profile_id == source.profile)
            && self.browser.user_profile_id_for_tab(source.tab) == Some(source.profile)
    }
    fn source_selected(&self, source: Source) -> bool {
        self.browser.space().id == source.space && self.browser.space().active_tab == Some(source.tab)
            && self.browser.active_user_profile_id() == source.profile
    }
    pub(crate) fn route_lookup_link(&mut self, identity: PageIdentity, origin: Option<InputOrigin>, url: String,
        background: bool, cx: &mut Context<Self>) -> bool {
        if self.transient.is_none() { return false; }
        if self.browser.space().active_tab != Some(identity.tab) { return true; }
        let Some(state) = &self.peek_owner else {
            // Little has one lookup, including target=_blank. Preserve its CEF
            // renderer, editable DOM and navigation history instead of making a
            // second invisible tab in the transient's private browser model.
            self.edit_browser(|b| b.navigate(identity.tab, url));
            cx.notify();
            return true;
        };
        if state.parked || state.close_requested || state.detached { return true; }
        let epoch = self.pages.as_ref().and_then(|p| p.borrow().page_document_epoch(identity.tab));
        let owner = state.owner.clone();
        let source = state.source;
        let fallback_space = self.transient.as_ref().unwrap().space;
        let child = cx.weak_entity();
        let handle = self.main_window;
        cx.defer(move |cx| {
            let valid = child.read_with(cx, |view, _| {
                view.browser.space().active_tab == Some(identity.tab)
                    && view.peek_owner.as_ref().is_some_and(|s| !s.parked && !s.close_requested && !s.detached)
                    && view.pages.as_ref().is_some_and(|pages| {
                        let pages = pages.borrow();
                        pages.accepts_identity(identity) && pages.page_document_epoch(identity.tab) == epoch && origin.is_none_or(|o| pages.accepts_input_origin(o))
                    })
            }).unwrap_or(false);
            if !valid { return; }
            let _ = owner.update(cx, |parent, cx| {
                if !parent.peek_child.is_some_and(|h| h.window_id() == handle.window_id())
                    || source.is_some_and(|s| !parent.source_exists(s) || !parent.source_selected(s)) { return; }
                let preferred = source.map_or(fallback_space, |s| s.space);
                let destination = parent.browser.spaces.iter().position(|s| s.id == preferred && s.user_profile_id == identity.profile)
                    .or_else(|| parent.browser.spaces.iter().position(|s| s.user_profile_id == identity.profile));
                let Some(destination) = destination else { return; };
                let tab = parent.edit_browser(|b| {
                    let mut selection = b.window_selection();
                    b.switch_space(destination);
                    let tab = b.new_tab(url.clone(), std::time::SystemTime::now());
                    if background { b.edit_in_window(&mut selection, |_| {}); }
                    tab
                });
                parent.edit_profile(|b| b.set_title(tab, crate::window::display_url(&url)));
                if !background {
                    parent.park_active_peek(cx);
                    let main = parent.main_window;
                    cx.defer(move |cx| { let _ = main.update(cx, |_, w, _| w.activate_window()); });
                    if let Some(pages) = &parent.pages { pages.borrow_mut().request_page_focus(); }
                }
                cx.notify();
            });
        });
        true
    }
    fn park_active_peek(&mut self, cx: &mut Context<Self>) {
        let Some(child) = self.peek_child.take() else { return; };
        let source = child.read_with(cx, |v, _| v.peek_owner.as_ref().and_then(|s| s.source)).ok().flatten();
        let Some(source) = source else {
            self.peek_child = Some(child);
            self.exit_peek(cx);
            return;
        };
        if let Some(previous) = self.parked_peeks.insert(source, child) {
            if previous.window_id() != child.window_id() {
                cx.defer(move |cx| { let _ = previous.update(cx, |v, _, cx| v.close_window(cx)); });
            }
        }
        self.set_peek_parked(child, true, None, cx);
    }
    fn set_peek_parked(&self, child: WindowHandle<BrowserWindow>, parked: bool,
        size: Option<gpui::Size<gpui::Pixels>>, cx: &mut Context<Self>) {
        let parent = self.native_view;
        cx.defer(move |cx| {
            let native = child.update(cx, |view, w, cx| {
                let Some(state) = view.peek_owner.as_mut().filter(|s| !s.detached && !s.close_requested) else { return None; };
                state.parked = parked;
                if parked { view.close_link_status(cx); }
                if let Some(size) = size { if w.viewport_size() != size { w.resize(size); } }
                if let Some(pages) = &view.pages {
                    let mut pages = pages.borrow_mut();
                    pages.sync(&view.browser, !parked);
                    if !parked { pages.request_page_focus(); }
                }
                cx.notify();
                view.native_view
            }).ok().flatten();
            // AppKit can re-enter on window visibility/focus changes. No entity
            // or page borrow survives these platform calls.
            if let (Some(parent), Some(native)) = (parent, native) {
                crate::native::set_overlay_visible(parent, native, !parked);
            }
            if !parked {
                let _ = child.update(cx, |_, w, cx| {
                    w.activate_window();
                    // Hidden native windows do not paint. Invalidate again
                    // after ordering in, even if the hidden notify was consumed.
                    w.refresh();
                    cx.notify();
                });
            }
        });
    }
    pub(crate) fn sync_parked_peeks(&mut self, size: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        let mut retire = Vec::new();
        self.parked_peeks.retain(|source, child| {
            let active = child.read_with(cx, |v, _| v.peek_owner.as_ref().is_some_and(|s| !s.detached && !s.close_requested)).unwrap_or(false);
            if !active { return false; }
            let exists = self.browser.spaces.iter().any(|s| s.id == source.space && s.user_profile_id == source.profile)
                && self.browser.user_profile_id_for_tab(source.tab) == Some(source.profile);
            if !exists { retire.push(*child); }
            exists
        });
        for child in retire { cx.defer(move |cx| { let _ = child.update(cx, |v, _, cx| v.close_window(cx)); }); }
        if let Some(child) = self.peek_child {
            let source = child.read_with(cx, |v, _| v.peek_owner.as_ref().and_then(|s| s.source)).ok().flatten();
            if source.is_some_and(|s| !self.source_exists(s)) {
                self.peek_child = None;
                cx.defer(move |cx| { let _ = child.update(cx, |v, _, cx| v.close_window(cx)); });
            } else if source.is_some_and(|s| !self.source_selected(s)) {
                self.park_active_peek(cx);
            }
        }
        if self.peek_child.is_none() {
            if let Some(source) = self.capture_peek_source() {
                if let Some(child) = self.parked_peeks.remove(&source) {
                    self.close_link_status(cx);
                    if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
                    self.peek_child = Some(child);
                    self.set_peek_parked(child, false, Some(size), cx);
                }
            }
        }
    }
}
