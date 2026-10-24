//! One editable profile and session writer, with read-only window projections.
use crate::{split::SplitDraft, window::BrowserWindow};
use browser_core::{Browser, SpaceId, TabId, UserProfileId, space_creation::{SpaceCreation, SpaceCreationError}, windows::{WindowProjection, WindowSelection}};
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
    _observe_history: Option<Subscription>,
}

impl Profile {
    fn session_json(&self) -> String { session_json(&self.data, &self.drafts) }

    fn flush(&mut self) {
        if crate::quit::is_quitting() { return; }
        let Some(path) = &self.path else { return; };
        let json = self.session_json();
        if self.saved.as_ref() == Some(&json) { return; }
        match browser_core::session::save(path, &json) {
            Ok(()) => self.saved = Some(json),
            Err(error) => eprintln!("Cannot save the session to {}: {error}", path.display()),
        }
    }

    fn save_session_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.data.borrow().is_private() || self.data.borrow().is_blank() || self.path.is_some() { return; }
        self.path = Some(path);
        self._save_on_quit = Some(cx.on_app_quit(|this, _| { this.flush(); async {} }));
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if this.update(cx, |this, _| this.flush()).is_err() { break; }
        }).detach();
    }

    fn archive_idle(&mut self) -> usize {
        if crate::quit::is_quitting() { return 0; }
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
    // Retains the actual normal profile (and its session writer), never a window snapshot.
    history_source: Option<(Box<ProfileHandle>, UserProfileId)>,
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
                path: None, saved: None, _save_on_quit: None, _observe_history: None }
        });
        Self { owner, data, drafts, selections, revision, pending: Rc::default(),
            executor: cx.foreground_executor().clone(), app: cx.to_async(), history_source: None }
    }

    /// Blank owns its tabs, archive, IDs and selections; only browsing history
    /// belongs to this specific retained normal profile, across all its Spaces.
    /// Other normal profiles remain isolated. Callers launching from Incognito
    /// must pass their retained normal handle, never the private window's handle.
    pub(crate) fn new_with_shared_history(browser: Browser, normal_source: &ProfileHandle, cx: &mut App) -> Self {
        assert!(browser.is_blank() && !browser.is_private(), "Shared history requires a Blank model");
        assert!(!normal_source.data.borrow().is_private() && !normal_source.data.borrow().is_blank(),
            "Blank history must come from the retained normal profile");
        let source_id = browser.blank_source_user_profile_id().expect("Blank retains its source profile");
        assert!(normal_source.data.borrow().user_profile(source_id).is_some());
        let mut handle = Self::new(browser, cx);
        handle.history_source = Some((Box::new(normal_source.clone()), source_id));
        handle.sync_history();
        let source = normal_source.data.clone();
        handle.owner.update(cx, |profile, cx| {
            profile._observe_history = Some(cx.observe(&normal_source.owner, move |profile, _, cx| {
                let history = source.borrow().user_profile(source_id).map(|profile| profile.history.clone()).unwrap_or_default();
                let mut browser = profile.data.borrow_mut();
                if let Some(profile) = browser.user_profile_mut(source_id) { profile.history = history; }
                drop(browser);
                profile.revision.set(profile.revision.get().wrapping_add(1));
                cx.notify();
            }));
        });
        handle
    }

    fn sync_history(&self) {
        if let Some((source, id)) = &self.history_source {
            let history = source.data.borrow().user_profile(*id).map(|profile| profile.history.clone()).unwrap_or_default();
            if let Some(profile) = self.data.borrow_mut().user_profile_mut(*id) { profile.history = history; }
        }
    }

    /// Sync before every edit, including edits unrelated to history: an older
    /// Blank projection must never overwrite visits recorded by another window.
    fn edit<R>(&self, edit: impl FnOnce(&mut Browser) -> R) -> R {
        self.sync_history();
        let result = edit(&mut self.data.borrow_mut());
        if let Some((source, id)) = &self.history_source {
            let history = self.data.borrow().user_profile(*id).expect("Blank source profile").history.clone();
            let mut normal = source.data.borrow_mut();
            if let Some(profile) = normal.user_profile_mut(*id).filter(|profile| !same_history(&profile.history, &history)) {
                profile.history = history;
                drop(normal);
                source.revision.set(source.revision.get().wrapping_add(1));
                source.notify();
            }
        }
        result
    }

    pub(crate) fn save_session_to(&self, path: PathBuf, cx: &mut App) {
        self.owner.update(cx, |profile, cx| profile.save_session_to(path, cx));
    }

    pub(crate) fn flush_for_quit(&self, cx: &mut App) -> Result<(), String> {
        self.owner.update(cx, |profile, _| {
            let Some(path) = &profile.path else { return Ok(()); };
            let json = profile.session_json();
            browser_core::session::save(path, &json).map_err(|error| format!("Cannot save {}: {error}", path.display()))?;
            profile.saved = Some(json);
            Ok(())
        })
    }

    /// Download ownership resolves to the exact retained normal writer. Never
    /// reconstruct a UserProfileId from the engine's opaque owner key.
    pub(crate) fn download_normal_owner(&self, captured: UserProfileId) -> Option<(Self, UserProfileId, bool)> {
        let browser = self.data.borrow();
        if browser.is_private() { return None; }
        if browser.is_blank() {
            let (source, id) = self.history_source.as_ref()?;
            if *id != captured || browser.blank_source_user_profile_id() != Some(*id) { return None; }
            let normal = source.data.borrow();
            if normal.is_private() || normal.is_blank() || normal.user_profile(*id).is_none() { return None; }
            return Some(((**source).clone(), *id, true));
        }
        browser.user_profile(captured)?;
        Some((self.clone(), captured, false))
    }

    /// One writer: changes canonical normal data directly, never a window's
    /// projection. Blank callers first resolve download_normal_owner above.
    pub(crate) fn record_completed_download(&self, profile: UserProfileId,
        completion: browser_core::downloads::DownloadCompletion)
        -> Result<browser_core::downloads::CompletedDownloadId, String>
    {
        let id = self.data.borrow_mut().record_completed_download(profile, completion)
            .map_err(|error| format!("Cannot record completed download: {error:?}"))?;
        self.revision.set(self.revision.get().wrapping_add(1));
        self.notify();
        Ok(id)
    }

    pub(crate) fn hide_completed_download(&self, id: browser_core::downloads::CompletedDownloadId) -> bool {
        if !self.data.borrow_mut().hide_completed_download(id) { return false; }
        self.revision.set(self.revision.get().wrapping_add(1));
        self.notify();
        true
    }

    /// Actual local profile names/IDs and completed records, independent of the
    /// selected Space or the existence of any source BrowserWindow.
    pub(crate) fn download_inventory(&self) -> Option<(Vec<(UserProfileId, String)>, Vec<browser_core::downloads::CompletedDownload>)> {
        let browser = self.data.borrow();
        if browser.is_private() || browser.is_blank() { return None; }
        let profiles = browser.user_profiles().map(|p| (p.id(), p.name().to_owned())).collect();
        let records = browser.user_profiles().flat_map(|p| browser.completed_downloads(p.id()).unwrap_or(&[]).iter().cloned()).collect();
        Some((profiles, records))
    }

    pub(crate) fn same_workspace(&self, other: &Self) -> bool { Rc::ptr_eq(&self.data, &other.data) }

    pub(crate) fn blank_browser(&self, id: UserProfileId) -> Browser {
        self.data.borrow().new_blank_for_user_profile(id).expect("retained normal source profile")
    }

    /// Settings edits use the canonical model and do not change a window's selection.
    pub(crate) fn settings_snapshot(&self) -> Browser { self.data.borrow().clone() }
    pub(crate) fn read_settings<R>(&self, read: impl FnOnce(&Browser) -> R) -> R { read(&self.data.borrow()) }

    pub(crate) fn edit_settings<R>(&self, edit: impl FnOnce(&mut Browser) -> R) -> R {
        let result = self.edit(edit);
        self.revision.set(self.revision.get().wrapping_add(1));
        self.notify();
        result
    }

    pub(crate) fn observe_settings<T: 'static>(&self, cx: &mut Context<T>) -> Subscription {
        cx.observe(&self.owner, |_, _, cx| cx.notify())
    }

    pub(crate) fn default_user_profile_id(&self) -> UserProfileId { self.data.borrow().default_user_profile_id() }

    pub(crate) fn selection(&self) -> WindowSelection { self.data.borrow().window_selection() }
    pub(crate) fn new_window_selection(&self) -> WindowSelection { self.data.borrow().new_window_selection() }
    pub(crate) fn projection(&self, selection: &WindowSelection) -> WindowProjection {
        self.sync_history();
        self.data.borrow().window_projection(selection)
    }

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
    seen_auto_archive: std::collections::HashSet<TabId>,
    _observe_profile: Subscription,
    _observe_window: Subscription,
}

