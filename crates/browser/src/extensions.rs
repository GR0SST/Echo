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
    FocusHandle, KeyBinding, Pixels, Point, Subscription, WeakEntity, Window,
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
actions!(extensions, [Dismiss]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("ExtensionPopup"))]);
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct ExtensionCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub shortcut: String,
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
        icons::ionicon("extension-puzzle-outline", extent, 1.)
    }
    pub(crate) fn key(&self) -> Option<[u8; 32]> {
        self.id.as_bytes().try_into().ok()
    }
}
struct Prompt {
    request: u64,
    data: serde_json::Value,
    profile: UserProfileId,
}
struct ActionPopup {
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
    next_attempt: Instant,
    prompt: Option<Prompt>,
    prompt_focus: FocusHandle,
    pub(crate) confirm_remove: Option<String>,
    pub(crate) menu: Option<String>,
    popup: Option<ActionPopup>,
    popup_generation: u64,
    pub(crate) pending_retirements: Rc<Cell<usize>>,
    pub(crate) overlay: Option<WindowHandle<Overlay>>,
    pub(crate) search: Entity<TextInput>,
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
            next_attempt: Instant::now(),
            prompt: None,
            prompt_focus: cx.focus_handle(),
            confirm_remove: None,
            menu: None,
            popup: None,
            popup_generation: 0,
            pending_retirements: Rc::new(Cell::new(0)),
            overlay: None,
            search,
            _search_changed: changed,
        }
    }
}

struct PopupFrame {
    browser: WeakEntity<BrowserWindow>,
    focus: FocusHandle,
    _activation: Subscription,
}
impl gpui::Render for PopupFrame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.clone();
        div()
            .key_context("ExtensionPopup")
            .track_focus(&self.focus)
            .size_full()
            .bg(gpui::white())
            .on_action(move |_: &Dismiss, _, cx| {
                let _ = browser.update(cx, |view, cx| view.close_extension_popup(cx));
            })
    }
}

