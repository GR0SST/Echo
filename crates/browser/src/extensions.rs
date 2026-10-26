//! GPUI extension surfaces, always bound to the profile and owner that supplied them.
use crate::{
    icons::{self, Icon},
    pages::PageIdentity,
    text_input::{Changed, TextInput},
    window::{BrowserWindow, Overlay, OverlayKind},
};
use browser_core::UserProfileId;
use engine::{ExtensionEvent, ExtensionService};
use gpui::{
    actions, div, point, prelude::*, px, size, AnyElement, App, Bounds, Context, Entity,
    FocusHandle, Focusable, KeyBinding, Pixels, Point, Subscription, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowOptions,
};
use serde::Deserialize;
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

pub(crate) const URL: &str = "browser://extensions";
pub(crate) const STORE: &str = "https://chromewebstore.google.com/category/extensions";
actions!(extensions, [Dismiss, ConfirmRemove]);
pub(crate) fn bind_keys(cx: &mut App) {
    crate::extensions_developer::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("ExtensionPopup")),
        KeyBinding::new("cmd-w", Dismiss, Some("ExtensionPopup")),
        KeyBinding::new("escape", Dismiss, Some("ExtensionRemove")),
        KeyBinding::new("enter", ConfirmRemove, Some("ExtensionRemove")),
        KeyBinding::new("escape", Dismiss, Some("ExtensionManager")),
    ]);
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct ExtensionCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub shortcut: String,
}
impl ExtensionCommand {
    pub(crate) fn display_description(&self, manifest: &serde_json::Value) -> String {
        if let Some(description) = manifest["commands"][&self.name]["description"]
            .as_str()
            .filter(|value| !value.is_empty() && !value.starts_with("__MSG_"))
        {
            return description.to_owned();
        }
        if self.name != "_execute_action"
            && (self.description.is_empty() || self.description == "Activate the extension")
        {
            return self.name.clone();
        }
        self.description.clone()
    }
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Extension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    #[serde(default)]
    pub manifest: serde_json::Value,
    #[serde(default)]
    pub commands: Vec<ExtensionCommand>,
    #[serde(default)]
    pub icon_base64: String,
    #[serde(default)]
    pub popup: String,
}
impl Extension {
    pub(crate) fn icon(&self, extent: f32) -> AnyElement {
        use base64::Engine;
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&self.icon_base64) {
            if !bytes.is_empty() && bytes.len() <= 1024 * 1024 {
                let format = if bytes.starts_with(b"<") {
                    gpui::ImageFormat::Svg
                } else {
                    gpui::ImageFormat::Png
                };
                return gpui::img(Arc::new(gpui::Image::from_bytes(format, bytes)))
                    .size(px(extent))
                    .into_any_element();
            }
        }
        extension_puzzle(extent, false)
    }
    pub(crate) fn key(&self) -> Option<[u8; 32]> {
        self.id.as_bytes().try_into().ok()
    }
}
#[derive(Clone, PartialEq, Eq)]
enum SheetIdentity {
    Install(u64, UserProfileId),
    Remove(String, UserProfileId),
    Pack(u64, UserProfileId),
}
struct Prompt {
    request: u64,
    data: serde_json::Value,
    profile: UserProfileId,
}
#[derive(Clone, Copy)]
pub(crate) enum PopupAnchor {
    Window,
    Trailing(Point<Pixels>),
}
#[derive(Clone, Copy)]
struct PopupPlacement {
    anchor: Point<Pixels>,
    trailing: bool,
    browser: Bounds<Pixels>,
    screen: Option<Bounds<Pixels>>,
}
impl PopupPlacement {
    fn bounds(self, width: i32, height: i32) -> Bounds<Pixels> {
        let size = size(px(width as f32), px(height as f32));
        let mut origin = self.anchor;
        if self.trailing {
            origin.x -= size.width;
        }
        origin.x = origin.x.clamp(
            self.browser.left(),
            (self.browser.right() - size.width).max(self.browser.left()),
        );
        if let Some(screen) = self.screen {
            origin.x = origin.x.clamp(
                screen.left(),
                (screen.right() - size.width).max(screen.left()),
            );
            origin.y = origin.y.clamp(
                screen.top(),
                (screen.bottom() - size.height).max(screen.top()),
            );
        }
        Bounds::new(origin, size)
    }
}
struct ActionPopup {
    header: i32,
    placement: PopupPlacement,
    window: WindowHandle<PopupFrame>,
    // Drop this before removing its native parent window.
    lease: Option<engine::ExtensionPopup>,
    view: Rc<Cell<usize>>,
    closed: Rc<Cell<bool>>,
    preferred: Rc<Cell<(i32, i32)>>,
    size: (i32, i32),
}
pub(crate) struct State {
    pub(crate) profile: Option<UserProfileId>,
    pub(crate) owner: Option<PageIdentity>,
    pub(crate) service: Option<Rc<dyn ExtensionService>>,
    pub(crate) entries: Vec<Extension>,
    pub(crate) error: Option<String>,
    pub(crate) picker_selection: Option<String>,
    pub(crate) developer: crate::extensions_developer::State,
    next_attempt: Instant,
    prompt: Option<Prompt>,
    pub(crate) prompt_focus: FocusHandle,
    pub(crate) confirm_remove: Option<String>,
    popup: Option<ActionPopup>,
    popup_generation: u64,
    pub(crate) pending_retirements: Rc<Cell<usize>>,
    pub(crate) overlay: Option<WindowHandle<Overlay>>,
    pub(crate) search: Entity<TextInput>,
    search_active: bool,
    manager_menu: bool,
    manager_focus: FocusHandle,
    _search_changed: Subscription,
}
impl State {
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let search = cx.new(|cx| TextInput::new("Search extensions", cx));
        let changed = cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify());
        Self {
            profile: None,
            owner: None,
            service: None,
            entries: vec![],
            error: None,
            picker_selection: None,
            developer: Default::default(),
            next_attempt: Instant::now(),
            prompt: None,
            prompt_focus: cx.focus_handle(),
            confirm_remove: None,
            popup: None,
            popup_generation: 0,
            pending_retirements: Rc::new(Cell::new(0)),
            overlay: None,
            search,
            search_active: false,
            manager_menu: false,
            manager_focus: cx.focus_handle(),
            _search_changed: changed,
        }
    }
}