impl ProfileBinding {
    pub(crate) fn new(handle: ProfileHandle, selection: WindowSelection, cx: &mut Context<BrowserWindow>) -> Self {
        let draft = DraftSlot::default();
        let protected = Rc::new(RefCell::new(selection.selected_tabs().collect()));
        handle.drafts.borrow_mut().push(Rc::downgrade(&draft));
        handle.selections.borrow_mut().push(Rc::downgrade(&protected));
        // Seed from the canonical archive: loading a saved session or opening
        // another window must not announce historical automatic archives.
        let seen_auto_archive = handle.data.borrow().archive.iter()
            .filter(|entry| entry.reason == browser_core::ArchiveReason::Automatic)
            .map(|entry| entry.tab.id).collect();
        let observe_profile = cx.observe(&handle.owner, |window, _, cx| {
            window.refresh_profile_projection();
            window.notify_auto_archive(cx);
            window.validate_user_profile_dialog(cx);
            cx.notify();
        });
        let observe_window = cx.observe_self(|window, cx| { window.profile.remember_draft(&window.split_draft); window.validate_user_profile_dialog(cx); });
        Self { revision: handle.revision.get(), handle, selection, draft, protected, seen_auto_archive,
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
    fn notify_auto_archive(&mut self, cx: &mut Context<Self>) {
        let count = self.browser.archive.iter().filter(|entry|
            entry.reason == browser_core::ArchiveReason::Automatic
                && entry.user_profile_id == self.browser.active_user_profile_id()
                && !self.profile.seen_auto_archive.contains(&entry.tab.id)).count();
        self.profile.seen_auto_archive = self.browser.archive.iter()
            .filter(|entry| entry.reason == browser_core::ArchiveReason::Automatic)
            .map(|entry| entry.tab.id).collect();
        if count > 0 { self.show_toast(format!("Auto archived {count} tabs"), cx); }
    }

    pub(crate) fn refresh_profile_projection(&mut self) {
        if self.profile.revision == self.profile.handle.revision.get() { return; }
        let projection = self.profile.handle.projection(&self.profile.selection);
        self.receive_profile_projection(projection);
        self.reconcile_sidebar_navigation();
        self.profile.selection = self.browser.window_selection();
        self.profile.remember_selection();
        self.profile.revision = self.profile.handle.revision.get();
    }

    /// Commit against the authoritative workspace before publishing a new
    /// projection. Failed validation never applies this window's selection.
    pub(crate) fn commit_space_creation_request(&mut self, request: &SpaceCreation) -> Result<SpaceId, SpaceCreationError> {
        let (id, projection) = self.profile.handle.edit(|browser| {
            browser.create_space_after_in_window(&mut self.profile.selection, request)
                .map(|id| (id, browser.window_projection(&self.profile.selection)))
        })?;
        self.sidebar_navigation.controller.committed(id);
        self.publish_profile_edit(projection);
        Ok(id)
    }

    pub(crate) fn edit_browser<R>(&mut self, edit: impl FnOnce(&mut Browser) -> R) -> R {
        let (result, projection) = self.profile.handle.edit(|browser| {
            let result = browser.edit_in_window(&mut self.profile.selection, edit);
            let projection = browser.window_projection(&self.profile.selection);
            (result, projection)
        });
        let selection_changed = self.browser.space().id != projection.space().id
            || self.browser.space().active_tab != projection.space().active_tab;
        if selection_changed && self.page_content_visible() && self.user_profile_dialog.is_none() && self.command_bar.is_none() && self.tab_rename.is_none() {
            if let Some(pages) = &self.pages {
                let mut pages = pages.borrow_mut();
                // A native Focused event has already selected its responder.
                if pages.shown() != projection.space().active_tab { pages.request_page_focus(); }
            }
        }
        self.publish_profile_edit(projection);
        result
    }

    /// Renderer metadata changes shared data without choosing this window as
    /// the session's active window. Background loads must not steal selection.
    pub(crate) fn edit_profile<R>(&mut self, edit: impl FnOnce(&mut Browser) -> R) -> R {
        let (result, projection) = self.profile.handle.edit(|browser| {
            let result = edit(browser);
            (result, browser.window_projection(&self.profile.selection))
        });
        self.publish_profile_edit(projection);
        result
    }

    fn publish_profile_edit(&mut self, projection: WindowProjection) {
        self.receive_profile_projection(projection);
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

fn same_history(a: &[browser_core::history::HistoryEntry], b: &[browser_core::history::HistoryEntry]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)|
        a.url == b.url && a.title == b.title && a.visited_at == b.visited_at)
}

#[cfg(test)]
#[path = "blank_history_tests.rs"]
mod blank_history_tests;
