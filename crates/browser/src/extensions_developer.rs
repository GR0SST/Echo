//! Profile-owned extension developer operations and their GPUI controls.
use crate::{
    extensions::{extension_outline_button, Extension},
    window::BrowserWindow,
};
use browser_core::UserProfileId;
use gpui::{
    actions, div, point, prelude::*, px, size, AnyElement, App, Bounds, Context, FocusHandle,
    Focusable, KeyBinding, Window, WindowBounds, WindowHandle, WindowOptions,
};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

actions!(
    extensions_developer,
    [CloseWorker, DismissPack, ConfirmPack]
);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-w", CloseWorker, Some("ExtensionWorkerInspector")),
        KeyBinding::new("escape", DismissPack, Some("ExtensionPack")),
        KeyBinding::new("enter", ConfirmPack, Some("ExtensionPack")),
    ]);
}
#[derive(Clone, Copy, Default)]
struct NativeMode {
    enabled: bool,
    managed: bool,
    can_load: bool,
}
impl NativeMode {
    fn read(value: &Value) -> Option<Self> {
        Some(Self {
            enabled: value.get("enabled")?.as_bool()?,
            managed: value.get("managed")?.as_bool()?,
            can_load: value.get("can_load_unpacked")?.as_bool()?,
        })
    }
}
enum Operation {
    Mode,
    Load,
    Update,
    Pack { dialog: u64 },
    Info { id: String, revision: u64 },
}
impl Operation {
    fn name(&self) -> &'static str {
        match self {
            Self::Mode => "mode",
            Self::Load => "load",
            Self::Update => "update",
            Self::Pack { .. } => "pack",
            Self::Info { .. } => "info",
        }
    }
}
struct Pending {
    profile: UserProfileId,
    generation: u64,
    operation: Operation,
}
#[derive(Default)]
pub(crate) struct State {
    generation: u64,
    next_request: u64,
    synced: bool,
    native: Option<NativeMode>,
    pending: BTreeMap<u64, Pending>,
    revisions: BTreeMap<String, u64>,
    infos: BTreeMap<String, (u64, Value)>,
    picker_pending: bool,
    pack: Option<PackDialog>,
    next_pack: u64,
    toast: Option<(String, Instant)>,
    worker_generation: u64,
    worker: Option<WorkerWindow>,
}
struct PackDialog {
    profile: UserProfileId,
    generation: u64,
    service_generation: u64,
    root: gpui::Entity<crate::text_input::TextInput>,
    key: gpui::Entity<crate::text_input::TextInput>,
    _subscriptions: Vec<gpui::Subscription>,
    busy: bool,
    result: Option<Value>,
    submitted: Option<(String, String)>,
}
struct WorkerWindow {
    profile: UserProfileId,
    window: WindowHandle<WorkerFrame>,
    closed: Rc<Cell<bool>>,
    lease: Option<Box<dyn engine::ExtensionWorkerInspector>>,
    size: gpui::Size<gpui::Pixels>,
    phase: Option<engine::InspectorPhase>,
}
struct WorkerFrame {
    browser: gpui::WeakEntity<BrowserWindow>,
    focus: FocusHandle,
}
impl gpui::Render for WorkerFrame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let close = self.browser.clone();
        let close_tab = self.browser.clone();
        let close_window = self.browser.clone();
        div()
            .key_context("ExtensionWorkerInspector")
            .track_focus(&self.focus)
            .size_full()
            .bg(gpui::rgb(0x202124))
            .on_action(move |_: &CloseWorker, _, cx| {
                let _ = close.update(cx, |this, cx| this.close_extension_worker(cx));
            })
            .on_action(move |_: &crate::window::CloseTab, _, cx| {
                let _ = close_tab.update(cx, |this, cx| this.close_extension_worker(cx));
            })
            .on_action(move |_: &crate::menus::CloseWindow, _, cx| {
                let _ = close_window.update(cx, |this, cx| this.close_extension_worker(cx));
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, window, _| window.activate_window()),
            )
    }
}
impl BrowserWindow {
    fn developer_capabilities(&self) -> engine::ExtensionDeveloperCapabilities {
        self.extensions
            .service
            .as_ref()
            .map(|service| service.developer_capabilities())
            .unwrap_or_default()
    }
    pub(crate) fn extension_developer_mode(&self) -> bool {
        self.extensions
            .developer
            .native
            .map(|state| state.enabled)
            .unwrap_or(self.browser.active_user_profile().extension_developer_mode)
    }
    pub(crate) fn extension_developer_managed(&self) -> bool {
        self.extensions
            .developer
            .native
            .is_some_and(|state| state.managed)
    }
    #[cfg(test)]
    pub(crate) fn extension_pack_result(&self) -> Option<&Value> {
        self.extensions.developer.pack.as_ref()?.result.as_ref()
    }
    #[cfg(test)]
    pub(crate) fn extension_worker_window(&self) -> Option<gpui::AnyWindowHandle> {
        self.extensions
            .developer
            .worker
            .as_ref()
            .map(|worker| worker.window.into())
    }
    pub(crate) fn extension_worker_owns_focus(&self) -> bool {
        self.extensions.developer.worker.is_some()
    }
    pub(crate) fn reset_extension_developer(&mut self, profile_changed: bool, cx: &mut App) {
        if profile_changed {
            self.close_extension_worker(cx);
        }
        let state = &mut self.extensions.developer;
        state.generation = state.generation.wrapping_add(1);
        state.synced = false;
        state.native = None;
        state.pending.clear();
        state.infos.clear();
        state.revisions.clear();
        state.toast = None;
        state.pack = None;
    }
    fn submit_developer(
        &mut self,
        value: Value,
        operation: Operation,
        cx: &mut Context<Self>,
    ) -> bool {
        let profile = self.browser.active_user_profile_id();
        if self.browser.is_private()
            || self.browser.is_blank()
            || self.extensions.profile != Some(profile)
        {
            return false;
        }
        let Some(service) = self.extensions.service.clone() else {
            return false;
        };
        let state = &mut self.extensions.developer;
        let Some(next) = state
            .next_request
            .checked_add(1)
            .filter(|next| *next < (1 << 63))
        else {
            self.extensions.error = Some("Extension developer request IDs are exhausted".into());
            return false;
        };
        state.next_request = next;
        let request = (1 << 63) | next;
        state.pending.insert(
            request,
            Pending {
                profile,
                generation: state.generation,
                operation,
            },
        );
        if let Err(error) = service.developer_operation(&value, request) {
            state.pending.remove(&request);
            self.extensions.error = Some(error);
            cx.notify();
            return false;
        }
        true
    }
    pub(crate) fn extension_developer_catalog(&mut self, data: &Value) {
        if self.developer_capabilities().available() {
            self.extensions.developer.native = NativeMode::read(&data["developer"]);
        }
        for extension in &self.extensions.entries {
            let revision = self
                .extensions
                .developer
                .revisions
                .entry(extension.id.clone())
                .or_default();
            *revision = revision.wrapping_add(1);
        }
        self.extensions
            .developer
            .infos
            .retain(|id, _| self.extensions.entries.iter().any(|entry| &entry.id == id));
    }
    pub(crate) fn extension_developer_changed(&mut self, id: &str) {
        let revision = self
            .extensions
            .developer
            .revisions
            .entry(id.into())
            .or_default();
        *revision = revision.wrapping_add(1);
    }
    pub(crate) fn extension_developer_result(
        &mut self,
        request: u64,
        result: &Value,
        cx: &mut Context<Self>,
    ) -> bool {
        if result.get("developer_operation").is_none() {
            return false;
        }
        let Some(pending) = self.extensions.developer.pending.remove(&request) else {
            return true;
        };
        if pending.profile != self.browser.active_user_profile_id()
            || pending.generation != self.extensions.developer.generation
        {
            return true;
        }
        if result["developer_operation"].as_str() != Some(pending.operation.name()) {
            self.extensions.error =
                Some("The extension developer response did not match its request".into());
            cx.notify();
            return true;
        }
        let success = result["success"].as_bool() == Some(true);
        match pending.operation {
            Operation::Mode => {
                if let Some(mode) = NativeMode::read(result) {
                    self.extensions.developer.native = Some(mode);
                    if success
                        && self.browser.active_user_profile().extension_developer_mode
                            != mode.enabled
                    {
                        self.edit_profile(|browser| {
                            if let Some(profile) = browser.user_profile_mut(pending.profile) {
                                profile.extension_developer_mode = mode.enabled;
                            }
                        });
                    }
                }
            }
            Operation::Info { id, revision } => {
                if self
                    .extensions
                    .developer
                    .revisions
                    .get(&id)
                    .copied()
                    .unwrap_or_default()
                    != revision
                {
                    return true;
                }
                if !self.extensions.entries.iter().any(|entry| entry.id == id) {
                    return true;
                }
                self.extensions
                    .developer
                    .infos
                    .insert(id.clone(), (revision, Value::Null));
                if success
                    && result["info"]["id"].as_str() == Some(id.as_str())
                    && self.extensions.entries.iter().any(|entry| {
                        entry.id == id
                            && result["info"]["version"].as_str() == Some(entry.version.as_str())
                    })
                {
                    self.extensions
                        .developer
                        .infos
                        .insert(id, (revision, result["info"].clone()));
                }
            }
            Operation::Update => {
                self.extensions.developer.toast =
                    success.then(|| ("Extensions updated".into(), Instant::now()));
            }
            Operation::Pack { dialog } => {
                if let Some(pack) = self
                    .extensions
                    .developer
                    .pack
                    .as_mut()
                    .filter(|pack| pack.generation == dialog)
                {
                    pack.busy = false;
                    pack.result = Some(result.clone());
                    // The form fields leave the tree while the result is shown.
                    // Move focus to the retained sheet so Return/Escape still work.
                    if let Some(overlay) = self.extensions.overlay {
                        let focus = self.extensions.prompt_focus.clone();
                        cx.defer(move |cx| {
                            let _ = overlay.update(cx, |_, window, _| focus.focus(window));
                        });
                    }
                }
            }
            Operation::Load => {}
        }
        if !success && result["developer_operation"] != "pack" {
            self.extensions.error = Some(
                result["error"]
                    .as_str()
                    .unwrap_or("Extension developer operation failed")
                    .into(),
            );
        }
        cx.notify();
        true
    }
    pub(crate) fn toggle_extension_developer_mode(
        &mut self,
        profile: UserProfileId,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.browser.active_user_profile_id() != profile || self.extension_developer_managed() {
            return;
        }
        if self
            .extensions
            .developer
            .pending
            .values()
            .any(|pending| matches!(pending.operation, Operation::Mode))
        {
            return;
        }
        if !self.developer_capabilities().available() {
            self.edit_profile(|browser| {
                if let Some(profile) = browser.user_profile_mut(profile) {
                    profile.extension_developer_mode = enabled;
                }
            });
        } else {
            self.submit_developer(
                json!({"operation":"mode","enabled":enabled}),
                Operation::Mode,
                cx,
            );
        }
        cx.notify();
    }
    pub(crate) fn poll_extension_developer(&mut self, cx: &mut Context<Self>) {
        let profile = self.browser.active_user_profile_id();
        if self.browser.is_private()
            || self.browser.is_blank()
            || self
                .extensions
                .developer
                .worker
                .as_ref()
                .is_some_and(|worker| worker.profile != profile)
        {
            self.close_extension_worker(cx);
        }
        if self
            .extensions
            .developer
            .worker
            .as_ref()
            .is_some_and(|worker| worker.closed.get())
        {
            self.close_extension_worker(cx);
        }
        let mut worker_events = Vec::new();
        if let Some(worker) = &mut self.extensions.developer.worker {
            let extent = worker
                .window
                .update(cx, |_, window, _| window.viewport_size())
                .ok();
            if let Some(lease) = &mut worker.lease {
                if let Some(extent) = extent.filter(|extent| *extent != worker.size) {
                    worker.size = extent;
                    lease.resize(engine::Bounds {
                        x: 0.,
                        y: 0.,
                        width: f64::from(extent.width).max(1.),
                        height: f64::from(extent.height).max(1.),
                    });
                }
                worker_events = lease.take_events();
                if let Some(status) = lease.status() {
                    if worker.phase != Some(status.phase) {
                        worker.phase = Some(status.phase);
                        eprintln!(
                            "[extension-worker] profile={:?} phase={:?}",
                            profile, status.phase
                        );
                    }
                }
            }
        }
        for event in worker_events {
            match event {
                engine::ExtensionWorkerEvent::CloseRequested => self.close_extension_worker(cx),
                engine::ExtensionWorkerEvent::Error(error) => {
                    self.extensions.error = Some(error);
                    self.close_extension_worker(cx);
                }
                engine::ExtensionWorkerEvent::Shortcut(shortcut) => {
                    if shortcut.cmd
                        && !shortcut.ctrl
                        && !shortcut.alt
                        && !shortcut.shift
                        && shortcut.key == "w"
                    {
                        self.close_extension_worker(cx);
                    }
                }
            }
        }
        if self.browser.is_private()
            || self.browser.is_blank()
            || self.extensions.profile != Some(profile)
        {
            return;
        }
        let caps = self.developer_capabilities();
        if caps.available() && !self.extensions.developer.synced {
            let enabled = self.browser.active_user_profile().extension_developer_mode;
            if self.submit_developer(
                json!({"operation":"mode","enabled":enabled}),
                Operation::Mode,
                cx,
            ) {
                self.extensions.developer.synced = true;
            }
        }
        let details = self
            .browser
            .active_tab()
            .and_then(|tab| tab.url.strip_prefix("browser://extensions?id="))
            .map(str::to_owned);
        let ids = if let Some(id) = details {
            vec![id]
        } else if self.extensions_is_open() && self.extension_developer_mode() {
            self.extensions
                .entries
                .iter()
                .map(|entry| entry.id.clone())
                .collect()
        } else {
            Vec::new()
        };
        if caps.details {
            for id in ids {
                if self
                    .extensions
                    .developer
                    .pending
                    .values()
                    .filter(|pending| matches!(pending.operation, Operation::Info { .. }))
                    .count()
                    >= 4
                {
                    break;
                }
                let revision = self
                    .extensions
                    .developer
                    .revisions
                    .get(&id)
                    .copied()
                    .unwrap_or_default();
                if self.extensions.developer.infos.get(&id).is_some_and(|(cached,_)| *cached == revision) ||
                    self.extensions.developer.pending.values().any(|pending| matches!(&pending.operation, Operation::Info { id: pending,.. } if pending == &id)) { continue; }
                self.submit_developer(
                    json!({"operation":"info","id":id}),
                    Operation::Info { id, revision },
                    cx,
                );
            }
        }
        if !self
            .extensions
            .developer
            .pending
            .values()
            .any(|pending| matches!(pending.operation, Operation::Update))
            && self
                .extensions
                .developer
                .toast
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(5))
        {
            self.extensions.developer.toast = None;
            cx.notify();
        }
    }
    pub(crate) fn close_extension_worker(&mut self, cx: &mut App) {
        self.extensions.developer.worker_generation =
            self.extensions.developer.worker_generation.wrapping_add(1);
        if let Some(mut worker) = self.extensions.developer.worker.take() {
            let pending = self.extensions.pending_retirements.clone();
            pending.set(pending.get() + 1);
            cx.defer(move |cx| {
                drop(worker.lease.take());
                let _ = worker
                    .window
                    .update(cx, |_, window, _| window.remove_window());
                pending.set(pending.get() - 1);
            });
        }
    }
    pub(crate) fn open_extension_worker(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.extension_developer_mode() || !self.developer_capabilities().inspect_worker {
            return;
        }
        let Some(service) = self.extensions.service.clone() else {
            return;
        };
        let profile = self.browser.active_user_profile_id();
        if self.browser.is_private()
            || !self
                .extensions
                .entries
                .iter()
                .any(|entry| entry.id == id && entry.enabled)
        {
            return;
        }
        self.close_extension_worker(cx);
        let generation = self.extensions.developer.worker_generation;
        let root = cx.entity().downgrade();
        let main = self.main_window;
        let id = id.to_owned();
        cx.defer(move |cx| {
            let valid = |cx: &App| {
                !crate::quit::is_quitting()
                    && root
                        .read_with(cx, |this, _| {
                            this.browser.active_user_profile_id() == profile
                                && !this.browser.is_private()
                                && this.extensions.developer.worker_generation == generation
                        })
                        .unwrap_or(false)
            };
            if !valid(cx) {
                return;
            }
            let Ok(origin) = main.update(cx, |_, window, _| window.bounds().origin) else {
                return;
            };
            let closed = Rc::new(Cell::new(false));
            let flag = closed.clone();
            let weak = root.clone();
            let native = Rc::new(Cell::new(0usize));
            let parent = native.clone();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    origin + point(px(40.), px(40.)),
                    size(px(900.), px(600.)),
                ))),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Developer Tools".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let Ok(window) = cx.open_window(options, |window, cx| {
                window.on_window_should_close(cx, move |_, _| {
                    flag.set(true);
                    false
                });
                if let Some(view) = crate::native::view_of(window) {
                    parent.set(view as usize);
                }
                let focus = cx.focus_handle();
                focus.focus(window);
                cx.new(|_| WorkerFrame {
                    browser: weak,
                    focus,
                })
            }) else {
                return;
            };
            let extent = window
                .update(cx, |_, window, _| window.viewport_size())
                .unwrap_or(size(px(900.), px(600.)));
            let result = unsafe {
                service.open_worker_inspector(
                    &id,
                    native.get() as *mut std::ffi::c_void,
                    engine::Bounds {
                        x: 0.,
                        y: 0.,
                        width: f64::from(extent.width),
                        height: f64::from(extent.height),
                    },
                )
            };
            match result {
                Ok(lease) if valid(cx) => {
                    let _ = root.update(cx, |this, cx| {
                        this.extensions.developer.worker = Some(WorkerWindow {
                            profile,
                            window,
                            closed,
                            lease: Some(lease),
                            size: extent,
                            phase: None,
                        });
                        cx.notify();
                    });
                }
                result => {
                    if let Err(error) = result {
                        let _ = root.update(cx, |this, cx| {
                            this.extensions.error = Some(error);
                            cx.notify();
                        });
                    }
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                }
            }
        });
    }
    pub(crate) fn render_extension_worker_link(
        &self,
        extension: &Extension,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((_, info)) = self.extensions.developer.infos.get(&extension.id) else {
            return div().into_any_element();
        };
        let worker = info["views"].as_array().and_then(|views| {
            views
                .iter()
                .find(|view| view["type"].as_str() == Some("EXTENSION_SERVICE_WORKER_BACKGROUND"))
        });
        let Some(worker) = worker else {
            return div().into_any_element();
        };
        let id = extension.id.clone();
        let selector_id = id.clone();
        let enabled = extension.enabled && self.developer_capabilities().inspect_worker;
        div()
            .text_size(px(13.))
            .line_height(px(18.))
            .flex()
            .gap(px(4.))
            .child("Inspect views")
            .child(
                div()
                    .id(gpui::SharedString::from(format!("extension-worker-{id}")))
                    .debug_selector(move || format!("extension-worker-{selector_id}"))
                    .when(enabled, |link| {
                        link.cursor_pointer().text_color(gpui::rgb(0xa8c7fa))
                    })
                    .child(if worker["renderProcessId"].as_i64() == Some(-1) {
                        "service worker (Inactive)"
                    } else {
                        "service worker"
                    })
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.open_extension_worker(&id, cx)),
                    ),
            )
            .into_any_element()
    }
    pub(crate) fn render_extension_permissions(&self, id: &str) -> AnyElement {
        let Some((_, info)) = self.extensions.developer.infos.get(id) else {
            return div().into_any_element();
        };
        let permissions = &info["permissions"];
        let mut body = div()
            .border_t_1()
            .border_color(gpui::rgb(0x3c4043))
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .text_size(px(13.))
            .child("Permissions");
        if let Some(items) = permissions["simplePermissions"].as_array() {
            for permission in items {
                if let Some(message) = permission["message"].as_str() {
                    body = body.child(message.to_owned());
                }
                if let Some(details) = permission["submessages"].as_array() {
                    for detail in details.iter().filter_map(Value::as_str) {
                        body = body.child(div().pl(px(16.)).child(detail.to_owned()));
                    }
                }
            }
        }
        if let Some(hosts) = permissions["runtimeHostPermissions"].as_object() {
            body = body.child("Site access");
            if let Some(access) = hosts.get("hostAccess").and_then(Value::as_str) {
                body = body.child(match access {
                    "ON_CLICK" => "On click",
                    "ON_SPECIFIC_SITES" => "On specific sites",
                    "ON_ALL_SITES" => "On all sites",
                    _ => "Managed by the extension",
                });
            }
            if let Some(hosts) = hosts.get("hosts").and_then(Value::as_array) {
                for host in hosts {
                    if host["granted"] != true {
                        continue;
                    }
                    if let Some(name) = host["host"].as_str() {
                        body = body.child(name.to_owned());
                    }
                }
            }
        }
        body.into_any_element()
    }
    pub(crate) fn extension_pack_identity(&self) -> Option<(u64, UserProfileId)> {
        self.extensions
            .developer
            .pack
            .as_ref()
            .map(|pack| (pack.generation, pack.profile))
    }
    pub(crate) fn extension_pack_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.extensions
            .developer
            .pack
            .as_ref()
            .map(|pack| pack.root.read(cx).focus_handle(cx))
    }
    pub(crate) fn clear_extension_pack(&mut self) {
        self.extensions.developer.pack = None;
    }
    fn show_extension_pack(&mut self, cx: &mut Context<Self>) {
        if !self.extension_developer_mode()
            || !self.developer_capabilities().pack
            || self.browser.is_private()
        {
            return;
        }
        self.close_extension_page_surfaces(cx);
        let root = cx.new(|cx| crate::text_input::TextInput::new("", cx));
        let key = cx.new(|cx| crate::text_input::TextInput::new("", cx));
        let subscriptions = [&root, &key]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, _: &crate::text_input::Changed, cx| {
                    cx.notify()
                })
            })
            .collect();
        let Some(dialog) = self.extensions.developer.next_pack.checked_add(1) else {
            self.extensions.error = Some("Extension packing dialog IDs are exhausted".into());
            return;
        };
        self.extensions.developer.next_pack = dialog;
        self.extensions.developer.pack = Some(PackDialog {
            profile: self.browser.active_user_profile_id(),
            generation: dialog,
            service_generation: self.extensions.developer.generation,
            root,
            key,
            _subscriptions: subscriptions,
            busy: false,
            result: None,
            submitted: None,
        });
        self.show_extension_prompt(cx);
        cx.notify();
    }
    fn dismiss_extension_pack(&mut self, cx: &mut Context<Self>) {
        if let Some(pack) = &mut self.extensions.developer.pack {
            if pack
                .result
                .as_ref()
                .is_some_and(|result| result["success"] != true)
            {
                pack.result = None;
                if let Some(overlay) = self.extensions.overlay {
                    let focus = pack.root.read(cx).focus_handle(cx);
                    cx.defer(move |cx| {
                        let _ = overlay.update(cx, |_, window, _| focus.focus(window));
                    });
                }
                cx.notify();
                return;
            }
        }
        self.clear_extension_pack();
        if let Some(window) = self.extensions.overlay.take() {
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            });
        }
        cx.notify();
    }
    fn choose_extension_pack_path(
        &mut self,
        private_key: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.developer.picker_pending {
            return;
        }
        let Some(pack) = &self.extensions.developer.pack else {
            return;
        };
        if pack.busy || pack.result.is_some() {
            return;
        }
        let identity = (pack.generation, pack.profile);
        let input = if private_key {
            pack.key.clone()
        } else {
            pack.root.clone()
        };
        self.extensions.developer.picker_pending = true;
        let chosen = crate::native::extension_files::choose(
            if private_key {
                crate::native::extension_files::Kind::PrivateKey
            } else {
                crate::native::extension_files::Kind::Directory
            },
            window,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let selected = chosen.await;
            let _ = this.update(cx, |this, cx| {
                this.extensions.developer.picker_pending = false;
                if this.extension_pack_identity() != Some(identity) {
                    return;
                }
                if let Some(path) = selected {
                    if let Some(path) = path.to_str() {
                        input.update(cx, |input, cx| input.set_text(path.to_owned(), cx));
                    } else {
                        this.extensions.error = Some("The selected path is not valid UTF-8".into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn confirm_extension_pack(&mut self, cx: &mut Context<Self>) {
        let Some(pack) = &self.extensions.developer.pack else {
            return;
        };
        if pack.busy
            || pack.profile != self.browser.active_user_profile_id()
            || pack.service_generation != self.extensions.developer.generation
            || !self.extension_developer_mode()
        {
            return;
        }
        let overwrite = pack
            .result
            .as_ref()
            .is_some_and(|result| result["can_overwrite"] == true);
        if pack.result.is_some() && !overwrite {
            self.dismiss_extension_pack(cx);
            return;
        }
        let dialog = pack.generation;
        let paths = if overwrite {
            pack.submitted.clone()
        } else {
            Some((
                pack.root.read(cx).text().to_owned(),
                pack.key.read(cx).text().to_owned(),
            ))
        };
        let Some((root, key)) = paths.filter(|(root, _)| !root.is_empty()) else {
            return;
        };
        if self.submit_developer(
            json!({"operation":"pack","path":root,"key_path":key,"overwrite":overwrite}),
            Operation::Pack { dialog },
            cx,
        ) {
            if let Some(pack) = &mut self.extensions.developer.pack {
                pack.busy = true;
                pack.result = None;
                pack.submitted = Some((root, key));
            }
        }
        cx.notify();
    }
    pub(crate) fn render_extension_pack(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(pack) = &self.extensions.developer.pack else {
            return div().into_any_element();
        };
        let ready = !pack.busy && !pack.root.read(cx).text().is_empty();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .text_size(px(13.))
            .line_height(px(20.));
        if let Some(result) = &pack.result {
            body = body.child(
                result["message"]
                    .as_str()
                    .or_else(|| result["error"].as_str())
                    .unwrap_or("Extension packing completed")
                    .to_owned(),
            );
        } else {
            body = body.child("Select the root directory of the extension to pack. To update an extension, also select the private key file to reuse.");
            for (private_key, label, input) in [
                (false, "Extension root directory", &pack.root),
                (true, "Private key file (optional)", &pack.key),
            ] {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(div().text_size(px(12.)).line_height(px(16.)).child(label))
                        .child(
                            div()
                                .flex()
                                .gap(px(12.))
                                .items_center()
                                .child(
                                    div()
                                        .id(if private_key {
                                            "extension-pack-key"
                                        } else {
                                            "extension-pack-root"
                                        })
                                        .debug_selector(move || {
                                            if private_key {
                                                "extension-pack-key".into()
                                            } else {
                                                "extension-pack-root".into()
                                            }
                                        })
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(36.))
                                        .px(px(10.))
                                        .pt(px(7.))
                                        .bg(gpui::rgb(0x444746))
                                        .rounded_t(px(7.))
                                        .border_b_1()
                                        .border_color(gpui::rgb(
                                            if input.read(cx).focus_handle(cx).is_focused(window) {
                                                0xa8c7fa
                                            } else {
                                                0x9aa0a6
                                            },
                                        ))
                                        .child(input.clone()),
                                )
                                .child(
                                    extension_outline_button(
                                        if private_key {
                                            "extension-pack-key-browse"
                                        } else {
                                            "extension-pack-root-browse"
                                        },
                                        "Browse",
                                    )
                                    .w(px(80.))
                                    .when(pack.busy, |button| button.opacity(0.4))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.choose_extension_pack_path(private_key, window, cx)
                                        },
                                    )),
                                ),
                        ),
                );
            }
        }
        let confirm = match &pack.result {
            Some(result) if result["can_overwrite"] == true => "Proceed anyway",
            Some(_) => "OK",
            None => "Pack extension",
        };
        let enabled = ready || pack.result.is_some();
        let show_cancel = pack.result.is_none()
            || pack
                .result
                .as_ref()
                .is_some_and(|result| result["can_overwrite"] == true);
        let title = match pack
            .result
            .as_ref()
            .and_then(|result| result["status"].as_str())
        {
            Some("warning") => "Pack extension warning",
            Some("error") => "Pack extension error",
            _ => "Pack extension",
        };
        let inset = crate::pages::CONTENT_INSET;
        let page_left =
            self.sidebar_width() * self.sidebar_amount() + inset * (1. - self.sidebar_amount());
        div()
            .key_context("ExtensionPack")
            .track_focus(&self.extensions.prompt_focus)
            .on_action(cx.listener(|this, _: &DismissPack, _, cx| this.dismiss_extension_pack(cx)))
            .on_action(cx.listener(|this, _: &ConfirmPack, _, cx| this.confirm_extension_pack(cx)))
            .size_full()
            .pl(px(page_left))
            .pr(px(inset))
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000099))
            .child(
                div()
                    .id("extension-pack-dialog")
                    .debug_selector(|| "extension-pack-dialog".into())
                    .w(px(512.))
                    .px(px(20.))
                    .pt(px(20.))
                    .pb(px(16.))
                    .rounded(px(8.))
                    .shadow_lg()
                    .bg(gpui::rgb(0x292a2d))
                    .text_color(gpui::rgb(0xe8eaed))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(div().text_size(px(15.)).line_height(px(15.)).child(title))
                    .child(body)
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(4.))
                            .when(show_cancel, |row| {
                                row.child(
                                    extension_outline_button("extension-pack-cancel", "Cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.dismiss_extension_pack(cx)
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .id("extension-pack-confirm")
                                    .debug_selector(|| "extension-pack-confirm".into())
                                    .h(px(36.))
                                    .px(px(16.))
                                    .rounded(px(18.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(confirm)
                                    .bg(gpui::rgb(if enabled { 0xa8c7fa } else { 0x3f4145 }))
                                    .text_color(gpui::rgb(if enabled {
                                        0x062e6f
                                    } else {
                                        0x9aa0a6
                                    }))
                                    .when(enabled, |button| {
                                        button
                                            .cursor_pointer()
                                            .hover(|button| button.bg(gpui::rgb(0xb8d1fa)))
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_extension_pack(cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn choose_extension_unpacked(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.extensions.developer.picker_pending
            || !self.extension_developer_mode()
            || !self.developer_capabilities().load_unpacked
            || self
                .extensions
                .developer
                .native
                .is_some_and(|mode| !mode.can_load)
        {
            return;
        }
        let profile = self.browser.active_user_profile_id();
        let generation = self.extensions.developer.generation;
        self.extensions.developer.picker_pending = true;
        let chosen = crate::native::extension_files::choose(
            crate::native::extension_files::Kind::Directory,
            window,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let selected = chosen.await;
            let _ = this.update(cx, |this, cx| {
                this.extensions.developer.picker_pending = false;
                if profile != this.browser.active_user_profile_id()
                    || generation != this.extensions.developer.generation
                    || !this.extension_developer_mode()
                {
                    return;
                }
                if let Some(path) = selected {
                    if let Some(path) = path.to_str() {
                        this.submit_developer(
                            json!({"operation":"load","path":path}),
                            Operation::Load,
                            cx,
                        );
                    } else {
                        this.extensions.error =
                            Some("The extension directory path is not valid UTF-8".into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn update_extensions(&mut self, cx: &mut Context<Self>) {
        if !self.extension_developer_mode()
            || !self.developer_capabilities().update
            || self
                .extensions
                .developer
                .pending
                .values()
                .any(|pending| matches!(pending.operation, Operation::Update))
        {
            return;
        }
        if self.submit_developer(json!({"operation":"update"}), Operation::Update, cx) {
            self.extensions.developer.toast = Some(("Updating...".into(), Instant::now()));
        }
        cx.notify();
    }
    pub(crate) fn render_extension_developer_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let caps = self.developer_capabilities();
        if !self.extension_developer_mode() || !(caps.load_unpacked || caps.update || caps.pack) {
            return div().into_any_element();
        }
        let loading = self.extensions.developer.picker_pending
            || self
                .extensions
                .developer
                .pending
                .values()
                .any(|pending| matches!(pending.operation, Operation::Load));
        let updating = self
            .extensions
            .developer
            .pending
            .values()
            .any(|pending| matches!(pending.operation, Operation::Update));
        let can_load = !loading
            && self
                .extensions
                .developer
                .native
                .is_none_or(|mode| mode.can_load);
        div()
            .h(px(58.))
            .flex_none()
            .pl(px(12.))
            .pt(px(10.))
            .pb(px(12.))
            .flex()
            .gap(px(20.))
            .border_b_1()
            .border_color(gpui::rgb(0x3c4043))
            .when(caps.load_unpacked, |row| {
                row.child(
                    extension_outline_button("extension-load-unpacked", "Load unpacked")
                        .w(px(128.))
                        .when(!can_load, |button| button.opacity(0.4))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.choose_extension_unpacked(window, cx)
                        })),
                )
            })
            .when(caps.pack, |row| {
                row.child(
                    extension_outline_button("extension-pack", "Pack extension")
                        .w(px(128.))
                        .on_click(cx.listener(|this, _, _, cx| this.show_extension_pack(cx))),
                )
            })
            .when(caps.update, |row| {
                row.child(
                    extension_outline_button("extension-update", "Update")
                        .w(px(80.))
                        .when(updating, |button| button.opacity(0.4))
                        .on_click(cx.listener(|this, _, _, cx| this.update_extensions(cx))),
                )
            })
            .into_any_element()
    }
    pub(crate) fn render_extension_developer_toast(&self) -> AnyElement {
        let Some((message, _)) = &self.extensions.developer.toast else {
            return div().into_any_element();
        };
        div()
            .absolute()
            .left(px(24.))
            .bottom(px(24.))
            .px(px(24.))
            .h(px(48.))
            .rounded(px(4.))
            .bg(gpui::rgb(0x323232))
            .text_color(gpui::rgb(0xf1f3f4))
            .text_size(px(13.))
            .flex()
            .items_center()
            .child(message.clone())
            .into_any_element()
    }
}