struct PopupFrame {
    browser: WeakEntity<BrowserWindow>,
    focus: FocusHandle,
    header: i32,
    _activation: Option<Subscription>,
}
impl gpui::Render for PopupFrame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let dismiss = self.browser.clone();
        let close_tab = self.browser.clone();
        let close_window = self.browser.clone();
        div()
            .key_context("ExtensionPopup")
            .track_focus(&self.focus)
            .size_full()
            .bg(gpui::rgb(if self.header > 0 { 0x303436 } else { 0xffffff }))
            .on_action(move |_: &Dismiss, _, cx| {
                let _ = dismiss.update(cx, |view, cx| view.close_extension_popup(cx));
            })
            .on_action(move |_: &crate::window::CloseTab, _, cx| {
                let _ = close_tab.update(cx, |view, cx| view.close_extension_popup(cx));
            })
            .on_action(move |_: &crate::menus::CloseWindow, _, cx| {
                let _ = close_window.update(cx, |view, cx| view.close_extension_popup(cx));
            })
            .when(self.header > 0, |frame| {
                frame.child(
                    div()
                        .h(px(self.header as f32))
                        .w_full()
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                            crate::native::start_extension_window_drag(window, cx)
                        }),
                )
            })
    }
}

impl BrowserWindow {
    pub(crate) fn extension_surface_owns_focus(&self) -> bool {
        self.extensions.popup.is_some() || self.extensions.overlay.is_some() || self.extension_worker_owns_focus()
    }
    pub(crate) fn extension_popup_window_id(&self) -> Option<gpui::WindowId> {
        self.extensions
            .popup
            .as_ref()
            .map(|popup| popup.window.window_id())
    }
    pub(crate) fn extensions_is_open(&self) -> bool {
        self.browser
            .active_tab()
            .is_some_and(|tab| is_manager_url(&tab.url))
            && self.browser.active_split().is_none()
    }
    pub(crate) fn poll_extensions(&mut self, cx: &mut Context<Self>) {
        self.poll_extension_developer(cx);
        if self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        let profile = self.browser.active_user_profile_id();
        if self.extensions.profile != Some(profile) {
            self.close_extension_surfaces(cx);
            self.reset_extension_developer(true, cx);
            self.extensions.service = None;
            self.extensions.owner = None;
            self.extensions.entries.clear();
            self.extensions.error = None;
            self.extensions.search_active = false;
            self.extensions.manager_menu = false;
            self.extensions
                .search
                .update(cx, |input, cx| input.set_text("", cx));
            self.extensions.profile = Some(profile);
            self.extensions.next_attempt = Instant::now();
        }
        let active_identity = self.browser.active_tab().and_then(|tab| {
            self.pages
                .as_ref()
                .and_then(|pages| pages.borrow().identity_for_tab(tab.id))
        });
        if active_identity.is_some()
            && active_identity != self.extensions.owner
            && self.extensions.service.is_some()
        {
            self.close_extension_page_surfaces(cx);
            self.reset_extension_developer(false, cx);
            self.extensions.service = None;
            self.extensions.owner = None;
            self.extensions.next_attempt = Instant::now();
        }
        if self.extensions.service.is_none() && Instant::now() >= self.extensions.next_attempt {
            self.extensions.next_attempt = Instant::now() + Duration::from_secs(1);
            let result = self
                .pages
                .as_ref()
                .ok_or("The page engine is unavailable".to_owned())
                .and_then(|pages| {
                    pages
                        .borrow_mut()
                        .extension_service(profile, self.browser.active_tab().map(|t| t.id))
                });
            match result {
                Ok((owner, service)) => {
                    self.extensions.owner = owner;
                    self.extensions.service = Some(Rc::from(service));
                    self.extensions.error = None;
                }
                Err(error) => self.extensions.error = Some(error),
            }
        }
        let events = self
            .extensions
            .service
            .as_ref()
            .map(|s| s.take_events())
            .unwrap_or_default();
        let changed = !events.is_empty();
        for event in events {
            match event {
                ExtensionEvent::Catalog(data) => {
                    match serde_json::from_value::<Vec<Extension>>(data["extensions"].clone()) {
                        Ok(mut entries) => {
                            entries.retain(|e| {
                                e.id.len() == 32 && e.id.bytes().all(|b| (b'a'..=b'p').contains(&b))
                            });
                            entries.sort_by_key(|e| e.name.to_lowercase());
                            self.extensions.entries = entries;
                            self.extension_developer_catalog(&data);
                            if self.extensions.confirm_remove.as_ref().is_some_and(|id| {
                                !self.extensions.entries.iter().any(|entry| &entry.id == id)
                            }) {
                                self.cancel_extension_remove(cx);
                            }
                        }
                        Err(error) => self.extensions.error = Some(error.to_string()),
                    }
                }
                ExtensionEvent::InstallPrompt { request, prompt } => {
                    self.close_extension_surfaces(cx);
                    self.extensions.prompt = Some(Prompt {
                        request,
                        data: prompt,
                        profile,
                    });
                    self.show_extension_prompt(cx);
                }
                ExtensionEvent::OperationComplete { request, result } => {
                    if !self.extension_developer_result(request, &result, cx) && result["success"] == false {
                        self.extensions.error = Some(result["error"].as_str().unwrap_or("Extension operation failed").into());
                    }
                }
                ExtensionEvent::InstallComplete { result, .. } => {
                    if result["success"] == false {
                        self.extensions.error = Some(
                            result["error"]
                                .as_str()
                                .unwrap_or("Extension operation failed")
                                .to_owned(),
                        );
                    }
                }
                ExtensionEvent::ActionRequested { id } => {
                    self.invoke_extension(&id, profile, None, cx)
                }
                ExtensionEvent::OpenUrlRequested { url } => {
                    if url::Url::parse(&url).is_ok_and(|url| {
                        matches!(url.scheme(), "http" | "https" | "chrome-extension")
                            || url.as_str() == "about:blank"
                    }) {
                        self.close_extension_popup(cx);
                        self.edit_browser(|browser| {
                            browser.new_tab(&url, SystemTime::now());
                        });
                    }
                }
                ExtensionEvent::DeveloperChanged { id } => self.extension_developer_changed(&id),
                ExtensionEvent::Closed => {
                    self.close_extension_page_surfaces(cx);
                    self.reset_extension_developer(false, cx);
                    self.extensions.service = None;
                    self.extensions.owner = None;
                    self.extensions.entries.clear();
                }
                ExtensionEvent::InvalidPayload(error) => self.extensions.error = Some(error),
            }
        }
        if let Some(popup) = &mut self.extensions.popup {
            if popup.closed.get() {
                self.close_extension_popup(cx);
            } else {
                let preferred = popup.preferred.get();
                if preferred != popup.size {
                    let previous = popup
                        .placement
                        .bounds(popup.size.0, popup.size.1 + popup.header);
                    popup.size = preferred;
                    let (width, height) = preferred;
                    let _ = popup.window.update(cx, |_, window, cx| {
                        let moved = window.bounds().origin - previous.origin;
                        popup.placement.anchor += moved;
                        popup.placement.browser.origin += moved;
                        popup.placement.screen = window.display(cx).map(|display| display.bounds());
                        crate::native::set_popup_bounds(
                            window,
                            popup.placement.bounds(width, height + popup.header),
                        );
                    });
                    if let Some(lease) = &popup.lease {
                        let _ = lease.resize(width, height);
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
    }
    pub(crate) fn open_extensions(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        self.poll_extensions(cx);
        self.close_site_controls(cx);
        self.close_command_bar(cx);
        let existing = self
            .browser
            .active_user_profile()
            .favorites
            .iter()
            .chain(self.browser.space().pinned.iter())
            .chain(self.browser.space().today.iter())
            .find(|t| is_manager_url(&t.url))
            .map(|t| t.id);
        self.edit_browser(|browser| {
            if let Some(id) = existing {
                browser.activate_tab(id, SystemTime::now());
            } else {
                let id = browser.new_tab(URL, SystemTime::now());
                browser.set_title(id, "Extensions");
            }
        });
        cx.notify();
    }
    pub(crate) fn open_extension_store(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        self.close_site_controls(cx);
        self.close_command_bar(cx);
        self.edit_browser(|browser| {
            let id = browser.new_tab(STORE, SystemTime::now());
            browser.set_title(id, "Chrome Web Store");
        });
        cx.notify();
    }
    pub(crate) fn extension_enable(
        &mut self,
        id: &str,
        profile: UserProfileId,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.profile != Some(profile)
            || self.browser.active_user_profile_id() != profile
        {
            return;
        }
        let result = self
            .extensions
            .service
            .as_ref()
            .ok_or("Extensions are unavailable".into())
            .and_then(|s| s.set_enabled(id, enabled, 1));
        if let Err(error) = result {
            self.extensions.error = Some(error);
        }
        cx.notify();
    }
    pub(crate) fn extension_pin(
        &mut self,
        id: &str,
        profile: UserProfileId,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.profile != Some(profile)
            || self.browser.active_user_profile_id() != profile
        {
            return;
        }
        let id = id.to_owned();
        self.edit_profile(|browser| {
            if let Some(profile) = browser.user_profile_mut(profile) {
                if !profile.pinned_extensions.remove(&id) {
                    profile.pinned_extensions.insert(id);
                }
            }
        });
        cx.notify();
    }
    pub(crate) fn extension_remove(
        &mut self,
        id: &str,
        profile: UserProfileId,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.profile != Some(profile)
            || self.browser.active_user_profile_id() != profile
            || self.extensions.confirm_remove.as_deref() != Some(id)
        {
            return;
        }
        self.extensions.confirm_remove = None;
        if let Err(error) = self
            .extensions
            .service
            .as_ref()
            .ok_or("Extensions are unavailable".into())
            .and_then(|s| s.remove(id, 2))
        {
            self.extensions.error = Some(error);
        }
        cx.notify();
    }
    pub(crate) fn close_extension_popup(&mut self, cx: &mut App) {
        self.extensions.popup_generation = self.extensions.popup_generation.wrapping_add(1);
        if let Some(mut popup) = self.extensions.popup.take() {
            // Native close and activation can call into GPUI. Finish them after
            // releasing this browser entity, while retaining the native parent.
            let pending = self.extensions.pending_retirements.clone();
            pending.set(pending.get() + 1);
            cx.defer(move |cx| {
                drop(popup.lease.take());
                crate::native::detach_extension_view(popup.view.replace(0));
                let _ = popup
                    .window
                    .update(cx, |_, window, _| window.remove_window());
                pending.set(pending.get() - 1);
            });
        }
    }
    pub(crate) fn retire_extensions_for_quit(&mut self, cx: &mut App) -> Rc<Cell<usize>> {
        self.close_extension_surfaces(cx);
        self.extensions.service = None;
        self.extensions.pending_retirements.clone()
    }
    pub(crate) fn close_extension_surfaces(&mut self, cx: &mut App) {
        self.close_extension_worker(cx);
        self.close_extension_page_surfaces(cx);
    }
    pub(crate) fn close_extension_page_surfaces(&mut self, cx: &mut App) {
        self.close_extension_popup(cx);
        if let Some(prompt) = self.extensions.prompt.take() {
            if let Some(service) = &self.extensions.service {
                let _ = service.approve_install(prompt.request, false);
            }
        }
        if let Some(overlay) = self.extensions.overlay.take() {
            cx.defer(move |cx| {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.extensions.confirm_remove = None;
        self.clear_extension_pack();
    }
    pub(crate) fn invoke_extension(
        &mut self,
        id: &str,
        profile: UserProfileId,
        anchor: Option<PopupAnchor>,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.profile != Some(profile)
            || self.browser.active_user_profile_id() != profile
            || !self
                .extensions
                .entries
                .iter()
                .any(|entry| entry.id == id && entry.enabled)
        {
            return;
        }
        self.close_extension_popup(cx);
        let Some(service) = self.extensions.service.clone() else {
            return;
        };
        let generation = self.extensions.popup_generation;
        let root = cx.entity().downgrade();
        let main = self.main_window;
        let main_parent = self.native_view;
        let id = id.to_owned();
        self.close_site_controls_with_focus(crate::window::PopupFocus::OnActivation, cx);
        cx.defer(move |cx| {
            let valid = || {
                root.read_with(cx, |view, _| {
                    view.extensions.profile == Some(profile)
                        && view.browser.active_user_profile_id() == profile
                        && view.extensions.popup_generation == generation
                })
                .unwrap_or(false)
            };
            if crate::quit::is_quitting() || !valid() {
                return;
            }
            let Ok((bounds, screen)) = main.update(cx, |_, window, cx| {
                (window.bounds(), window.display(cx).map(|d| d.bounds()))
            }) else {
                return;
            };
            let titled = !matches!(anchor, Some(PopupAnchor::Trailing(_)));
            let header = if titled { 32 } else { 0 };
            let (at, trailing) = match anchor {
                Some(PopupAnchor::Trailing(at)) => (at, true),
                _ => (point(px(40.), px(10.)), false),
            };
            let placement = PopupPlacement {
                anchor: bounds.origin + at,
                trailing,
                browser: bounds,
                screen,
            };
            let closed = Rc::new(Cell::new(false));
            let preferred = Rc::new(Cell::new((320, 240)));
            let view = Rc::new(Cell::new(0));
            let parent = Rc::new(Cell::new(0usize));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(placement.bounds(320, 240 + header))),
                titlebar: titled.then(|| gpui::TitlebarOptions {
                    title: Some("".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(9.), px(9.))),
                    ..Default::default()
                }),
                is_movable: titled,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            };
            let host = parent.clone();
            let weak = root.clone();
            let flag = closed.clone();
            let Ok(handle) = cx.open_window(options, |window, cx| {
                let focus = cx.focus_handle();
                focus.focus(window);
                let closing = closed.clone();
                window.on_window_should_close(cx, move |_, _| {
                    closing.set(true);
                    false
                });
                if let Some(native) = crate::native::view_of(window) {
                    host.set(native as usize);
                    if let Some(main) = main_parent {
                        if titled {
                            crate::native::attach_extension_window(main, native);
                        } else {
                            crate::native::attach_overlay(main, native, false);
                        }
                    }
                }
                cx.new(|cx| {
                    let activation = (!titled).then(|| {
                        cx.observe_window_activation(window, move |_, window, _| {
                            if !window.is_window_active() {
                                flag.set(true);
                            }
                        })
                    });
                    PopupFrame {
                        browser: weak,
                        focus,
                        header,
                        _activation: activation,
                    }
                })
            }) else {
                return;
            };
            let (p, v, c, dimensions) = (
                parent.clone(),
                view.clone(),
                closed.clone(),
                preferred.clone(),
            );
            let result = unsafe {
                service.invoke_action(
                    &id,
                    Box::new(move |event| match event.kind {
                        1 => {
                            v.set(event.native_view);
                            crate::native::attach_extension_view(
                                p.get(),
                                event.native_view,
                                event.width,
                                event.height,
                            );
                        }
                        4 => {
                            crate::native::detach_extension_view(v.replace(0));
                            c.set(true);
                        }
                        5 => dimensions
                            .set((event.width.clamp(25, 800), event.height.clamp(25, 600))),
                        _ => {}
                    }),
                )
            };
            match result {
                Ok(lease) => {
                    let popup = ActionPopup {
                        header,
                        placement,
                        window: handle,
                        lease: Some(lease),
                        view,
                        closed,
                        preferred,
                        size: (320, 240),
                    };
                    let mut pending = Some(popup);
                    let _ = root.update(cx, |root, cx| {
                        if root.extensions.profile == Some(profile)
                            && root.extensions.popup_generation == generation
                        {
                            root.extensions.popup = pending.take();
                            cx.notify();
                        }
                    });
                    if let Some(mut popup) = pending {
                        drop(popup.lease.take());
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
                Err(error) => {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    let _ = root.update(cx, |root, cx| {
                        root.extensions.error = Some(error);
                        cx.notify();
                    });
                }
            }
        });
    }
    fn extension_sheet_identity(&self) -> Option<SheetIdentity> {
        if let Some((generation, profile)) = self.extension_pack_identity() { return Some(SheetIdentity::Pack(generation, profile)); }
        if let Some(prompt) = &self.extensions.prompt {
            return Some(SheetIdentity::Install(prompt.request, prompt.profile));
        }
        Some(SheetIdentity::Remove(
            self.extensions.confirm_remove.clone()?,
            self.extensions.profile?,
        ))
    }
    pub(crate) fn request_extension_remove(
        &mut self,
        id: &str,
        profile: UserProfileId,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.profile != Some(profile)
            || self.browser.active_user_profile_id() != profile
            || !self.extensions.entries.iter().any(|entry| entry.id == id)
        {
            return;
        }
        self.close_extension_surfaces(cx);
        self.extensions.confirm_remove = Some(id.to_owned());
        self.show_extension_prompt(cx);
        cx.notify();
    }
    fn cancel_extension_remove(&mut self, cx: &mut Context<Self>) {
        self.extensions.confirm_remove = None;
        if let Some(handle) = self.extensions.overlay.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
        cx.notify();
    }
    fn confirm_extension_remove(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.extensions.confirm_remove.clone() else {
            return;
        };
        let Some(profile) = self.extensions.profile else {
            return;
        };
        self.extension_remove(&id, profile, cx);
        self.cancel_extension_remove(cx);
    }
    fn render_extension_remove(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(extension) = self
            .extensions
            .confirm_remove
            .as_ref()
            .and_then(|id| self.extensions.entries.iter().find(|entry| &entry.id == id))
        else {
            return div().into_any_element();
        };
        div()
            .key_context("ExtensionRemove")
            .track_focus(&self.extensions.prompt_focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.cancel_extension_remove(cx)))
            .on_action(
                cx.listener(|this, _: &ConfirmRemove, _, cx| this.confirm_extension_remove(cx)),
            )
            .size_full()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(70.))
            .bg(gpui::rgba(0x00000018))
            .child(
                div()
                    .id("extension-remove-sheet")
                    .debug_selector(|| "extension-remove-sheet".into())
                    .w(px(450.))
                    .min_h(px(273.))
                    .rounded(px(14.))
                    .overflow_hidden()
                    .bg(gpui::rgb(0x151c2f))
                    .text_color(gpui::white())
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px(px(38.))
                            .pt(px(38.))
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(
                                div()
                                    .size(px(62.))
                                    .rounded(px(15.))
                                    .bg(gpui::rgb(0x3b3e55))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(extension_puzzle(32., true)),
                            )
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .line_height(px(24.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(format!(
                                        "Remove “{}” extension\nfrom Echo?",
                                        extension.name
                                    )),
                            ),
                    )
                    .child(div().flex_1().min_h(px(45.)))
                    .child(
                        div()
                            .px(px(28.))
                            .pb(px(28.))
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                extension_sheet_button("extension-remove-cancel", "Cancel", false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_extension_remove(cx)
                                    })),
                            )
                            .child(
                                extension_sheet_button(
                                    "extension-remove-confirm",
                                    "Remove Extension",
                                    true,
                                )
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.confirm_extension_remove(cx)),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(crate) fn show_extension_prompt(&mut self, cx: &mut Context<Self>) {
        if self.extensions.overlay.is_some() {
            return;
        }
        let Some(identity) = self.extension_sheet_identity() else {
            return;
        };
        let root = cx.entity().downgrade();
        let main = self.main_window;
        let parent = self.native_view;
        let focus = self.extensions.prompt_focus.clone();
        cx.defer(move |cx| {
            if crate::quit::is_quitting()
                || !root
                    .read_with(cx, |view, _| {
                        view.extension_sheet_identity().as_ref() == Some(&identity)
                            && view.browser.active_user_profile_id()
                                == match &identity {
                                    SheetIdentity::Install(_, profile)
                                    | SheetIdentity::Remove(_, profile)
                                    | SheetIdentity::Pack(_, profile) => *profile,
                                }
                    })
                    .unwrap_or(false)
            {
                return;
            }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else {
                return;
            };
            let weak = root.clone();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let pack_focus = root.read_with(cx, |view,cx| view.extension_pack_focus(cx)).ok().flatten();
            if let Ok(handle) = cx.open_window(options, |window, cx| {
                pack_focus.as_ref().unwrap_or(&focus).focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| Overlay::new(weak, OverlayKind::ExtensionPrompt, window, cx))
            }) {
                let _ = root.update(cx, |view, cx| {
                    view.extensions.overlay = Some(handle);
                    cx.notify();
                });
            }
        });
    }
    pub(crate) fn decide_extension_install(&mut self, accepted: bool, cx: &mut Context<Self>) {
        let Some(prompt) = self.extensions.prompt.take() else {
            return;
        };
        if self.extensions.profile != Some(prompt.profile)
            || self.browser.active_user_profile_id() != prompt.profile
        {
            return;
        }
        if let Some(service) = &self.extensions.service {
            if let Err(error) = service.approve_install(prompt.request, accepted) {
                self.extensions.error = Some(error);
            }
        }
        if let Some(handle) = self.extensions.overlay.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
        cx.notify();
    }
    pub(crate) fn render_extension_prompt(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if self.extension_pack_identity().is_some() { return self.render_extension_pack(window, cx); }
        if self.extensions.confirm_remove.is_some() {
            return self.render_extension_remove(cx);
        }
        let Some(prompt) = &self.extensions.prompt else {
            return div().into_any_element();
        };
        let data = &prompt.data;
        div()
            .key_context("ExtensionPopup")
            .track_focus(&self.extensions.prompt_focus)
            .on_action(
                cx.listener(|this, _: &Dismiss, _, cx| this.decide_extension_install(false, cx)),
            )
            .size_full()
            .flex()
            .items_start()
            .justify_center()
            .pt(px(70.))
            .bg(gpui::hsla(0., 0., 0., 0.25))
            .child(
                div()
                    .w(px(430.))
                    .p(px(22.))
                    .rounded(px(12.))
                    .bg(gpui::rgb(0x28292b))
                    .text_color(gpui::white())
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(
                                data["title"]
                                    .as_str()
                                    .unwrap_or("Add extension?")
                                    .to_owned(),
                            ),
                    )
                    .child(data["heading"].as_str().unwrap_or_default().to_owned())
                    .children(data["permissions"].as_array().into_iter().flatten().map(
                        |permission| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(
                                    permission["message"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_owned(),
                                )
                                .child(
                                    div().text_size(px(12.)).child(
                                        permission["details"]
                                            .as_str()
                                            .unwrap_or_default()
                                            .to_owned(),
                                    ),
                                )
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(12.))
                            .child(
                                button(
                                    "extension-install-cancel",
                                    data["cancel"].as_str().unwrap_or("Cancel"),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| this.decide_extension_install(false, cx),
                                )),
                            )
                            .child(
                                button(
                                    "extension-install-confirm",
                                    data["accept"].as_str().unwrap_or("Add extension"),
                                )
                                .bg(gpui::rgb(0x4b82df))
                                .on_click(cx.listener(
                                    |this, _, _, cx| this.decide_extension_install(true, cx),
                                )),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn active_extensions(&self) -> impl Iterator<Item = &Extension> {
        let current = self.extensions.profile == Some(self.browser.active_user_profile_id())
            && !self.browser.is_private();
        self.extensions.entries.iter().filter(move |_| current)
    }
    pub(crate) fn render_pinned_extensions(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.browser.active_user_profile_id();
        let pins = &self.browser.active_user_profile().pinned_extensions;
        div()
            .flex_none()
            .flex()
            .items_center()
            .children(
                self.active_extensions()
                    .filter(|entry| entry.enabled && pins.contains(&entry.id))
                    .map(|extension| {
                        let id = extension.id.clone();
                        let selector = format!("toolbar-extension-{id}");
                        let label = extension.name.clone();
                        let anchor = Rc::new(Cell::new(None::<Bounds<Pixels>>));
                        let painted = anchor.clone();
                        div()
                            .id(gpui::SharedString::from(selector.clone()))
                            .debug_selector(move || selector.clone())
                            .w(px(32.))
                            .h(px(32.))
                            .flex_none()
                            .relative()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|item| item.bg(gpui::hsla(0., 0., 0.5, 0.12)))
                            .tooltip(move |_, cx| {
                                cx.new(|_| ExtensionTooltip(label.clone())).into()
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invoke_extension(
                                    &id,
                                    profile,
                                    anchor.get().map(|bounds| {
                                        PopupAnchor::Trailing(point(
                                            bounds.right(),
                                            bounds.bottom() + px(8.),
                                        ))
                                    }),
                                    cx,
                                );
                            }))
                            .child(extension.icon(16.))
                            .child(
                                gpui::canvas(
                                    |_, _, _| (),
                                    move |bounds, _, _, _| painted.set(Some(bounds)),
                                )
                                .absolute()
                                .inset_0(),
                            )
                    }),
            )
            .into_any_element()
    }
    pub(crate) fn open_extensions_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.site_controls.is_some() {
            self.close_site_controls(cx);
            return;
        }
        self.poll_extensions(cx);
        self.toggle_site_controls(window, cx);
        if self.site_controls.is_some() {
            self.extensions.picker_selection =
                Some(self.extension_picker_ids().into_iter().next().unwrap());
            cx.notify();
        }
    }
    fn extension_picker_ids(&self) -> Vec<String> {
        self.active_extensions()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.id.clone())
            .chain(std::iter::once("+".into()))
            .collect()
    }
    pub(crate) fn move_extension_picker(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(selected) = &self.extensions.picker_selection else {
            return;
        };
        let ids = self.extension_picker_ids();
        let index = ids.iter().position(|id| id == selected).unwrap_or(0);
        let next = index as isize + delta;
        if next >= 0 && (next as usize) < ids.len() {
            self.extensions.picker_selection = Some(ids[next as usize].clone());
            cx.notify();
        }
    }
    pub(crate) fn confirm_extension_picker(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.extensions.picker_selection.clone() else {
            return;
        };
        if selected == "+" {
            self.open_extension_store(cx);
        } else if self
            .active_extensions()
            .any(|entry| entry.enabled && entry.id == selected)
        {
            self.invoke_extension(
                &selected,
                self.browser.active_user_profile_id(),
                Some(PopupAnchor::Window),
                cx,
            );
        }
    }
    pub(crate) fn render_extension_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.browser.active_user_profile_id();
        let entries = self
            .active_extensions()
            .filter(|e| e.enabled)
            .cloned()
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .children(entries.into_iter().map(|extension| {
                        let id = extension.id.clone();
                        let menu_id = id.clone();
                        let action_id = id.clone();
                        let label = extension.name.clone();
                        let selected =
                            self.extensions.picker_selection.as_deref() == Some(id.as_str());
                        div()
                            .id(gpui::SharedString::from(format!("site-extension-{id}")))
                            .debug_selector(move || format!("site-extension-{id}"))
                            .border_2()
                            .border_color(if selected {
                                gpui::white()
                            } else {
                                gpui::transparent_black()
                            })
                            .w(px(48.))
                            .h(px(32.))
                            .rounded(px(6.))
                            .bg(gpui::hsla(0., 0., 1., 0.10))
                            .hover(|s| s.bg(gpui::hsla(0., 0., 1., 0.18)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .relative()
                            .tooltip(move |_, cx| {
                                cx.new(|_| ExtensionTooltip(label.clone())).into()
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let at = Some(PopupAnchor::Window);
                                this.invoke_extension(&action_id, profile, at, cx);
                            }))
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        if let Ok(id) = menu_id.as_bytes().try_into() {
                                            this.open_sidebar_menu(
                                                crate::tab_menu::MenuTarget::Extension(id, profile),
                                                event.position,
                                                window,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                            .child(extension.icon(16.))
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .id("site-add-extension")
                            .debug_selector(|| "site-add-extension".into())
                            .border_2()
                            .border_color(
                                if self.extensions.picker_selection.as_deref() == Some("+") {
                                    gpui::white()
                                } else {
                                    gpui::transparent_black()
                                },
                            )
                            .w(px(48.))
                            .h(px(32.))
                            .rounded(px(6.))
                            .bg(gpui::hsla(0., 0., 1., 0.10))
                            .hover(|s| s.bg(gpui::hsla(0., 0., 1., 0.18)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::control(Icon::Plus))
                            .on_click(cx.listener(|this, _, _, cx| this.open_extension_store(cx))),
                    ),
            )
            .into_any_element()
    }
    pub(crate) fn open_extension_details(
        &mut self,
        id: &str,
        profile: UserProfileId,
        cx: &mut Context<Self>,
    ) {
        if self.extensions.profile != Some(profile)
            || self.browser.active_user_profile_id() != profile
        {
            return;
        }
        let Some(extension) = self
            .active_extensions()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        let Some(tab) = self.browser.active_tab().map(|tab| tab.id) else {
            return;
        };
        self.edit_browser(|browser| {
            browser.set_url(tab, format!("{URL}?id={id}"));
            browser.set_title(tab, format!("Extensions - {}", extension.name));
        });
        cx.notify();
    }
    fn extension_details_back(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self
            .browser
            .active_tab()
            .filter(|tab| is_manager_url(&tab.url))
            .map(|tab| tab.id)
        else {
            return;
        };
        self.edit_browser(|browser| {
            browser.set_url(tab, URL);
            browser.set_title(tab, "Extensions");
        });
        cx.notify();
    }
    fn dismiss_extension_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.extensions.search_active = false;
        self.extensions.manager_menu = false;
        self.extensions
            .search
            .update(cx, |input, cx| input.set_text("", cx));
        self.extensions.manager_focus.focus(window);
        cx.notify();
    }
    fn render_extension_manager_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.browser.active_user_profile_id();
        let developer = self.extension_developer_mode();
        div()
            .h(px(56.))
            .pl(px(18.))
            .pr(px(12.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .when(!self.extensions.search_active, |header| {
                header
                    .child(
                        div()
                            .id("extension-manager-menu-button")
                            .debug_selector(|| "extension-manager-menu-button".into())
                            .size(px(32.))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::ionicon("menu-outline", 18., 1.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.extensions.manager_menu = !this.extensions.manager_menu;
                                this.extensions.manager_focus.focus(window);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .ml(px(9.))
                            .text_size(px(20.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child("Extensions"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("extension-search-open")
                            .debug_selector(|| "extension-search-open".into())
                            .size(px(32.))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::control_sized(Icon::Search, 18.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.extensions.search_active = true;
                                this.extensions.manager_menu = false;
                                this.extensions
                                    .search
                                    .read(cx)
                                    .focus_handle(cx)
                                    .focus(window);
                                cx.notify();
                            })),
                    )
            })
            .when(self.extensions.search_active, |header| {
                header
                    .child(
                        div()
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::control_sized(Icon::Search, 18.)),
                    )
                    .child(
                        div()
                            .id("extension-search-field")
                            .debug_selector(|| "extension-search-field".into())
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .child(self.extensions.search.clone()),
                    )
                    .child(
                        div()
                            .id("extension-search-close")
                            .debug_selector(|| "extension-search-close".into())
                            .size(px(32.))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::control_sized(Icon::Close, 18.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_extension_manager(window, cx)
                            })),
                    )
            })
            .child(
                div()
                    .ml(px(14.))
                    .w(px(142.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(13.))
                    .child("Developer mode")
                    .child(
                        extension_switch("extension-developer-mode", developer).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if this.browser.active_user_profile_id() != profile {
                                    return;
                                }
                                this.toggle_extension_developer_mode(profile, !developer, cx);
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }
    fn render_extension_manager_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .inset_0()
            .bg(gpui::rgba(0x00000060))
            .child(
                div()
                    .id("extension-manager-menu")
                    .debug_selector(|| "extension-manager-menu".into())
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(256.))
                    .bg(gpui::rgb(0x292a2d))
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.extensions.manager_menu = false;
                        cx.notify();
                    }))
                    .flex()
                    .flex_col()
                    .px(px(8.))
                    .child(
                        div()
                            .h(px(56.))
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .text_size(px(20.))
                            .child("Extensions"),
                    )
                    .child(
                        extension_manager_menu_item(
                            "extension-manager-my-extensions",
                            "My extensions",
                            "extension-puzzle-outline",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.extensions.manager_menu = false;
                            this.extension_details_back(cx);
                        })),
                    )
                    .child(
                        extension_manager_menu_item(
                            "extension-manager-shortcuts",
                            "Keyboard shortcuts",
                            "keypad-outline",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.extensions.manager_menu = false;
                            this.retain_normal_source(cx);
                            crate::settings::open_shortcuts(cx);
                            cx.notify();
                        })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .px(px(16.))
                            .pb(px(24.))
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .child("Discover more extensions and themes on the ")
                            .child(
                                div()
                                    .id("extension-manager-store")
                                    .debug_selector(|| "extension-manager-store".into())
                                    .cursor_pointer()
                                    .underline()
                                    .text_color(gpui::rgb(0xa8c7fa))
                                    .child("Chrome Web Store")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.extensions.manager_menu = false;
                                        this.open_extension_store(cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn render_extension_details(
        &self,
        extension: Option<Extension>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let profile = self.browser.active_user_profile_id();
        let mut content = div()
            .id("extension-details")
            .debug_selector(|| "extension-details".into())
            .w_full()
            .max_w(px(680.))
            .bg(gpui::rgb(0x292a2d))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(56.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .id("extension-details-back")
                            .debug_selector(|| "extension-details-back".into())
                            .size(px(24.))
                            .mr(px(8.))
                            .cursor_pointer()
                            .child(icons::ionicon("arrow-back-outline", 20., 1.))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.extension_details_back(cx)),
                            ),
                    )
                    .when_some(extension.clone(), |row, entry| {
                        row.child(entry.icon(24.)).child(
                            div()
                                .text_size(px(14.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(entry.name),
                        )
                    }),
            );
        if let Some(extension) = extension {
            let id = extension.id.clone();
            let toggle = id.clone();
            let pin = id.clone();
            let remove = id.clone();
            let pinned = self
                .browser
                .active_user_profile()
                .pinned_extensions
                .contains(&id);
            content = content
                .child(
                    div()
                        .h(px(56.))
                        .px(px(20.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .text_color(gpui::rgb(0xa8c7fa))
                        .child(if extension.enabled { "On" } else { "Off" })
                        .child(div().flex_1())
                        .child(
                            extension_switch(
                                gpui::SharedString::from(format!("extension-details-toggle-{id}")),
                                extension.enabled,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.extension_enable(&toggle, profile, !extension.enabled, cx)
                                },
                            )),
                        ),
                )
                .child(extension_detail_row(
                    "Description",
                    extension.manifest["description"]
                        .as_str()
                        .unwrap_or_default(),
                ))
                .child(extension_detail_row("Version", &extension.version))
                .child(extension_detail_row("ID", &extension.id))
                .child(self.render_extension_permissions(&extension.id))
                .child(
                    div()
                        .border_t_1()
                        .border_color(gpui::rgb(0x3c4043))
                        .px(px(20.))
                        .h(px(56.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .child("Pin to toolbar")
                        .child(div().flex_1())
                        .child(
                            extension_switch(
                                gpui::SharedString::from(format!("extension-pin-{id}")),
                                pinned,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.extension_pin(&pin, profile, cx),
                            )),
                        ),
                )
                .child(
                    div()
                        .border_t_1()
                        .border_color(gpui::rgb(0x3c4043))
                        .p(px(20.))
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!("extension-remove-{id}")))
                                .debug_selector(move || format!("extension-remove-{id}"))
                                .cursor_pointer()
                                .text_size(px(13.))
                                .text_color(gpui::rgb(0xa8c7fa))
                                .child("Remove extension")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.request_extension_remove(&remove, profile, cx);
                                })),
                        ),
                );
        } else {
            content = content.child(
                div()
                    .p(px(20.))
                    .child("This extension is no longer installed."),
            );
        }
        div()
            .id("extensions-page")
            .debug_selector(|| "extensions-page".into())
            .key_context("ExtensionManager")
            .track_focus(&self.extensions.manager_focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                this.dismiss_extension_manager(window, cx)
            }))
            .absolute()
            .inset_0()
            .rounded(px(8.))
            .overflow_hidden()
            .bg(gpui::rgb(0x202124))
            .text_color(gpui::rgb(0xe8eaed))
            .flex()
            .flex_col()
            .child(self.render_extension_manager_header(cx))
            .child(
                div()
                    .id("extension-details-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .px(px(24.))
                    .child(content),
            )
            .when(self.extensions.manager_menu, |page| {
                page.child(self.render_extension_manager_menu(cx))
            })
            .into_any_element()
    }
    pub(crate) fn render_extensions(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(id) = self
            .browser
            .active_tab()
            .and_then(|tab| tab.url.strip_prefix("browser://extensions?id="))
        {
            return self.render_extension_details(
                self.active_extensions()
                    .find(|entry| entry.id == id)
                    .cloned(),
                cx,
            );
        }
        let profile = self.browser.active_user_profile_id();
        let developer = self.extension_developer_mode();
        let query = self.extensions.search.read(cx).text().to_lowercase();
        let entries = self
            .active_extensions()
            .filter(|entry| query.is_empty() || entry.name.to_lowercase().contains(&query))
            .cloned()
            .collect::<Vec<_>>();
        div()
            .id("extensions-page")
            .debug_selector(|| "extensions-page".into())
            .key_context("ExtensionManager")
            .track_focus(&self.extensions.manager_focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                this.dismiss_extension_manager(window, cx)
            }))
            .absolute()
            .inset_0()
            .rounded(px(8.))
            .overflow_hidden()
            .bg(gpui::rgb(0x202124))
            .text_color(gpui::rgb(0xe8eaed))
            .flex()
            .flex_col()
            .child(self.render_extension_manager_header(cx))
            .child(self.render_extension_developer_toolbar(cx))
            .child(
                div()
                    .id("extensions-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .pt(px(52.))
                    .pb(px(24.))
                    .px(px(24.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .when_some(self.extensions.error.clone(), |page, error| {
                        page.child(
                            div()
                                .debug_selector(|| "extensions-error".into())
                                .child(error),
                        )
                    })
                    .when(entries.is_empty(), |page| {
                        page.child(
                            div()
                                .debug_selector(|| "extensions-empty".into())
                                .flex()
                                .justify_center()
                                .gap(px(4.))
                                .text_size(px(16.))
                                .child("Find extensions and themes in the")
                                .child(
                                    div()
                                        .id("extensions-store-link")
                                        .debug_selector(|| "extensions-store-link".into())
                                        .cursor_pointer()
                                        .text_color(gpui::rgb(0xa8c7fa))
                                        .underline()
                                        .child("Chrome Web Store")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_extension_store(cx)
                                        })),
                                ),
                        )
                    })
                    .when(!entries.is_empty(), |page| {
                        page.child(
                            div()
                                .w_full()
                                .max_w(px(400.))
                                .h(px(24.))
                                .text_size(px(16.))
                                .child("All Extensions"),
                        )
                    })
                    .children(entries.into_iter().map(|extension| {
                        let id = extension.id.clone();
                        let enabled = extension.enabled;
                        let remove = id.clone();
                        let details = id.clone();
                        div()
                            .id(gpui::SharedString::from(format!("extension-card-{id}")))
                            .debug_selector(move || format!("extension-card-{id}"))
                            .w_full()
                            .max_w(px(400.))
                            .h(px(if developer { 208. } else { 160. }))
                            .flex_shrink_0()
                            .bg(gpui::rgb(0x292a2d))
                            .rounded(px(8.))
                            .shadow_sm()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex_1()
                                    .px(px(24.))
                                    .pt(px(20.))
                                    .flex()
                                    .items_start()
                                    .gap(px(32.))
                                    .child(
                                        div()
                                            .size(px(36.))
                                            .flex_shrink_0()
                                            .child(extension.icon(36.)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.))
                                            .text_size(px(13.))
                                            .line_height(px(20.))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.))
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .truncate()
                                                            .font_weight(gpui::FontWeight::MEDIUM)
                                                            .child(extension.name.clone()),
                                                    )
                                                    .when(developer, |line| {
                                                        line.child(
                                                            div()
                                                                .flex_shrink_0()
                                                                .child(extension.version.clone()),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                div().max_h(px(60.)).overflow_hidden().child(
                                                    extension.manifest["description"]
                                                        .as_str()
                                                        .unwrap_or_default()
                                                        .to_owned(),
                                                ),
                                            )
                                            .when(developer, |body| {
                                                body.child(div().flex_1()).child(
                                                    div()
                                                        .pb(px(12.))
                                                        .text_size(px(13.))
                                                        .line_height(px(18.))
                                                        .child(format!("ID: {}", extension.id))
                                                        .child(self.render_extension_worker_link(&extension, cx)),
                                                )
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(48.))
                                    .flex_shrink_0()
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        extension_outline_button(
                                            gpui::SharedString::from(format!(
                                                "extension-details-{details}"
                                            )),
                                            "Details",
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.open_extension_details(&details, profile, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        extension_outline_button(
                                            gpui::SharedString::from(format!(
                                                "extension-remove-{remove}"
                                            )),
                                            "Remove",
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.request_extension_remove(&remove, profile, cx);
                                            }),
                                        ),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        extension_switch(
                                            gpui::SharedString::from(format!(
                                                "extension-toggle-{}",
                                                extension.id
                                            )),
                                            enabled,
                                        )
                                        .mr(px(12.))
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.extension_enable(
                                                    &extension.id,
                                                    profile,
                                                    !enabled,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    ),
                            )
                    })),
            )
            .child(self.render_extension_developer_toast())
            .when(self.extensions.manager_menu, |page| {
                page.child(self.render_extension_manager_menu(cx))
            })
            .into_any_element()
    }
}
fn button(id: impl Into<gpui::SharedString>, label: &str) -> gpui::Stateful<gpui::Div> {
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .px(px(12.))
        .py(px(6.))
        .rounded(px(5.))
        .border_1()
        .border_color(gpui::rgb(0x5f6368))
        .cursor_pointer()
        .text_size(px(13.))
        .child(label.to_owned())
}

