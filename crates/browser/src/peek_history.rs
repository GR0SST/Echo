//! Undo retains a hidden live document, never a serialized replacement URL.
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};
use browser_core::{SpaceId, windows::WindowSelection};
use gpui::{App, Context, Global, WeakEntity};
use crate::{new_window::NormalProfile, pages::Pages, profile::ProfileHandle, window::BrowserWindow};

pub(crate) struct Document {
    pub profile: ProfileHandle,
    pub selection: WindowSelection,
    pub pages: Option<Rc<RefCell<Pages>>>,
    pub source: NormalProfile,
    pub space: SpaceId,
    pub origin: gpui::Point<gpui::Pixels>,
}
enum State { Closed(Document), Open(WeakEntity<BrowserWindow>) }
struct Entry { owner_window: gpui::WindowId, owner: WeakEntity<BrowserWindow>, history: ProfileHandle, state: State }
#[derive(Default)]
struct Journal { next: u64, entries: BTreeMap<u64, Entry> }
#[derive(Clone, Default)]
struct Retained(Rc<RefCell<Journal>>);
impl Global for Retained {}

fn journal(cx: &mut App) -> Rc<RefCell<Journal>> {
    if !cx.has_global::<Retained>() {
        cx.set_global(Retained::default());
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(Duration::from_millis(16)).await;
            if crate::quit::is_quitting() { break; }
            if cx.update(poll).is_err() { break; }
        }).detach();
    }
    cx.global::<Retained>().0.clone()
}

fn poll(cx: &mut App) {
    let Some(retained) = cx.try_global::<Retained>().cloned() else { return; };
    let mut journal = retained.0.borrow_mut();
    journal.entries.retain(|token, entry| entry.owner.upgrade().is_some()
        && entry.history.read_settings(|browser| browser.has_peek_history(*token)));
    for entry in journal.entries.values_mut() {
        if let State::Closed(document) = &mut entry.state { document.poll(); }
    }
}
impl Document {
    fn poll(&mut self) {
        let Some(pages) = &self.pages else { return; };
        let events = pages.borrow_mut().take_routed_events();
        for event in events {
            if !event.is_metadata() { continue; }
            let ready = pages.borrow().claim_metadata_through(&event);
            for queued in ready {
                if !pages.borrow().accepts_identity(queued.identity) { continue; }
                let id = queued.identity.tab;
                let profile = queued.identity.profile;
                if !self.profile.read_settings(|browser| browser.user_profile_id_for_tab(id) == Some(profile)) { continue; }
                pages.borrow_mut().record(&queued.event);
                self.profile.edit_settings(|browser| match queued.event {
                    engine::Event::TitleChanged { title, .. } if !title.is_empty() => {
                        browser.set_title(id, title.clone());
                        let url = browser.all_tabs().find(|tab| tab.id == id).map(|tab| tab.url.clone());
                        if let Some(url) = url {
                            browser.update_history_title(profile, &url, &title);
                        }
                    }
                    engine::Event::UrlChanged { url, .. } => {
                        browser.set_url(id, url.clone());
                        browser.record_visit(profile, &url, std::time::SystemTime::now());
                    }
                    engine::Event::Favicon { page_url, png, .. } => { browser.cache_favicon(profile, &page_url, png); },
                    _ => {}
                });
            }
        }
    }
}