impl BrowserWindow {
    pub(crate) fn extension_popup_window_id(&self) -> Option<gpui::WindowId> {
        self.extensions
            .popup
            .as_ref()
            .map(|popup| popup.window.window_id())
    }
    pub(crate) fn extensions_is_open(&self) -> bool {
        self.browser.active_tab().is_some_and(|tab| tab.url == URL)
            && self.browser.active_split().is_none()
    }
    pub(crate) fn poll_extensions(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        let profile = self.browser.active_user_profile_id();
        if self.extensions.profile != Some(profile) {
            self.close_extension_surfaces(cx);
            self.extensions.service = None;
            self.extensions.owner = None;
            self.extensions.entries.clear();
            self.extensions.error = None;
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
            self.close_extension_surfaces(cx);
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
                        }
                        Err(error) => self.extensions.error = Some(error.to_string()),
                    }
                }
                ExtensionEvent::InstallPrompt { request, prompt } => {
                    self.extensions.prompt = Some(Prompt {
                        request,
                        data: prompt,
                        profile,
                    });
                    self.show_extension_prompt(cx);
                }
                ExtensionEvent::InstallComplete { result, .. }
                | ExtensionEvent::OperationComplete { result, .. } => {
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
                ExtensionEvent::Closed => {
                    self.close_extension_surfaces(cx);
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
                    popup.size = preferred;
                    let (width, height) = preferred;
                    let _ = popup.window.update(cx, |_, window, _| {
                        window.resize(size(px(width as f32), px(height as f32)))
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
            .find(|t| t.url == URL)
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
                if !profile.unpinned_extensions.remove(&id) {
                    profile.unpinned_extensions.insert(id);
                }
            }
        });
        self.extensions.menu = None;
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
        self.extensions.menu = None;
        self.extensions.confirm_remove = None;
    }
    pub(crate) fn invoke_extension(
        &mut self,
        id: &str,
        profile: UserProfileId,
        anchor: Option<Point<Pixels>>,
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
            let mut origin = bounds.origin + anchor.unwrap_or(point(px(245.), px(100.)));
            if let Some(screen) = screen {
                origin.x = origin.x.clamp(
                    screen.left(),
                    (screen.right() - px(320.)).max(screen.left()),
                );
                origin.y = origin
                    .y
                    .clamp(screen.top(), (screen.bottom() - px(240.)).max(screen.top()));
            }
            let closed = Rc::new(Cell::new(false));
            let preferred = Rc::new(Cell::new((320, 240)));
            let view = Rc::new(Cell::new(0));
            let parent = Rc::new(Cell::new(0usize));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    origin,
                    size(px(320.), px(240.)),
                ))),
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            };
            let host = parent.clone();
            let weak = root.clone();
            let flag = closed.clone();
            let Ok(handle) = cx.open_window(options, |window, cx| {
                let focus = cx.focus_handle();
                focus.focus(window);
                if let Some(native) = crate::native::view_of(window) {
                    host.set(native as usize);
                    if let Some(main) = main_parent {
                        crate::native::attach_overlay(main, native, false);
                    }
                }
                cx.new(|cx| {
                    let activation = cx.observe_window_activation(window, move |_, window, _| {
                        if !window.is_window_active() {
                            flag.set(true);
                        }
                    });
                    PopupFrame {
                        browser: weak,
                        focus,
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
    fn show_extension_prompt(&mut self, cx: &mut Context<Self>) {
        if self.extensions.overlay.is_some() {
            return;
        }
        let Some(prompt) = &self.extensions.prompt else {
            return;
        };
        let (request, profile) = (prompt.request, prompt.profile);
        let root = cx.entity().downgrade();
        let main = self.main_window;
        let parent = self.native_view;
        let focus = self.extensions.prompt_focus.clone();
        cx.defer(move |cx| {
            if crate::quit::is_quitting()
                || !root
                    .read_with(cx, |view, _| {
                        view.extensions
                            .prompt
                            .as_ref()
                            .is_some_and(|p| p.request == request && p.profile == profile)
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
            if let Ok(handle) = cx.open_window(options, |window, cx| {
                focus.focus(window);
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
    pub(crate) fn render_extension_prompt(&self, cx: &mut Context<Self>) -> AnyElement {
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
    pub(crate) fn render_extension_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.browser.active_user_profile_id();
        let entries = self
            .extensions
            .entries
            .iter()
            .filter(|e| {
                e.enabled
                    && !self
                        .browser
                        .active_user_profile()
                        .unpinned_extensions
                        .contains(&e.id)
            })
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
                        let anchor = Rc::new(Cell::new(None::<Bounds<Pixels>>));
                        let painted = anchor.clone();
                        div()
                            .id(gpui::SharedString::from(format!("site-extension-{id}")))
                            .debug_selector(move || format!("site-extension-{id}"))
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
                                let at = anchor.get().map(|b| point(b.left(), b.bottom() + px(8.)));
                                this.invoke_extension(&action_id, profile, at, cx);
                            }))
                            .on_mouse_down(
                                gpui::MouseButton::Right,
                                cx.listener(move |this, _, _, cx| {
                                    this.extensions.menu = Some(menu_id.clone());
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(extension.icon(16.))
                            .child(
                                gpui::canvas(
                                    |_, _, _| (),
                                    move |bounds, _, _, _| painted.set(Some(bounds)),
                                )
                                .absolute()
                                .inset_0(),
                            )
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .id("site-add-extension")
                            .debug_selector(|| "site-add-extension".into())
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
            .when_some(self.extensions.menu.clone(), |grid, id| {
                let pin = id.clone();
                grid.child(
                    div()
                        .p(px(5.))
                        .rounded(px(6.))
                        .bg(gpui::hsla(0., 0., 0.1, 0.95))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            button(
                                "site-extension-pin",
                                if self
                                    .browser
                                    .active_user_profile()
                                    .unpinned_extensions
                                    .contains(&id)
                                {
                                    "Pin"
                                } else {
                                    "Unpin"
                                },
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.extension_pin(&pin, profile, cx),
                            )),
                        )
                        .child(
                            button("site-manage-extensions", "Manage Extensions…")
                                .on_click(cx.listener(|this, _, _, cx| this.open_extensions(cx))),
                        ),
                )
            })
            .into_any_element()
    }
    pub(crate) fn render_extensions(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.browser.active_user_profile_id();
        let query = self.extensions.search.read(cx).text().to_lowercase();
        let entries = self
            .extensions
            .entries
            .iter()
            .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
            .cloned()
            .collect::<Vec<_>>();
        div()
            .id("extensions-page")
            .debug_selector(|| "extensions-page".into())
            .absolute()
            .inset_0()
            .rounded(px(8.))
            .overflow_hidden()
            .bg(gpui::rgb(0x202124))
            .text_color(gpui::rgb(0xe8eaed))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(56.))
                    .px(px(26.))
                    .flex()
                    .items_center()
                    .gap(px(20.))
                    .child(icons::ionicon("menu-outline", 22., 1.))
                    .child(div().text_size(px(20.)).child("Extensions"))
                    .child(div().flex_1())
                    .child(div().w(px(230.)).child(self.extensions.search.clone())),
            )
            .child(
                div()
                    .id("extensions-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
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
                                .mt(px(80.))
                                .flex()
                                .justify_center()
                                .gap(px(4.))
                                .child("Find extensions and themes in the")
                                .child(
                                    div()
                                        .id("extensions-store-link")
                                        .debug_selector(|| "extensions-store-link".into())
                                        .cursor_pointer()
                                        .text_color(gpui::rgb(0x8ab4f8))
                                        .child("Chrome Web Store")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_extension_store(cx)
                                        })),
                                ),
                        )
                    })
                    .children(entries.into_iter().map(|extension| {
                        let id = extension.id.clone();
                        let enabled = extension.enabled;
                        let remove = id.clone();
                        let pin = id.clone();
                        div()
                            .id(gpui::SharedString::from(format!("extension-card-{id}")))
                            .debug_selector(move || format!("extension-card-{id}"))
                            .w_full()
                            .max_w(px(800.))
                            .p(px(20.))
                            .border_1()
                            .border_color(gpui::rgb(0x5f6368))
                            .rounded(px(8.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(20.))
                                    .child(extension.icon(32.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.))
                                            .child(extension.name.clone())
                                            .child(
                                                div().text_size(px(13.)).child(
                                                    extension.manifest["description"]
                                                        .as_str()
                                                        .unwrap_or_default()
                                                        .to_owned(),
                                                ),
                                            ),
                                    )
                                    .child(
                                        div().text_size(px(12.)).child(extension.version.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(
                                        button(
                                            gpui::SharedString::from(format!(
                                                "extension-pin-{pin}"
                                            )),
                                            if self
                                                .browser
                                                .active_user_profile()
                                                .unpinned_extensions
                                                .contains(&pin)
                                            {
                                                "Pin"
                                            } else {
                                                "Unpin"
                                            },
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.extension_pin(&pin, profile, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        button(
                                            gpui::SharedString::from(format!(
                                                "extension-remove-{remove}"
                                            )),
                                            "Remove",
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.extensions.confirm_remove =
                                                    Some(remove.clone());
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        button(
                                            gpui::SharedString::from(format!(
                                                "extension-toggle-{}",
                                                extension.id
                                            )),
                                            if enabled { "On" } else { "Off" },
                                        )
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
                    }))
                    .when_some(self.extensions.confirm_remove.clone(), |page, id| {
                        let confirm = id.clone();
                        page.child(
                            div()
                                .p(px(20.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(gpui::rgb(0x5f6368))
                                .child("Remove this extension?")
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(12.))
                                        .child(
                                            button("extension-remove-cancel", "Cancel").on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.extensions.confirm_remove = None;
                                                    cx.notify();
                                                }),
                                            ),
                                        )
                                        .child(
                                            button("extension-remove-confirm", "Remove").on_click(
                                                cx.listener(move |this, _, _, cx| {
                                                    this.extension_remove(&confirm, profile, cx)
                                                }),
                                            ),
                                        ),
                                ),
                        )
                    }),
            )
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