struct ExtensionTooltip(String);
impl gpui::Render for ExtensionTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(5.))
            .rounded(px(5.))
            .bg(gpui::rgb(0x303134))
            .text_color(gpui::white())
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

pub(crate) fn extension_outline_button(
    id: impl Into<gpui::SharedString>,
    label: &str,
) -> gpui::Stateful<gpui::Div> {
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .h(px(36.))
        .px(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(18.))
        .border_1()
        .border_color(gpui::rgb(0x397c9d))
        .text_color(gpui::rgb(0xa8c7fa))
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|item| item.bg(gpui::rgba(0xa8c7fa14)))
        .child(label.to_owned())
}
fn extension_switch(id: impl Into<gpui::SharedString>, enabled: bool) -> gpui::Stateful<gpui::Div> {
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .w(px(26.))
        .h(px(16.))
        .rounded(px(8.))
        .cursor_pointer()
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(px(2.))
        .bg(gpui::rgb(if enabled { 0xa8c7fa } else { 0x5f6368 }))
        .when(enabled, |track| track.justify_end())
        .child(
            div()
                .size(px(12.))
                .rounded(px(6.))
                .bg(gpui::rgb(if enabled { 0x174ea6 } else { 0xbdbdbd })),
        )
}

fn is_manager_url(url: &str) -> bool {
    url == URL
        || url
            .strip_prefix("browser://extensions?id=")
            .is_some_and(|id| {
                id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
            })
}
fn extension_detail_row(label: &str, value: &str) -> AnyElement {
    div()
        .border_t_1()
        .border_color(gpui::rgb(0x3c4043))
        .px(px(20.))
        .py(px(16.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .text_size(px(13.))
        .line_height(px(18.))
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(label.to_owned()),
        )
        .child(value.to_owned())
        .into_any_element()
}

fn extension_sheet_button(
    id: &'static str,
    label: &str,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(36.))
        .w(px(if primary { 171. } else { 110. }))
        .px(px(12.))
        .rounded(px(6.))
        .border_1()
        .border_color(gpui::rgb(if primary { 0x6a5efa } else { 0x59528b }))
        .bg(gpui::rgb(if primary { 0x3d35ef } else { 0x33305e }))
        .text_color(gpui::white())
        .text_size(px(13.))
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_between()
        .hover(|button| button.opacity(0.9))
        .child(label.to_owned())
        .child(
            div()
                .h(px(20.))
                .px(px(4.))
                .rounded(px(4.))
                .bg(gpui::rgba(0xffffff26))
                .text_size(px(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(if primary {
                    icons::control_sized(Icon::Enter, 18.)
                } else {
                    div().child("ESC").into_any_element()
                }),
        )
}

fn extension_manager_menu_item(
    id: &'static str,
    label: &str,
    icon: &str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(48.))
        .px(px(16.))
        .rounded(px(24.))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(24.))
        .text_size(px(14.))
        .hover(|row| row.bg(gpui::rgba(0xffffff0a)))
        .child(if icon == "extension-puzzle-outline" {
            extension_puzzle(18., false)
        } else {
            icons::ionicon(icon, 18., 1.)
        })
        .child(label.to_owned())
}

