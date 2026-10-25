//! Immutable page/profile authority for website passwords. Secrets never enter session state.
use crate::{
    pages::PageIdentity,
    password_prompt::{Decision, PasswordPrompt},
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use engine::{LoginCredential, PasswordEvent, PasswordToken};
use gpui::{
    div, prelude::*, AnyElement, App, Context, Entity, Focusable, Global, Subscription,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct PasswordFlow {
    documents: HashMap<PageIdentity, (PasswordToken, String, bool)>,
    pending: Option<Candidate>,
    pub(crate) prompt: Option<PromptHost>,
    serial: u64,
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
struct Writes(Arc<AtomicUsize>);
impl Global for Writes {}
struct WriteLease(Arc<AtomicUsize>);
impl Drop for WriteLease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
pub(crate) fn writes_pending(cx: &App) -> bool {
    cx.try_global::<Writes>()
        .is_some_and(|w| w.0.load(Ordering::SeqCst) > 0)
}
fn lease(cx: &mut App) -> WriteLease {
    if !cx.has_global::<Writes>() {
        cx.set_global(Writes::default());
    }
    let n = cx.global::<Writes>().0.clone();
    n.fetch_add(1, Ordering::SeqCst);
    WriteLease(n)
}
fn profile_key(identity: PageIdentity) -> String {
    identity
        .profile
        .storage_key()
        .unwrap_or_else(|| "default".into())
}

trait Store: Send + Sync {
    fn lookup(&self, profile: &str, origin: &str) -> Result<(bool, Vec<LoginCredential>), String>;
    fn save(&self, profile: &str, origin: &str, credential: &LoginCredential)
        -> Result<(), String>;
    fn block(&self, profile: &str, origin: &str) -> Result<(), String>;
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
}
struct Storage(Arc<dyn Store>);
impl Global for Storage {}
fn storage(cx: &mut App) -> Arc<dyn Store> {
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
        HashMap<(String, String, String), LoginCredential>,
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
                .map(|(_, c)| c.clone())
                .collect(),
        ))
    }
    fn save(&self, p: &str, o: &str, c: &LoginCredential) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .0
            .insert((p.into(), o.into(), c.username().into()), c.clone());
        Ok(())
    }
    fn block(&self, p: &str, o: &str) -> Result<(), String> {
        self.0.lock().unwrap().1.insert((p.into(), o.into()));
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
    fn password_owner(&self, id: PageIdentity) -> bool {
        self.password_source(id) && self.browser.space().active_tab == Some(id.tab)
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
            PasswordEvent::Document {
                token,
                origin,
                has_password_form,
            } => {
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
                self.maybe_password_prompt(cx);
            }
            PasswordEvent::FillRequested {
                token,
                origin,
                username_hint,
            } => {
                if !self.password_owner(identity) {
                    return;
                }
                if !self
                    .passwords
                    .documents
                    .get(&identity)
                    .is_some_and(|d| d.0 == token && d.1 == origin)
                {
                    return;
                }
                let backend = storage(cx);
                let key = profile_key(identity);
                let task = cx.background_executor().spawn(async move {
                    let (_, rows) = backend.lookup(&key, &origin)?;
                    let mut matches = rows.into_iter().filter(|r| {
                        username_hint.is_empty() || r.username() == username_hint.as_str()
                    });
                    let first = matches.next();
                    let unique = matches.next().is_none();
                    drop(matches);
                    Ok::<_, String>((origin, if unique { first } else { None }))
                });
                cx.spawn(async move |this, cx| {
                    if let Ok((origin, Some(credential))) = task.await {
                        let _ = this.update(cx, |this, _| {
                            if this.password_owner(identity)
                                && this
                                    .passwords
                                    .documents
                                    .get(&identity)
                                    .is_some_and(|d| d.0 == token && d.1 == origin)
                            {
                                if let Some(p) = &this.pages {
                                    p.borrow_mut().fill_password(
                                        identity,
                                        token,
                                        &origin,
                                        &credential,
                                    );
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
                    cx.background_executor()
                        .spawn(async move {
                            let _accepted = accepted;
                            let result = if decision == Decision::Never {
                                backend.block(&key, &host.candidate.origin)
                            } else {
                                backend.save(
                                    &key,
                                    &host.candidate.origin,
                                    &host.candidate.credential,
                                )
                            };
                            if result.is_err() {
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
    pub(crate) fn resize_password_prompt(&self, window: &gpui::Window, cx: &mut Context<Self>) {
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
        self.passwords.pending = None;
        if let Some(h) = self.passwords.prompt.take() {
            h.dialog.update(cx, |d, cx| d.close(cx));
            self.close_popup_overlay(h.overlay, focus, true, cx);
            cx.notify();
        }
    }
    pub(crate) fn take_password_child(&mut self, cx: &mut App) -> Option<WindowHandle<Overlay>> {
        self.passwords.pending = None;
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
