//! GPUI's tab/window projection consumed by the CEF extension API adapters.
//! This contains model state, not Chrome window or toolbar objects.
use crate::window::{BrowserWindow, Overlay};
use browser_core::UserProfileId;
use engine::{ExtensionTab, ExtensionTabsSnapshot, ExtensionWindow};
use gpui::{Context, Window};
use std::collections::{BTreeMap, BTreeSet};
mod operations;
use operations::{Mutation, Operation};
use std::{cell::RefCell, rc::Rc, time::SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ProfileScope(pub UserProfileId, pub bool);
impl ProfileScope {
    pub(crate) fn key(self) -> String {
        if self.1 {
            "private".into()
        } else {
            self.0.storage_key().unwrap_or_else(|| "default".into())
        }
    }
}
#[derive(Clone)]
pub(crate) struct WindowModel {
    pub window: ExtensionWindow,
    pub scope: ProfileScope,
    pub tabs: Vec<(ProfileScope, ExtensionTab)>,
    pub focused: bool,
    pub serial: u64,
    pub profiles: BTreeSet<ProfileScope>,
}
#[derive(Default)]
pub(crate) struct Model {
    pub windows: BTreeMap<u64, WindowModel>,
    ownership: BTreeMap<(ProfileScope, String), u64>,
    last_focused: BTreeMap<ProfileScope, u64>,
    selected: BTreeMap<(ProfileScope, u64), String>,
    serial: u64,
    auto_discardable: BTreeMap<(ProfileScope, String), bool>,
    openers: BTreeMap<(ProfileScope, String), String>,
    known_scopes: BTreeSet<ProfileScope>,
    split_ids: BTreeMap<(u64, u64), i32>,
    next_split_id: i32,
    retired_windows: BTreeSet<u64>,
}
impl gpui::Global for Model {}
impl Model {
    fn split_id(&mut self, namespace: u64, id: u64) -> i32 {
        *self.split_ids.entry((namespace, id)).or_insert_with(|| {
            self.next_split_id = self
                .next_split_id
                .checked_add(1)
                .expect("extension split ID capacity");
            self.next_split_id
        })
    }
    fn retire_window(&mut self, id: u64) {
        self.retired_windows.insert(id);
        self.windows.remove(&id);
        self.last_focused.retain(|_, window| *window != id);
        self.selected.retain(|(_, window), _| *window != id);
    }
    fn tab_target(&self, profile: &str, key: &str) -> Result<(u64, ProfileScope), String> {
        self.ownership
            .iter()
            .find(|((scope, tab), _)| scope.key() == profile && tab == key)
            .map(|((scope, _), window)| (*window, *scope))
            .ok_or_else(|| "Extension target tab closed or changed profile".into())
    }
    fn window_target(&self, profile: &str, key: &str) -> Result<(u64, ProfileScope), String> {
        self.windows
            .iter()
            .find_map(|(id, window)| {
                if window.window.key != key {
                    return None;
                }
                let scope = window
                    .profiles
                    .iter()
                    .copied()
                    .chain(std::iter::once(window.scope))
                    .chain(window.tabs.iter().map(|(scope, _)| *scope))
                    .find(|scope| scope.key() == profile)?;
                Some((*id, scope))
            })
            .ok_or_else(|| "Extension target window closed or changed profile".into())
    }
    pub(crate) fn update(&mut self, id: u64, mut window: WindowModel, live: &BTreeSet<u64>) {
        if self.retired_windows.contains(&id) {
            return;
        }
        self.serial = self.serial.wrapping_add(1);
        window.serial = self.serial;
        self.known_scopes.extend(window.profiles.iter().copied());
        self.known_scopes.insert(window.scope);
        self.known_scopes
            .extend(window.tabs.iter().map(|(scope, _)| *scope));
        if window.focused {
            // Physical focus has one owner, even if another root has not yet
            // observed its corresponding deactivation notification.
            for existing in self.windows.values_mut() {
                existing.focused = false;
            }
            self.last_focused.insert(window.scope, id);
        }
        self.windows.insert(id, window);
        self.windows.retain(|id, _| live.contains(id));
        self.last_focused.retain(|_, id| live.contains(id));
        self.selected.retain(|(_, id), _| live.contains(id));
    }
    pub(crate) fn snapshots(
        &mut self,
    ) -> Result<Vec<(ProfileScope, ExtensionTabsSnapshot)>, String> {
        let mut candidates: BTreeMap<
            (ProfileScope, String),
            Vec<(u64, &WindowModel, &ExtensionTab)>,
        > = BTreeMap::new();
        let mut scopes = self.known_scopes.clone();
        for (&id, window) in &self.windows {
            scopes.insert(window.scope);
            for (scope, tab) in &window.tabs {
                scopes.insert(*scope);
                candidates
                    .entry((*scope, tab.key.clone()))
                    .or_default()
                    .push((id, window, tab));
            }
        }
        self.ownership.retain(|key, _| candidates.contains_key(key));
        self.auto_discardable
            .retain(|key, _| candidates.contains_key(key));
        self.openers.retain(|key, opener| {
            candidates.contains_key(key) && candidates.contains_key(&(key.0, opener.clone()))
        });
        let mut assigned: BTreeMap<ProfileScope, Vec<(u64, ExtensionTab)>> = BTreeMap::new();
        for (key, candidates) in candidates {
            // A native attachment is an explicit observation from Pages. The
            // newest observation wins during a cross-window renderer handoff.
            // Dormant tabs retain their previous owner while that root lives.
            let selected = candidates
                .iter()
                .filter(|(_, _, tab)| tab.browser_id.is_some())
                .max_by_key(|(_, window, _)| window.serial)
                .or_else(|| {
                    self.ownership
                        .get(&key)
                        .and_then(|owner| candidates.iter().find(|(id, _, _)| id == owner))
                })
                .or_else(|| {
                    candidates
                        .iter()
                        .filter(|(_, _, tab)| tab.active)
                        .max_by_key(|(_, window, _)| window.serial)
                })
                .unwrap_or(&candidates[0]);
            let (id, window, tab) = *selected;
            let mut tab = tab.clone();
            if let Some(value) = self.auto_discardable.get(&key) {
                tab.auto_discardable = *value;
            }
            tab.opener_key = self.openers.get(&key).cloned();
            tab.window = window.window.key.clone();
            self.ownership.insert(key.clone(), id);
            if tab.active {
                self.selected.insert((key.0, id), key.1);
            }
            assigned.entry(key.0).or_default().push((id, tab));
        }
        let mut result = Vec::new();
        for scope in scopes {
            let mut rows = assigned.remove(&scope).unwrap_or_default();
            let relevant: BTreeSet<_> = rows
                .iter()
                .map(|(id, _)| *id)
                .chain(
                    self.windows
                        .iter()
                        .filter(|(_, w)| w.profiles.contains(&scope))
                        .map(|(id, _)| *id),
                )
                .collect();
            let mut windows = Vec::new();
            let mut focused = None;
            let mut last = self
                .last_focused
                .get(&scope)
                .copied()
                .filter(|id| relevant.contains(id))
                .or_else(|| relevant.first().copied());
            for id in &relevant {
                let source = &self.windows[id];
                let mut window = source.window.clone();
                window.incognito = scope.1;
                if source.focused && source.scope == scope {
                    focused = Some(window.key.clone());
                    last = Some(*id);
                }
                windows.push(window);
            }
            // Inactive profiles retain their last selected page in the same
            // physical root. They do not become focused when another profile's
            // Space is selected in that root.
            for (id, tab) in &mut rows {
                let source = &self.windows[id];
                if source.scope != scope {
                    tab.active = self.selected.get(&(scope, *id)) == Some(&tab.key);
                    tab.highlighted = tab.active;
                }
            }
            rows.sort_by(|(a_id, a), (b_id, b)| {
                a_id.cmp(b_id)
                    .then(a.index.cmp(&b.index))
                    .then(a.key.cmp(&b.key))
            });
            let mut positions = BTreeMap::new();
            let tabs = rows
                .into_iter()
                .map(|(id, mut tab)| {
                    let index = positions.entry(id).or_insert(0);
                    tab.index = *index;
                    *index += 1;
                    tab
                })
                .collect();
            let snapshot = ExtensionTabsSnapshot {
                profile_key: scope.key(),
                windows,
                tabs,
                focused_window: focused,
                last_focused_window: last.map(|id| self.windows[&id].window.key.clone()),
            };
            snapshot.validate()?;
            result.push((scope, snapshot));
        }
        Ok(result)
    }
}

fn profile_selections(
    browser: &browser_core::Browser,
) -> BTreeMap<UserProfileId, browser_core::TabId> {
    let mut selections = BTreeMap::new();
    for space in &browser.spaces {
        let Some(tab) = space
            .active_tab
            .and_then(|id| browser.all_tabs().find(|tab| tab.id == id))
        else {
            continue;
        };
        if browser.user_profile_id_for_tab(tab.id) != Some(space.user_profile_id) {
            continue;
        }
        let selected = selections
            .entry(space.user_profile_id)
            .or_insert((tab.id, tab.last_active));
        if tab.last_active > selected.1 {
            *selected = (tab.id, tab.last_active);
        }
    }
    selections
        .into_iter()
        .map(|(profile, (tab, _))| (profile, tab))
        .collect()
}

impl BrowserWindow {
    fn extension_model_tab(
        &self,
        scope: ProfileScope,
        key: &str,
    ) -> Result<browser_core::TabId, String> {
        if scope.1 != self.browser.is_private() {
            return Err("Extension privacy scope changed".into());
        }
        let namespace = self.profile.handle.model_namespace();
        self.browser
            .all_tabs()
            .find(|tab| {
                format!("{namespace}:{}", tab.id.get()) == key
                    && self.browser.user_profile_id_for_tab(tab.id) == Some(scope.0)
            })
            .map(|tab| tab.id)
            .ok_or_else(|| "Extension tab no longer belongs to this profile".into())
    }
    fn apply_extension_mutation(
        &mut self,
        scope: ProfileScope,
        mutation: &Mutation,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        if crate::quit::is_quitting()
            || scope.key() != mutation.profile
            || scope.1 != self.browser.is_private()
            || self.browser.user_profile(scope.0).is_none()
        {
            return Err("Extension profile is no longer available".into());
        }
        let namespace = self.profile.handle.model_namespace();
        let mut affected = None;
        let mut navigate = None;
        let mut reload = None;
        let mut overrides = None;
        match &mutation.operation {
            Operation::Create {
                url,
                active,
                pinned,
                index,
                opener,
                ..
            } => {
                if let Some(opener) = opener {
                    self.extension_model_tab(scope, opener)?;
                }
                let id = self.edit_browser(|browser| {
                    browser.extension_create_tab(
                        scope.0,
                        url,
                        *active,
                        *pinned,
                        *index,
                        SystemTime::now(),
                    )
                })?;
                let key = format!("{namespace}:{}", id.get());
                affected = Some(id);
                overrides = Some((key, None, opener.clone()));
            }
            Operation::Update {
                tab,
                url,
                active,
                highlighted,
                pinned,
                muted,
                auto_discardable,
                opener,
            } => {
                let id = self.extension_model_tab(scope, tab)?;
                if let Some(opener) = opener {
                    if opener == tab {
                        return Err("A tab cannot be its own opener".into());
                    }
                    self.extension_model_tab(scope, opener)?;
                }
                if let Some(highlighted) = highlighted {
                    if active == &Some(true) && !highlighted
                        || (active != &Some(true)
                            && !self.extension_highlight_supported(id, *highlighted))
                    {
                        return Err(
                            "The requested highlight state is unavailable in this Space".into()
                        );
                    }
                }
                let update = browser_core::extension_tabs::ExtensionTabUpdate {
                    url: url.clone(),
                    active: *active == Some(true),
                    pinned: *pinned,
                    muted: *muted,
                };
                self.edit_browser(|browser| {
                    browser.extension_update_tab(scope.0, id, &update, SystemTime::now())
                })?;
                if let Some(highlighted) = highlighted {
                    self.set_extension_highlight(id, *highlighted, cx);
                }
                affected = Some(id);
                navigate = url.as_ref().map(|url| (id, url.clone()));
                overrides = Some((tab.clone(), *auto_discardable, opener.clone()));
            }
            Operation::Remove { tabs } => {
                // Every target in this root is validated before its first edit.
                let ids = tabs
                    .iter()
                    .map(|key| self.extension_model_tab(scope, key))
                    .collect::<Result<Vec<_>, _>>()?;
                self.edit_browser(|browser| {
                    for id in ids {
                        browser.extension_remove_tab(scope.0, id, SystemTime::now())?;
                    }
                    Ok::<_, String>(())
                })?;
            }
            Operation::Reload { tab, bypass } => {
                let id = self.extension_model_tab(scope, tab)?;
                affected = Some(id);
                reload = Some((id, *bypass));
            }
        }
        if let Some((key, discardable, opener)) = overrides {
            let model = cx.global_mut::<Model>();
            if let Some(value) = discardable {
                model.auto_discardable.insert((scope, key.clone()), value);
            }
            if let Some(opener) = opener {
                model.openers.insert((scope, key), opener);
            }
        }
        if let Some(pages) = &self.pages {
            pages
                .borrow_mut()
                .sync(&self.browser, self.page_content_visible());
        }
        // Publish first, so a lazy page can adopt its reserved session ID.
        self.sync_extension_model(window, cx);
        if let Some(id) = affected {
            if let Some(pages) = &self.pages {
                let mut pages = pages.borrow_mut();
                let created = pages.materialize_extension_tab(&self.browser, id)?;
                if let Some((tab, url)) = navigate {
                    pages.navigate_extension_tab(tab, &url);
                }
                if let Some((tab, bypass)) = reload {
                    if !created {
                        if bypass {
                            pages.force_reload(tab);
                        } else {
                            pages.reload(tab);
                        }
                    }
                }
            }
        }
        self.sync_extension_model(window, cx);
        cx.notify();
        Ok(affected.map_or_else(
            || serde_json::json!({}),
            |id| serde_json::json!({"tabKey":format!("{namespace}:{}",id.get())}),
        ))
    }
    pub(crate) fn poll_extension_model_requests(&mut self, cx: &mut Context<Self>) {
        let Some(origin) = self.pages.clone() else {
            return;
        };
        let requests = origin.borrow_mut().take_extension_model_requests();
        for request in requests {
            let origin = origin.clone();
            cx.defer(move |cx| dispatch(origin, request, cx));
        }
    }
    pub(crate) fn forget_extension_model(&self, cx: &mut gpui::App) {
        if cx.has_global::<Model>() {
            cx.global_mut::<Model>()
                .retire_window(self.main_window.window_id().as_u64());
        }
    }
    pub(crate) fn retire_extension_model(&mut self, cx: &mut gpui::App) {
        if crate::quit::is_quitting() {
            return;
        }
        self.forget_extension_model(cx);
        if !cx.has_global::<Model>() {
            return;
        }
        let model = cx.global_mut::<Model>();
        let Ok(snapshots) = model.snapshots() else {
            return;
        };
        let Some(pages) = &self.pages else {
            return;
        };
        if let Ok(mut pages) = pages.try_borrow_mut() {
            for (scope, snapshot) in snapshots {
                if scope.1 == self.browser.is_private() && pages.has_extension_model_host(scope.0) {
                    let _ = pages.publish_extension_tabs(scope.0, scope.1, &snapshot);
                }
            }
        }
    }
    pub(crate) fn sync_extension_model(&mut self, window: &Window, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() {
            return;
        }
        if cx.try_global::<Model>().is_none() {
            cx.set_global(Model::default());
        }
        if cx
            .global::<Model>()
            .retired_windows
            .contains(&self.main_window.window_id().as_u64())
        {
            return;
        }
        let Some(pages) = self.pages.clone() else {
            return;
        };
        let Ok(pages_read) = pages.try_borrow() else {
            return;
        };
        let id = self.main_window.window_id().as_u64();
        let namespace = self.profile.handle.model_namespace();
        let key = format!("window:{id}");
        let scope = ProfileScope(
            self.browser.active_user_profile_id(),
            self.browser.is_private(),
        );
        let bounds = window.bounds();
        let focused = cx.active_window().is_some_and(|active| {
            active.window_id() == self.main_window.window_id()
                || Some(active.window_id()) == self.extension_popup_window_id()
                || active.downcast::<Overlay>().is_some_and(|overlay| {
                    overlay
                        .read(cx)
                        .is_ok_and(|overlay| overlay.belongs_to(cx.entity_id()))
                })
        });
        let selected_profiles = profile_selections(&self.browser);
        let split_ids: BTreeMap<_, _> = self
            .browser
            .all_tabs()
            .filter_map(|tab| {
                self.browser.split_for(tab.id).map(|split| {
                    (
                        tab.id.get(),
                        cx.global_mut::<Model>().split_id(namespace, split.id.get()),
                    )
                })
            })
            .collect();
        let tabs =
            self.browser
                .all_tabs()
                .enumerate()
                .filter_map(|(index, tab)| {
                    let profile = self.browser.user_profile_id_for_tab(tab.id)?;
                    let (browser_id, state) = pages_read.extension_attachment(tab.id);
                    let active = self
                        .browser
                        .active_tab()
                        .is_some_and(|active| active.id == tab.id)
                        || (profile != scope.0 && selected_profiles.get(&profile) == Some(&tab.id));
                    let status = if state.as_ref().is_some_and(|state| state.loading) {
                        engine::ExtensionTabStatus::Loading
                    } else if state.is_some() {
                        engine::ExtensionTabStatus::Complete
                    } else {
                        engine::ExtensionTabStatus::Unloaded
                    };
                    Some((
                        ProfileScope(profile, scope.1),
                        ExtensionTab {
                            key: format!("{namespace}:{}", tab.id.get()),
                            window: key.clone(),
                            browser_id,
                            index: index as u32,
                            active,
                            highlighted: active || self.sidebar_item_selected(tab.id),
                            pinned: self.browser.is_favorite(tab.id)
                                || self.browser.spaces.iter().any(|space| {
                                    space.pinned.iter().any(|pinned| pinned.id == tab.id)
                                }),
                            muted: tab.muted,
                            audible: state.as_ref().is_some_and(|state| state.audible),
                            auto_discardable: true,
                            discarded: false,
                            frozen: false,
                            group_id: -1,
                            split_view_id: split_ids.get(&tab.id.get()).copied().unwrap_or(-1),
                            opener_key: None,
                            status,
                            url: tab.url.clone(),
                            title: tab.page_title.as_ref().unwrap_or(&tab.title).clone(),
                            favicon_url: None,
                            last_accessed: tab
                                .last_active
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs_f64()
                                * 1000.,
                        },
                    ))
                })
                .collect();
        drop(pages_read);
        let record = WindowModel {
            window: ExtensionWindow {
                key,
                kind: if self.transient.is_some() {
                    engine::ExtensionWindowKind::Popup
                } else {
                    engine::ExtensionWindowKind::Normal
                },
                state: if crate::native::window_minimized(window) {
                    engine::ExtensionWindowState::Minimized
                } else if window.is_fullscreen() {
                    engine::ExtensionWindowState::Fullscreen
                } else if window.is_maximized() {
                    engine::ExtensionWindowState::Maximized
                } else {
                    engine::ExtensionWindowState::Normal
                },
                left: f32::from(bounds.origin.x).round() as i32,
                top: f32::from(bounds.origin.y).round() as i32,
                width: f32::from(bounds.size.width).max(0.).round() as u32,
                height: f32::from(bounds.size.height).max(0.).round() as u32,
                incognito: scope.1,
            },
            scope,
            tabs,
            focused,
            serial: 0,
            profiles: self
                .browser
                .spaces
                .iter()
                .map(|space| ProfileScope(space.user_profile_id, scope.1))
                .collect(),
        };
        let live = cx
            .windows()
            .into_iter()
            .filter(|handle| handle.downcast::<BrowserWindow>().is_some())
            .map(|handle| handle.window_id().as_u64())
            .collect();
        let snapshots = {
            let model = cx.global_mut::<Model>();
            model.update(id, record, &live);
            model.snapshots()
        };
        let Ok(snapshots) = snapshots else {
            return;
        };
        if let Ok(mut pages) = pages.try_borrow_mut() {
            for (scope, snapshot) in snapshots {
                if scope.1 != self.browser.is_private() {
                    continue;
                }
                if let Err(error) = pages.publish_extension_tabs(scope.0, scope.1, &snapshot) {
                    self.extensions.error = Some(error);
                }
            }
        };
    }
}

fn dispatch(
    origin: Rc<RefCell<crate::pages::Pages>>,
    request: engine::ExtensionModelRequest,
    cx: &mut gpui::App,
) {
    let mut execute = || -> Result<serde_json::Value, String> {
        if crate::quit::is_quitting() || !origin.borrow().extension_model_request_pending(&request)
        {
            return Err("Extension model request expired or its owner closed".into());
        }
        let mutation = Mutation::parse(request.data.clone())?;
        let model = cx
            .try_global::<Model>()
            .ok_or("Extension model is unavailable")?;
        let mut targets: Vec<(u64, ProfileScope, Operation)> = Vec::new();
        match &mutation.operation {
            Operation::Create { window, .. } => {
                let (id, scope) = model.window_target(&mutation.profile, window)?;
                targets.push((id, scope, mutation.operation.clone()));
            }
            Operation::Update { tab, .. } | Operation::Reload { tab, .. } => {
                let (id, scope) = model.tab_target(&mutation.profile, tab)?;
                targets.push((id, scope, mutation.operation.clone()));
            }
            Operation::Remove { tabs } => {
                let mut groups: BTreeMap<(u64, ProfileScope), Vec<String>> = BTreeMap::new();
                for tab in tabs {
                    let target = model.tab_target(&mutation.profile, tab)?;
                    groups.entry(target).or_default().push(tab.clone());
                }
                targets.extend(
                    groups
                        .into_iter()
                        .map(|((id, scope), tabs)| (id, scope, Operation::Remove { tabs })),
                );
            }
        }
        if let Operation::Create {
            opener: Some(opener),
            ..
        }
        | Operation::Update {
            opener: Some(opener),
            ..
        } = &mutation.operation
        {
            if model.tab_target(&mutation.profile, opener)?.0 != targets[0].0 {
                return Err("An opener must belong to the target window".into());
            }
        }
        // Resolve every real root before applying an operation to any of them.
        let handles = targets
            .into_iter()
            .map(|(id, scope, operation)| {
                let handle = cx
                    .windows()
                    .into_iter()
                    .find(|handle| handle.window_id().as_u64() == id)
                    .and_then(|handle| handle.downcast::<BrowserWindow>())
                    .ok_or("Extension target window closed")?;
                Ok((handle, scope, operation))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut result = serde_json::json!({});
        for (handle, scope, operation) in handles {
            if !origin.borrow().extension_model_request_pending(&request) {
                return Err("Extension model request expired".into());
            }
            let command = Mutation {
                profile: mutation.profile.clone(),
                extension: mutation.extension.clone(),
                operation,
            };
            result = handle
                .update(cx, |view, window, cx| {
                    view.apply_extension_mutation(scope, &command, window, cx)
                })
                .map_err(|_| "Extension target window closed")??;
        }
        Ok(result)
    };
    let result = execute().unwrap_or_else(|error| serde_json::json!({"error":error}));
    let _ = origin
        .borrow()
        .complete_extension_model_request(&request, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> ProfileScope {
        ProfileScope(browser_core::Browser::new().active_user_profile_id(), false)
    }
    fn tab(key: &str, browser: Option<i32>, active: bool) -> ExtensionTab {
        ExtensionTab {
            key: key.into(),
            window: String::new(),
            browser_id: browser,
            index: 0,
            active,
            highlighted: active,
            pinned: false,
            muted: false,
            audible: false,
            auto_discardable: true,
            discarded: false,
            frozen: false,
            group_id: -1,
            split_view_id: -1,
            opener_key: None,
            status: engine::ExtensionTabStatus::Complete,
            url: "https://example.com/".into(),
            title: "Example".into(),
            favicon_url: None,
            last_accessed: 1.,
        }
    }
    fn window(
        id: u64,
        scope: ProfileScope,
        tabs: Vec<(ProfileScope, ExtensionTab)>,
        focused: bool,
    ) -> WindowModel {
        WindowModel {
            window: ExtensionWindow {
                key: format!("window:{id}"),
                kind: engine::ExtensionWindowKind::Normal,
                state: engine::ExtensionWindowState::Normal,
                left: 100,
                top: 100,
                width: 1000,
                height: 760,
                incognito: scope.1,
            },
            scope,
            tabs,
            focused,
            serial: 0,
            profiles: BTreeSet::from([scope]),
        }
    }
    #[test]
    fn an_inactive_profiles_first_projection_uses_its_preserved_core_selection() {
        let mut browser = browser_core::Browser::new();
        let now = SystemTime::UNIX_EPOCH;
        let a = browser.active_user_profile_id();
        let original = browser.new_tab("https://a.example/", now);
        let b = browser.create_user_profile("Other profile").unwrap();
        browser.add_space("Other", 30.);
        let id = browser.spaces.last().unwrap().id;
        browser.assign_space_user_profile(id, b).unwrap();
        browser.switch_space(browser.spaces.len() - 1);
        let other = browser.new_tab("https://b.example/", now);
        browser.switch_space(0);
        assert_eq!(browser.active_tab().unwrap().id, original);
        let selected = profile_selections(&browser);
        assert_eq!(selected[&a], original);
        assert_eq!(selected[&b], other);
    }
    #[test]
    fn retiring_the_last_root_clears_open_tabs_and_windows_without_erasing_session_inventory() {
        let scope = scope();
        let mut model = Model::default();
        model.update(
            1,
            window(
                1,
                scope,
                vec![(scope, tab("session:1", Some(17), true))],
                true,
            ),
            &BTreeSet::from([1]),
        );
        model.snapshots().unwrap();
        model.retire_window(1);
        let snapshot = model.snapshots().unwrap().remove(0).1;
        assert!(snapshot.tabs.is_empty());
        assert!(snapshot.windows.is_empty());
        assert!(snapshot.focused_window.is_none());
        assert!(snapshot.last_focused_window.is_none());
        assert!(model.retired_windows.contains(&1));
    }
    #[test]
    fn an_empty_assigned_profile_can_create_tabs_and_removed_assignments_clear_native_inventory() {
        let mut browser = browser_core::Browser::new();
        let a = ProfileScope(browser.active_user_profile_id(), false);
        let b = ProfileScope(browser.create_user_profile("Empty profile").unwrap(), false);
        let mut model = Model::default();
        let live = BTreeSet::from([1]);
        let mut root = window(1, a, vec![(a, tab("session:1", Some(17), true))], true);
        root.profiles.insert(b);
        model.update(1, root.clone(), &live);
        let first = model.snapshots().unwrap();
        let empty = &first.iter().find(|(scope, _)| *scope == b).unwrap().1;
        assert_eq!(empty.windows.len(), 1);
        assert!(empty.tabs.is_empty());
        assert!(empty.focused_window.is_none());
        root.profiles.remove(&b);
        model.update(1, root, &live);
        let next = model.snapshots().unwrap();
        let removed = &next.iter().find(|(scope, _)| *scope == b).unwrap().1;
        assert!(removed.windows.is_empty());
        assert!(removed.tabs.is_empty());
        assert!(next.iter().find(|(scope, _)| *scope == a).unwrap().1.tabs[0].active);
    }
    #[test]
    fn split_ids_survive_repeated_observations_and_do_not_alias_other_model_namespaces() {
        let mut model = Model::default();
        let first = model.split_id(1, 1);
        assert_eq!(model.split_id(1, 1), first);
        assert_ne!(model.split_id(2, 1), first);
        assert_ne!(model.split_id(1, 2), first);
    }
    #[test]
    fn mirrored_sidebars_do_not_duplicate_tabs_and_renderer_handoffs_keep_the_model_key() {
        let scope = scope();
        let mut model = Model::default();
        let live = BTreeSet::from([1, 2]);
        model.update(
            1,
            window(
                1,
                scope,
                vec![(scope, tab("session:1", Some(17), true))],
                true,
            ),
            &live,
        );
        model.update(
            2,
            window(
                2,
                scope,
                vec![(scope, tab("session:1", None, false))],
                false,
            ),
            &live,
        );
        let first = model.snapshots().unwrap().remove(0).1;
        assert_eq!(first.tabs.len(), 1);
        assert_eq!(first.tabs[0].window, "window:1");
        model.update(
            2,
            window(
                2,
                scope,
                vec![(scope, tab("session:1", Some(17), true))],
                true,
            ),
            &live,
        );
        let moved = model.snapshots().unwrap().remove(0).1;
        assert_eq!(moved.tabs.len(), 1);
        assert_eq!(moved.tabs[0].key, first.tabs[0].key);
        assert_eq!(moved.tabs[0].window, "window:2");
        assert_eq!(moved.focused_window.as_deref(), Some("window:2"));
    }
    #[test]
    fn profile_changes_do_not_focus_another_profiles_window_or_forget_its_selected_tab() {
        let mut browser = browser_core::Browser::new();
        let a = ProfileScope(browser.active_user_profile_id(), false);
        let b = ProfileScope(browser.create_user_profile("Other").unwrap(), false);
        let live = BTreeSet::from([1]);
        let mut model = Model::default();
        model.update(
            1,
            window(
                1,
                a,
                vec![
                    (a, tab("session:a", Some(17), true)),
                    (b, tab("session:b", None, false)),
                ],
                true,
            ),
            &live,
        );
        model.snapshots().unwrap();
        model.update(
            1,
            window(
                1,
                b,
                vec![
                    (a, tab("session:a", Some(17), false)),
                    (b, tab("session:b", Some(23), true)),
                ],
                true,
            ),
            &live,
        );
        let snapshots = model.snapshots().unwrap();
        let old = &snapshots.iter().find(|(scope, _)| *scope == a).unwrap().1;
        assert!(old.focused_window.is_none());
        assert_eq!(old.last_focused_window.as_deref(), Some("window:1"));
        assert_eq!(old.tabs.len(), 1);
        assert!(old.tabs[0].active);
        assert_eq!(old.tabs[0].key, "session:a");
        assert_eq!(
            snapshots
                .iter()
                .find(|(scope, _)| *scope == b)
                .unwrap()
                .1
                .tabs[0]
                .key,
            "session:b"
        );
    }
    #[test]
    fn closing_a_root_rehomes_retained_inventory_without_aliasing_a_blank_windows_tab_ids() {
        let scope = scope();
        let live = BTreeSet::from([1, 2]);
        let mut model = Model::default();
        let tabs = vec![
            (scope, tab("normal:1", None, false)),
            (scope, tab("blank:1", None, false)),
        ];
        model.update(1, window(1, scope, tabs.clone(), true), &live);
        model.update(2, window(2, scope, tabs.clone(), false), &live);
        assert_eq!(model.snapshots().unwrap()[0].1.tabs.len(), 2);
        model.update(2, window(2, scope, tabs, false), &BTreeSet::from([2]));
        let snapshot = model.snapshots().unwrap().remove(0).1;
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.tabs.len(), 2);
        assert!(snapshot.tabs.iter().all(|tab| tab.window == "window:2"));
        assert_ne!(snapshot.tabs[0].key, snapshot.tabs[1].key);
    }
}