/// Fixed geometry for the extension puzzle, including dialogs without an icon.
pub(crate) fn extension_puzzle(size: f32, filled: bool) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let at = |x, y| bounds.origin + point(px(x * size / 24.), px(y * size / 24.));
            let mut path = if filled {
                gpui::PathBuilder::fill()
            } else {
                gpui::PathBuilder::stroke(px(size / 16.))
            };
            path.move_to(at(3., 5.));
            path.line_to(at(8., 5.));
            path.line_to(at(8., 3.));
            path.cubic_bezier_to(at(14., 3.), at(8., 0.), at(14., 0.));
            path.line_to(at(14., 5.));
            path.line_to(at(19., 5.));
            path.line_to(at(19., 10.));
            path.line_to(at(21., 10.));
            path.cubic_bezier_to(at(21., 16.), at(24., 10.), at(24., 16.));
            path.line_to(at(19., 16.));
            path.line_to(at(19., 21.));
            path.line_to(at(14., 21.));
            path.line_to(at(14., 19.));
            path.cubic_bezier_to(at(8., 19.), at(14., 16.), at(8., 16.));
            path.line_to(at(8., 21.));
            path.line_to(at(3., 21.));
            path.line_to(at(3., 16.));
            path.line_to(at(5., 16.));
            path.cubic_bezier_to(at(5., 10.), at(8., 16.), at(8., 10.));
            path.line_to(at(3., 10.));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .size(px(size))
    .flex_none()
    .into_any_element()
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    #[test]
    fn toolbar_popup_keeps_its_trailing_edge_when_extension_content_grows() {
        let placement = PopupPlacement {
            anchor: point(px(1000.), px(150.)),
            trailing: true,
            browser: Bounds::new(point(px(100.), px(100.)), size(px(1000.), px(760.))),
            screen: Some(Bounds::new(
                point(px(0.), px(0.)),
                size(px(1512.), px(982.)),
            )),
        };
        for (width, height) in [(320, 240), (400, 580), (720, 600)] {
            let bounds = placement.bounds(width, height);
            assert_eq!(bounds.right(), px(1000.));
            assert_eq!(bounds.top(), px(150.));
        }
    }
    #[test]
    fn action_popup_remains_visible_at_browser_and_negative_display_edges() {
        let browser = Bounds::new(point(px(-1920.), px(100.)), size(px(1000.), px(760.)));
        let placement = PopupPlacement {
            anchor: point(px(-1700.), px(900.)),
            trailing: true,
            browser,
            screen: Some(Bounds::new(
                point(px(-1920.), px(0.)),
                size(px(1920.), px(1080.)),
            )),
        };
        let bounds = placement.bounds(480, 580);
        assert_eq!(bounds.left(), px(-1920.));
        assert_eq!(bounds.bottom(), px(1080.));
        let leading = PopupPlacement {
            anchor: point(px(-950.), px(300.)),
            trailing: false,
            ..placement
        }
        .bounds(400, 240);
        assert_eq!(leading.right(), browser.right());
    }
}
