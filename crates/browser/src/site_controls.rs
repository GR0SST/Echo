//! The site controls panel opened from the address field, as the
//! reference's Site Control Center: page actions, extensions and the
//! connection's security, drawn over the page in a child window.
use crate::icons::Icon;
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use gpui::{
    actions, div, hsla, point, prelude::*, px, AnyElement, App, Bounds, Context, FocusHandle,
    KeyBinding, Pixels, Point, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
};
use std::{cell::Cell, rc::Rc};

const CONTEXT: &str = "SiteControls";
const PERMISSION_CONTEXT: &str = "SitePermissionPrompt";
const PADDING: f32 = 9.0;
const SITE_RADIUS: f32 = 20.0;
const TILE_WIDTH: f32 = 48.0;
const TILE_HEIGHT: f32 = 32.0;
const TILE_GAP: f32 = 8.0;
const COLUMNS: f32 = 4.0;
pub(crate) const WIDTH: f32 = PADDING * 2. + TILE_WIDTH * COLUMNS + TILE_GAP * (COLUMNS - 1.) + 2.;
/// Space between the address field's button and the panel.
const ANCHOR_GAP: f32 = 14.0;
actions!(site_controls, [Dismiss, Next, Previous, Confirm]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(PERMISSION_CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SiteScope {
    tab: browser_core::TabId,
    profile: browser_core::UserProfileId,
    origin: Option<String>,
}

#[derive(Clone, Copy)]
enum SiteAction {
    ClearCache,
    ClearCookies,
    ManageBoosts,
    NewBoost,
}

pub(crate) struct SiteControls {
    scope: SiteScope,
    permission_request: Option<engine::PermissionRequest>,
    material_alive: Rc<Cell<bool>>,
    material_installed: Rc<Cell<bool>>,
    info: Option<engine::SiteInfo>,
    more: bool,
    selected: usize,
    more_anchor: Anchor,

    position: Point<Pixels>,
    focus: FocusHandle,
    pub overlay: Option<WindowHandle<Overlay>>,
}

/// Where the address field's site controls button was last painted, in
/// its own window's coordinates.
pub(crate) type Anchor = Rc<Cell<Option<Bounds<Pixels>>>>;

impl BrowserWindow {
    pub(crate) fn developer_site(&self) -> Option<String> {
        let tab = self.browser.active_tab()?;
        let url = url::Url::parse(&tab.url).ok()?;
        matches!(url.scheme(), "http" | "https").then(|| url.host_str().map(str::to_owned)).flatten()
    }
    pub(crate) fn developer_mode_enabled(&self) -> bool {
        self.developer_site().is_some_and(|host| self.browser.active_user_profile().developer_sites.contains(&host))
    }
    pub(crate) fn toggle_developer_mode(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.developer_site() else { return; };
        let profile = self.browser.active_user_profile_id();
        self.edit_profile(|model| {
            if let Some(profile) = model.user_profile_mut(profile) {
                if !profile.developer_sites.remove(&host) { profile.developer_sites.insert(host); }
            }
        });
        cx.notify();
    }

    pub(crate) fn toggle_site_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.site_controls.is_some() {
            self.close_site_controls(cx);
            return;
        }
        let Some(tab) = self.browser.active_tab() else {
            return;
        };
        let scope = SiteScope {
            tab: tab.id,
            profile: self.browser.active_user_profile_id(),
            origin: browser_core::site_settings::SiteSettings::origin(&tab.url),
        };
        let info = self
            .pages
            .as_ref()
            .and_then(|pages| pages.borrow().site_info(scope.tab))
            .filter(|info| Some(&info.origin) == scope.origin.as_ref());
        self.close_user_profile_dialog(cx);
        self.close_tab_menu(cx);
        self.close_command_bar(cx);
        let anchor = self.site_controls_anchor.get().unwrap_or_default();
        let focus = cx.focus_handle();
        let origin = window.bounds().origin;
        let at = point(
            anchor.center().x - px(WIDTH / 2.),
            anchor.bottom() + px(ANCHOR_GAP),
        );
        self.site_controls = Some(SiteControls {
            scope,
            permission_request: None,
            material_alive: Rc::new(Cell::new(true)),
            material_installed: Rc::new(Cell::new(false)),
            info,
            more: false,
            selected: 0,
            more_anchor: Default::default(),
            position: at,
            focus: focus.clone(),
            overlay: None,
        });
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.notify();
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.site_controls
                        .as_ref()
                        .is_some_and(|panel| panel.focus == focus)
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok((bounds, screen)) = main.update(cx, |_, window, cx| {
                (
                    window.bounds(),
                    window.display(cx).map(|display| display.bounds()),
                )
            }) else {
                return;
            };
            let _ = browser.update(cx, |this, _| {
                if let Some(panel) = &mut this.site_controls {
                    let at = at + (origin - bounds.origin);
                    let max_x = screen
                        .map(|screen| screen.right() - bounds.origin.x - px(WIDTH + 8.))
                        .unwrap_or(bounds.size.width - px(WIDTH + 8.));
                    panel.position = point(at.x.clamp(px(8.), max_x.max(px(8.))), at.y.max(px(8.)));
                }
            });
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    bounds.origin,
                    gpui::size(
                        bounds.size.width + px(WIDTH + 180.),
                        bounds.size.height + px(300.),
                    ),
                ))),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                focus.focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::SiteControls, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(panel) = &mut this.site_controls {
                        panel.overlay = Some(overlay);
                    }
                });
            }
        });
    }

    pub(crate) fn close_site_controls(&mut self, cx: &mut Context<Self>) {
        self.close_site_controls_with_focus(crate::window::PopupFocus::Restore, cx);
    }

    pub(crate) fn close_site_controls_with_focus(
        &mut self,
        focus: crate::window::PopupFocus,
        cx: &mut Context<Self>,
    ) {
        self.clear_site_controls_backdrop();
        let Some(panel) = self.site_controls.take() else {
            return;
        };
        if let Some(request) = panel.permission_request {
            if let Some(pages) = &self.pages {
                pages.borrow_mut().respond_permission(
                    panel.scope.tab,
                    request.id,
                    engine::PermissionDecision::Dismiss,
                );
            }
        }
        self.close_popup_overlay(panel.overlay, focus, false, cx);
        cx.notify();
    }

    pub(crate) fn clear_site_controls_backdrop(&self) {
        if let Some(panel) = &self.site_controls {
            panel.material_alive.set(false);
        }
        if let Some(view) = self.native_view {
            crate::native::set_content_backdrop(
                view,
                "site-controls-page",
                None,
                0.,
                site_backdrop_config(),
            );
        }
    }

    fn site_scope_is_current(&self, scope: &SiteScope) -> bool {
        self.browser.active_user_profile_id() == scope.profile
            && self.browser.active_tab().is_some_and(|tab| {
                tab.id == scope.tab
                    && browser_core::site_settings::SiteSettings::origin(&tab.url) == scope.origin
            })
    }

    fn share_current_site(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(panel) = self.site_controls.as_ref().filter(|panel| {
            panel.permission_request.is_none() && self.site_scope_is_current(&panel.scope)
        }) else {
            return;
        };
        let Some(overlay) = panel.overlay else {
            return;
        };
        let scope = panel.scope.clone();
        let Some(url) = self.browser.active_tab().map(|tab| tab.url.clone()) else {
            return;
        };
        // Native service menus track a nested AppKit loop. Leave all GPUI and
        // page borrows first, and anchor to the panel so it remains visible.
        cx.spawn(async move |owner, cx| {
            let current = owner
                .read_with(cx, |this, _| {
                    this.site_scope_is_current(&scope)
                        && this.browser.active_tab().is_some_and(|tab| tab.url == url)
                        && this
                            .site_controls
                            .as_ref()
                            .is_some_and(|panel| panel.overlay == Some(overlay))
                })
                .unwrap_or(false);
            if !current {
                return;
            }
            let Ok(view) = overlay.update(cx, |_, window, _| crate::native::view_of(window)) else {
                return;
            };
            let mut picker = crate::native::sharing::show(
                view,
                [f32::from(position.x), f32::from(position.y), 0., 0.],
                &[url],
            );
            let previous = owner.update(cx, |this, _| std::mem::replace(&mut this.share_picker, picker.take())).ok().flatten();
            drop(previous);
            drop(picker);
        })
        .detach();
    }

    fn dismiss_site_controls(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = &mut self.site_controls {
            if panel.more {
                panel.more = false;
                cx.notify();
                return;
            }
        }
        self.close_site_controls(cx);
    }

    fn clear_current_site_data(&mut self, kind: engine::ClearSiteData, cx: &mut Context<Self>) {
        let Some(scope) = self
            .site_controls
            .as_ref()
            .map(|panel| panel.scope.clone())
            .filter(|scope| self.site_scope_is_current(scope))
        else {
            return;
        };
        let accepted = scope.origin.as_ref().is_some_and(|origin| {
            self.pages
                .as_ref()
                .is_some_and(|pages| pages.borrow_mut().clear_site_data(scope.tab, origin, kind))
        });
        if accepted {
            self.close_site_controls(cx);
        }
    }

    fn site_menu_action(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(scope) = self
            .site_controls
            .as_ref()
            .map(|panel| panel.scope.clone())
            .filter(|scope| self.site_scope_is_current(scope))
        else {
            return;
        };
        let _ = scope;
        if index==6 {self.open_all_site_settings(cx);return;}
        match [
            SiteAction::ClearCache,
            SiteAction::ClearCookies,
            SiteAction::ManageBoosts,
            SiteAction::NewBoost,
        ]
        .get(index)
        {
            Some(SiteAction::ClearCache) => {
                self.clear_current_site_data(engine::ClearSiteData::Cache, cx)
            }
            Some(SiteAction::ClearCookies) => {
                self.clear_current_site_data(engine::ClearSiteData::Cookies, cx)
            }
            Some(SiteAction::ManageBoosts) => {
                self.close_site_controls(cx);
                self.open_library_boosts(cx);
            }
            Some(SiteAction::NewBoost) => self.new_boost(cx),
            None => {}
        }
    }

    fn site_command_bar(&mut self, edit: bool, cx: &mut Context<Self>) {
        let (target, text) = if edit {
            self.browser
                .active_tab()
                .map(|tab| (crate::window::Target::Tab(tab.id), tab.url.clone()))
                .unwrap_or((crate::window::Target::NewTab, String::new()))
        } else {
            (crate::window::Target::NewTab, String::new())
        };
        let main = self.main_window;
        let browser = cx.weak_entity();
        self.close_site_controls_with_focus(crate::window::PopupFocus::OnActivation, cx);
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = browser.update(cx, |view, cx| {
                    view.open_command_bar(target, &text, window, cx)
                });
            });
        });
    }

    fn show_site_certificate(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self
            .site_controls
            .as_ref()
            .map(|panel| panel.scope.clone())
            .filter(|scope| self.site_scope_is_current(scope))
        else {
            return;
        };
        let Some(info) = self
            .pages
            .as_ref()
            .and_then(|pages| pages.borrow().site_info(scope.tab))
            .filter(|info| Some(&info.origin) == scope.origin.as_ref())
        else {
            return;
        };
        let Some(certificate) = info.certificate else {
            return;
        };
        let Some(parent) = self.native_view else {
            return;
        };
        let browser = cx.weak_entity();
        self.close_site_controls_with_focus(crate::window::PopupFocus::OnActivation, cx);
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |view, _| view.site_scope_is_current(&scope))
                .unwrap_or(false)
            {
                return;
            }
            if !crate::native::show_certificate(parent, &certificate.der_chain, &info.host) {
                let _ = browser.update(cx, |view, cx| {
                    view.show_toast(
                        "Certificate viewer is unavailable on this platform".into(),
                        cx,
                    )
                });
            }
        });
    }

    pub(crate) fn show_site_permission_request(
        &mut self,
        tab: browser_core::TabId,
        profile: browser_core::UserProfileId,
        request: engine::PermissionRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.browser.active_tab().is_some_and(|active| {
            active.id == tab
                && browser_core::site_settings::SiteSettings::origin(&active.url).as_deref()
                    == Some(request.top_origin.as_str())
        }) && self.browser.active_user_profile_id() == profile
            && self.command_bar.is_none()
            && window.is_window_active()
            && permission_purpose(&request.permissions).is_some()
            && !self
                .site_controls
                .as_ref()
                .is_some_and(|panel| panel.permission_request.is_some());
        if !current {
            if let Some(pages) = &self.pages {
                pages.borrow_mut().respond_permission(
                    tab,
                    request.id,
                    engine::PermissionDecision::Dismiss,
                );
            }
            return;
        }
        self.edit_profile(|model| {
            if let Some(profile) = model.user_profile_mut(profile) {
                for kind in &request.permissions {
                    profile
                        .site_settings
                        .record_permission(&request.top_origin, core_permission(*kind));
                }
            }
        });
        self.close_site_controls(cx);
        self.toggle_site_controls(window, cx);
        if let Some(panel) = &mut self.site_controls {
            panel.permission_request = Some(request);
        }
        cx.notify();
    }

    pub(crate) fn site_permission_dismissed(
        &mut self,
        request: engine::PermissionRequestId,
        cx: &mut Context<Self>,
    ) {
        if self.site_controls.as_ref().is_some_and(|panel| {
            panel
                .permission_request
                .as_ref()
                .is_some_and(|r| r.id == request)
        }) {
            self.site_controls.as_mut().unwrap().permission_request = None;
            self.close_site_controls(cx);
        }
    }

    pub(crate) fn resolve_site_permission(
        &mut self,
        expected: engine::PermissionRequestId,
        decision: engine::PermissionDecision,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .site_controls
            .as_ref()
            .filter(|panel| {
                panel
                    .permission_request
                    .as_ref()
                    .is_some_and(|request| request.id == expected)
            })
            .map(|panel| self.site_scope_is_current(&panel.scope));
        let Some(current) = current else {
            return;
        };
        let decision = if current {
            decision
        } else {
            engine::PermissionDecision::Dismiss
        };
        let panel = self.site_controls.as_mut().unwrap();
        let Some(request) = panel.permission_request.take() else {
            return;
        };
        let tab = panel.scope.tab;
        if let Some(pages) = &self.pages {
            pages
                .borrow_mut()
                .respond_permission(tab, request.id, decision);
        }
        self.close_site_controls(cx);
    }

    fn render_site_permission_prompt(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let panel = self.site_controls.as_ref().unwrap();
        let request = panel.permission_request.as_ref().unwrap();
        let request_id = request.id;
        let kind = request
            .permissions
            .first()
            .copied()
            .unwrap_or(engine::PermissionKind::Camera);
        let Some(purpose) = permission_purpose(&request.permissions) else {
            return div().into_any_element();
        };
        // GPUI treats '-' as part of a word. Match the reference's CoreText
        // URL breaks with explicit, measured lines rather than splitting a host.
        let title = format!("Allow {} to {}?", request.origin, purpose);
        let mut font = gpui::font(".SystemUIFont");
        font.weight = gpui::FontWeight::SEMIBOLD;
        let title = wrap_permission_title(&title, WIDTH - 2. * PADDING - 2. - 32. - 8., |text| {
            let run = gpui::TextRun {
                len: text.len(),
                font: font.clone(),
                color: hsla(0., 0., 1., 0.75),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(text.to_owned().into(), px(14.), &[run], None)
                    .width,
            )
        });
        let arrow_x = self
            .site_controls_anchor
            .get()
            .map(|b| b.center().x)
            .unwrap_or(panel.position.x + px(WIDTH / 2.));
        let button = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(32.))
                .flex_1()
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(14.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(hsla(0., 0., 1., 0.75))
        };
        div()
            .id("site-permission-backdrop")
            .size_full()
            .key_context(PERMISSION_CONTEXT)
            .track_focus(&panel.focus)
            .on_action(
                cx.listener(|this, _: &crate::window::OpenCommandBar, _, cx| {
                    this.site_command_bar(false, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::window::EditCurrentUrl, _, cx| {
                    this.site_command_bar(true, cx)
                }),
            )
            // The reference requires a choice; Escape does not dismiss this question.
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.stop_propagation()))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_site_controls(cx)),
            )
            .child(
                self.site_panel_shell(panel, "site-permission-prompt")
                    .absolute()
                    .left(panel.position.x)
                    .top(panel.position.y)
                    .w(px(WIDTH))
                    .p(px(PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .child(
                                div()
                                    .size(px(32.))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(hsla(0., 0., 1., 0.08))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(hsla(0., 0., 1., 0.65))
                                    .child(crate::icons::ionicon(permission_icon(kind), 18., 1.)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(14.))
                                    .line_height(px(17.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(hsla(0., 0., 1., 0.75))
                                    .whitespace_nowrap()
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                button("site-permission-not-now")
                                    .bg(hsla(0., 0., 1., 0.07))
                                    .hover(|s| s.bg(hsla(0., 0., 1., 0.13)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.resolve_site_permission(
                                            request_id,
                                            engine::PermissionDecision::Block,
                                            cx,
                                        )
                                    }))
                                    .child("Not Now"),
                            )
                            .child(
                                button("site-permission-allow")
                                    .border_1()
                                    .border_color(hsla(0., 0., 0., 0.4))
                                    .bg(gpui::linear_gradient(
                                        180.,
                                        gpui::linear_color_stop(gpui::rgb(0x919192), 0.),
                                        gpui::linear_color_stop(gpui::rgb(0x747475), 1.),
                                    ))
                                    .hover(|s| s.bg(gpui::rgb(0x919192)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.resolve_site_permission(
                                            request_id,
                                            engine::PermissionDecision::Allow,
                                            cx,
                                        )
                                    }))
                                    .child("Allow"),
                            ),
                    ),
            )
            .child(site_arrow(point(
                arrow_x - px(9.),
                panel.position.y - px(8.),
            )))
            .into_any_element()
    }

    fn site_panel_shell(
        &self,
        panel: &SiteControls,
        id: &'static str,
    ) -> gpui::Stateful<gpui::Div> {
        let material_alive = panel.material_alive.clone();
        let material_installed = panel.material_installed.clone();
        let parent = self.native_view.map(|view| view as usize);
        div()
            .id(id)
            .debug_selector(move || id.into())
            .relative()
            .rounded(px(SITE_RADIUS))
            .bg(if material_installed.get() {
                hsla(0., 0., 0.36, 0.38)
            } else {
                hsla(240. / 360., 0.04, 0.13, 1.)
            })
            .border_1()
            .border_color(hsla(0., 0., 1., 0.12))
            .shadow_lg()
            .text_color(hsla(0., 0., 1., 0.8))
            .font_family(".SystemUIFont")
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        if !material_alive.get() {
                            return;
                        }
                        let installed = parent.is_some_and(|view| {
                            crate::native::set_content_backdrop(
                                view as *mut std::ffi::c_void,
                                "site-controls-page",
                                Some([
                                    f32::from(bounds.origin.x) as f64,
                                    f32::from(bounds.origin.y) as f64,
                                    f32::from(bounds.size.width) as f64,
                                    f32::from(bounds.size.height) as f64,
                                ]),
                                SITE_RADIUS as f64,
                                site_backdrop_config(),
                            ) == crate::material::BackdropInstallation::Installed
                        });
                        if material_installed.replace(installed) != installed {
                            window.defer(cx, |window, _| window.refresh());
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
    }

    pub(crate) fn render_site_controls(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self
            .site_controls
            .as_ref()
            .is_some_and(|panel| !self.site_scope_is_current(&panel.scope))
        {
            self.close_site_controls(cx);
        }
        if let Some(scope) = self.site_controls.as_ref().map(|panel| panel.scope.clone()) {
            let info = self.pages.as_ref().and_then(|pages| {
                pages
                    .try_borrow()
                    .ok()
                    .map(|pages| pages.site_info(scope.tab))
            });
            if let Some(info) = info {
                if let Some(panel) = &mut self.site_controls {
                    panel.info = info.filter(|info| Some(&info.origin) == scope.origin.as_ref());
                }
            }
        }
        let Some(panel) = &self.site_controls else {
            return div().into_any_element();
        };
        if let Some(request) = &panel.permission_request {
            let current = self.pages.as_ref().is_some_and(|pages| {
                pages.try_borrow().map_or(true, |pages| {
                    pages.permission_pending(panel.scope.tab, request.id)
                })
            });
            if !current {
                self.site_controls.as_mut().unwrap().permission_request = None;
                self.close_site_controls(cx);
                return div().into_any_element();
            }
            return self.render_site_permission_prompt(window, cx);
        }
        let boosts = self.site_boosts();
        let profile = self.browser.active_user_profile_id();
        let secure = panel.info.as_ref().is_some_and(|info| info.secure);
        let more = panel.more;
        let more_anchor = panel.more_anchor.clone();
        let menu_position = more_anchor
            .get()
            .map(|bounds| point(bounds.center().x, bounds.center().y - px(4.)))
            .unwrap_or(panel.position);
        let arrow_x = self
            .site_controls_anchor
            .get()
            .map(|bounds| bounds.center().x)
            .unwrap_or(panel.position.x + px(WIDTH / 2.))
            .clamp(
                panel.position.x + px(20.),
                panel.position.x + px(WIDTH - 20.),
            );
        let arrow_at = point(arrow_x - px(9.), panel.position.y - px(8.));
        let scope = panel.scope.clone();
        let permissions: Vec<_> = self
            .browser
            .user_profile(scope.profile)
            .into_iter()
            .flat_map(|profile| {
                profile
                    .site_settings
                    .permissions(scope.origin.as_deref().unwrap_or(""))
            })
            .filter_map(|permission| {
                panel
                    .info
                    .as_ref()?
                    .permissions
                    .iter()
                    .find(|setting| core_permission(setting.kind) == permission)
                    .cloned()
            })
            .filter(|setting| {
                matches!(
                    setting.value,
                    engine::PermissionValue::Allow | engine::PermissionValue::Block
                )
            })
            .collect();
        let tile = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .w(px(TILE_WIDTH))
                .h(px(TILE_HEIGHT))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 1., 0.1))
        };
        let top_tile = |id: &'static str| {
            tile(id)
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(gpui::rgb(0x333333), 0.),
                    gpui::linear_color_stop(gpui::rgb(0x161616), 1.),
                ))
                .border_1()
                .border_color(hsla(0., 0., 1., 0.2))
                .text_color(hsla(0., 0., 1., 0.75))
                .hover(|style| {
                    style.bg(gpui::linear_gradient(
                        180.,
                        gpui::linear_color_stop(gpui::rgb(0x444444), 0.),
                        gpui::linear_color_stop(gpui::rgb(0x262626), 1.),
                    ))
                })
        };
        let heading = |text: &'static str| {
            div()
                .pt(px(if text == "Extensions" { 8. } else { 13. }))
                .pb(px(8.))
                .text_size(px(14.))
                .line_height(px(21.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(text)
        };
        div()
            .id("site-controls-backdrop")
            .debug_selector(|| "site-controls-backdrop".into())
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&panel.focus)
            .on_action(
                cx.listener(|this, _: &crate::window::OpenCommandBar, _, cx| {
                    this.site_command_bar(false, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::window::EditCurrentUrl, _, cx| {
                    this.site_command_bar(true, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.dismiss_site_controls(cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| {
                if let Some(panel) = &mut this.site_controls {
                    if panel.more {
                        panel.selected = (panel.selected + 1) % 7;
                        cx.notify();
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &Previous, _, cx| {
                if let Some(panel) = &mut this.site_controls {
                    if panel.more {
                        panel.selected = (panel.selected + 6) % 7;
                        cx.notify();
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                if let Some(index) = this
                    .site_controls
                    .as_ref()
                    .filter(|panel| panel.more)
                    .map(|panel| panel.selected)
                {
                    this.site_menu_action(index, cx);
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_site_controls(cx)),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, _, _, cx| this.close_site_controls(cx)),
            )
            .child(
                self.site_panel_shell(panel, "site-controls")
                    .absolute()
                    .left(panel.position.x)
                    .top(panel.position.y)
                    .w(px(WIDTH))
                    .p(px(PADDING))
                    .pt(px(9.))
                    .flex()
                    .flex_col()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .gap(px(TILE_GAP))
                            .child(
                                top_tile("site-share")
                                    .on_click(cx.listener(
                                        |this, event: &gpui::ClickEvent, _, cx| {
                                            this.share_current_site(event.position(), cx);
                                        },
                                    ))
                                    .child(crate::icons::control_sized(Icon::Share, 22.)),
                            )
                            .child(
                                top_tile("site-boost")
                                    .on_click(cx.listener(|this, _, _, cx| this.customize_page(cx)))
                                    .child(crate::icons::ionicon("brush", 22., 1.)),
                            )
                            .child(
                                top_tile("site-capture")
                                    .child(crate::icons::control_sized(Icon::Capture, 22.)),
                            )
                            .child(
                                top_tile("site-screenshot")
                                    .child(crate::icons::control_sized(Icon::Screenshot, 22.)),
                            ),
                    )
                    .child(heading("Extensions"))
                    .child(
                        div().flex().flex_wrap().gap(px(TILE_GAP)).child(
                            tile("site-add-extension")
                                .hover(|style| style.bg(hsla(0., 0., 1., 0.18)))
                                .text_color(hsla(0., 0., 1., 0.7))
                                .child(crate::icons::control(Icon::Plus)),
                        ),
                    )
                    .when(!boosts.is_empty(), |panel| {
                        panel
                            .child(heading("Boosts"))
                            .children(boosts.into_iter().enumerate().map(|(index, boost)| {
                                let id = boost.id;
                                let enabled = boost.enabled;
                                div()
                                    .id(gpui::SharedString::from(format!("site-boost-{id:?}")))
                                    .debug_selector(|| "site-boost-toggle".into())
                                    .flex()
                                    .items_center()
                                    .h(px(33.))
                                    .mt(px(if index == 0 { 0. } else { 10. }))
                                    .gap(px(8.))
                                    .cursor_pointer()
                                    .rounded(px(6.))
                                    .hover(|el| el.bg(hsla(0., 0., 1., 0.08)))
                                    .child(
                                        div()
                                            .debug_selector(|| "site-boost-indicator".into())
                                            .size(px(32.))
                                            .rounded_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(gpui::rgb(0xecedfe))
                                            .text_color(hsla(0., 0., 0.2, 1.))
                                            .child(crate::icons::ionicon("brush", 22., 1.)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(px(14.))
                                            .line_height(px(17.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(boost.name)
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        if enabled {
                                                            "site-boost-enabled".into()
                                                        } else {
                                                            "site-boost-disabled".into()
                                                        }
                                                    })
                                                    .text_size(px(12.))
                                                    .line_height(px(14.))
                                                    .font_weight(gpui::FontWeight::NORMAL)
                                                    .text_color(hsla(0., 0., 1., 0.45))
                                                    .child(if enabled {
                                                        "Enabled"
                                                    } else {
                                                        "Disabled"
                                                    }),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.edit_browser(|model| {
                                            if let Some(boost) = model
                                                .user_profile_mut(profile)
                                                .and_then(|profile| profile.boosts.get_mut(id))
                                            {
                                                boost.enabled = !boost.enabled;
                                            }
                                        });
                                        this.sync_boosts(cx);
                                    }))
                            }))
                    })
                    .child(heading("Settings"))
                    .child(div().id("site-developer-mode").debug_selector(|| "site-developer-mode".into())
                        .h(px(33.)).flex().items_center().gap(px(8.)).rounded(px(6.)).cursor_pointer()
                        .hover(|row| row.bg(hsla(0.,0.,1.,0.08)))
                        .child(div().size(px(32.)).flex_none().rounded_full().flex().items_center().justify_center()
                            .bg(if self.developer_mode_enabled() { gpui::rgb(0xecedfe).into() } else { hsla(0.,0.,1.,0.3) })
                            .text_color(hsla(0.,0.,0.2,1.)).child(crate::icons::ionicon("hammer",18.,1.)))
                        .child(div().flex().flex_col().text_size(px(14.)).line_height(px(17.)).font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Developer Mode").child(div().font_weight(gpui::FontWeight::NORMAL).text_color(hsla(0.,0.,1.,0.45))
                                .child(if self.developer_mode_enabled() { "On" } else { "Off" })))
                        .on_click(cx.listener(move |this,_,_,cx| {
                            if this.site_controls.as_ref().is_some_and(|panel| this.site_scope_is_current(&panel.scope)) {
                                this.toggle_developer_mode(cx);
                            }
                        })))
                    .when(!permissions.is_empty(), |panel| {
                        panel.children(
                            permissions
                                .into_iter()
                                .enumerate()
                                .map(|(index, permission)| {
                                    let kind = permission.kind;
                                    let enabled =
                                        permission.value == engine::PermissionValue::Allow;
                                    let scope = scope.clone();
                                    div()
                                        .id(gpui::SharedString::from(format!(
                                            "site-permission-{kind:?}"
                                        )))
                                        .debug_selector(move || format!("site-permission-{kind:?}"))
                                        .h(px(33.))
                                        .mt(px(if index == 0 { 0. } else { 10. }))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .hover(|el| el.bg(hsla(0., 0., 1., 0.08)))
                                        .child(
                                            div()
                                                .size(px(32.))
                                                .flex_none()
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .bg(if enabled {
                                                    gpui::rgb(0xecedfe).into()
                                                } else {
                                                    hsla(0., 0., 1., 0.3)
                                                })
                                                .text_color(hsla(0., 0., 0.2, 1.))
                                                .child(crate::icons::ionicon(
                                                    "settings-sharp",
                                                    18.,
                                                    1.,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .text_size(px(14.))
                                                .line_height(px(17.))
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child(permission_label(kind))
                                                .child(
                                                    div()
                                                        .text_size(px(12.))
                                                        .line_height(px(14.))
                                                        .font_weight(gpui::FontWeight::NORMAL)
                                                        .text_color(hsla(0., 0., 1., 0.45))
                                                        .child(if enabled {
                                                            "Allowed"
                                                        } else {
                                                            "Blocked"
                                                        }),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if !this.site_scope_is_current(&scope) {
                                                return;
                                            }
                                            let Some(origin) = scope.origin.as_ref() else {
                                                return;
                                            };
                                            let value = if enabled {
                                                engine::PermissionValue::Block
                                            } else {
                                                engine::PermissionValue::Allow
                                            };
                                            if !this.pages.as_ref().is_some_and(|pages| {
                                                pages.borrow_mut().set_site_permission(
                                                    scope.tab, origin, kind, value,
                                                )
                                            }) {
                                                return;
                                            }
                                            this.edit_profile(|model| {
                                                if let Some(profile) =
                                                    model.user_profile_mut(scope.profile)
                                                {
                                                    profile.site_settings.record_permission(
                                                        origin,
                                                        core_permission(kind),
                                                    );
                                                }
                                            });
                                            let info = this.pages.as_ref().and_then(|pages| {
                                                pages.borrow().site_info(scope.tab)
                                            });
                                            if let Some(panel) = &mut this.site_controls {
                                                panel.info = info;
                                            }
                                            cx.notify();
                                        }))
                                }),
                        )
                    })
                    .child(
                        div()
                            .mt(px(15.))
                            .pt(px(10.))
                            .border_t_1()
                            .border_color(hsla(0., 0., 1., 0.2))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .id("site-security")
                                    .debug_selector(|| "site-security".into())
                                    .h(px(TILE_HEIGHT))
                                    .px(px(9.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .bg(hsla(0., 0., 1., 0.1))
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(if secure {
                                        hsla(0., 0., 1., 0.85)
                                    } else {
                                        gpui::rgb(0xff4d00).into()
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.show_site_certificate(cx)
                                        }),
                                    )
                                    .child(security_symbol(secure))
                                    .child(
                                        div()
                                            .debug_selector(move || {
                                                if secure {
                                                    "site-security-secure".into()
                                                } else {
                                                    "site-security-insecure".into()
                                                }
                                            })
                                            .child(if secure { "Secure" } else { "Insecure" }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("site-more")
                                    .debug_selector(|| "site-more".into())
                                    .size(px(TILE_HEIGHT))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(hsla(0., 0., 1., 0.1))
                                    .hover(|style| style.bg(hsla(0., 0., 1., 0.18)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(panel) = &mut this.site_controls {
                                            panel.more = !panel.more;
                                            panel.selected = 0;
                                            cx.notify();
                                        }
                                    }))
                                    .child(crate::icons::more())
                                    .child(
                                        gpui::canvas(
                                            |_, _, _| (),
                                            move |bounds, _, _, _| more_anchor.set(Some(bounds)),
                                        )
                                        .absolute()
                                        .inset_0(),
                                    ),
                            ),
                    ),
            )
            .child(site_arrow(arrow_at))
            .when(more, |root| {
                root.child(div().absolute().inset_0().on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(panel) = &mut this.site_controls {
                            panel.more = false;
                            cx.notify();
                        }
                    }),
                ))
                .child(self.render_site_more(menu_position, cx))
            })
            .into_any_element()
    }
    fn render_site_more(&self, position: Point<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let selected = self
            .site_controls
            .as_ref()
            .map(|panel| panel.selected)
            .unwrap_or(0);
        let mut menu = crate::panel::menu_shell(
            "site-more-menu",
            "site-more",
            self.accent(),
            Default::default(),
        )
        .debug_selector(|| "site-more-menu".into())
        .absolute()
        .left(position.x)
        .top(position.y)
        .w(px(162.))
        .py(px(5.))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (index, label) in [
            "Clear Cache",
            "Clear Cookies",
            "Manage Boosts…",
            "New Boost…",
            "Manage Extensions…",
            "Add Extension…",
            "All Site Settings…",
        ]
        .into_iter()
        .enumerate()
        {
            if matches!(index, 2 | 4 | 6) {
                menu = menu.child(
                    div()
                        .h(px(11.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .child(div().w_full().h(px(1.)).bg(hsla(0., 0., 1., 0.2))),
                );
            }
            menu = menu.child(
                div()
                    .id(("site-more-action", index))
                    .debug_selector(move || format!("site-more-action-{index}"))
                    .mx(px(5.))
                    .px(px(12.))
                    .h(px(24.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .when(index == selected, |row| {
                        row.bg(gpui::rgb(0x007aff)).text_color(gpui::white())
                    })
                    .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                        if *over {
                            if let Some(panel) = &mut this.site_controls {
                                panel.selected = index;
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| this.site_menu_action(index, cx)))
                    .child(label),
            );
        }
        menu.into_any_element()
    }
}

pub(crate) fn core_permission(
    kind: engine::PermissionKind,
) -> browser_core::site_settings::Permission {
    use browser_core::site_settings::Permission as C;
    match kind {
        engine::PermissionKind::Camera => C::Camera,
        engine::PermissionKind::Microphone => C::Microphone,
        engine::PermissionKind::Location => C::Location,
        engine::PermissionKind::Notifications => C::Notifications,
        engine::PermissionKind::Popups => C::Popups,
        engine::PermissionKind::Clipboard => C::Clipboard,
        engine::PermissionKind::AutomaticDownloads => C::AutomaticDownloads,
        engine::PermissionKind::Sound => C::Sound,
        engine::PermissionKind::MotionSensors => C::MotionSensors,
        engine::PermissionKind::Javascript => C::Javascript,
        engine::PermissionKind::Images => C::Images,
        engine::PermissionKind::IntrusiveAds => C::IntrusiveAds,
        engine::PermissionKind::BackgroundSync => C::BackgroundSync,
        engine::PermissionKind::MidiDevices => C::MidiDevices,
        engine::PermissionKind::UsbDevices => C::UsbDevices,
        engine::PermissionKind::SerialPorts => C::SerialPorts,
        engine::PermissionKind::FileEditing => C::FileEditing,
        engine::PermissionKind::HidDevices => C::HidDevices,
        engine::PermissionKind::PaymentHandlers => C::PaymentHandlers,
        engine::PermissionKind::InsecureContent => C::InsecureContent,
        engine::PermissionKind::JavascriptOptimization => C::JavascriptOptimization,
        engine::PermissionKind::AugmentedReality => C::AugmentedReality,
        engine::PermissionKind::VirtualReality => C::VirtualReality,
        engine::PermissionKind::DeviceUse => C::DeviceUse,
        engine::PermissionKind::WindowManagement => C::WindowManagement,
        engine::PermissionKind::Fonts => C::Fonts,
        engine::PermissionKind::AutomaticPictureInPicture => C::AutomaticPictureInPicture,
        engine::PermissionKind::CapturedSurfaceControl => C::CapturedSurfaceControl,
        engine::PermissionKind::WebAppInstallation => C::WebAppInstallation,
        engine::PermissionKind::LocalNetwork => C::LocalNetwork,
        engine::PermissionKind::LoopbackNetwork => C::LoopbackNetwork,
    }
}
pub(crate) fn permission_label(kind: engine::PermissionKind) -> &'static str {
    match kind {
        engine::PermissionKind::Camera => "Camera",
        engine::PermissionKind::Microphone => "Microphone",
        engine::PermissionKind::Location => "Location",
        engine::PermissionKind::Notifications => "Notifications",
        engine::PermissionKind::Popups => "Pop-ups and redirects",
        engine::PermissionKind::Clipboard => "Clipboard",
        engine::PermissionKind::AutomaticDownloads => "Automatic downloads",
        engine::PermissionKind::Sound => "Sound",
        engine::PermissionKind::MotionSensors => "Motion sensors",
        engine::PermissionKind::Javascript => "JavaScript",
        engine::PermissionKind::Images => "Images",
        engine::PermissionKind::IntrusiveAds => "Intrusive ads",
        engine::PermissionKind::BackgroundSync => "Background sync",
        engine::PermissionKind::MidiDevices => "MIDI device control & reprogram",
        engine::PermissionKind::UsbDevices => "USB devices",
        engine::PermissionKind::SerialPorts => "Serial ports",
        engine::PermissionKind::FileEditing => "File editing",
        engine::PermissionKind::HidDevices => "HID devices",
        engine::PermissionKind::PaymentHandlers => "Payment handlers",
        engine::PermissionKind::InsecureContent => "Insecure content",
        engine::PermissionKind::JavascriptOptimization => "JavaScript optimization & security",
        engine::PermissionKind::AugmentedReality => "Augmented reality",
        engine::PermissionKind::VirtualReality => "Virtual reality",
        engine::PermissionKind::DeviceUse => "Your device use",
        engine::PermissionKind::WindowManagement => "Window management",
        engine::PermissionKind::Fonts => "Fonts",
        engine::PermissionKind::AutomaticPictureInPicture => "Automatic picture-in-picture",
        engine::PermissionKind::CapturedSurfaceControl => "Scrolling and zooming shared tabs",
        engine::PermissionKind::WebAppInstallation => "Web app installations",
        engine::PermissionKind::LocalNetwork => "Local network",
        engine::PermissionKind::LoopbackNetwork => "Apps on device",
    }
}

fn security_symbol(secure: bool) -> AnyElement {
    if secure {
        return crate::icons::control_sized(Icon::Lock, 14.);
    }
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let at = |x, y| bounds.origin + point(px(x), px(y));
            let mut path = gpui::PathBuilder::stroke(px(1.2));
            path.move_to(at(4., 7.));
            path.line_to(at(4., 5.));
            path.cubic_bezier_to(at(10., 5.), at(4., 1.), at(10., 1.));
            path.line_to(at(10., 7.));
            path.move_to(at(3., 7.));
            path.line_to(at(11., 7.));
            path.line_to(at(11., 13.));
            path.line_to(at(3., 13.));
            path.close();
            path.move_to(at(1.5, 1.5));
            path.line_to(at(12.5, 14.));
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .size(px(14.))
    .flex_none()
    .into_any_element()
}

fn site_backdrop_config() -> crate::material::BackdropConfig {
    use crate::material::*;
    BackdropConfig {
        appearance: EffectAppearance::Dark,
        state: EffectState::Active,
        blending: EffectBlending::WithinWindow,
        emphasized: false,
        effect_alpha: 1.,
        material: EffectMaterial::Popover,
    }
}

pub(crate) fn permission_icon(kind: engine::PermissionKind) -> &'static str {
    match kind {
        engine::PermissionKind::Camera => "camera",
        engine::PermissionKind::Microphone => "mic",
        engine::PermissionKind::Location => "location",
        engine::PermissionKind::Notifications => "notifications",
        engine::PermissionKind::Popups => "browsers",
        engine::PermissionKind::Clipboard => "clipboard",
        engine::PermissionKind::AutomaticDownloads => "download",
        engine::PermissionKind::Sound => "volume-high",
        engine::PermissionKind::MotionSensors => "radio",
        engine::PermissionKind::Javascript => "code",
        engine::PermissionKind::Images => "image",
        engine::PermissionKind::IntrusiveAds => "browsers",
        engine::PermissionKind::BackgroundSync => "sync",
        engine::PermissionKind::MidiDevices => "musical-notes",
        engine::PermissionKind::UsbDevices => "hardware-chip",
        engine::PermissionKind::SerialPorts => "hardware-chip",
        engine::PermissionKind::FileEditing => "document",
        engine::PermissionKind::HidDevices => "game-controller",
        engine::PermissionKind::PaymentHandlers => "card",
        engine::PermissionKind::InsecureContent => "warning",
        engine::PermissionKind::JavascriptOptimization => "speedometer",
        engine::PermissionKind::AugmentedReality => "glasses",
        engine::PermissionKind::VirtualReality => "glasses",
        engine::PermissionKind::DeviceUse => "desktop",
        engine::PermissionKind::WindowManagement => "browsers",
        engine::PermissionKind::Fonts => "text",
        engine::PermissionKind::AutomaticPictureInPicture => "albums",
        engine::PermissionKind::CapturedSurfaceControl => "laptop",
        engine::PermissionKind::WebAppInstallation => "download",
        engine::PermissionKind::LocalNetwork => "wifi",
        engine::PermissionKind::LoopbackNetwork => "desktop",
    }
}

fn site_arrow(position: Point<Pixels>) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let at = |x, y| bounds.origin + point(px(x), px(y));
            let mut path = gpui::PathBuilder::fill();
            path.move_to(at(0., 10.));
            path.line_to(at(7.5, 1.5));
            path.curve_to(at(10.5, 1.5), at(9., 0.));
            path.line_to(at(18., 10.));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, hsla(0., 0., 0.36, 0.38));
            }
        },
    )
    .absolute()
    .left(position.x)
    .top(position.y)
    .w(px(18.))
    .h(px(10.))
    .into_any_element()
}

fn permission_purpose(kinds: &[engine::PermissionKind]) -> Option<&'static str> {
    Some(match kinds {
        [engine::PermissionKind::Camera, engine::PermissionKind::Microphone] => {
            "access your camera and microphone"
        }
        [engine::PermissionKind::Camera] => "access your camera",
        [engine::PermissionKind::Microphone] => "access your microphone",
        [engine::PermissionKind::Location] => "access your location",
        [engine::PermissionKind::Notifications] => "send you notifications",
        _ => return None,
    })
}

fn wrap_permission_title(title: &str, width: f32, mut measure: impl FnMut(&str) -> f32) -> String {
    let mut opportunities: Vec<_> = title
        .char_indices()
        .filter_map(|(at, ch)| (ch == ' ' || ch == '-').then_some(at + ch.len_utf8()))
        .collect();
    opportunities.push(title.len());
    let mut start = 0;
    let mut lines = Vec::new();
    while start < title.len() {
        while title[start..].starts_with(' ') {
            start += 1;
        }
        if start == title.len() {
            break;
        }
        let mut fit = start;
        for end in opportunities.iter().copied().filter(|end| *end > start) {
            if measure(title[start..end].trim_end()) > width {
                break;
            }
            fit = end;
        }
        if fit == start {
            for (at, ch) in title[start..].char_indices() {
                let end = start + at + ch.len_utf8();
                if measure(&title[start..end]) > width && fit > start {
                    break;
                }
                fit = end;
            }
        }
        lines.push(title[start..fit].trim_end());
        start = fit;
    }
    lines.join("\n")
}

#[cfg(test)]
mod permission_title_tests {
    use super::wrap_permission_title;
    #[test]
    fn url_wrapping_keeps_scheme_and_hyphens_and_does_not_lose_utf8_text() {
        let title = "Allow http://echo-site-controls-oct24.localhost:48763 to access your camera?";
        let wrapped = wrap_permission_title(title, 30., |text| text.chars().count() as f32);
        assert!(wrapped.contains("http://"));
        assert!(wrapped.lines().any(|line| line.ends_with('-')));
        assert_eq!(
            wrapped
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>(),
            title
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        );
        let unicode = "Allow https://bücher.example to access your camera?";
        let wrapped = wrap_permission_title(unicode, 8., |text| text.chars().count() as f32);
        assert!(wrapped.lines().all(|line| line.chars().count() <= 8));
        assert_eq!(
            wrapped
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>(),
            unicode
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        );
    }
}
