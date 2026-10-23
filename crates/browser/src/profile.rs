//! One editable profile and session writer, with read-only window projections.
use crate::{split::SplitDraft, window::BrowserWindow};
use browser_core::{Browser, TabId, windows::{WindowProjection, WindowSelection}};
use gpui::{prelude::*, App, AsyncApp, Context, Entity, ForegroundExecutor, Subscription};
use std::{cell::{Cell, RefCell}, path::PathBuf, rc::{Rc, Weak}, time::{Duration, SystemTime}};

type DraftSlot = Rc<RefCell<Option<SplitDraft>>>;
type Drafts = Rc<RefCell<Vec<Weak<RefCell<Option<SplitDraft>>>>>>;
type Selections = Rc<RefCell<Vec<Weak<RefCell<Vec<TabId>>>>>>;

pub(crate) struct Profile {
    data: Rc<RefCell<Browser>>,
    drafts: Drafts,
    selections: Selections,
    revision: Rc<Cell<u64>>,
    path: Option<PathBuf>,
    saved: Option<String>,
    _save_on_quit: Option<Subscription>,
}

impl Profile {
    fn session_json(&self) -> String { session_json(&self.data, &self.drafts) }

    fn flush(&mut self) {
        let Some(path) = &self.path else { return; };
        let json = self.session_json();
        if self.saved.as_ref() == Some(&json) { return; }
        match browser_core::session::save(path, &json) {
            Ok(()) => self.saved = Some(json),
            Err(error) => eprintln!("Cannot save the session to {}: {error}", path.display()),
        }
    }

    fn save_session_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.data.borrow().is_private() || self.path.is_some() { return; }
        self.path = Some(path);
        self._save_on_quit = Some(cx.on_app_quit(|this, _| { this.flush(); async {} }));
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if this.update(cx, |this, _| this.flush()).is_err() { break; }
        }).detach();
    }

    fn archive_idle(&mut self) -> usize {
        let mut slots = self.selections.borrow_mut();
        slots.retain(|slot| slot.strong_count() > 0);
        let protected: Vec<_> = slots.iter().filter_map(Weak::upgrade).flat_map(|slot| slot.borrow().clone()).collect();
        self.data.borrow_mut().archive_idle_except(SystemTime::now(), &protected)
    }
}

#[derive(Clone)]
pub(crate) struct ProfileHandle {
    owner: Entity<Profile>,
    data: Rc<RefCell<Browser>>,
    drafts: Drafts,
    selections: Selections,
    revision: Rc<Cell<u64>>,
    pending: Rc<Cell<bool>>,
    executor: ForegroundExecutor,
    app: AsyncApp,
}

impl ProfileHandle {
    pub(crate) fn new(browser: Browser, cx: &mut App) -> Self {
        let data = Rc::new(RefCell::new(browser));
        let drafts = Drafts::default();
        let selections = Selections::default();
        let revision = Rc::<Cell<u64>>::default();
        let owner = cx.new(|cx| {
            cx.on_release(|profile: &mut Profile, _| profile.flush()).detach();
            cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(Duration::from_secs(60)).await;
                if this.update(cx, |this: &mut Profile, cx| {
                    if this.archive_idle() > 0 {
                        this.revision.set(this.revision.get().wrapping_add(1));
                        cx.notify();
                    }
                }).is_err() { break; }
            }).detach();
            Profile { data: data.clone(), drafts: drafts.clone(), selections: selections.clone(), revision: revision.clone(),
                path: None, saved: None, _save_on_quit: None }
        });
        Self { owner, data, drafts, selections, revision, pending: Rc::default(),
            executor: cx.foreground_executor().clone(), app: cx.to_async() }
    }

    pub(crate) fn selection(&self) -> WindowSelection { self.data.borrow().window_selection() }
    pub(crate) fn new_window_selection(&self) -> WindowSelection { self.data.borrow().new_window_selection() }
    pub(crate) fn projection(&self, selection: &WindowSelection) -> WindowProjection { self.data.borrow().window_projection(selection) }

    fn notify(&self) {
        if self.pending.replace(true) { return; }
        let pending = self.pending.clone();
        let owner = self.owner.downgrade();
        let mut app = self.app.clone();
        // The model edit has already finished. Notify on the foreground queue
        // without trying to borrow GPUI's App again inside the current handler.
        self.executor.spawn(async move {
            pending.set(false);
            let _ = owner.update(&mut app, |_, cx| cx.notify());
        }).detach();
    }
}

