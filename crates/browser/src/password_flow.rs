//! Immutable page/profile authority for website passwords. Secrets never enter session state.
use crate::{
    pages::PageIdentity,
    password_prompt::{Decision, PasswordPrompt},
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use engine::{LoginCredential, PasswordEvent, PasswordToken};
use gpui::{
    AnyElement, App, Context, Entity, Focusable, Global, Subscription, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, div, prelude::*,
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct PasswordFlow {
    documents: HashMap<PageIdentity, (PasswordToken, String, bool)>,
    pub(crate) accounts: Option<crate::password_accounts::Accounts>,
    frames: HashMap<(PageIdentity, String), (PasswordToken, String)>,
    pending: Option<Candidate>,
    pub(crate) prompt: Option<PromptHost>,
    pub(crate) serial: u64,
    pub(crate) lookup_generation: u64,
}
struct Candidate {
    identity: PageIdentity,
    origin: String,
    credential: LoginCredential,
    created: Instant,
    ready: bool,
    serial: u64,
}
pub(crate) struct PromptHost {
    dialog: Entity<PasswordPrompt>,
    overlay: Option<WindowHandle<Overlay>>,
    _events: Subscription,
    token: PasswordToken,
    candidate: Candidate,
}
#[derive(Default)]
struct Changes(HashMap<String, u64>);
impl Global for Changes {}
pub(crate) fn revision(profile: &str, cx: &App) -> u64 {
    cx.try_global::<Changes>()
        .and_then(|c| c.0.get(profile).copied())
        .unwrap_or(0)
}
pub(crate) fn publish_change(profile: String, cx: &mut App) {
    if !cx.has_global::<Changes>() {
        cx.set_global(Changes::default());
    }
    cx.update_global::<Changes, _>(|changes, _| {
        let n = changes.0.entry(profile).or_default();
        *n = n.wrapping_add(1);
    });
}
pub(crate) fn observe_changes(cx: &mut Context<BrowserWindow>) -> Subscription {
    if !cx.has_global::<Changes>() {
        cx.set_global(Changes::default());
    }
    cx.observe_global::<Changes>(|this, cx| this.password_store_changed(cx))
}
#[derive(Default)]
struct Writes(Arc<AtomicUsize>);
impl Global for Writes {}
pub(crate) struct WriteLease(Arc<AtomicUsize>);
impl Drop for WriteLease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
pub(crate) fn writes_pending(cx: &App) -> bool {
    cx.try_global::<Writes>()
        .is_some_and(|w| w.0.load(Ordering::SeqCst) > 0)
}
pub(crate) fn lease(cx: &mut App) -> WriteLease {
    if !cx.has_global::<Writes>() {
        cx.set_global(Writes::default());
    }
    let n = cx.global::<Writes>().0.clone();
    n.fetch_add(1, Ordering::SeqCst);
    WriteLease(n)
}
pub(crate) fn profile_key(identity: PageIdentity) -> String {
    identity
        .profile
        .storage_key()
        .unwrap_or_else(|| "default".into())
}

#[derive(Clone)]
pub(crate) struct Record {
    pub origin: String,
    pub credential: LoginCredential,
    pub note: zeroize::Zeroizing<String>,
}
impl Record {
    fn imported(&self) -> importer::Credential {
        importer::Credential {
            origin: self.origin.clone(),
            username: self.credential.username().into(),
            password: self.credential.password().into(),
        }
    }
}
pub(crate) trait Store: Send + Sync {
    fn lookup(&self, profile: &str, origin: &str) -> Result<(bool, Vec<LoginCredential>), String>;
    fn save(&self, profile: &str, origin: &str, credential: &LoginCredential)
    -> Result<(), String>;
    fn block(&self, profile: &str, origin: &str) -> Result<(), String>;
    fn list(&self, profile: &str) -> Result<Vec<Record>, String>;
    fn insert(&self, profile: &str, record: &Record) -> Result<(), String>;
    fn replace(
        &self,
        profile: &str,
        old_origin: &str,
        old_user: &str,
        record: &Record,
    ) -> Result<(), String>;
    fn remove(&self, profile: &str, origin: &str, user: &str) -> Result<(), String>;
}
struct NativeStore;
impl Store for NativeStore {
    fn lookup(&self, profile: &str, origin: &str) -> Result<(bool, Vec<LoginCredential>), String> {
        let vault = crate::import_credentials::vault(profile)?;
        let (blocked, rows) = vault.lookup(origin)?;
        Ok((
            blocked,
            rows.into_iter()
                .filter_map(|r| {
                    LoginCredential::new(&r.credential.username, &r.credential.password)
                })
                .collect(),
        ))
    }
    fn save(
        &self,
        profile: &str,
        origin: &str,
        credential: &LoginCredential,
    ) -> Result<(), String> {
        crate::import_credentials::vault(profile)?.upsert(
            &importer::Credential {
                origin: origin.into(),
                username: credential.username().into(),
                password: credential.password().into(),
            },
            chrono::Utc::now().timestamp(),
        )
    }
    fn block(&self, profile: &str, origin: &str) -> Result<(), String> {
        crate::import_credentials::vault(profile)?.block(origin)
    }
    fn list(&self, profile: &str) -> Result<Vec<Record>, String> {
        Ok(crate::import_credentials::vault(profile)?
            .list()?
            .into_iter()
            .filter_map(|r| {
                Some(Record {
                    origin: r.credential.origin.clone(),
                    credential: LoginCredential::new(
                        &r.credential.username,
                        &r.credential.password,
                    )?,
                    note: r.note,
                })
            })
            .collect())
    }
    fn insert(&self, profile: &str, r: &Record) -> Result<(), String> {
        crate::import_credentials::vault(profile)?.insert(
            &r.imported(),
            &r.note,
            chrono::Utc::now().timestamp(),
        )
    }
    fn replace(&self, profile: &str, o: &str, u: &str, r: &Record) -> Result<(), String> {
        crate::import_credentials::vault(profile)?.replace(
            o,
            u,
            &r.imported(),
            &r.note,
            chrono::Utc::now().timestamp(),
        )
    }
    fn remove(&self, profile: &str, o: &str, u: &str) -> Result<(), String> {
        crate::import_credentials::vault(profile)?.remove(o, u)
    }
}
struct Storage(Arc<dyn Store>);
impl Global for Storage {}
pub(crate) fn storage(cx: &mut App) -> Arc<dyn Store> {
    if !cx.has_global::<Storage>() {
        #[cfg(not(test))]
        let backend: Arc<dyn Store> = Arc::new(NativeStore);
        #[cfg(test)]
        let backend: Arc<dyn Store> = Arc::new(MemoryStore::default());
        cx.set_global(Storage(backend));
    }
    cx.global::<Storage>().0.clone()
}
#[cfg(test)]
#[derive(Default)]
struct MemoryStore(
    std::sync::Mutex<(
        HashMap<(String, String, String), Record>,
        std::collections::HashSet<(String, String)>,
    )>,
);
#[cfg(test)]
impl Store for MemoryStore {
    fn lookup(&self, p: &str, o: &str) -> Result<(bool, Vec<LoginCredential>), String> {
        let s = self.0.lock().unwrap();
        Ok((
            s.1.contains(&(p.into(), o.into())),
            s.0.iter()
                .filter(|((profile, origin, _), _)| profile == p && origin == o)
                .map(|(_, c)| c.credential.clone())
                .collect(),
        ))
    }
    fn save(&self, p: &str, o: &str, c: &LoginCredential) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        let key = (p.into(), o.into(), c.username().into());
        let note = s.0.get(&key).map(|r| r.note.clone()).unwrap_or_default();
        s.0.insert(
            key,
            Record {
                origin: o.into(),
                credential: c.clone(),
                note,
            },
        );
        Ok(())
    }
    fn block(&self, p: &str, o: &str) -> Result<(), String> {
        self.0.lock().unwrap().1.insert((p.into(), o.into()));
        Ok(())
    }
    fn list(&self, p: &str) -> Result<Vec<Record>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .0
            .iter()
            .filter(|((profile, _, _), _)| profile == p)
            .map(|(_, r)| r.clone())
            .collect())
    }
    fn insert(&self, p: &str, r: &Record) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        let key = (p.into(), r.origin.clone(), r.credential.username().into());
        if s.0.contains_key(&key) {
            return Err("Existing password".into());
        }
        s.0.insert(key, r.clone());
        Ok(())
    }
    fn replace(&self, p: &str, o: &str, u: &str, r: &Record) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        let old = (p.into(), o.into(), u.into());
        let new = (p.into(), r.origin.clone(), r.credential.username().into());
        if !s.0.contains_key(&old) || (old != new && s.0.contains_key(&new)) {
            return Err("Changed password".into());
        }
        s.0.remove(&old);
        s.0.insert(new, r.clone());
        Ok(())
    }
    fn remove(&self, p: &str, o: &str, u: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .0
            .remove(&(p.into(), o.into(), u.into()));
        Ok(())
    }
}
impl BrowserWindow {
    fn password_source(&self, id: PageIdentity) -> bool {
        !self.browser.is_private()
            && self.browser.user_profile(id.profile).is_some()
            && self.browser.user_profile_id_for_tab(id.tab) == Some(id.profile)
            && self
                .pages
                .as_ref()
                .is_some_and(|p| p.borrow().accepts_identity(id))
    }
    pub(crate) fn password_owner(&self, id: PageIdentity) -> bool {
        self.password_source(id) && self.browser.space().active_tab == Some(id.tab)
    }
    pub(crate) fn password_cross_origin(&self, identity: PageIdentity, origin: &str) -> bool {
        self.passwords
            .documents
            .get(&identity)
            .is_some_and(|d| d.1 != origin)
    }
    pub(crate) fn password_document(
        &self,
        identity: PageIdentity,
        token: PasswordToken,
        origin: &str,
    ) -> bool {
        self.passwords
            .documents
            .get(&identity)
            .is_some_and(|d| d.0 == token && d.1 == origin)
            || self
                .passwords
                .frames
                .iter()
                .any(|((id, _), (t, o))| *id == identity && *t == token && o == origin)
    }
    pub(crate) fn password_event(
        &mut self,
        identity: PageIdentity,
        event: PasswordEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.password_source(identity) {
            return;
        }
        match event {
            PasswordEvent::Dismissed { token, origin } => {
                if self.password_owner(identity)
                    && self.passwords.accounts.as_ref().is_some_and(|a|
                        a.identity == identity && a.token == token && a.origin == origin)
                {
                    self.close_password_accounts(cx);
                }
            }
            PasswordEvent::FrameDocument {
                token,
                origin,
                frame,
                ..
            } => {
                self.passwords
                    .frames
                    .insert((identity, frame), (token, origin));
                if self
                    .passwords
                    .accounts
                    .as_ref()
                    .is_some_and(|a| !self.password_document(a.identity, a.token, &a.origin))
                {
                    self.close_password_accounts(cx);
                }
            }
            PasswordEvent::Document {
                token,
                origin,
                has_password_form,
            } => {
                if self
                    .passwords
                    .documents
                    .get(&identity)
                    .is_none_or(|d| d.0 != token)
                {
                    self.passwords.frames.retain(|(id, _), _| *id != identity);
                }
                if let Some(pages) = &self.pages {
                    self.passwords
                        .documents
                        .retain(|id, _| pages.borrow().accepts_identity(*id));
                }
                self.passwords
                    .documents
                    .insert(identity, (token, origin, has_password_form));
                if self
                    .passwords
                    .prompt
                    .as_ref()
                    .is_some_and(|h| h.candidate.identity == identity && h.token != token)
                {
                    self.close_password_prompt(PopupFocus::Restore, cx);
                }
                if self
                    .passwords
                    .accounts
                    .as_ref()
                    .is_some_and(|a| !self.password_document(a.identity, a.token, &a.origin))
                {
                    self.close_password_accounts(cx);
                }
                self.maybe_password_prompt(cx);
            }
            PasswordEvent::FillRequested {
                token,
                origin,
                username_hint,
                anchor,
                field,
            } => {
                if !self.password_owner(identity) {
                    return;
                }
                if !self.password_document(identity, token, &origin) {
                    return;
                }
                self.passwords.lookup_generation = self.passwords.lookup_generation.wrapping_add(1);
                let generation = self.passwords.lookup_generation;
                let backend = storage(cx);
                let key = profile_key(identity);
                let store_revision = revision(&key, cx);
                let task = cx.background_executor().spawn(async move {
                    let (_, mut rows) = backend.lookup(&key, &origin)?;
                    rows.retain(|r| {
                        username_hint.is_empty()
                            || if anchor.is_some() {
                                r.username()
                                    .to_lowercase()
                                    .starts_with(&username_hint.to_lowercase())
                            } else {
                                r.username() == username_hint.as_str()
                            }
                    });
                    rows.sort_by(|a, b| a.username().cmp(b.username()));
                    Ok::<_, String>((origin, rows))
                });
                cx.spawn(async move |this, cx| {
                    if let Ok((origin, rows)) = task.await {
                        let _ = this.update(cx, |this, cx| {
                            if revision(&profile_key(identity), cx) != store_revision
                                || this.passwords.lookup_generation != generation
                                || !this.password_owner(identity)
                                || !this.password_document(identity, token, &origin)
                            {
                                return;
                            }
                            if let Some(anchor) = anchor {
                                this.show_password_accounts(
                                    identity, token, origin, rows, anchor, field, cx,
                                );
                            } else if this
                                .passwords
                                .documents
                                .get(&identity)
                                .is_some_and(|d| d.1 == origin)
                            {
                                if let (Some(p), Some(credential)) = (&this.pages, rows.first()) {
                                    p.borrow_mut()
                                        .fill_password(identity, token, &origin, credential);
                                }
                            }
                        });
                    }
                })
                .detach();
            }
            PasswordEvent::Submitted {
                token: _,
                origin,
                credential,
            } => {
                if !self.password_owner(identity) {
                    return;
                }
                if self.passwords.prompt.is_some() {
                    return;
                }
                self.passwords.serial = self.passwords.serial.wrapping_add(1);
                let serial = self.passwords.serial;
                let check = credential.clone();
                let source = origin.clone();
                let key = profile_key(identity);
                let backend = storage(cx);
                self.passwords.pending = Some(Candidate {
                    identity,
                    origin,
                    credential,
                    created: Instant::now(),
                    ready: false,
                    serial,
                });
                let task = cx.background_executor().spawn(async move {
                    let (blocked, rows) = backend.lookup(&key, &source)?;
                    Ok::<_, String>(!blocked && !rows.iter().any(|r| r == &check))
                });
                cx.spawn(async move |this, cx| {
                    let show = task.await.unwrap_or(false);
                    let _ = this.update(cx, |this, cx| {
                        if this
                            .passwords
                            .pending
                            .as_ref()
                            .is_some_and(|p| p.serial == serial)
                        {
                            if show {
                                this.passwords.pending.as_mut().unwrap().ready = true;
                                this.maybe_password_prompt(cx);
                            } else {
                                this.passwords.pending = None;
                            }
                        }
                    });
                })
                .detach();
            }
        }
    }
    pub(crate) fn maybe_password_prompt(&mut self, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() {
            self.passwords.pending = None;
            return;
        }
        let Some(candidate) = self.passwords.pending.as_ref() else {
            return;
        };
        if !self.password_owner(candidate.identity)
            || candidate.created.elapsed() > Duration::from_secs(30)
        {
            self.passwords.pending = None;
            return;
        }
        if !candidate.ready
            || !self
                .passwords
                .documents
                .get(&candidate.identity)
                .is_some_and(|d| !d.2)
            || self.command_bar.is_some()
            || self.user_profile_dialog.is_some()
            || self.setup.is_some()
        {
            return;
        }
        let token = self.passwords.documents[&candidate.identity].0;
        let candidate = self.passwords.pending.take().unwrap();
        self.close_password_accounts(cx);
        let host = url::Url::parse(&candidate.origin)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        if let Some(p) = &self.pages {
            p.borrow_mut().request_ui_focus();
        }
        let dialog = cx.new(|cx| PasswordPrompt::new(host, cx));
        let events = cx.subscribe(
            &dialog,
            |this: &mut Self, sender: Entity<PasswordPrompt>, event: &Decision, cx| {
                if !this
                    .passwords
                    .prompt
                    .as_ref()
                    .is_some_and(|h| h.dialog == sender)
                {
                    return;
                }
                let Some(host) = this.passwords.prompt.take() else {
                    return;
                };
                let valid = this.password_owner(host.candidate.identity)
                    && this
                        .passwords
                        .documents
                        .get(&host.candidate.identity)
                        .is_some_and(|d| d.0 == host.token);
                this.close_popup_overlay(host.overlay, PopupFocus::Restore, true, cx);
                if valid && *event != Decision::No {
                    let decision = *event;
                    let backend = storage(cx);
                    let key = profile_key(host.candidate.identity);
                    let accepted = lease(cx);
                    let changed_key = key.clone();
                    let worker = cx.background_executor().spawn(async move {
                        let _accepted = accepted;
                        if decision == Decision::Never {
                            backend.block(&key, &host.candidate.origin)
                        } else {
                            backend.save(&key, &host.candidate.origin, &host.candidate.credential)
                        }
                    });
                    cx.spawn(async move |_, cx| {
                        if worker.await.is_ok() {
                            let _ = cx.update(|cx| publish_change(changed_key, cx));
                        } else {
                            eprintln!("The local password store could not be updated.");
                        }
                    })
                    .detach();
                }
                cx.notify();
            },
        );
        self.passwords.prompt = Some(PromptHost {
            dialog: dialog.clone(),
            overlay: None,
            _events: events,
            token,
            candidate,
        });
        let owner = cx.entity().downgrade();
        let main = self.main_window;
        let parent = self.native_view;
        cx.notify();
        cx.defer(move |cx| {
            if !owner
                .read_with(cx, |this, _| {
                    this.passwords
                        .prompt
                        .as_ref()
                        .is_some_and(|h| h.dialog == dialog)
                })
                .unwrap_or(false)
            {
                return;
            }
            if crate::quit::is_quitting() {
                return;
            }
            let Ok((bounds, active)) =
                main.update(cx, |_, w, _| (w.bounds(), w.is_window_active()))
            else {
                return;
            };
            if !active {
                let _ = owner.update(cx, |this, _| {
                    if this
                        .passwords
                        .prompt
                        .as_ref()
                        .is_some_and(|h| h.dialog == dialog)
                    {
                        this.passwords.pending = this.passwords.prompt.take().map(|h| h.candidate);
                    }
                });
                return;
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::PasswordPrompt, window, cx))
            }) else {
                let _ = owner.update(cx, |this, cx| {
                    this.close_password_prompt(PopupFocus::Restore, cx)
                });
                return;
            };
            let attached = owner
                .update(cx, |this, _| {
                    if let Some(h) = this
                        .passwords
                        .prompt
                        .as_mut()
                        .filter(|h| h.dialog == dialog)
                    {
                        h.overlay = Some(overlay);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if attached {
                let _ = overlay.update(cx, |_, w, cx| {
                    w.activate_window();
                    dialog.read(cx).focus_handle(cx).focus(w);
                });
            } else {
                let _ = overlay.update(cx, |_, w, _| w.remove_window());
            }
        });
    }
    pub(crate) fn resize_password_prompt(&mut self, window: &gpui::Window, cx: &mut Context<Self>) {
        self.close_password_accounts(cx);
        let Some(child) = self.passwords.prompt.as_ref().and_then(|h| h.overlay) else {
            return;
        };
        let size = window.viewport_size();
        cx.defer(move |cx| {
            let _ = child.update(cx, |_, window, cx| {
                if window.viewport_size() != size {
                    window.resize(size);
                }
                cx.notify();
            });
        });
    }
    pub(crate) fn validate_password_prompt(&mut self, cx: &mut Context<Self>) {
        if self.passwords.accounts.as_ref().is_some_and(|a| {
            !self.password_owner(a.identity)
                || !self.password_document(a.identity, a.token, &a.origin)
                || revision(&profile_key(a.identity), cx) != a.revision
        }) {
            self.close_password_accounts(cx);
        }
        if self
            .passwords
            .prompt
            .as_ref()
            .is_some_and(|h| !self.password_owner(h.candidate.identity))
        {
            self.close_password_prompt(PopupFocus::OnActivation, cx);
        }
    }
    pub(crate) fn close_password_prompt(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        self.close_password_accounts(cx);
        self.passwords.pending = None;
        if let Some(h) = self.passwords.prompt.take() {
            h.dialog.update(cx, |d, cx| d.close(cx));
            self.close_popup_overlay(h.overlay, focus, true, cx);
            cx.notify();
        }
    }
    pub(crate) fn take_password_child(&mut self, cx: &mut App) -> Option<WindowHandle<Overlay>> {
        self.passwords.pending = None;
        if let Some(a) = self.passwords.accounts.take() {
            if let Some(p) = &self.pages {
                p.borrow_mut().password_selection_active(a.identity, false);
            }
            return a.overlay;
        }
        let h = self.passwords.prompt.take()?;
        h.dialog.update(cx, |d, cx| d.close(cx));
        h.overlay
    }
    pub(crate) fn render_password_prompt(&self) -> AnyElement {
        self.passwords.prompt.as_ref().map_or_else(
            || div().into_any_element(),
            |h| h.dialog.clone().into_any_element(),
        )
    }
}
#[cfg(test)]
impl BrowserWindow {
    pub(crate) fn password_overlay(&self) -> Option<WindowHandle<Overlay>> {
        self.passwords.prompt.as_ref().and_then(|h| h.overlay)
    }
    pub(crate) fn password_saved(&self, origin: &str, cx: &mut App) -> (bool, usize) {
        let p = self.browser.active_user_profile().id();
        let (blocked, rows) = storage(cx)
            .lookup(&p.storage_key().unwrap_or_else(|| "default".into()), origin)
            .unwrap();
        (blocked, rows.len())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn accepted_password_writes_hold_quit_until_the_worker_drops_its_lease(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            assert!(!writes_pending(cx));
            let first = lease(cx);
            let second = lease(cx);
            assert!(writes_pending(cx));
            drop(first);
            assert!(writes_pending(cx));
            drop(second);
            assert!(!writes_pending(cx));
        });
    }
    #[test]
    fn password_memory_backend_keeps_profiles_origins_accounts_and_never_policy_separate() {
        let store = MemoryStore::default();
        let credential = LoginCredential::new("test-user", "test-password").unwrap();
        store
            .save("default", "https://one.invalid", &credential)
            .unwrap();
        store.block("profile-2", "https://one.invalid").unwrap();
        assert_eq!(
            store
                .lookup("default", "https://one.invalid")
                .unwrap()
                .1
                .len(),
            1
        );
        assert_eq!(
            store
                .lookup("default", "https://other.invalid")
                .unwrap()
                .1
                .len(),
            0
        );
        let (blocked, rows) = store.lookup("profile-2", "https://one.invalid").unwrap();
        assert!(blocked);
        assert!(rows.is_empty());
        assert!(!store.lookup("default", "https://one.invalid").unwrap().0);
    }
}