impl BrowserWindow {
    pub(crate) fn retain_closed_peek(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.peek_owner.as_mut() else { return; };
        if state.detached { return; }
        let owner = state.owner.clone();
        let reused = state.redo_close.take();
        let origin = state.origin;
        let Ok((parent, history, owner_window)) = owner.update(cx, |owner, _| (owner.native_view, owner.profile.handle.clone(), owner.main_window.window_id())) else { return; };
        let parent = parent.unwrap_or(std::ptr::null_mut());
        #[cfg(test)]
        let parent = if parent.is_null() { std::ptr::dangling_mut::<std::ffi::c_void>() } else { parent };
        let Some(transient) = &self.transient else { return; };
        let pages = self.pages.as_ref().map(|pages| {
            pages.borrow_mut().mute_retained_document();
            Rc::new(RefCell::new(pages.borrow().for_host(parent)))
        });
        let document = Document { profile: self.profile.handle.clone(), selection: self.profile.selection(), pages,
            source: transient.source.clone(), space: transient.space, origin };
        let retained = journal(cx);
        let token = reused.unwrap_or_else(|| {
            let mut journal = retained.borrow_mut();
            journal.next = journal.next.checked_add(1).expect("Peek history token exhausted");
            journal.next
        });
        if reused.is_none() { history.edit_settings(|browser| browser.record_peek_close(token)); }
        retained.borrow_mut().entries.insert(token, Entry { owner_window, owner, history, state: State::Closed(document) });
        // Low-level child retirement must not invalidate the retained entry.
        self.peek_owner.as_mut().unwrap().history_token = None;
        if let Some(pages) = self.pages.take() { pages.borrow_mut().release_host(); }
    }

    pub(crate) fn edit_peek_history(&mut self, redo: bool, window: &mut gpui::Window, cx: &mut Context<Self>) -> bool {
        if let Some(owner) = self.peek_owner.as_ref().map(|state| state.owner.clone()) {
            cx.defer(move |cx| { let _ = owner.update(cx, |view, cx| {
                let root = view.main_window.downcast::<BrowserWindow>().unwrap();
                cx.defer(move |cx| { let _ = root.update(cx, |view, window, cx| { view.edit_peek_history(redo, window, cx); }); });
            }); });
            return true;
        }
        let Some(token) = self.browser.peek_history_token(redo) else { return false; };
        let retained = journal(cx);
        let Some(entry) = retained.borrow_mut().entries.remove(&token) else {
            self.edit_browser(|browser| browser.discard_peek_history(token));
            return false;
        };
        let can_apply = match &entry.state {
            State::Closed(_) => !redo && self.peek_child.is_none(),
            State::Open(child) => redo && child.upgrade().is_some(),
        };
        if !can_apply { retained.borrow_mut().entries.insert(token, entry); return true; }
        self.edit_browser(|browser| { browser.take_peek_history(redo); });
        match entry.state {
            State::Closed(_) => {
                retained.borrow_mut().entries.insert(token, entry);
                self.restore_peek(token, window, cx);
            }
            State::Open(child) => {
                let closing = child.clone();
                cx.defer(move |cx| { let _ = closing.update(cx, |view, cx| {
                    if let Some(state) = &mut view.peek_owner { state.redo_close = Some(token); }
                    view.exit_peek(cx);
                }); });
                retained.borrow_mut().entries.insert(token, Entry { state: State::Open(child), ..entry });
            }
        }
        true
    }
}

pub(crate) fn take_document(token: u64, cx: &mut App) -> Option<Document> {
    let retained = cx.try_global::<Retained>()?.0.clone();
    let entry = retained.borrow_mut().entries.remove(&token)?;
    let State::Closed(mut document) = entry.state else { return None; };
    document.poll();
    Some(document)
}

pub(crate) fn restored(token: u64, owner: WeakEntity<BrowserWindow>, history: ProfileHandle,
    child: WeakEntity<BrowserWindow>, cx: &mut App) {
    let Some(root) = owner.upgrade() else { return; };
    let owner_window = root.read(cx).main_window.window_id();
    journal(cx).borrow_mut().entries.insert(token, Entry { owner_window, owner, history, state: State::Open(child) });
}
pub(crate) fn discard(token: u64, cx: &mut App) {
    if let Some(retained) = cx.try_global::<Retained>().cloned() {
        if let Some(entry) = retained.0.borrow_mut().entries.remove(&token) {
            entry.history.edit_settings(|browser| browser.discard_peek_history(token));
        }
    }
}
pub(crate) fn discard_owner(owner: gpui::WindowId, cx: &mut App) {
    let Some(retained) = cx.try_global::<Retained>().cloned() else { return; };
    let tokens: Vec<_> = retained.0.borrow().entries.iter()
        .filter(|(_, entry)| entry.owner_window == owner).map(|(id, _)| *id).collect();
    for token in tokens { discard(token, cx); }
}