pub(crate) struct ProfileBinding {
    pub(crate) handle: ProfileHandle,
    selection: WindowSelection,
    revision: u64,
    draft: DraftSlot,
    protected: Rc<RefCell<Vec<TabId>>>,
    _observe_profile: Subscription,
    _observe_window: Subscription,
}

impl ProfileBinding {
    pub(crate) fn new(handle: ProfileHandle, selection: WindowSelection, cx: &mut Context<BrowserWindow>) -> Self {
        let draft = DraftSlot::default();
        let protected = Rc::new(RefCell::new(selection.selected_tabs().collect()));
        handle.drafts.borrow_mut().push(Rc::downgrade(&draft));
        handle.selections.borrow_mut().push(Rc::downgrade(&protected));
        let observe_profile = cx.observe(&handle.owner, |window, _, cx| {
            window.refresh_profile_projection();
            cx.notify();
        });
        let observe_window = cx.observe_self(|window, _| window.profile.remember_draft(&window.split_draft));
        Self { revision: handle.revision.get(), handle, selection, draft, protected,
            _observe_profile: observe_profile, _observe_window: observe_window }
    }

    fn remember_draft(&self, draft: &Option<SplitDraft>) { *self.draft.borrow_mut() = draft.clone(); }
    fn remember_selection(&self) { *self.protected.borrow_mut() = self.selection.selected_tabs().collect(); }

    pub(crate) fn save_session_to(&self, path: PathBuf, cx: &mut App) {
        self.handle.owner.update(cx, |profile, cx| profile.save_session_to(path, cx));
    }

    pub(crate) fn flush(&self, cx: &mut App) { self.handle.owner.update(cx, |profile, _| profile.flush()); }

    #[cfg(test)]
    pub(crate) fn session_json(&self, draft: &Option<SplitDraft>) -> String {
        self.remember_draft(draft);
        session_json(&self.handle.data, &self.handle.drafts)
    }
}

impl BrowserWindow {
    pub(crate) fn refresh_profile_projection(&mut self) {
        if self.profile.revision == self.profile.handle.revision.get() { return; }
        let projection = self.profile.handle.projection(&self.profile.selection);
        self.receive_profile_projection(projection);
        self.profile.selection = self.browser.window_selection();
        self.profile.remember_selection();
        self.profile.revision = self.profile.handle.revision.get();
    }

    pub(crate) fn edit_browser<R>(&mut self, edit: impl FnOnce(&mut Browser) -> R) -> R {
        let (result, projection) = {
            let mut browser = self.profile.handle.data.borrow_mut();
            let result = browser.edit_in_window(&mut self.profile.selection, edit);
            let projection = browser.window_projection(&self.profile.selection);
            (result, projection)
        };
        self.publish_profile_edit(projection);
        result
    }

    /// Renderer metadata changes shared data without choosing this window as
    /// the session's active window. Background loads must not steal selection.
    pub(crate) fn edit_profile<R>(&mut self, edit: impl FnOnce(&mut Browser) -> R) -> R {
        let (result, projection) = {
            let mut browser = self.profile.handle.data.borrow_mut();
            let result = edit(&mut browser);
            (result, browser.window_projection(&self.profile.selection))
        };
        self.publish_profile_edit(projection);
        result
    }

    fn publish_profile_edit(&mut self, projection: WindowProjection) {
        if self.browser.space().id == projection.space().id && self.browser.active_space != projection.active_space {
            self.reset_profile_pager(projection.active_space);
        }
        self.browser = projection;
        self.profile.remember_draft(&self.split_draft);
        self.profile.remember_selection();
        let revision = self.profile.handle.revision.get().wrapping_add(1);
        self.profile.handle.revision.set(revision);
        self.profile.revision = revision;
        self.profile.handle.notify();
    }
}

fn session_json(data: &RefCell<Browser>, drafts: &RefCell<Vec<Weak<RefCell<Option<SplitDraft>>>>>) -> String {
    let mut browser = data.borrow().clone();
    let mut drafts = drafts.borrow_mut();
    drafts.retain(|draft| draft.strong_count() > 0);
    for slot in drafts.iter().filter_map(Weak::upgrade) {
        if let Some(draft) = slot.borrow().as_ref() {
            // Filtering one window's preview must not replace another
            // window's current page with the draft's previous selection.
            let preserve = (!draft.owns_selection(&browser)).then(|| browser.window_selection());
            draft.rollback(&mut browser);
            if let Some(mut selection) = preserve { browser.edit_in_window(&mut selection, |_| {}); }
        }
    }
    browser_core::session::to_json(&browser)
}
