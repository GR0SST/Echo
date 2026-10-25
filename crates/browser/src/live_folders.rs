//! GitHub Live Folders use local keychain credentials and direct REST polling.
use crate::{
    text_input::TextInput,
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use browser_core::{
    folders::FolderId,
    live_folders::{LiveItem, PullRequestFilter},
    SpaceId, UserProfileId,
};
use echo_ai::credentials::{CredentialStore, Secret};
use gpui::{
    actions, div, hsla, prelude::*, px, AnyElement, App, Context, Entity, Focusable, KeyBinding,
    Subscription, Task, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

actions!(live_folders, [Accept, Cancel]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Accept, Some("GitHubConnection")),
        KeyBinding::new("escape", Cancel, Some("GitHubConnection")),
    ]);
}
#[derive(Clone)]
pub(crate) struct Services {
    pub store: Arc<dyn CredentialStore>,
    pub transport: Arc<dyn echo_github::Transport>,
}
impl gpui::Global for Services {}
impl Default for Services {
    fn default() -> Self {
        Self {
            store: Arc::new(echo_ai::credentials::NativeCredentialStore),
            transport: Arc::new(echo_github::RestTransport::default()),
        }
    }
}
#[derive(Default)]
pub(crate) struct State {
    pub sheet: Option<Sheet>,
    client: Option<Arc<Mutex<echo_github::Client>>>,
    pub poll: Option<Task<()>>,
    pub cancel: Option<Arc<AtomicBool>>,
    generation: u64,
    only_next: Option<FolderId>,
    pub errors: HashMap<FolderId, String>,
    fetched: HashMap<FolderId, SystemTime>,
}
pub(crate) struct Sheet {
    folder: FolderId,
    space: SpaceId,
    profile: UserProfileId,
    pub key: Entity<TextInput>,
    pub overlay: Option<WindowHandle<Overlay>>,
    busy: bool,
    error: Option<String>,
    generation: u64,
    deleting: bool,
    focus: gpui::FocusHandle,
    _changed: Subscription,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl BrowserWindow {
    fn github_client(&mut self, cx: &App) -> Arc<Mutex<echo_github::Client>> {
        self.live_folders
            .client
            .get_or_insert_with(|| {
                let services = cx.try_global::<Services>().cloned().unwrap_or_default();
                Arc::new(Mutex::new(echo_github::Client::new(
                    services.transport,
                    services.store,
                )))
            })
            .clone()
    }
    pub(crate) fn create_live_folder(&mut self, space: SpaceId, cx: &mut Context<Self>) {
        if let Some(id) = self.edit_browser(|b| b.create_github_live_folder(space)) {
            if self
                .browser
                .folder(id)
                .and_then(|f| f.live.as_ref())
                .is_some_and(|l| !l.credential_id.is_empty())
            {
                self.refresh_live_folders(cx);
            } else {
                self.open_live_connection(id, cx);
            }
        }
    }
    pub(crate) fn open_live_connection(&mut self, id: FolderId, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if self.browser.is_private() || self.browser.is_blank() || crate::quit::is_quitting() {
            return;
        }
        let Some(space) = self.browser.folder_space(id) else {
            return;
        };
        let Some(profile) = self
            .browser
            .spaces
            .iter()
            .find(|s| s.id == space)
            .map(|s| s.user_profile_id)
        else {
            return;
        };
        self.close_live_connection(cx);
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        self.close_find(cx);
        self.close_user_profile_dialog(cx);
        self.close_setup(cx);
        self.close_theme_editor(cx);
        self.close_site_controls(cx);
        self.close_new_menu(cx);
        if let Some(pages) = &self.pages {
            pages.borrow_mut().request_ui_focus();
        }
        self.live_folders.generation = self.live_folders.generation.wrapping_add(1);
        let generation = self.live_folders.generation;
        let key = cx.new(|cx| TextInput::secret("Fine-grained personal access token", cx));
        let changed = cx.subscribe(&key, |_, _, _: &crate::text_input::Changed, cx| cx.notify());
        self.live_folders.sheet = Some(Sheet {
            folder: id,
            space,
            profile,
            key: key.clone(),
            overlay: None,
            busy: false,
            error: None,
            generation,
            deleting: false,
            focus: cx.focus_handle(),
            _changed: changed,
        });
        let owner = cx.entity().downgrade();
        let main = self.main_window;
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !owner
                .read_with(cx, |b, _| {
                    b.live_folders
                        .sheet
                        .as_ref()
                        .is_some_and(|s| s.generation == generation)
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok(bounds) = main.update(cx, |_, w, _| w.bounds()) else {
                return;
            };
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
                cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::GitHubConnection, window, cx))
            }) else {
                let _ = owner.update(cx, |b, cx| b.close_live_connection(cx));
                return;
            };
            let attached = owner
                .update(cx, |b, _| {
                    if let Some(s) = b
                        .live_folders
                        .sheet
                        .as_mut()
                        .filter(|s| s.generation == generation)
                    {
                        s.overlay = Some(overlay);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if !attached {
                let _ = overlay.update(cx, |_, w, _| w.remove_window());
                return;
            }
            let _ = overlay.update(cx, |_, w, cx| {
                w.activate_window();
                let focus = owner
                    .read_with(cx, |b, _| {
                        b.live_folders
                            .sheet
                            .as_ref()
                            .filter(|s| s.generation == generation && s.deleting)
                            .map(|s| s.focus.clone())
                    })
                    .ok()
                    .flatten();
                if let Some(focus) = focus {
                    focus.focus(w);
                } else {
                    key.focus_handle(cx).focus(w);
                }
            });
        });
        cx.notify();
    }
    pub(crate) fn open_live_delete(&mut self, id: FolderId, cx: &mut Context<Self>) {
        self.open_live_connection(id, cx);
        if let Some(sheet) = &mut self.live_folders.sheet {
            sheet.deleting = true;
        }
        cx.notify();
    }
    fn retire_github_credential(&mut self, id: String, cx: &mut Context<Self>) {
        if self
            .browser
            .user_profiles()
            .any(|p| p.github.as_ref().is_some_and(|a| a.credential_id == id))
        {
            return;
        }
        if self
            .browser
            .spaces
            .iter()
            .flat_map(|s| &s.folders)
            .any(|f| f.live.as_ref().is_some_and(|l| l.credential_id == id))
        {
            return;
        }
        let client = self.github_client(cx);
        cx.background_executor()
            .spawn(async move {
                let _ = client.lock().unwrap().disconnect(&id);
            })
            .detach();
    }
    pub(crate) fn close_live_connection(&mut self, cx: &mut Context<Self>) {
        self.close_live_connection_with_focus(PopupFocus::Restore, cx);
    }
    pub(crate) fn close_live_connection_with_focus(
        &mut self,
        focus: PopupFocus,
        cx: &mut Context<Self>,
    ) {
        if let Some(sheet) = self.live_folders.sheet.take() {
            sheet.key.update(cx, |key, cx| key.set_text("", cx));
            self.close_popup_overlay(sheet.overlay, focus, true, cx);
            cx.notify();
        }
    }
    pub(crate) fn validate_live_connection(&mut self, cx: &mut Context<Self>) {
        if self.live_folders.sheet.as_ref().is_some_and(|sheet| {
            self.browser.folder_space(sheet.folder) != Some(sheet.space)
                || self
                    .browser
                    .spaces
                    .iter()
                    .find(|s| s.id == sheet.space)
                    .is_none_or(|s| s.user_profile_id != sheet.profile)
        }) {
            self.close_live_connection(cx);
        }
    }
    pub(crate) fn submit_live_connection(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        self.validate_live_connection(cx);
        let Some(sheet) = self.live_folders.sheet.as_ref().filter(|s| !s.busy) else {
            return;
        };
        if sheet.deleting {
            let id = sheet.folder;
            let credential = self
                .browser
                .folder(id)
                .and_then(|f| f.live.as_ref())
                .map(|l| l.credential_id.clone());
            self.edit_browser(|b| b.delete_empty_folder(id));
            self.close_live_connection(cx);
            if let Some(id) = credential.filter(|id| !id.is_empty()) {
                self.retire_github_credential(id, cx);
            }
            return;
        }
        let token = Secret::new(sheet.key.read(cx).text().trim().to_owned());
        if token.expose().is_empty() {
            return;
        }
        let generation = sheet.generation;
        let folder = sheet.folder;
        let profile = sheet.profile;
        let space = sheet.space;
        let Ok(unique) = echo_ai::oauth::new_credential_id() else {
            return;
        };
        let id = format!("github-live:{unique}");
        let client = self.github_client(cx);
        let worker_client = client.clone();
        let worker_id = id.clone();
        let sheet = self.live_folders.sheet.as_mut().unwrap();
        sheet.busy = true;
        sheet.key.update(cx, |key, cx| key.set_text("", cx));
        let work = cx.background_executor().spawn(async move {
            worker_client
                .lock()
                .unwrap()
                .connect(&worker_id, &token, now())
        });
        cx.spawn(async move |owner, cx| {
            let result = work.await;
            let accepted = owner
                .update(cx, |b, cx| {
                    b.refresh_profile_projection();
                    b.validate_live_connection(cx);
                    if !b.live_folders.sheet.as_ref().is_some_and(|s| {
                        s.generation == generation
                            && s.folder == folder
                            && s.profile == profile
                            && s.space == space
                    }) || crate::quit::is_quitting()
                    {
                        return false;
                    }
                    match &result {
                        Ok(login) => {
                            let prior = b
                                .browser
                                .folder(folder)
                                .and_then(|f| f.live.as_ref())
                                .map(|l| l.credential_id.clone());
                            b.edit_browser(|model| {
                                model.connect_github_profile(
                                    profile,
                                    browser_core::live_folders::GitHubAccount {
                                        credential_id: id.clone(),
                                        login: login.clone(),
                                    },
                                );
                            });
                            b.close_live_connection(cx);
                            if let Some(prior) =
                                prior.filter(|prior| !prior.is_empty() && prior != &id)
                            {
                                b.retire_github_credential(prior, cx);
                            }
                            b.refresh_live_folders(cx);
                        }
                        Err(error) => {
                            if let Some(s) = &mut b.live_folders.sheet {
                                s.busy = false;
                                s.error = Some(error.to_string());
                            }
                            cx.notify();
                        }
                    }
                    true
                })
                .unwrap_or(false);
            if !accepted && result.is_ok() {
                cx.background_executor()
                    .spawn(async move {
                        let _ = client.lock().unwrap().disconnect(&id);
                    })
                    .await;
            }
        })
        .detach();
        cx.notify();
    }
    pub(crate) fn refresh_live_folders(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.live_folders.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        self.live_folders.poll.take();
        self.sync_live_folders(cx);
        cx.notify();
    }
    pub(crate) fn refresh_live_folder(&mut self, id: FolderId, cx: &mut Context<Self>) {
        self.live_folders.only_next = Some(id);
        self.refresh_live_folders(cx);
    }
    pub(crate) fn sync_live_folders(&mut self, cx: &mut Context<Self>) {
        self.validate_live_connection(cx);
        if crate::quit::is_quitting()
            || self.browser.is_blank()
            || self.browser.is_private()
            || self.live_folders.poll.is_some()
        {
            return;
        }
        let only = self.live_folders.only_next.take();
        let jobs: Vec<_> = self
            .browser
            .spaces
            .iter()
            .flat_map(|s| {
                s.folders
                    .iter()
                    .filter(move |f| only.is_none_or(|id| f.id == id))
                    .filter_map(move |f| {
                        f.live
                            .as_ref()
                            .filter(|l| !l.credential_id.is_empty())
                            .map(|l| (f.id, s.user_profile_id, l.clone()))
                    })
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        let client = self.github_client(cx);
        let cancel = Arc::new(AtomicBool::new(false));
        self.live_folders.cancel = Some(cancel.clone());
        let work = cx.background_executor().spawn(async move {
            let mut client = client.lock().unwrap();
            jobs.into_iter()
                .map(|(folder, profile, live)| {
                    let mut items = Vec::new();
                    let mut failure = None;
                    for filter in &live.filters {
                        match client.poll_with_cancel(
                            &live.credential_id,
                            &live.login,
                            *filter,
                            now(),
                            &|| cancel.load(Ordering::Acquire),
                        ) {
                            Ok(rows) => items.extend(rows),
                            Err(e) => {
                                failure = Some(e);
                                break;
                            }
                        }
                    }
                    (folder, profile, live, failure.map_or(Ok(items), Err))
                })
                .collect::<Vec<_>>()
        });
        self.live_folders.poll = Some(cx.spawn(async move |owner, cx| {
            let results = work.await;
            let _ = owner.update(cx, |b, cx| {
                if crate::quit::is_quitting() {
                    return;
                }
                b.refresh_profile_projection();
                for (folder, profile, requested, result) in results {
                    let current = b
                        .browser
                        .folder_space(folder)
                        .and_then(|space| b.browser.spaces.iter().find(|s| s.id == space))
                        .is_some_and(|s| s.user_profile_id == profile)
                        && b.browser
                            .folder(folder)
                            .and_then(|f| f.live.as_ref())
                            .is_some_and(|l| {
                                l.credential_id == requested.credential_id
                                    && l.filters == requested.filters
                            });
                    if !current {
                        continue;
                    }
                    match result {
                        Ok(items) => {
                            b.edit_browser(|model| {
                                if let Some(live) = model.live_folder_mut(folder) {
                                    live.replace(items);
                                }
                            });
                            b.live_folders.errors.remove(&folder);
                            b.live_folders.fetched.insert(folder, SystemTime::now());
                        }
                        Err(error) => {
                            b.live_folders.errors.insert(folder, error.to_string());
                        }
                    }
                }
                cx.notify();
            });
            cx.background_executor()
                .timer(Duration::from_secs(60))
                .await;
            let _ = owner.update(cx, |b, cx| {
                b.live_folders.poll = None;
                cx.notify();
            });
        }));
    }
    pub(crate) fn toggle_live_filter(
        &mut self,
        id: FolderId,
        filter: PullRequestFilter,
        cx: &mut Context<Self>,
    ) {
        self.edit_browser(|b| {
            if let Some(l) = b.live_folder_mut(id) {
                if l.filters.contains(&filter) {
                    l.filters.retain(|f| *f != filter);
                } else {
                    l.filters.push(filter);
                }
            }
        });
        self.refresh_live_folder(id, cx);
    }
    pub(crate) fn toggle_live_repository(
        &mut self,
        id: FolderId,
        repo: String,
        cx: &mut Context<Self>,
    ) {
        if !self
            .browser
            .folder(id)
            .and_then(|f| f.live.as_ref())
            .is_some_and(|l| l.items.iter().any(|i| i.repository == repo))
        {
            return;
        }
        self.edit_browser(|b| {
            if let Some(l) = b.live_folder_mut(id) {
                if l.hidden_repositories.contains(&repo) {
                    l.hidden_repositories.retain(|r| r != &repo);
                } else {
                    l.hidden_repositories.push(repo);
                }
            }
        });
        cx.notify();
    }
    pub(crate) fn live_folder_menu(&self, id: FolderId) -> Vec<crate::tab_menu::Entry> {
        use crate::tab_menu::{item, pending, Command, Entry};
        let Some(folder) = self.browser.folder(id) else {
            return vec![];
        };
        let Some(live) = &folder.live else {
            return vec![];
        };
        let ago = self
            .live_folders
            .fetched
            .get(&id)
            .and_then(|t| t.elapsed().ok())
            .map(|d| {
                if d.as_secs() < 60 {
                    "Just now".into()
                } else {
                    format!("{}m ago", d.as_secs() / 60)
                }
            })
            .unwrap_or_else(|| "Never".into());
        let status = if self.live_folders.errors.contains_key(&id) {
            "Failed to update".into()
        } else {
            format!("Last fetch: {ago}")
        };
        let mut filters = vec![
            pending(status).enabled(false).into(),
            item("Refresh", Command::LiveRefresh).into(),
            Entry::Separator,
        ];
        for filter in [
            PullRequestFilter::CreatedByMe,
            PullRequestFilter::ReviewRequests,
            PullRequestFilter::TeamReviewRequests,
        ] {
            filters.push(
                item(filter.label(), Command::LiveFilter(filter))
                    .checked(live.filters.contains(&filter))
                    .into(),
            );
        }
        if live.credential_id.is_empty() || self.live_folders.errors.contains_key(&id) {
            filters.push(Entry::Separator);
            filters.push(
                item(
                    if live.credential_id.is_empty() {
                        "Connect GitHub…"
                    } else {
                        "Reconnect GitHub…"
                    },
                    Command::LiveConnect,
                )
                .into(),
            );
        }
        let mut rows = vec![pending("Pull Requests")
            .live_icon()
            .submenu(filters)
            .submenu_width(184.)
            .into()];
        let mut repos: Vec<_> = live.items.iter().map(|i| i.repository.clone()).collect();
        repos.sort();
        repos.dedup();
        if !repos.is_empty() {
            let choices = repos
                .iter()
                .map(|r| {
                    item(r.clone(), Command::LiveRepository)
                        .checked(!live.hidden_repositories.contains(r))
                        .into()
                })
                .collect();
            rows.push(pending("Repositories").submenu(choices).into());
        }
        let source = self.browser.folder_space(id);
        let profile = source
            .and_then(|id| self.browser.spaces.iter().find(|s| s.id == id))
            .map(|s| s.user_profile_id);
        let destinations = self
            .browser
            .spaces
            .iter()
            .filter(|s| Some(s.user_profile_id) == profile)
            .map(|s| {
                item(s.name.clone(), Command::LiveMove(s.id))
                    .space_destination(s.id)
                    .enabled(Some(s.id) != source)
                    .into()
            })
            .collect();
        rows.extend([
            Entry::Separator,
            pending("Move to").submenu(destinations).into(),
            Entry::Separator,
            item("Change Icon…", Command::FolderIcon).into(),
            item("Rename…", Command::FolderRename).into(),
            Entry::Separator,
            item("Delete Live Folder", Command::FolderDelete).into(),
        ]);
        rows
    }
    pub(crate) fn render_live_items(
        &self,
        id: FolderId,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(folder) = self.browser.folder(id).filter(|f| !f.collapsed) else {
            return vec![];
        };
        let Some(live) = &folder.live else {
            return vec![];
        };
        let Some(space) = self.browser.folder_space(id) else {
            return vec![];
        };
        let Some(profile) = self
            .browser
            .spaces
            .iter()
            .find(|s| s.id == space)
            .map(|s| s.user_profile_id)
        else {
            return vec![];
        };
        live.visible_items()
            .map(|item| {
                let LiveItem {
                    id: item_id,
                    title,
                    url,
                    ..
                } = item;
                let item_id = *item_id;
                let url = url.clone();
                let unread = item.unread();
                let selector = self.sidebar_debug_name(format!("live-item-{}-{item_id}", id.get()));
                let title = title.clone();
                let page_title = title.clone();
                div()
                    .id(gpui::SharedString::from(format!(
                        "live-item-{}-{item_id}",
                        id.get()
                    )))
                    .debug_selector(move || selector.clone())
                    .h(px(crate::window::SIDEBAR_ROW_HEIGHT))
                    .flex_none()
                    .min_w_0()
                    .ml(px(16.))
                    .pl(px(9.))
                    .pr(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(px(crate::window::SIDEBAR_ROW_RADIUS))
                    .hover(|s| s.bg(hsla(0., 0., 1., 0.1)))
                    .text_size(px(14.))
                    .child(request_icon())
                    .child(div().flex_1().min_w_0().truncate().child(title.clone()))
                    .when(unread, |r| {
                        r.child(div().size(px(5.)).rounded_full().bg(gpui::rgb(0x4d89ff)))
                    })
                    .on_click(cx.listener(move |b, _, _, cx| {
                        if !b.prepare_sidebar_target(space, profile) {
                            return;
                        }
                        let current = b
                            .browser
                            .folder(id)
                            .and_then(|f| f.live.as_ref())
                            .is_some_and(|l| {
                                l.items.iter().any(|i| i.id == item_id && i.url == url)
                            });
                        if !current {
                            return;
                        }
                        b.edit_browser(|model| {
                            if let Some(live) = model.live_folder_mut(id) {
                                live.mark_read(item_id);
                            }
                        });
                        b.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(space), cx);
                        let tab = b.new_command_tab(&url, false, cx);
                        b.edit_browser(|model| model.set_title(tab, page_title.clone()));
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect()
    }
    pub(crate) fn render_live_connection(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(sheet) = &self.live_folders.sheet else {
            return div().into_any_element();
        };
        let palette = crate::material::DialogPalette::new(self.theme_is_dark());
        if sheet.deleting {
            return self.render_live_delete(cx);
        }
        let busy = sheet.busy;
        let enabled = !busy && !sheet.key.read(cx).text().trim().is_empty();
        div().size_full().font_family(".SystemUIFont").bg(hsla(0.,0.,0.,if palette.dark{0.55}else{0.18})).track_focus(&sheet.focus).flex().items_center().justify_center().key_context("GitHubConnection")
            .on_action(cx.listener(|b,_:&Accept,_,cx|b.submit_live_connection(cx))).on_action(cx.listener(|b,_:&Cancel,_,cx|b.close_live_connection(cx)))
            .child(div().id("github-connection").debug_selector(||"github-connection".into()).w(px(450.)).p(px(28.)).rounded(px(18.)).border_1().border_color(palette.border).bg(palette.surface).shadow_lg().text_color(palette.text).flex().flex_col().gap(px(16.))
                .child(div().text_size(px(18.)).font_weight(gpui::FontWeight::BOLD).child("Connect GitHub"))
                .child(div().text_size(px(14.)).line_height(px(20.)).text_color(palette.muted).child("Pull requests from you and your team will show up here automatically"))
                .child(div().h(px(36.)).px(px(10.)).rounded(px(6.)).border_1().border_color(palette.border).bg(palette.input).flex().items_center().child(sheet.key.clone()))
                .child(div().text_size(px(12.)).line_height(px(17.)).text_color(palette.muted).child("Use a fine-grained personal access token with read access to your repositories. Your token is stored in the system keychain."))
                .when_some(sheet.error.as_ref(),|r,e|r.child(div().text_size(px(12.)).child(e.clone())))
                .child(div().flex().justify_end().gap(px(8.)).text_size(px(13.))
                    .child(div().id("github-cancel").debug_selector(||"github-cancel".into()).px(px(16.)).py(px(8.)).rounded(px(6.)).bg(palette.control).child("Cancel").on_click(cx.listener(|b,_,_,cx|b.close_live_connection(cx))))
                    .child(div().id("github-connect").debug_selector(||"github-connect".into()).px(px(16.)).py(px(8.)).rounded(px(6.)).bg(gpui::rgb(0x5966d6)).text_color(gpui::white()).opacity(if enabled{1.}else{0.45}).child(if busy{"Connecting…"}else{"Connect"}).on_click(cx.listener(|b,_,_,cx|b.submit_live_connection(cx))))))
            .into_any_element()
    }
}

/// Original vector, matching the measured 18px blue Live Folder silhouette.
pub(crate) fn folder_icon(open: bool) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let origin = bounds.origin;
            let blue = gpui::rgb(0x212afb);
            window.paint_quad(
                gpui::fill(
                    gpui::Bounds::new(
                        origin + gpui::point(px(1.), px(2.)),
                        gpui::size(px(7.), px(4.)),
                    ),
                    blue,
                )
                .corner_radii(px(1.5)),
            );
            window.paint_quad(
                gpui::fill(
                    gpui::Bounds::new(
                        origin + gpui::point(px(1.), px(5.)),
                        gpui::size(px(16.), px(12.)),
                    ),
                    blue,
                )
                .corner_radii(px(2.)),
            );
            let mut p = gpui::PathBuilder::stroke(px(1.2));
            for (i, (x, y)) in [(5., 14.), (5., 8.), (11., 8.), (13., 10.), (13., 14.)]
                .into_iter()
                .enumerate()
            {
                let v = origin + gpui::point(px(x), px(y));
                if i == 0 {
                    p.move_to(v)
                } else {
                    p.line_to(v)
                }
            }
            if let Ok(p) = p.build() {
                window.paint_path(p, gpui::white());
            }
            if open {
                window.paint_quad(
                    gpui::fill(
                        gpui::Bounds::new(
                            origin + gpui::point(px(4.), px(11.)),
                            gpui::size(px(11.), px(5.)),
                        ),
                        gpui::hsla(0., 0., 1., 0.18),
                    )
                    .corner_radii(px(1.)),
                );
            }
        },
    )
    .size(px(18.))
    .into_any_element()
}
impl BrowserWindow {
    fn render_live_delete(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(sheet) = &self.live_folders.sheet else {
            return div().into_any_element();
        };
        let Some(folder) = self.browser.folder(sheet.folder) else {
            return div().into_any_element();
        };
        let p = crate::material::DialogPalette::new(self.theme_is_dark());
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .w(px(if primary { 141. } else { 110. }))
                .h(px(36.))
                .rounded(px(6.))
                .border_1()
                .border_color(gpui::rgb(if primary {
                    0xff614b
                } else if p.dark {
                    0x615a9d
                } else {
                    0xdedee5
                }))
                .bg(gpui::rgb(if primary {
                    0xff3514
                } else if p.dark {
                    0x302c63
                } else {
                    0xffffff
                }))
                .text_color(if primary || p.dark {
                    gpui::white().into()
                } else {
                    p.text
                })
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(label)
                .child(
                    div()
                        .w(px(if primary { 28. } else { 34. }))
                        .h(px(20.))
                        .rounded(px(3.))
                        .bg(hsla(
                            0.,
                            0.,
                            if p.dark || primary { 1. } else { 0. },
                            if p.dark || primary { 0.18 } else { 0.10 },
                        ))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(if primary {
                            crate::icons::control_sized(crate::icons::Icon::Enter, 18.)
                        } else {
                            div().child("ESC").into_any_element()
                        }),
                )
        };
        div().size_full().font_family(".SystemUIFont").bg(hsla(0.,0.,0.,if p.dark{0.55}else{0.18})).track_focus(&sheet.focus).key_context("GitHubConnection").flex().items_center().justify_center()
            .on_action(cx.listener(|b,_:&Accept,_,cx|b.submit_live_connection(cx))).on_action(cx.listener(|b,_:&Cancel,_,cx|b.close_live_connection(cx)))
            .child(div().id("live-folder-delete").debug_selector(||"live-folder-delete".into()).relative().w(px(450.)).h(px(292.)).rounded(px(18.)).border_1()
                .border_color(gpui::rgb(if p.dark{0x020202}else{0xbcbcc4})).bg(p.surface).shadow_lg().text_color(p.text)
                .child(gpui::img(Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png,include_bytes!("../assets/echo-icon.png").to_vec()))).absolute().left(px(38.)).top(px(42.)).size(px(52.)))
                // Match the native title ink width with GPUI's system-font metrics.
                .child(div().absolute().left(px(37.)).right(px(28.)).top(px(115.)).text_size(px(17.2)).line_height(px(22.)).font_weight(gpui::FontWeight::BOLD).text_color(if p.dark{gpui::white().into()}else{p.text}).child(format!("Delete your {} Live Folder?",folder.name)))
                .child(div().absolute().left(px(37.)).right(px(38.)).top(px(152.)).text_size(px(14.)).line_height(px(18.)).text_color(gpui::rgb(if p.dark{0xb8b8c0}else{0x44455d})).child("You can restore this folder at any time by right-clicking the sidebar."))
                .child(div().absolute().right(px(27.)).bottom(px(28.)).flex().gap(px(8.))
                    .child(button("live-delete-cancel","Cancel",false).on_click(cx.listener(|b,_,_,cx|b.close_live_connection(cx))))
                    .child(button("live-delete-confirm","Delete Folder",true).on_click(cx.listener(|b,_,_,cx|b.submit_live_connection(cx))))))
            .into_any_element()
    }
}

pub(crate) fn request_icon() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let origin = bounds.origin;
            let ink = window.text_style().color;
            let mut p = gpui::PathBuilder::stroke(px(1.4));
            p.move_to(origin + gpui::point(px(4.), px(5.)));
            p.line_to(origin + gpui::point(px(4.), px(13.)));
            p.move_to(origin + gpui::point(px(10.), px(4.)));
            p.line_to(origin + gpui::point(px(13.), px(7.)));
            p.line_to(origin + gpui::point(px(13.), px(13.)));
            p.move_to(origin + gpui::point(px(10.), px(4.)));
            p.line_to(origin + gpui::point(px(10.), px(8.)));
            if let Ok(p) = p.build() {
                window.paint_path(p, ink);
            }
            for (x, y) in [(4., 3.), (4., 15.), (13., 15.)] {
                window.paint_quad(
                    gpui::fill(
                        gpui::Bounds::new(
                            origin + gpui::point(px(x - 1.5), px(y - 1.5)),
                            gpui::size(px(3.), px(3.)),
                        ),
                        ink,
                    )
                    .corner_radii(px(1.5)),
                );
            }
        },
    )
    .size(px(18.))
    .into_any_element()
}
