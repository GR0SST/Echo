//! GPUI password manager. Layout/copy come from Arc's resource 20047 and locale.pak.
use crate::{
    icons::{self, Icon},
    password_flow::{self, Record},
    text_input::{Changed, TextInput},
    window::{BrowserWindow, PopupFocus},
};
use browser_core::{TabId, UserProfileId};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, Context, Entity, FocusHandle, Focusable,
    Subscription, Window,
};
use std::{collections::BTreeSet, time::Duration};
use zeroize::Zeroizing;
pub(crate) const URL: &str = "browser://password-manager/passwords";
const HELP: &str = "https://support.google.com/chrome?p=settings_password";
gpui::actions!(password_manager, [Accept, Cancel, Next, Previous, Activate]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", Accept, Some("PasswordManagerEditor")),
        gpui::KeyBinding::new("escape", Cancel, Some("PasswordManagerEditor")),
        gpui::KeyBinding::new("tab", Next, Some("PasswordManagerEditor")),
        gpui::KeyBinding::new("shift-tab", Previous, Some("PasswordManagerEditor")),
        gpui::KeyBinding::new("space", Activate, Some("PasswordManagerButton")),
    ]);
}
#[derive(Clone, PartialEq, Eq)]
struct Scope {
    profile: UserProfileId,
    tab: TabId,
    location: String,
}
struct Editor {
    id: u64,
    busy: bool,
    scope: Scope,
    old: Option<Record>,
    site: Entity<TextInput>,
    user: Entity<TextInput>,
    password: Entity<TextInput>,
    note: Entity<TextInput>,
    reveal: bool,
    eye: FocusHandle,
    cancel: FocusHandle,
    save: FocusHandle,
    return_focus: FocusHandle,
    _events: Vec<Subscription>,
}
struct Notice {
    text: &'static str,
    undo: Option<Record>,
    generation: u64,
}
pub(crate) struct State {
    scope: Option<Scope>,
    filter: Entity<TextInput>,
    _filter: Subscription,
    focus: FocusHandle,
    back: FocusHandle,
    add: FocusHandle,
    edit_focus: std::collections::HashMap<String, FocusHandle>,
    rows: Vec<Record>,
    loaded: bool,
    generation: u64,
    revision: u64,
    next_notice: u64,
    next_editor: u64,
    editor: Option<Editor>,
    detail: Option<String>,
    revealed: BTreeSet<String>,
    notice: Option<Notice>,
    scroll: gpui::ScrollHandle,
}
impl State {
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let (filter, changed) = new_filter(cx);
        Self {
            scope: None,
            filter,
            _filter: changed,
            focus: cx.focus_handle(),
            back: cx.focus_handle(),
            add: cx.focus_handle(),
            edit_focus: Default::default(),
            rows: Vec::new(),
            loaded: false,
            generation: 0,
            revision: 0,
            next_notice: 0,
            next_editor: 0,
            editor: None,
            detail: None,
            revealed: BTreeSet::new(),
            notice: None,
            scroll: gpui::ScrollHandle::new(),
        }
    }
}
fn new_filter(cx: &mut Context<BrowserWindow>) -> (Entity<TextInput>, Subscription) {
    let filter = cx.new(|cx| TextInput::new("Search passwords", cx));
    let changed = cx.subscribe(
        &filter,
        |this: &mut BrowserWindow, sender, _: &Changed, cx| {
            if this.password_manager.filter != sender {
                return;
            }
            let source = this.password_manager.scope.clone();
            let owner = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = owner.update(cx, |this, cx| {
                    if this.password_manager.scope == source
                        && this.password_manager.filter == sender
                    {
                        this.password_manager_query(cx);
                    }
                });
            });
        },
    );
    (filter, changed)
}
fn key(profile: UserProfileId) -> String {
    profile.storage_key().unwrap_or_else(|| "default".into())
}
fn site_name(origin: &str) -> String {
    origin.strip_prefix("https://").unwrap_or(origin).into()
}
fn parse_site(text: &str) -> Option<String> {
    let text = text.trim();
    if text.len() > 8192 {
        return None;
    }
    let parsed = url::Url::parse(text)
        .or_else(|_| url::Url::parse(&format!("https://{text}")))
        .ok()?;
    (matches!(parsed.scheme(), "http" | "https")
        && parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none())
    .then(|| parsed.origin().ascii_serialization())
}
fn record_key(r: &Record) -> String {
    format!("{}\0{}", r.origin, r.credential.username())
}
impl BrowserWindow {
    fn password_manager_scope(&self) -> Option<Scope> {
        if self.browser.is_private() || self.browser.active_split().is_some() {
            return None;
        }
        let tab = self.browser.active_tab()?;
        let u = url::Url::parse(&tab.url).ok()?;
        if u.scheme() != "browser"
            || u.host_str() != Some("password-manager")
            || !(u.path() == "/passwords" || u.path().starts_with("/passwords/"))
        {
            return None;
        }
        let profile = u
            .query_pairs()
            .find(|(k, _)| k == "profile")
            .map(|(_, v)| serde_json::from_str(&v).ok())
            .unwrap_or(Some(
                self.browser
                    .blank_source_user_profile_id()
                    .unwrap_or(self.browser.active_user_profile_id()),
            ))?;
        self.profile.handle.read_settings(|b| {
            b.user_profile(profile).map(|_| Scope {
                profile,
                tab: tab.id,
                location: tab.url.clone(),
            })
        })
    }
    pub(crate) fn password_manager_is_open(&self) -> bool {
        self.password_manager_scope().is_some()
    }
    fn manager_current(&self, scope: &Scope) -> bool {
        self.password_manager_scope().as_ref() == Some(scope)
            && self.password_manager.scope.as_ref() == Some(scope)
    }
    pub(crate) fn password_store_changed(&mut self, cx: &mut Context<Self>) {
        if !crate::quit::is_quitting() && self.password_manager.scope.is_some() {
            self.sync_password_manager(cx);
            cx.notify();
        }
    }
    pub(crate) fn sync_password_manager(&mut self, cx: &mut Context<Self>) {
        let scope = self.password_manager_scope();
        if self.password_manager.scope == scope {
            if let Some(scope) = scope {
                let revision = password_flow::revision(&key(scope.profile), cx);
                if self.password_manager.revision != revision {
                    self.password_manager.revision = revision;
                    self.password_manager.generation =
                        self.password_manager.generation.wrapping_add(1);
                    self.load_password_manager(cx);
                }
            }
            return;
        }
        if self.password_manager.scope.is_some() && scope.is_none() {
            let owner = cx.entity().downgrade();
            let main = self.main_window;
            let focus = self.focus_handle.clone();
            cx.defer(move |cx| {
                let wanted = owner
                    .read_with(cx, |this, _| {
                        this.password_manager_scope().is_none()
                            && this
                                .browser
                                .active_tab()
                                .is_some_and(|t| t.url.starts_with("browser://password-manager/"))
                            && !this.popup_owns_focus()
                    })
                    .unwrap_or(false);
                if wanted {
                    let _ = main.update(cx, |_, window, _| {
                        if window.is_window_active() {
                            focus.focus(window);
                        }
                    });
                }
            });
        }
        self.password_manager.scope = scope.clone();
        let (filter, changed) = new_filter(cx);
        self.password_manager.filter = filter;
        self.password_manager._filter = changed;
        self.password_manager.rows.clear();
        self.password_manager.edit_focus.clear();
        self.password_manager.loaded = false;
        self.password_manager.editor = None;
        self.password_manager.detail = None;
        self.password_manager.revealed.clear();
        self.password_manager.notice = None;
        self.password_manager.generation = self.password_manager.generation.wrapping_add(1);
        let Some(scope) = scope else {
            return;
        };
        if self
            .browser
            .active_tab()
            .is_some_and(|t| t.title != "Password Manager")
        {
            self.edit_browser(|b| b.set_title(scope.tab, "Password Manager"));
        }
        self.password_manager.revision = password_flow::revision(&key(scope.profile), cx);
        let u = url::Url::parse(&scope.location).unwrap();
        let query = u
            .query_pairs()
            .find(|(k, _)| k == "q")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
        if let Some(path) = u.path().strip_prefix("/passwords/") {
            self.password_manager.detail =
                url::form_urlencoded::parse(format!("site={path}").as_bytes())
                    .next()
                    .map(|(_, v)| v.into_owned())
                    .filter(|s| parse_site(s).as_deref() == Some(s));
        }
        self.password_manager
            .filter
            .update(cx, |input, cx| input.set_text_at_end(query, cx));
        self.load_password_manager(cx);
        let focus = if self.password_manager.detail.is_some() {
            self.password_manager.back.clone()
        } else {
            self.password_manager.filter.read(cx).focus_handle(cx)
        };
        self.manager_focus(focus, cx);
    }
    fn load_password_manager(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        let generation = self.password_manager.generation;
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let task = cx
            .background_executor()
            .spawn(async move { store.list(&profile) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.password_manager.generation != generation
                    || !this
                        .password_manager
                        .scope
                        .as_ref()
                        .is_some_and(|s| s.profile == scope.profile && s.tab == scope.tab)
                    || this.password_manager_scope().is_none()
                {
                    return;
                }
                match result {
                    Ok(mut rows) => {
                        rows.sort_by(|a, b| {
                            (&a.origin, a.credential.username())
                                .cmp(&(&b.origin, b.credential.username()))
                        });
                        for r in &rows {
                            this.password_manager
                                .edit_focus
                                .entry(record_key(r))
                                .or_insert_with(|| cx.focus_handle());
                        }
                        this.password_manager.rows = rows;
                        this.password_manager.loaded = true;
                        if this.password_manager.detail.as_ref().is_some_and(|o| {
                            !this.password_manager.rows.iter().any(|r| r.origin == *o)
                        }) {
                            this.password_manager.detail = None;
                            this.manager_route(None, cx);
                        }
                    }
                    Err(_) => eprintln!("The local password store could not be opened."),
                };
                cx.notify();
            });
        })
        .detach();
    }
    fn manager_focus(&self, focus: FocusHandle, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        let main = self.main_window;
        let owner = cx.entity().downgrade();
        cx.defer(move |cx| {
            if owner
                .read_with(cx, |this, _| {
                    this.manager_current(&scope) && this.command_bar.is_none()
                })
                .unwrap_or(false)
            {
                let _ = main.update(cx, |_, window, _| {
                    if window.is_window_active() {
                        focus.focus(window);
                    }
                });
            }
        });
    }
    fn password_manager_query(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        let text = self.password_manager.filter.read(cx).text().to_owned();
        let mut u = url::Url::parse(&scope.location).unwrap();
        let retained = u
            .query_pairs()
            .filter(|(k, _)| k != "q")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        u.set_query(None);
        {
            let mut q = u.query_pairs_mut();
            q.extend_pairs(retained);
            if !text.is_empty() {
                q.append_pair("q", &text);
            }
        }
        let location = u.to_string();
        self.password_manager.scope.as_mut().unwrap().location = location.clone();
        self.edit_browser(|b| b.set_url(scope.tab, location));
        cx.notify();
    }
    fn manager_route(&mut self, detail: Option<String>, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        let mut u = url::Url::parse(&scope.location).unwrap();
        let path = detail
            .as_ref()
            .map(|o| {
                format!(
                    "/passwords/{}",
                    url::form_urlencoded::byte_serialize(o.as_bytes()).collect::<String>()
                )
            })
            .unwrap_or_else(|| "/passwords".into());
        u.set_path(&path);
        let location = u.to_string();
        self.password_manager.detail = detail;
        self.password_manager.revealed.clear();
        self.password_manager.scope.as_mut().unwrap().location = location.clone();
        self.edit_browser(|b| b.set_url(scope.tab, location));
        let focus = if self.password_manager.detail.is_some() {
            self.password_manager.back.clone()
        } else {
            self.password_manager.filter.read(cx).focus_handle(cx)
        };
        self.manager_focus(focus, cx);
        cx.notify();
    }
    fn manager_select(&mut self, origin: String, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        // Re-read through the OS-held key before entering an existing credential.
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let task = cx
            .background_executor()
            .spawn(async move { store.list(&profile) });
        cx.spawn(async move |this, cx| {
            if let Ok(mut rows) = task.await {
                let _ = this.update(cx, |this, cx| {
                    if this.manager_current(&scope) && rows.iter().any(|r| r.origin == origin) {
                        rows.sort_by(|a, b| {
                            (&a.origin, a.credential.username())
                                .cmp(&(&b.origin, b.credential.username()))
                        });
                        for r in &rows {
                            this.password_manager
                                .edit_focus
                                .entry(record_key(r))
                                .or_insert_with(|| cx.focus_handle());
                        }
                        this.password_manager.rows = rows;
                        this.manager_route(Some(origin), cx);
                    }
                });
            }
        })
        .detach();
    }
    fn present_password_editor(
        &mut self,
        old: Option<Record>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        let site = cx.new(|cx| TextInput::new("example.com", cx).with_placeholder_opacity(0.78));
        let user = cx.new(|cx| TextInput::new("", cx));
        let password = cx.new(|cx| TextInput::secret("", cx));
        let note = cx.new(TextInput::form_textarea);
        if let Some(r) = &old {
            site.update(cx, |i, cx| i.set_text(r.origin.clone(), cx));
            user.update(cx, |i, cx| {
                i.set_text(r.credential.username().to_owned(), cx)
            });
            password.update(cx, |i, cx| {
                i.set_text(r.credential.password().to_owned(), cx)
            });
            note.update(cx, |i, cx| i.set_text(r.note.as_str().to_owned(), cx));
        }
        let events = [&site, &user, &password, &note]
            .into_iter()
            .map(|i| cx.subscribe(i, |_, _, _: &Changed, cx| cx.notify()))
            .collect();
        let focus = if old.is_some() {
            user.read(cx).focus_handle(cx)
        } else {
            site.read(cx).focus_handle(cx)
        };
        self.password_manager.next_editor = self.password_manager.next_editor.wrapping_add(1);
        let return_focus = old
            .as_ref()
            .and_then(|r| self.password_manager.edit_focus.get(&record_key(r)))
            .unwrap_or(&self.password_manager.add)
            .clone();
        self.password_manager.editor = Some(Editor {
            id: self.password_manager.next_editor,
            busy: false,
            return_focus,
            scope,
            old,
            site,
            user,
            password,
            note,
            reveal: false,
            eye: cx.focus_handle(),
            cancel: cx.focus_handle(),
            save: cx.focus_handle(),
            _events: events,
        });
        focus.focus(window);
        cx.notify();
    }
    fn password_editor_value(&self, cx: &App) -> Option<Record> {
        let e = self.password_manager.editor.as_ref()?;
        if e.busy || !self.manager_current(&e.scope) {
            return None;
        }
        let origin = parse_site(e.site.read(cx).text())?;
        let note = e.note.read(cx).text();
        if note.encode_utf16().count() > 1000 {
            return None;
        }
        let credential =
            engine::LoginCredential::new(e.user.read(cx).text(), e.password.read(cx).text())?;
        if self.password_manager.rows.iter().any(|r| {
            r.origin == origin
                && r.credential.username() == credential.username()
                && !e.old.as_ref().is_some_and(|old| {
                    old.origin == r.origin && old.credential.username() == r.credential.username()
                })
        }) {
            return None;
        }
        Some(Record {
            origin,
            credential,
            note: Zeroizing::new(note.into()),
        })
    }
    fn password_editor_is(&self, id: u64) -> bool {
        self.password_manager
            .editor
            .as_ref()
            .is_some_and(|e| e.id == id && self.manager_current(&e.scope))
    }
    fn cancel_password_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.password_manager.editor.take() {
            editor.return_focus.focus(window);
        }
        cx.notify();
    }
    fn password_editor_focus(&self, backwards: bool, window: &mut Window, cx: &App) {
        let Some(e) = &self.password_manager.editor else {
            return;
        };
        let mut handles = Vec::new();
        if e.old.is_none() {
            handles.push(e.site.read(cx).focus_handle(cx));
        }
        handles.extend([
            e.user.read(cx).focus_handle(cx),
            e.password.read(cx).focus_handle(cx),
            e.eye.clone(),
            e.note.read(cx).focus_handle(cx),
            e.cancel.clone(),
        ]);
        if self.password_editor_value(cx).is_some() {
            handles.push(e.save.clone());
        }
        let i = handles.iter().position(|h| h.is_focused(window));
        let index = match (i, backwards) {
            (Some(i), true) => (i + handles.len() - 1) % handles.len(),
            (Some(i), false) => (i + 1) % handles.len(),
            (None, true) => handles.len() - 1,
            (None, false) => 0,
        };
        handles[index].focus(window);
    }
    fn toggle_password_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = &mut self.password_manager.editor {
            e.reveal = !e.reveal;
            e.password.update(cx, |i, cx| i.set_secure(!e.reveal, cx));
            cx.notify();
        }
    }
    fn accept_password_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = &self.password_manager.editor else {
            return;
        };
        if e.cancel.is_focused(window) {
            self.cancel_password_editor(window, cx);
            return;
        }
        if e.eye.is_focused(window) {
            self.toggle_password_editor(cx);
            return;
        }
        let Some(record) = self.password_editor_value(cx) else {
            return;
        };
        let scope = e.scope.clone();
        let id = e.id;
        let old = e.old.clone();
        self.password_manager.editor.as_mut().unwrap().busy = true;
        self.password_manager.focus.focus(window);
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let lease = password_flow::lease(cx);
        let changed_key = profile.clone();
        let task = cx.background_executor().spawn(async move {
            let _lease = lease;
            if let Some(old) = old {
                store.replace(&profile, &old.origin, old.credential.username(), &record)
            } else {
                store.insert(&profile, &record)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            if result.is_ok() {
                let _ = cx.update(|cx| password_flow::publish_change(changed_key, cx));
            }
            let _ = this.update(cx, |this, cx| {
                if this.manager_current(&scope) {
                    if this
                        .password_manager
                        .editor
                        .as_ref()
                        .is_some_and(|e| e.id == id)
                    {
                        if result.is_ok() {
                            this.password_manager.editor = None;
                        } else {
                            this.password_manager.editor.as_mut().unwrap().busy = false;
                        }
                    }
                    if result.is_err() {
                        eprintln!("The local password store could not be updated.");
                    }
                    this.load_password_manager(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn manager_copy(&mut self, r: Record, password: bool, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        if !password {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                r.credential.username().into(),
            ));
            self.manager_notice("Username copied to clipboard", None, cx);
            return;
        }
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let origin = r.origin.clone();
        let user = r.credential.username().to_owned();
        let task = cx.background_executor().spawn(async move {
            store
                .lookup(&profile, &origin)
                .map(|(_, rows)| rows.into_iter().find(|r| r.username() == user))
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Some(credential)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    if this.manager_current(&scope) {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            credential.password().into(),
                        ));
                        this.manager_notice("Password copied to clipboard", None, cx);
                    }
                });
            }
        })
        .detach();
    }
    fn manager_reveal(&mut self, r: Record, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        let identity = record_key(&r);
        if self.password_manager.revealed.remove(&identity) {
            cx.notify();
            return;
        }
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let origin = r.origin.clone();
        let user = r.credential.username().to_owned();
        let task = cx.background_executor().spawn(async move {
            store
                .lookup(&profile, &origin)
                .map(|(_, rows)| rows.into_iter().find(|r| r.username() == user))
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Some(credential)) = task.await {
                let _ = this.update(cx, |this, cx| {
                    if this.manager_current(&scope) {
                        if let Some(row) = this
                            .password_manager
                            .rows
                            .iter_mut()
                            .find(|r| record_key(r) == identity)
                        {
                            row.credential = credential;
                            this.password_manager.revealed.insert(identity);
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
    }
    fn manager_delete(&mut self, r: Record, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let accepted = password_flow::lease(cx);
        let removed = r.clone();
        let changed_key = profile.clone();
        let task = cx.background_executor().spawn(async move {
            let _accepted = accepted;
            store.remove(&profile, &removed.origin, removed.credential.username())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            if result.is_ok() {
                let _ = cx.update(|cx| password_flow::publish_change(changed_key, cx));
            }
            let _ = this.update(cx, |this, cx| {
                if this.manager_current(&scope) && result.is_ok() {
                    this.manager_route(None, cx);
                    this.manager_notice("Password deleted", Some(r), cx);
                    this.load_password_manager(cx);
                }
            });
        })
        .detach();
    }
    fn manager_notice(&mut self, text: &'static str, undo: Option<Record>, cx: &mut Context<Self>) {
        self.password_manager.next_notice = self.password_manager.next_notice.wrapping_add(1);
        let generation = self.password_manager.next_notice;
        self.password_manager.notice = Some(Notice {
            text,
            undo,
            generation,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .password_manager
                    .notice
                    .as_ref()
                    .is_some_and(|n| n.generation == generation)
                {
                    this.password_manager.notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn manager_undo(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.password_manager.scope.clone() else {
            return;
        };
        if !self.manager_current(&scope) {
            return;
        }
        let Some(record) = self.password_manager.notice.take().and_then(|n| n.undo) else {
            return;
        };
        let store = password_flow::storage(cx);
        let profile = key(scope.profile);
        let accepted = password_flow::lease(cx);
        let changed_key = profile.clone();
        let task = cx.background_executor().spawn(async move {
            let _accepted = accepted;
            store.insert(&profile, &record)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            if result.is_ok() {
                let _ = cx.update(|cx| password_flow::publish_change(changed_key, cx));
            }
            let _ = this.update(cx, |this, cx| {
                if this.manager_current(&scope) && result.is_ok() {
                    this.load_password_manager(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn manager_open_site(&mut self, profile: UserProfileId, url: String, cx: &mut Context<Self>) {
        if !self
            .profile
            .handle
            .read_settings(|b| b.user_profile(profile).is_some())
        {
            return;
        }
        if let Some(space) = self
            .browser
            .spaces
            .iter()
            .position(|s| s.user_profile_id == profile)
        {
            self.switch_space(space, cx);
            self.edit_browser(|b| b.new_tab(url, std::time::SystemTime::now()));
            cx.notify();
        } else {
            cx.defer(move |cx| crate::blank_window::open_url_for_profile(profile, url, cx));
        }
    }
}
impl BrowserWindow {
    pub(crate) fn render_password_manager(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark=self.theme_is_dark(); let rgb=move|v|manager_color(dark,v);
        let Some(scope) = self
            .password_manager
            .scope
            .clone()
            .filter(|s| self.manager_current(s))
        else {
            return div().into_any_element();
        };
        let query = self.password_manager.filter.read(cx).text().to_lowercase();
        let origins = self
            .password_manager
            .rows
            .iter()
            .filter(|r| {
                r.origin.to_lowercase().contains(query.trim())
                    || r.credential
                        .username()
                        .to_lowercase()
                        .contains(query.trim())
            })
            .map(|r| r.origin.clone())
            .collect::<BTreeSet<_>>();
        let add_scope = scope.clone();
        let clear_scope = scope.clone();
        let help_scope = scope.clone();
        let mut page=div().id("password-manager-page").debug_selector(||"password-manager-page".into()).absolute().inset_0().bg(rgb(0x202124)).text_color(rgb(0xe8eaed)).font_family(".SystemUIFont").text_size(px(13.)).line_height(px(20.)).flex().flex_col().track_focus(&self.password_manager.focus)
   .child(div().h(px(56.)).flex_none().pl(px(22.)).pr(px(4.)).flex().items_center().gap(px(8.)).text_color(rgb(0xbdc1c6))
    .child(icons::control_sized(Icon::Search,16.))
    .child(div().flex_1().min_w_0().text_color(rgb(0xe8eaed)).child(self.password_manager.filter.clone()))
    .when(!query.is_empty(),|row|row.child(div().id("password-filter-clear").debug_selector(||"password-filter-clear".into()).size(px(32.)).flex().items_center().justify_center().cursor_pointer()
      .on_click(cx.listener(move|this,_,window,cx|{if this.manager_current(&clear_scope){this.password_manager.filter.update(cx,|i,cx|i.set_text("",cx));this.password_manager.filter.read(cx).focus_handle(cx).focus(window);}}))
      .child(div().size(px(16.)).rounded_full().bg(rgb(0xbdc1c6)).text_color(rgb(0x202124)).flex().items_center().justify_center().child(icons::control_sized(Icon::Close,10.)))))
    .child(div().id("password-manager-help").size(px(32.)).ml(px(6.)).flex().items_center().justify_center().cursor_pointer().on_click(cx.listener(move|this,_,_,cx|{if this.manager_current(&help_scope){this.manager_open_site(help_scope.profile,HELP.into(),cx);}})).child(help_icon(dark))))
   .child(div().id("password-manager-scroll").flex_1().min_h_0().w_full().overflow_y_scroll().track_scroll(&self.password_manager.scroll)
    .child(div().w_full().max_w(px(680.)).mx_auto().pt(px(8.)).pb(px(24.))
     .when(self.password_manager.detail.is_none(),|body|body
      .child(div().h(px(36.)).flex().items_center().justify_between().child(div().relative().top(px(-1.)).font_family("SF Pro Display").text_size(px(20.)).line_height(px(24.)).font_weight(gpui::FontWeight::MEDIUM).child("Passwords"))
       .child(pill("Add",66.,false,dark).id("password-add").track_focus(&self.password_manager.add).debug_selector(||"password-add".into()).cursor_pointer().on_click(cx.listener(move|this,_,window,cx|{if this.manager_current(&add_scope){this.present_password_editor(None,window,cx);}}))))
      .when(self.password_manager.loaded && origins.is_empty(),|body|body.child(div().mt(px(8.)).child(if query.is_empty(){"Saved passwords will appear here."}else{"No passwords found"})))
      .when(!origins.is_empty(),|body|body.child(div().mt(px(28.)).rounded(px(8.)).overflow_hidden().bg(rgb(0x292a2d)).shadow_sm().children(origins.into_iter().enumerate().map(|(index,origin)|{
       let source=scope.clone();let click_origin=origin.clone();let matching=self.password_manager.rows.iter().find(|r|r.origin==origin && !query.is_empty() && r.credential.username().to_lowercase().contains(query.trim()));let count=self.password_manager.rows.iter().filter(|r|r.origin==origin).count();
       div().id(("password-row",index)).debug_selector(move||format!("password-row-{index}")).h(px(48.)).px(px(20.)).flex().items_center().gap(px(20.)).cursor_pointer().hover(|s|s.bg(rgb(0x3c4043))).when(index>0,|s|s.border_t_1().border_color(rgb(0x3c4043)))
        .on_click(cx.listener(move|this,_,_,cx|{if this.manager_current(&source){this.manager_select(click_origin.clone(),cx);}}))
        .child(div().text_color(rgb(0x9aa0a6)).child(manager_globe(16.,dark)))
        .child(div().flex_1().min_w_0().overflow_hidden().flex().items_center().child(site_name(&origin)).when_some(matching,|label,r|label.child(" • ").child(div().font_weight(gpui::FontWeight::BOLD).child(r.credential.username().to_owned()))).when(count>1 && query.is_empty(),|label|label.child(format!("  {count} accounts"))))
        .child(div().text_color(rgb(0x9aa0a6)).child(row_arrow(dark)))
      })))))
     .when_some(self.password_manager.detail.as_ref(),|body,origin|body
      .child(div().h(px(36.)).mb(px(28.)).flex().items_center().gap(px(12.))
       .child(div().id("password-details-back").debug_selector(||"password-details-back".into()).size(px(32.)).track_focus(&self.password_manager.back).cursor_pointer().child(icons::control(Icon::Back)).on_click(cx.listener(|this,_,_,cx|this.manager_route(None,cx))))
       .child(manager_globe(20.,dark)).child(div().relative().top(px(-1.)).font_family("SF Pro Display").text_size(px(20.)).line_height(px(24.)).font_weight(gpui::FontWeight::MEDIUM).child(site_name(origin))))
      .children(self.password_manager.rows.iter().filter(|r|r.origin==*origin).enumerate().map(|(index,r)|self.render_password_record(index,r,&scope,cx))))));
        if let Some(e) = &self.password_manager.editor {
            page = page.child(self.render_password_editor(e, window, cx));
        }
        if let Some(n) = &self.password_manager.notice {
            let notice_scope = scope.clone();
            let notice_id = n.generation;
            page = page.child(
                div()
                    .absolute()
                    .bottom(px(12.))
                    .left(px(12.))
                    .min_w(px(288.))
                    .rounded(px(4.))
                    .bg(rgb(0x323232))
                    .text_color(rgb(0xf1f3f4))
                    .px(px(24.))
                    .py(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(24.))
                    .shadow_lg()
                    .child(n.text)
                    .when(n.undo.is_some(), |toast| {
                        toast.child(
                            div()
                                .id("password-undo")
                                .debug_selector(|| "password-undo".into())
                                .text_color(rgb(0x8ab4f8))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.manager_current(&notice_scope)
                                        && this
                                            .password_manager
                                            .notice
                                            .as_ref()
                                            .is_some_and(|n| n.generation == notice_id)
                                    {
                                        this.manager_undo(cx);
                                    }
                                }))
                                .child("Undo"),
                        )
                    }),
            );
        }
        page.into_any_element()
    }
    fn render_password_record(
        &self,
        index: usize,
        r: &Record,
        scope: &Scope,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark=self.theme_is_dark(); let rgb=move|v|manager_color(dark,v);
        let copy_user = r.clone();
        let copy_password = r.clone();
        let edit = r.clone();
        let deleted = r.clone();
        let site = r.origin.clone();
        let revealed = self.password_manager.revealed.contains(&record_key(r));
        let reveal_record = r.clone();
        let a = scope.clone();
        let b = scope.clone();
        let c = scope.clone();
        let d = scope.clone();
        let e = scope.clone();
        let f = scope.clone();
        let copy_user = icon_button("password-copy-user", index, copy_icon(dark),dark).on_click(
            cx.listener(move |this, _, _, cx| {
                if this.manager_current(&a) {
                    this.manager_copy(copy_user.clone(), false, cx);
                }
            }),
        );
        let copy_password = icon_button("password-copy-password", index, copy_icon(dark),dark).on_click(
            cx.listener(move |this, _, _, cx| {
                if this.manager_current(&b) {
                    this.manager_copy(copy_password.clone(), true, cx);
                }
            }),
        );
        let reveal = icon_button("password-reveal", index, eye_icon(revealed,dark),dark).on_click(
            cx.listener(move |this, _, _, cx| {
                if this.manager_current(&c) {
                    this.manager_reveal(reveal_record.clone(), cx);
                }
            }),
        );
        div()
            .mb(px(20.))
            .rounded(px(8.))
            .bg(rgb(0x292a2d))
            .overflow_hidden()
            .child(
                div()
                    .pt(px(12.))
                    .pb(px(16.))
                    .px(px(20.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .mt(px(16.))
                            .flex()
                            .child(
                                div().w_1_2().pr(px(16.)).child(label("Username",dark)).child(
                                    div()
                                        .h(px(36.))
                                        .flex()
                                        .items_center()
                                        .child(
                                            div().flex_1().min_w_0().truncate().child(
                                                if r.credential.username().is_empty() {
                                                    "No username"
                                                } else {
                                                    r.credential.username()
                                                }
                                                .to_owned(),
                                            ),
                                        )
                                        .child(copy_user),
                                ),
                            )
                            .child(
                                div().w_1_2().child(label("Sites",dark)).child(
                                    div()
                                        .id(("password-site", index))
                                        .text_color(rgb(0x8ab4f8))
                                        .cursor_pointer()
                                        .truncate()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if this.manager_current(&d) {
                                                this.manager_open_site(d.profile, site.clone(), cx);
                                            }
                                        }))
                                        .child(site_name(&r.origin)),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(16.))
                            .flex()
                            .child(
                                div().w_1_2().pr(px(16.)).child(label("Password",dark)).child(
                                    div()
                                        .h(px(36.))
                                        .flex()
                                        .items_center()
                                        .child(div().flex_1().min_w_0().truncate().child(
                                            if revealed {
                                                r.credential.password().to_owned()
                                            } else {
                                                "•".repeat(
                                                    r.credential.password().encode_utf16().count(),
                                                )
                                            },
                                        ))
                                        .child(reveal)
                                        .child(copy_password),
                                ),
                            )
                            .child(
                                div().w_1_2().child(label("Note",dark)).child(
                                    div().whitespace_normal().child(
                                        if r.note.is_empty() {
                                            "No note added"
                                        } else {
                                            r.note.as_str()
                                        }
                                        .to_owned(),
                                    ),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(12.))
                    .border_t_1()
                    .border_color(rgb(0x3c4043))
                    .px(px(16.))
                    .py(px(20.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        pill("Edit",66.,false,dark)
                            .id(("password-edit", index))
                            .when_some(
                                self.password_manager.edit_focus.get(&record_key(r)),
                                |button, focus| button.track_focus(focus),
                            )
                            .debug_selector(move || format!("password-edit-{index}"))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.manager_current(&e) {
                                    this.present_password_editor(Some(edit.clone()), window, cx);
                                }
                            })),
                    )
                    .child(
                        pill("Delete",76.,false,dark)
                            .id(("password-delete", index))
                            .debug_selector(move || format!("password-delete-{index}"))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.manager_current(&f) {
                                    this.manager_delete(deleted.clone(), cx);
                                }
                            })),
                    ),
            )
            .into_any_element()
    }
    fn render_password_editor(
        &self,
        e: &Editor,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark=self.theme_is_dark(); let rgb=move|v|manager_color(dark,v);
        let valid = self.password_editor_value(cx).is_some();
        let id = e.id;
        let old = e.old.is_some();
        let rows = if old {
            vec![("Username", &e.user), ("Password", &e.password)]
        } else {
            vec![
                ("Site", &e.site),
                ("Username", &e.user),
                ("Password", &e.password),
            ]
        };
        div().absolute().inset_0().bg(gpui::black().opacity(0.6)).flex().items_center().justify_center().key_context("PasswordManagerEditor")
   .on_action(cx.listener(move |this,_:&Accept,window,cx|{if this.password_editor_is(id){this.accept_password_editor(window,cx)}}))
   .on_action(cx.listener(move |this,_:&Cancel,window,cx|{if this.password_editor_is(id){this.cancel_password_editor(window,cx)}}))
   .on_action(cx.listener(move |this,_:&Next,window,cx|{if this.password_editor_is(id){this.password_editor_focus(false,window,cx)}}))
   .on_action(cx.listener(move |this,_:&Previous,window,cx|{if this.password_editor_is(id){this.password_editor_focus(true,window,cx)}}))
   .on_action(cx.listener(move |this,_:&Activate,window,cx|{if this.password_editor_is(id){this.accept_password_editor(window,cx)}}))
   .on_mouse_down(gpui::MouseButton::Left,|_,_,cx|cx.stop_propagation())
   .child(div().id("password-editor").debug_selector(||"password-editor".into()).w(px(512.)).max_w_full().rounded(px(8.)).bg(rgb(0x292a2d)).shadow_lg().relative().px(px(20.)).pt(px(20.)).pb(px(15.)).flex().flex_col().text_color(rgb(0xe8eaed))
    .child(div().relative().top(px(-2.)).text_size(px(15.)).line_height(px(20.)).font_weight(gpui::FontWeight::SEMIBOLD).child(if old{"Edit password"}else{"Add new password"}))
    .child(div().mt(px(16.)).flex().flex_col()
     .when_some(e.old.as_ref(),|body,r|body.child(label("Sites",dark)).child(div().text_color(rgb(0x8ab4f8)).mb(px(20.)).child(site_name(&r.origin))))
     .children(rows.into_iter().map(|(name,input)|div().when(name!="Password",|row|row.mb(px(18.))).child(label(name,dark)).child(div().mt(px(6.)).h(px(36.)).rounded_t(px(8.)).bg(rgb(0x444746)).border_b_1().border_color(rgb(0x9aa0a6)).when(input.read(cx).focus_handle(cx).is_focused(window),|s|s.border_color(rgb(0xa8c7fa)).border_b_2()).px(px(10.)).text_size(px(12.)).line_height(px(16.)).flex().items_center().child(div().flex_1().min_w_0().child(input.clone())).when(name=="Password",|field|field.child(div().id("password-editor-eye").debug_selector(||"password-editor-eye".into()).key_context("PasswordManagerButton").track_focus(&e.eye).size(px(32.)).flex().items_center().justify_center().text_color(rgb(0xbdc1c6)).cursor_pointer().on_click(cx.listener(move |this,_,_,cx|{if this.password_editor_is(id){this.toggle_password_editor(cx)}})).child(eye_icon(e.reveal,dark)))))))
     .child(div().ml(px(2.)).mt(px(16.)).child(if old{format!("Make sure the password you are saving matches your password for {}",site_name(&e.old.as_ref().unwrap().origin))}else{"Make sure you're saving your current password for this site".into()}))
     .child(div().mt(px(16.)).when(!old,|line|line.border_t_1().border_color(rgb(0x3c4043)).pt(px(16.))).child(label("Note",dark)).child(div().mt(px(8.)).h(px(68.)).rounded_t(px(8.)).bg(rgb(0x444746)).border_b_1().border_color(rgb(0x9aa0a6)).px(px(10.)).py(px(8.)).child(e.note.clone()))))
    .child(div().mt(px(43.)).mr(px(-4.)).h(px(36.)).flex().items_center().justify_end().gap(px(8.))
     .child(pill("Cancel",76.,false,dark).id("password-editor-cancel").debug_selector(||"password-editor-cancel".into()).key_context("PasswordManagerButton").track_focus(&e.cancel).cursor_pointer().on_click(cx.listener(move |this,_,window,cx|{if this.password_editor_is(id){this.cancel_password_editor(window,cx)}})))
     .child(pill("Save",66.,true,dark).id("password-editor-save").debug_selector(||"password-editor-save".into()).key_context("PasswordManagerButton").track_focus(&e.save).when(!valid,|b|b.bg(rgb(0x404043)).text_color(rgb(0x80868b))).when(valid,|b|b.cursor_pointer()).on_click(cx.listener(move |this,_,window,cx|{if this.password_editor_is(id){this.accept_password_editor(window,cx)}})))))
   .into_any_element()
    }
}
// Arc's cr-form-field-label has 0.4 px tracking. These labels are fixed ASCII;
// individual text runs preserve that spacing without changing editable fields.
fn label(text: &'static str,dark:bool) -> impl IntoElement {
    let rgb=move|v|manager_color(dark,v);
    div()
        .h(px(16.))
        .flex()
        .items_center()
        .gap(px(0.4))
        .text_size(px(11.))
        .line_height(px(16.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0xbdc1c6))
        .children(
            text.chars()
                .map(|ch| div().relative().top(px(-2.)).child(ch.to_string())),
        )
}
fn pill(text: &'static str, width: f32, primary: bool,dark:bool) -> gpui::Stateful<gpui::Div> {
    let rgb=move|v|manager_color(dark,v);
    div()
        .id(text)
        .h(px(36.))
        .w(px(width))
        .rounded(px(18.))
        .border_1()
        .border_color(rgb(if primary { 0xa8c7fa } else { 0x0086bd }))
        .when(primary, |b| b.border_0().bg(rgb(0xa8c7fa)))
        .text_color(rgb(if primary { 0x202124 } else { 0xa8c7fa }))
        .text_size(px(13.))
        .line_height(px(20.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .justify_center()
        .focus(|s| s.border_color(rgb(0xe8eaed)))
        .child(text)
}
fn icon_button(
    name: &'static str,
    index: usize,
    icon: impl IntoElement, dark:bool,
) -> gpui::Stateful<gpui::Div> {
    let rgb=move|v|manager_color(dark,v);
    div()
        .id((name, index))
        .debug_selector(move || format!("{name}-{index}"))
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(rgb(0xbdc1c6))
        .child(icon)
}
fn help_icon(dark:bool) -> impl IntoElement {
    glyph(0,dark)
}
fn eye_icon(shown: bool,dark:bool) -> impl IntoElement {
    glyph(if shown { 3 } else { 1 },dark)
}
fn copy_icon(dark:bool) -> impl IntoElement {
    glyph(2,dark)
}
pub(crate) fn favicon() -> AnyElement {
    glyph(4,true).into_any_element()
}
fn glyph(kind: u8,dark:bool) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let at = |x: f32, y: f32| bounds.origin + gpui::point(px(x), px(y));
            let mut p = gpui::PathBuilder::stroke(px(1.4));
            if kind == 4 {
                p.move_to(at(7., 8.));
                p.cubic_bezier_to(at(2., 8.), at(7., 11.4), at(2., 11.4));
                p.cubic_bezier_to(at(7., 8.), at(2., 4.6), at(7., 4.6));
                p.close();
                p.move_to(at(7., 8.));
                p.line_to(at(15., 8.));
                p.move_to(at(11., 8.));
                p.line_to(at(11., 11.));
                p.move_to(at(14., 8.));
                p.line_to(at(14., 10.));
            } else if kind == 2 {
                for (x, y, w, h) in [(5., 2., 9., 10.), (2., 5., 9., 10.)] {
                    p.move_to(at(x, y));
                    p.line_to(at(x + w, y));
                    p.line_to(at(x + w, y + h));
                    p.line_to(at(x, y + h));
                    p.close();
                }
            } else if kind == 1 || kind == 3 {
                p.move_to(at(1., 8.));
                p.cubic_bezier_to(at(15., 8.), at(5., 1.), at(11., 1.));
                p.cubic_bezier_to(at(1., 8.), at(11., 15.), at(5., 15.));
                p.close();
                p.move_to(at(8., 5.5));
                p.cubic_bezier_to(at(8., 10.5), at(11.3, 5.5), at(11.3, 10.5));
                p.cubic_bezier_to(at(8., 5.5), at(4.7, 10.5), at(4.7, 5.5));
            } else {
                p.move_to(at(15., 8.));
                p.cubic_bezier_to(at(8., 15.), at(15., 11.87), at(11.87, 15.));
                p.cubic_bezier_to(at(1., 8.), at(4.13, 15.), at(1., 11.87));
                p.cubic_bezier_to(at(8., 1.), at(1., 4.13), at(4.13, 1.));
                p.cubic_bezier_to(at(15., 8.), at(11.87, 1.), at(15., 4.13));
                p.close();
                p.move_to(at(6., 5.5));
                p.cubic_bezier_to(at(8., 8.5), at(9., 3.), at(12., 7.));
                p.line_to(at(8., 10.));
                p.move_to(at(8., 11.7));
                p.line_to(at(8., 12.3));
            }
            if kind == 3 {
                p.move_to(at(2., 2.));
                p.line_to(at(14., 14.));
            }
            if let Ok(path) = p.build() {
                window.paint_path(
                    path,
                    manager_color(dark,if kind == 4 {
                        0x1a73e8
                    } else if kind == 1 || kind == 3 {
                        0xbdc1c6
                    } else {
                        0x9aa0a6
                    }),
                );
            }
        },
    )
    .size(px(16.))
}
pub(crate) fn open_from_settings(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    cx: &mut App,
) {
    cx.defer(move |cx| open_settings_now(profile, owner, selected, true, cx));
}
fn open_settings_now(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    create: bool,
    cx: &mut App,
) {
    if crate::quit::is_quitting() {
        return;
    }
    let preferred = owner
        .and_then(|o| o.upgrade())
        .map(|o| o.read(cx).main_window);
    let target = cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<BrowserWindow>())
        .filter(|w| {
            w.read(cx).is_ok_and(|v| {
                !v.browser.is_private()
                    && !v.browser.is_blank()
                    && v.transient.is_none()
                    && v.peek_owner.is_none()
                    && v.profile.handle.same_workspace(&profile)
            })
        })
        .min_by_key(|w| {
            if preferred.is_some_and(|p| p.window_id() == w.window_id()) {
                0
            } else {
                1
            }
        });
    if let Some(target) = target {
        let _ = target.update(cx, |this, window, cx| {
            this.open_password_manager(selected, window, cx)
        });
    } else if create {
        crate::new_window::open(false, cx);
        cx.defer(move |cx| open_settings_now(profile, None, selected, false, cx));
    }
}
impl BrowserWindow {
    pub(crate) fn passwords_from_command(&mut self, cx: &mut Context<Self>) {
        self.close_command_bar_with_focus(PopupFocus::OnActivation, cx);
        let selected = self
            .browser
            .blank_source_user_profile_id()
            .unwrap_or(self.browser.active_user_profile_id());
        let profile = if self.browser.is_private() || self.browser.is_blank() {
            let Ok(source) = crate::new_window::normal_source(cx) else {
                return;
            };
            source.profile
        } else {
            self.profile.handle.clone()
        };
        let owner = (!self.browser.is_private() && !self.browser.is_blank())
            .then(|| cx.entity().downgrade());
        open_from_settings(profile, owner, selected, cx);
    }
    pub(crate) fn open_password_manager(
        &mut self,
        profile: UserProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .profile
            .handle
            .read_settings(|b| b.user_profile(profile).is_some())
        {
            return;
        }
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        let location = format!("{URL}?profile={}", serde_json::to_string(&profile).unwrap());
        let existing = self
            .browser
            .space()
            .today
            .iter()
            .chain(&self.browser.space().pinned)
            .find(|t| t.url == location)
            .map(|t| t.id);
        let tab = self.edit_browser(|b| match existing {
            Some(id) => {
                b.activate_tab(id, std::time::SystemTime::now());
                id
            }
            None => b.new_tab(location, std::time::SystemTime::now()),
        });
        self.edit_browser(|b| b.set_title(tab, "Password Manager"));
        self.sidebar_navigation.controller.select(
            &self.browser,
            crate::sidebar_pages::SidebarPage::Space(self.browser.space().id),
        );
        self.sync_password_manager(cx);
        self.password_manager
            .filter
            .read(cx)
            .focus_handle(cx)
            .focus(window);
        window.activate_window();
        cx.notify();
    }
}
#[cfg(test)]
impl BrowserWindow {
    pub(crate) fn password_manager_snapshot(&self) -> (usize, bool, bool) {
        (
            self.password_manager.rows.len(),
            self.password_manager.editor.is_some(),
            self.password_manager.detail.is_some(),
        )
    }
    pub(crate) fn password_manager_notes(&self, expected: &str) -> bool {
        self.password_manager
            .rows
            .iter()
            .all(|r| r.note.as_str() == expected)
    }
    pub(crate) fn password_manager_filter(&self) -> Entity<TextInput> {
        self.password_manager.filter.clone()
    }
}

fn row_arrow(dark:bool) -> AnyElement {
    div()
        .size(px(32.))
        .mr(px(-10.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let at = |x, y| bounds.origin + gpui::point(px(x), px(y));
                    let mut p = gpui::PathBuilder::fill();
                    p.move_to(at(10.1, 7.8));
                    p.cubic_bezier_to(at(9.6, 8.3), at(9.8, 7.8), at(9.6, 8.0));
                    p.line_to(at(9.6, 15.7));
                    p.cubic_bezier_to(at(10.4, 16.1), at(9.6, 16.2), at(10.0, 16.3));
                    p.line_to(at(14., 12.5));
                    p.cubic_bezier_to(at(14., 11.5), at(14.3, 12.2), at(14.3, 11.8));
                    p.line_to(at(10.4, 8.0));
                    p.close();
                    if let Ok(path) = p.build() {
                        window.paint_path(path, manager_color(dark,0x9aa0a6));
                    }
                },
            )
            .size(px(24.)),
        )
        .into_any_element()
}
fn manager_globe(size: f32,dark:bool) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let scale = size / 16.;
            let at = |x: f32, y: f32| bounds.origin + gpui::point(px(x * scale), px(y * scale));
            let mut p = gpui::PathBuilder::fill();
            let r = 6.667;
            let k = r * 0.5522848;
            p.move_to(at(8. + r, 8.));
            p.cubic_bezier_to(at(8., 8. + r), at(8. + r, 8. + k), at(8. + k, 8. + r));
            p.cubic_bezier_to(at(8. - r, 8.), at(8. - k, 8. + r), at(8. - r, 8. + k));
            p.cubic_bezier_to(at(8., 8. - r), at(8. - r, 8. - k), at(8. - k, 8. - r));
            p.cubic_bezier_to(at(8. + r, 8.), at(8. + k, 8. - r), at(8. + r, 8. - k));
            p.close();
            for points in [
                vec![
                    (7.3, 13.3),
                    (4.7, 13.),
                    (2.7, 10.7),
                    (2.7, 8.),
                    (2.8, 6.8),
                    (6., 10.),
                    (6., 10.7),
                    (7.3, 12.),
                ],
                vec![
                    (11.9, 11.6),
                    (11.6, 10.9),
                    (10.7, 10.7),
                    (10., 10.7),
                    (10., 8.7),
                    (9.3, 8.),
                    (5.3, 8.),
                    (5.3, 6.7),
                    (6.7, 6.7),
                    (7.3, 6.),
                    (7.3, 4.7),
                    (8.7, 4.7),
                    (10., 3.3),
                    (10., 3.1),
                    (12.1, 4.4),
                    (13.3, 6.4),
                    (13.3, 8.),
                    (12.5, 10.7),
                ],
            ] {
                let mut points = points;
                let area = points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
                    .sum::<f32>();
                if area > 0. {
                    points.reverse();
                }
                for (i, (x, y)) in points.into_iter().enumerate() {
                    if i == 0 {
                        p.move_to(at(x, y));
                    } else {
                        p.line_to(at(x, y));
                    }
                }
                p.close();
            }
            if let Ok(path) = p.build() {
                window.paint_path(path, manager_color(dark,0x9aa0a6));
            }
        },
    )
    .size(px(size))
    .into_any_element()
}

// Light values sampled from our filtered Arc Add reference; dark values retain
// the previously verified surface. Website appearance controls this WebUI peer.
fn manager_color(dark:bool,value:u32)->gpui::Rgba {
    rgb(if dark {value} else {match value {
        0x202124|0x292a2d=>0xffffff,0xe8eaed=>0x1f1f1f,
        0xbdc1c6=>0x444746,0x9aa0a6=>0x757775,0x444746=>0xe1e3e1,
        0xa8c7fa|0x8ab4f8=>0x2656c9,0x0086bd=>0xaec6f6,
        0x3c4043=>0xf0f0f0,0x404043=>0xe4e4e4,0x80868b=>0x999999,
        _=>value,
    }})
}
