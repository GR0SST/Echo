//! Arc's site-details page, implemented in GPUI; no Chromium WebUI is hosted.
use crate::{
    icons,
    site_controls::{permission_icon, permission_label},
    window::BrowserWindow,
};
use browser_core::{TabId, UserProfileId};
use engine::{PermissionKind as K, PermissionValue as V, SiteSetting};
use gpui::{actions, div, prelude::*, px, rgb, AnyElement, App, Context, KeyBinding, ScrollHandle};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
const URL: &str = "browser://settings/content/siteDetails";
const CONTEXT: &str = "SiteSettingsPage";
actions!(
    site_settings_page,
    [Dismiss, NextChoice, PreviousChoice, ConfirmChoice]
);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", NextChoice, Some(CONTEXT)),
        KeyBinding::new("up", PreviousChoice, Some(CONTEXT)),
        KeyBinding::new("enter", ConfirmChoice, Some(CONTEXT)),
    ]);
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    tab: TabId,
    profile: UserProfileId,
    origin: String,
    private: bool,
}
#[derive(Default)]
pub(crate) struct State {
    scope: Option<Scope>,
    settings: Vec<SiteSetting>,
    popup: Option<K>,
    popup_selected: usize,
    popup_offset: usize,
    reset: bool,
    delete: bool,
    pub(crate) source: Option<DataSource>,
    scroll: ScrollHandle,
    anchors: Rc<RefCell<Vec<(K, gpui::Bounds<gpui::Pixels>)>>>,
    bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    focus: Option<gpui::FocusHandle>,
    pub(crate) return_focus: Option<(gpui::WindowHandle<BrowserWindow>, std::time::Instant)>,
}
#[derive(Clone)]
pub(crate) struct DataSource {
    owner: gpui::WeakEntity<BrowserWindow>,
    tab: TabId,
    profile: UserProfileId,
    origin: String,
}
impl DataSource {
    pub(crate) fn capture(view: &BrowserWindow, cx: &Context<BrowserWindow>) -> Option<Self> {
        let tab = view.browser.active_tab()?;
        Some(Self {
            owner: cx.weak_entity(),
            tab: tab.id,
            profile: view.browser.active_user_profile_id(),
            origin: browser_core::site_settings::SiteSettings::origin(&tab.url)?,
        })
    }
    fn current(&self, view: &BrowserWindow) -> bool {
        view.browser.user_profile_id_for_tab(self.tab) == Some(self.profile)
            && view
                .browser
                .all_tabs()
                .find(|tab| tab.id == self.tab)
                .is_some_and(|tab| {
                    browser_core::site_settings::SiteSettings::origin(&tab.url).as_ref()
                        == Some(&self.origin)
                })
    }
}
pub(crate) fn site_url(origin: &str) -> Option<String> {
    let origin = browser_core::site_settings::SiteSettings::origin(origin)?;
    let mut url = url::Url::parse(URL).ok()?;
    url.query_pairs_mut().append_pair("site", &origin);
    Some(url.into())
}
pub(crate) fn is_site_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "browser"
            && u.host_str() == Some("settings")
            && u.path() == "/content/siteDetails"
            && u.query_pairs().any(|(k, v)| {
                k == "site" && browser_core::site_settings::SiteSettings::origin(&v).is_some()
            })
    })
}
const ROWS: [K; 31] = [
    K::Location,
    K::Camera,
    K::Microphone,
    K::MotionSensors,
    K::Notifications,
    K::Javascript,
    K::Images,
    K::Popups,
    K::IntrusiveAds,
    K::BackgroundSync,
    K::Sound,
    K::AutomaticDownloads,
    K::MidiDevices,
    K::UsbDevices,
    K::SerialPorts,
    K::FileEditing,
    K::HidDevices,
    K::Clipboard,
    K::PaymentHandlers,
    K::InsecureContent,
    K::JavascriptOptimization,
    K::AugmentedReality,
    K::VirtualReality,
    K::DeviceUse,
    K::WindowManagement,
    K::Fonts,
    K::AutomaticPictureInPicture,
    K::CapturedSurfaceControl,
    K::WebAppInstallation,
    K::LocalNetwork,
    K::LoopbackNetwork,
];
fn value_label(kind: K, value: V) -> &'static str {
    match value {
        V::Allow => "Allow",
        V::Block if kind == K::Sound => "Mute",
        V::Block => "Block",
        V::Ask => "Ask",
        V::Default => "Default",
    }
}
fn default_label(setting: &SiteSetting) -> String {
    if setting.kind == K::Sound && setting.default_value == V::Allow {
        "Automatic (default)".into()
    } else {
        format!(
            "{} (default)",
            value_label(setting.kind, setting.default_value)
        )
    }
}
fn choices(setting: &SiteSetting) -> Vec<V> {
    let mut values = vec![V::Default];
    for v in [V::Allow, V::Block, V::Ask] {
        if setting.kind.accepts(v)
            && (v != V::Ask
                || matches!(
                    setting.kind,
                    K::UsbDevices | K::SerialPorts | K::FileEditing | K::HidDevices
                )
                || (setting.value == V::Ask && setting.uses_default == Some(false)))
        {
            values.push(v);
        }
    }
    values
}
impl BrowserWindow {
    fn settings_scope(&self) -> Option<Scope> {
        let tab = self.browser.active_tab()?;
        if !is_site_url(&tab.url) {
            return None;
        }
        let url = url::Url::parse(&tab.url).ok()?;
        let origin = url.query_pairs().find(|(k, _)| k == "site")?.1.into_owned();
        Some(Scope {
            tab: tab.id,
            profile: self.browser.active_user_profile_id(),
            private: self.browser.is_private(),
            origin: browser_core::site_settings::SiteSettings::origin(&origin)?,
        })
    }
    pub(crate) fn focus_site_settings(&self, window: &mut gpui::Window) -> bool {
        if self.settings_scope().is_some() {
            if let Some(focus) = &self.site_settings_page.focus {
                focus.focus(window);
                return true;
            }
        }
        false
    }
    pub(crate) fn site_settings_is_open(&self) -> bool {
        self.settings_scope().is_some()
    }
    pub(crate) fn sync_site_settings(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let scope = self.settings_scope();
        if self.site_settings_page.scope == scope {
            return;
        }
        let source = self.site_settings_page.source.take().filter(|source| {
            scope.as_ref().is_some_and(|scope| {
                source.profile == scope.profile && source.origin == scope.origin
            })
        });
        self.site_settings_page = State {
            source,
            scope: scope.clone(),
            focus: Some(cx.focus_handle()),
            ..Default::default()
        };
        let Some(scope) = scope else {
            return;
        };
        self.site_settings_page
            .focus
            .as_ref()
            .unwrap()
            .focus(window);
        if let Some(pages) = &self.pages {
            pages.borrow_mut().focus_ui();
        }
        self.refresh_site_settings(&scope, cx);
        cx.spawn(async move |owner, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            if !owner
                .update(cx, |view, cx| {
                    if view.settings_scope().as_ref() != Some(&scope) || crate::quit::is_quitting()
                    {
                        return false;
                    }
                    view.refresh_site_settings(&scope, cx);
                    true
                })
                .unwrap_or(false)
            {
                break;
            }
        })
        .detach();
    }
    fn settings_profile_exists(&self, scope: &Scope) -> bool {
        let profile = self
            .transient
            .as_ref()
            .map(|state| &state.source.profile)
            .unwrap_or(&self.profile.handle);
        profile.read_settings(|browser| browser.user_profile(scope.profile).is_some())
    }
    fn refresh_site_settings(&mut self, scope: &Scope, cx: &mut Context<Self>) {
        let settings = self
            .pages
            .as_ref()
            .and_then(|pages| {
                pages
                    .borrow()
                    .profile_site_settings(scope.profile, scope.private, &scope.origin)
            })
            .unwrap_or_default();
        if settings != self.site_settings_page.settings {
            self.site_settings_page.settings = settings;
            cx.notify();
        }
    }
    pub(crate) fn open_all_site_settings(&mut self, cx: &mut Context<Self>) {
        let Some(origin) = self
            .browser
            .active_tab()
            .and_then(|tab| browser_core::site_settings::SiteSettings::origin(&tab.url))
        else {
            return;
        };
        let tab = self.browser.active_tab().unwrap().id;
        let profile = self.browser.active_user_profile_id();
        let expected_origin = origin.clone();
        let Some(url) = site_url(&origin) else {
            return;
        };
        self.close_site_controls_with_focus(crate::window::PopupFocus::OnActivation, cx);
        let owner = cx.weak_entity();
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = owner.update(cx, |view, cx| {
                    view.refresh_profile_projection();
                    if view.browser.active_user_profile_id() != profile
                        || !view.browser.active_tab().is_some_and(|active| {
                            active.id == tab
                                && browser_core::site_settings::SiteSettings::origin(&active.url)
                                    .as_ref()
                                    == Some(&expected_origin)
                        })
                    {
                        return;
                    }
                    view.open_site_settings_peek(url, window, cx)
                });
            });
        });
    }
    fn change_site_setting(&mut self, scope: &Scope, kind: K, value: V, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if !self.settings_profile_exists(scope)
            || self.settings_scope().as_ref() != Some(scope)
            || !self
                .site_settings_page
                .settings
                .iter()
                .any(|s| s.kind == kind && choices(s).contains(&value))
        {
            return;
        }
        let accepted = self.pages.as_ref().is_some_and(|pages| {
            pages.borrow_mut().set_profile_site_setting(
                scope.profile,
                scope.private,
                &scope.origin,
                kind,
                value,
            )
        });
        if accepted {
            self.site_settings_page.popup = None;
            let profile = self
                .transient
                .as_ref()
                .map(|state| state.source.profile.clone())
                .unwrap_or_else(|| self.profile.handle.clone());
            profile.edit_settings(|browser| {
                if let Some(profile) = browser.user_profile_mut(scope.profile) {
                    if value == V::Default {
                        profile.site_settings.forget_permission(
                            &scope.origin,
                            crate::site_controls::core_permission(kind),
                        );
                    } else {
                        profile.site_settings.record_permission(
                            &scope.origin,
                            crate::site_controls::core_permission(kind),
                        );
                    }
                }
            });
            self.refresh_site_settings(scope, cx);
            cx.notify();
        }
    }
    fn reset_site_settings(&mut self, scope: &Scope, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if !self.settings_profile_exists(scope) || self.settings_scope().as_ref() != Some(scope) {
            return;
        }
        let accepted = self.pages.as_ref().is_some_and(|pages| {
            let mut pages = pages.borrow_mut();
            ROWS.into_iter().all(|kind| {
                pages.set_profile_site_setting(
                    scope.profile,
                    scope.private,
                    &scope.origin,
                    kind,
                    V::Default,
                )
            })
        });
        if accepted {
            self.site_settings_page.reset = false;
            let profile = self
                .transient
                .as_ref()
                .map(|state| state.source.profile.clone())
                .unwrap_or_else(|| self.profile.handle.clone());
            profile.edit_settings(|browser| {
                if let Some(profile) = browser.user_profile_mut(scope.profile) {
                    profile.site_settings.forget_permissions(&scope.origin);
                }
            });
            self.refresh_site_settings(scope, cx);
            cx.notify();
        }
    }
    fn delete_site_data(&mut self, scope: &Scope, cx: &mut Context<Self>) {
        if !self.settings_profile_exists(scope) || self.settings_scope().as_ref() != Some(scope) {
            return;
        }
        let Some(source) = self
            .site_settings_page
            .source
            .clone()
            .filter(|source| source.profile == scope.profile && source.origin == scope.origin)
        else {
            return;
        };
        let accepted = source
            .owner
            .update(cx, |view, cx| {
                view.refresh_profile_projection();
                if !source.current(view) {
                    return false;
                }
                let accepted = view.pages.as_ref().is_some_and(|pages| {
                    pages.borrow_mut().clear_site_data(
                        source.tab,
                        &source.origin,
                        engine::ClearSiteData::Cookies,
                    )
                });
                if accepted {
                    // The engine's clear-data operation expires after five seconds.
                    view.site_settings_page.return_focus = view.peek_child.map(|child| {
                        (
                            child,
                            cx.background_executor().now() + Duration::from_secs(6),
                        )
                    });
                }
                accepted
            })
            .unwrap_or(false);
        if accepted {
            self.site_settings_page.delete = false;
            cx.notify();
        }
    }
    fn step_site_choice(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(kind) = self.site_settings_page.popup else {
            return;
        };
        let Some(setting) = self
            .site_settings_page
            .settings
            .iter()
            .find(|s| s.kind == kind)
        else {
            return;
        };
        let count = choices(setting).len() as isize;
        self.site_settings_page.popup_selected =
            (self.site_settings_page.popup_selected as isize + delta).rem_euclid(count) as usize;
        cx.notify();
    }
    pub(crate) fn render_site_settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(scope) = self.settings_scope() else {
            return div().into_any_element();
        };
        let state = &self.site_settings_page;
        let can_delete = state.source.as_ref().is_some_and(|source| {
            source
                .owner
                .read_with(cx, |view, _| source.current(view))
                .unwrap_or(false)
        });
        let mut body = div()
            .w_full()
            .max_w(px(680.))
            .mx_auto()
            .px(px(20.))
            .pb(px(40.))
            .bg(rgb(0x292a2d))
            .child(
                div()
                    .mt(px(12.))
                    .ml(px(-10.))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .id("site-settings-back")
                            .size(px(32.))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(0x8ab4f8))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(0x8ab4f8))
                            .child(icons::control_sized(icons::Icon::Back, 20.)),
                    )
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .truncate()
                            .child(scope.origin.clone()),
                    ),
            )
            .child(
                div()
                    .mt(px(32.))
                    .h(px(16.))
                    .line_height(px(16.))
                    .text_color(rgb(0xbdc1c6))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Usage"),
            )
            .child(
                div()
                    .mt(px(6.))
                    .h(px(36.))
                    .pl(px(40.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div())
                    .child(
                        outline_button("site-settings-delete", "Delete data")
                            .when(can_delete, |button| {
                                let owner = scope.clone();
                                button.on_click(cx.listener(move |view, _, _, cx| {
                                    if view.settings_scope().as_ref() == Some(&owner) {
                                        view.site_settings_page.delete = true;
                                        view.site_settings_page.popup = None;
                                        cx.notify();
                                    }
                                }))
                            })
                            .when(!can_delete, |button| button.opacity(0.45)),
                    ),
            )
            .child(
                div()
                    .mt(px(30.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_color(rgb(0xbdc1c6))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Permissions"),
                    )
                    .child(
                        outline_button("site-settings-reset", "Reset permissions").on_click(
                            cx.listener(|view, _, _, cx| {
                                view.site_settings_page.reset = true;
                                view.site_settings_page.popup = None;
                                cx.notify();
                            }),
                        ),
                    ),
            );
        for kind in ROWS {
            let setting = state.settings.iter().find(|s| s.kind == kind).cloned();
            let selected = state.popup == Some(kind);
            let label = setting
                .as_ref()
                .map(|s| {
                    if s.uses_default == Some(true) {
                        default_label(s)
                    } else {
                        value_label(kind, s.value).into()
                    }
                })
                .unwrap_or_default();
            let mut row = div()
                .id(gpui::SharedString::from(format!("site-setting-{kind:?}")))
                .debug_selector(move || format!("site-setting-{kind:?}"))
                .relative()
                .min_h(px(if kind == K::IntrusiveAds { 76. } else { 48. }))
                .pl(px(40.))
                .flex()
                .items_center()
                .gap(px(16.))
                .child(
                    div()
                        .w(px(20.))
                        .flex_none()
                        .text_color(rgb(0x9aa0a6))
                        .child(icons::ionicon(
                            if kind == K::Camera {
                                "videocam-outline"
                            } else {
                                permission_icon(kind)
                            },
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
                        .child(permission_label(kind))
                        .when(kind == K::IntrusiveAds, |v| {
                            v.child(
                                div()
                                    .text_color(rgb(0xbdc1c6))
                                    .line_height(px(20.))
                                    .child("Block if site shows intrusive or misleading ads"),
                            )
                        }),
                );
            let mut button = div()
                .id(gpui::SharedString::from(format!(
                    "site-setting-control-{kind:?}"
                )))
                .debug_selector(move || format!("site-setting-control-{kind:?}"))
                .w(px(200.))
                .h(px(36.))
                .flex_none()
                .px(px(10.))
                .text_size(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(if selected { 0x8ab4f8 } else { 0x76787b }))
                .flex()
                .items_center()
                .justify_between()
                .child(label)
                .child(div().text_color(rgb(0x9aa0a6)).child(icons::ionicon(
                    "caret-down-sharp",
                    10.,
                    1.,
                )));
            if let Some(setting) = setting.as_ref() {
                let owner = scope.clone();
                let values = choices(setting);
                let selected = values
                    .iter()
                    .position(|value| {
                        if *value == V::Default {
                            setting.uses_default == Some(true)
                        } else {
                            setting.value == *value && setting.uses_default != Some(true)
                        }
                    })
                    .unwrap_or(0);
                button = button.focusable().tab_stop(true).on_click(cx.listener(
                    move |view, _, window, cx| {
                        view.focus_site_settings(window);
                        if view.settings_scope().as_ref() != Some(&owner) {
                            return;
                        }
                        view.site_settings_page.popup =
                            if view.site_settings_page.popup == Some(kind) {
                                None
                            } else {
                                Some(kind)
                            };
                        view.site_settings_page.popup_selected = selected;
                        view.site_settings_page.popup_offset = selected;
                        cx.notify();
                    },
                ));
            }
            let anchors = state.anchors.clone();
            button = button.relative().child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, _, _| {
                        let mut anchors = anchors.borrow_mut();
                        anchors.retain(|(k, _)| *k != kind);
                        anchors.push((kind, bounds));
                    },
                )
                .absolute()
                .inset_0(),
            );
            row = row.child(button);
            body = body.child(row);
        }
        let body = div()
            .id("site-settings-scroll")
            .debug_selector(|| "site-settings-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .px(px(16.))
            .child(body);
        let painted_bounds = state.bounds.clone();
        let mut root = div()
            .debug_selector(|| "site-settings-page".into())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .key_context(CONTEXT)
            .when_some(state.focus.as_ref(), |root, focus| root.track_focus(focus))
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .text_color(rgb(0xe8eaed))
            .bg(rgb(0x202124))
            .on_action(cx.listener(|view, _: &NextChoice, _, cx| view.step_site_choice(1, cx)))
            .on_action(cx.listener(|view, _: &PreviousChoice, _, cx| view.step_site_choice(-1, cx)))
            .on_action(cx.listener(|view, _: &ConfirmChoice, _, cx| {
                let Some(scope) = view.settings_scope() else {
                    return;
                };
                let Some(kind) = view.site_settings_page.popup else {
                    return;
                };
                let Some(value) = view
                    .site_settings_page
                    .settings
                    .iter()
                    .find(|s| s.kind == kind)
                    .and_then(|s| {
                        choices(s)
                            .get(view.site_settings_page.popup_selected)
                            .copied()
                    })
                else {
                    return;
                };
                view.change_site_setting(&scope, kind, value, cx);
            }))
            .on_action(cx.listener(|view, _: &Dismiss, _, cx| {
                if view.site_settings_page.reset
                    || view.site_settings_page.delete
                    || view.site_settings_page.popup.is_some()
                {
                    view.site_settings_page.reset = false;
                    view.site_settings_page.delete = false;
                    view.site_settings_page.popup = None;
                    cx.notify();
                } else {
                    view.exit_peek(cx);
                }
            }))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |painted, _, _, _| painted_bounds.set(Some(painted)),
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .h(px(56.))
                    .flex_none()
                    .px(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .bg(rgb(0x202124))
                    .child(icons::ionicon("menu", 20., 1.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(22.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Settings"),
                    )
                    .child(icons::control_sized(icons::Icon::Search, 18.)),
            )
            .child(body);
        if let Some((kind, setting, anchor, bounds)) = state.popup.and_then(|kind| {
            Some((
                kind,
                state.settings.iter().find(|s| s.kind == kind)?,
                state.anchors.borrow().iter().find(|(k, _)| *k == kind)?.1,
                state.bounds.get()?,
            ))
        }) {
            let height = px(choices(setting).len() as f32 * 22. + 10.);
            let top = (anchor.top() - bounds.top() - px(4.) - px(state.popup_offset as f32 * 22.))
                .min(bounds.size.height - height - px(8.))
                .max(px(0.));
            let mut menu = div()
                .id("site-setting-options")
                .debug_selector(|| "site-setting-options".into())
                .absolute()
                .left(anchor.left() - bounds.left())
                .top(top)
                .w(px(200.))
                .py(px(5.))
                .rounded(px(12.))
                .bg(rgb(0x3c4043))
                .shadow_lg()
                .occlude();
            for (index, value) in choices(setting).into_iter().enumerate() {
                let owner = scope.clone();
                let label = if value == V::Default {
                    default_label(setting)
                } else {
                    value_label(kind, value).into()
                };
                menu = menu.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "site-setting-choice-{value:?}"
                        )))
                        .debug_selector(move || format!("site-setting-choice-{value:?}"))
                        .h(px(22.))
                        .mx(px(5.))
                        .px(px(6.))
                        .rounded(px(5.))
                        .gap(px(6.))
                        .when(index == state.popup_selected, |row| row.bg(rgb(0x007aff)))
                        .flex()
                        .items_center()
                        .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                            if *hovered {
                                view.site_settings_page.popup_selected = index;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.change_site_setting(&owner, kind, value, cx)
                        }))
                        .child(div().w(px(12.)).child(
                            if (value == V::Default && setting.uses_default == Some(true))
                                || (value != V::Default
                                    && setting.uses_default != Some(true)
                                    && setting.value == value)
                            {
                                icons::control_sized(icons::Icon::Check, 12.)
                            } else {
                                div().into_any_element()
                            },
                        ))
                        .child(label),
                );
            }
            root = root
                .child(div().absolute().inset_0().on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        cx.stop_propagation();
                        view.site_settings_page.popup = None;
                        cx.notify();
                    }),
                ))
                .child(menu);
        }
        if state.reset || state.delete {
            let deleting = state.delete;
            let owner = scope.clone();
            let mut dialog = div()
                .w(px(512.))
                .px(px(20.))
                .pt(px(20.))
                .pb(px(16.))
                .rounded(px(8.))
                .bg(rgb(0x292a2d))
                .shadow_lg()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(16.))
                        .line_height(px(20.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(if deleting {
                            "Delete site data?"
                        } else {
                            "Reset permissions?"
                        }),
                )
                .child(div().mt(px(12.)).line_height(px(20.)).child(if deleting {
                    format!(
                        "All data and cookies stored by {} will be deleted.",
                        scope.origin
                    )
                } else {
                    format!(
                        "The next time you visit, {} will use default permissions",
                        scope.origin
                    )
                }));
            if deleting {
                let mut details = div().mt(px(8.)).flex().flex_col();
                for (icon, label) in [
                    (
                        "person-circle",
                        "You’ll be signed out of this site, including in open tabs",
                    ),
                    ("checkmark-circle", "Any offline data will be deleted"),
                    (
                        "pricetag",
                        "Data that affects ad personalization is deleted",
                    ),
                ] {
                    details = details.child(
                        div()
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .gap(px(16.))
                            .child(
                                div()
                                    .text_color(rgb(0x9aa0a6))
                                    .child(icons::ionicon(icon, 18., 1.)),
                            )
                            .child(label),
                    );
                }
                dialog = dialog.child(details);
            }
            dialog = dialog.child(
                div()
                    .mt(px(if deleting { 12. } else { 16. }))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        outline_button("site-reset-cancel", "Cancel").on_click(cx.listener(
                            |view, _, _, cx| {
                                view.site_settings_page.reset = false;
                                view.site_settings_page.delete = false;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        outline_button(
                            if deleting {
                                "site-delete-confirm"
                            } else {
                                "site-reset-confirm"
                            },
                            if deleting { "Delete" } else { "Reset" },
                        )
                        .bg(rgb(0xa8c7fa))
                        .text_color(rgb(0x202124))
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                if deleting {
                                    view.delete_site_data(&owner, cx)
                                } else {
                                    view.reset_site_settings(&owner, cx)
                                }
                                window.activate_window();
                                if let Some(pages) = &view.pages {
                                    pages.borrow_mut().focus_ui();
                                }
                                view.focus_site_settings(window);
                            },
                        )),
                    ),
            );
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::black().opacity(0.6))
                    .occlude()
                    .child(dialog),
            );
        }
        root.into_any_element()
    }
}
fn outline_button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(36.))
        .px(px(16.))
        .rounded_full()
        .border_1()
        .border_color(rgb(0x3f8fc3))
        .text_color(rgb(0xa8c7fa))
        .flex()
        .items_center()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label)
}
