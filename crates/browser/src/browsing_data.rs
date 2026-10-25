//! Arc's local Clear Browsing Data surface, rendered entirely in GPUI.
use crate::{
    icons::{self, Icon},
    window::BrowserWindow,
};
use browser_core::{browsing_data::BrowsingDataRange as Range, TabId, UserProfileId};
use gpui::{div, prelude::*, px, rgb, AnyElement, App, Context, FocusHandle, KeyBinding, Window};
use std::time::{Duration, SystemTime};
pub(crate) const PRIVACY: &str = "browser://settings/privacy";
pub(crate) const CLEAR: &str = "browser://settings/clearBrowserData";
const LABELS: [&str; 7] = [
    "Browsing history",
    "Cookies and other site data",
    "Cached images and files",
    "Download history",
    "Autofill form data",
    "Site settings",
    "Hosted app data",
];
gpui::actions!(browsing_data, [Cancel, Next, Previous, Activate]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some("BrowsingData")),
        KeyBinding::new("tab", Next, Some("BrowsingData")),
        KeyBinding::new("shift-tab", Previous, Some("BrowsingData")),
        KeyBinding::new("enter", Activate, Some("BrowsingData")),
        KeyBinding::new("space", Activate, Some("BrowsingData")),
    ]);
}
#[derive(Clone, PartialEq, Eq)]
struct Scope {
    tab: TabId,
    profile: UserProfileId,
}
pub(crate) struct State {
    scope: Option<Scope>,
    primary: Vec<usize>,
    counters: Option<engine::BrowsingDataCounters>,
    count_range: Option<Range>,
    pub(crate) range: Range,
    pub(crate) mask: u32,
    pub(crate) expanded: bool,
    more: bool,
    pub(crate) manage: bool,
    busy: bool,
    focus: FocusHandle,
    controls: Vec<FocusHandle>,
    scroll: gpui::ScrollHandle,
}
impl State {
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        Self {
            scope: None,
            primary: vec![0, 1, 2],
            counters: None,
            count_range: None,
            range: Range::Hour,
            mask: 7,
            expanded: false,
            more: false,
            manage: false,
            busy: false,
            focus: cx.focus_handle(),
            controls: (0..20).map(|_| cx.focus_handle()).collect(),
            scroll: gpui::ScrollHandle::new(),
        }
    }
}
pub(crate) fn is_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "browser"
            && u.host_str() == Some("settings")
            && matches!(u.path(), "/privacy" | "/clearBrowserData")
    })
}
impl BrowserWindow {
    fn data_scope(&self) -> Option<Scope> {
        if self.browser.is_private()
            || self.browser.is_blank()
            || self.browser.active_split().is_some()
        {
            return None;
        }
        let tab = self.browser.active_tab()?;
        if !is_url(&tab.url) {
            return None;
        }
        let url = url::Url::parse(&tab.url).ok()?;
        let profile = match url.query_pairs().find(|(k, _)| k == "profile") {
            Some((_, v)) => serde_json::from_str(&v).ok()?,
            None => self.browser.active_user_profile_id(),
        };
        self.profile
            .handle
            .read_settings(|b| b.user_profile(profile).is_some())
            .then_some(Scope {
                tab: tab.id,
                profile,
            })
    }
    pub(crate) fn browsing_data_is_open(&self) -> bool {
        self.data_scope().is_some()
    }
    fn data_dialog(&self) -> bool {
        self.browser
            .active_tab()
            .is_some_and(|t| t.url.starts_with(CLEAR))
    }
    pub(crate) fn sync_browsing_data(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.data_scope();
        if self.browsing_data.scope != scope {
            self.browsing_data = State::new(cx);
            self.browsing_data.scope = scope;
            if let Some(scope) = &self.browsing_data.scope {
                let prefs = self.profile.handle.read_settings(|b| {
                    b.user_profile(scope.profile).unwrap().browsing_data.clone()
                });
                self.browsing_data.range = prefs.range;
                self.browsing_data.mask = prefs.categories & 127;
                self.browsing_data.primary = (0..7)
                    .filter(|i| *i < 3 || prefs.categories & (1 << i) != 0)
                    .collect();
            }
            if self.data_dialog() {
                // Arc's dialog ignores the Enter that navigated to this route.
                // Defer autofocus so that keystroke cannot activate Cancel.
                let owner = self.browsing_data.scope.clone();
                let handle = self
                    .main_window
                    .downcast::<BrowserWindow>()
                    .expect("Browsing-data root window");
                cx.spawn(async move |_, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(32))
                        .await;
                    let _ = handle.update(cx, |this, window, cx| {
                        if this.data_dialog()
                            && this.data_scope() == owner
                            && this.command_bar.is_none()
                            && this.tab_menu.is_none()
                        {
                            this.browsing_data.controls[18].focus(window);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
        self.start_data_counters(cx);
    }
    fn start_data_counters(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.data_scope() else {
            return;
        };
        if !self.data_dialog() || self.browsing_data.count_range == Some(self.browsing_data.range) {
            return;
        }
        let range = self.browsing_data.range;
        let since = range
            .since(SystemTime::now())
            .map(|s| {
                s.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_micros()
                    .min(i64::MAX as u128) as i64
            })
            .unwrap_or(-1);
        let Some(counters) = self
            .pages
            .as_ref()
            .and_then(|p| p.borrow().browsing_data_counters(scope.profile, since).ok())
        else {
            return;
        };
        self.browsing_data.counters = Some(counters);
        self.browsing_data.count_range = Some(range);
        cx.spawn(async move |this, cx| {
            for _ in 0..100 {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        if this.data_scope().as_ref() != Some(&scope)
                            || this.browsing_data.range != range
                        {
                            return false;
                        }
                        cx.notify();
                        this.browsing_data
                            .counters
                            .as_ref()
                            .is_some_and(|r| r.snapshot().iter().any(Option::is_none))
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }
    fn data_route(&mut self, dialog: bool, cx: &mut Context<Self>) {
        let Some(s) = self.data_scope() else {
            return;
        };
        let url = format!(
            "{}?profile={}",
            if dialog { CLEAR } else { PRIVACY },
            serde_json::to_string(&s.profile).unwrap()
        );
        self.edit_browser(|b| {
            b.navigate(s.tab, url);
        });
        self.browsing_data.more = false;
        if dialog {
            if let Some(prefs) = self
                .profile
                .handle
                .read_settings(|b| b.user_profile(s.profile).map(|p| p.browsing_data.clone()))
            {
                self.browsing_data.range = prefs.range;
                self.browsing_data.mask = prefs.categories & 127;
                self.browsing_data.primary = (0..7)
                    .filter(|i| *i < 3 || prefs.categories & (1 << i) != 0)
                    .collect();
            }
            self.browsing_data.expanded = false;
            self.browsing_data.manage = false;
            self.browsing_data.scroll = gpui::ScrollHandle::new();

            self.browsing_data.count_range = None;
            self.browsing_data.counters = None;
        }
        cx.notify();
    }
    fn data_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browsing_data.busy || self.data_scope().is_none() || self.command_bar.is_some() {
            return;
        }
        let return_focus = if self.browsing_data.more {
            self.browsing_data.more = false;
            self.browsing_data.controls[4].clone()
        } else if self.browsing_data.manage {
            self.browsing_data.manage = false;
            self.browsing_data.controls[13].clone()
        } else {
            self.data_route(false, cx);
            self.browsing_data.controls[16].clone()
        };
        return_focus.focus(window);
        cx.notify();
    }
    fn data_focus(&self, back: bool, window: &mut Window) {
        let mut indices = vec![0, 1, 2, 3, 4];
        if self.browsing_data.more {
            let h = if back { 15 } else { 14 };
            self.browsing_data.controls[h].focus(window);
            return;
        }
        if self.browsing_data.manage {
            let h = if self.browsing_data.controls[17].is_focused(window) {
                18
            } else {
                17
            };
            self.browsing_data.controls[h].focus(window);
            return;
        }
        indices.extend(if self.browsing_data.expanded {
            (5..12).collect::<Vec<_>>()
        } else {
            self.browsing_data.primary.iter().map(|i| 5 + i).collect()
        });
        if !self.browsing_data.expanded {
            indices.push(12);
        }
        indices.extend([13, 18, 19]);
        let current = indices
            .iter()
            .position(|&i| self.browsing_data.controls[i].is_focused(window));
        let n = indices.len();
        let next = match current {
            Some(i) if back => (i + n - 1) % n,
            Some(i) => (i + 1) % n,
            None => 0,
        };
        self.browsing_data.controls[indices[next]].focus(window);
    }
    fn data_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browsing_data.busy || self.data_scope().is_none() || self.command_bar.is_some() {
            return;
        }
        let i = self
            .browsing_data
            .controls
            .iter()
            .position(|h| h.is_focused(window));
        match i {
            Some(0..=3) => self.browsing_data.range = Range::ALL[i.unwrap()],
            Some(4) => self.browsing_data.more = !self.browsing_data.more,
            Some(5..=11) => self.browsing_data.mask ^= 1 << (i.unwrap() - 5),
            Some(12) => self.browsing_data.expanded = true,
            Some(13) => self.browsing_data.manage = true,
            Some(14) | Some(15) => {
                self.browsing_data.range = if i == Some(14) {
                    Range::FourWeeks
                } else {
                    Range::AllTime
                };
                self.browsing_data.more = false;
                self.browsing_data.controls[4].focus(window);
            }
            Some(16) if !self.browsing_data.manage => {
                self.data_route(true, cx);
                self.browsing_data.controls[18].focus(window);
            }
            Some(16) if self.browsing_data.manage => self.data_cancel(window, cx),
            Some(17) if self.browsing_data.manage => {
                if let Some(scope) = self.data_scope() {
                    self.open_password_manager(scope.profile, window, cx);
                }
            }
            Some(18) if self.data_dialog() => self.data_cancel(window, cx),
            Some(19) => self.accept_browsing_data(cx),
            _ => {}
        }
        cx.notify();
    }
    fn accept_browsing_data(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.data_scope() else {
            return;
        };
        if self.browsing_data.scope.as_ref() != Some(&scope)
            || self.browsing_data.busy
            || self.browsing_data.mask == 0
            || crate::quit::is_quitting()
        {
            return;
        }
        let mask = self.browsing_data.mask;
        let since = self.browsing_data.range.since(SystemTime::now());
        let timestamp = match since {
            None => -1,
            Some(s) => match s
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .and_then(|d| i64::try_from(d.as_micros()).ok())
            {
                Some(t) => t,
                None => return,
            },
        };
        let result = self
            .pages
            .as_ref()
            .ok_or_else(|| "The browser engine is unavailable".to_string())
            .and_then(|pages| {
                pages
                    .borrow_mut()
                    .clear_browsing_data(scope.profile, mask, timestamp)
            });
        let mut operation = match result {
            Ok(op) => op,
            Err(error) => {
                eprintln!("Cannot delete browsing data: {error}");
                return;
            }
        };
        self.profile.handle.edit_settings(|b| {
            if let Some(p) = b.user_profile_mut(scope.profile) {
                p.browsing_data.range = self.browsing_data.range;
                p.browsing_data.categories = mask;
            }
        });
        self.browsing_data.busy = true;
        let profile = self.profile.handle.clone();
        let lease = crate::password_flow::lease(cx);
        let owner_window = self
            .main_window
            .downcast::<BrowserWindow>()
            .expect("Browsing-data root window");
        cx.spawn(async move |_this, cx| {
            let _lease = lease;
            let result = loop {
                if let Some(result) = operation() {
                    break result;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            };
            #[cfg(debug_assertions)]
            if result.is_ok()
                && std::env::var("BROWSER_BROWSING_DATA_FIXTURE")
                    .ok()
                    .as_deref()
                    == Some("1")
            {
                eprintln!(
                    "[browsing-data-ui] profile={} categories={} since={} failed_bits=0",
                    serde_json::to_string(&scope.profile).unwrap(),
                    mask,
                    timestamp
                );
            }
            if result.is_ok() {
                profile.edit_settings(|b| b.clear_browsing_metadata(scope.profile, mask, since));
                let _ = cx.update(|cx| profile.flush_for_quit(cx));
            }
            let _ = owner_window.update(cx, |this, window, cx| {
                if this.data_scope().as_ref() == Some(&scope) {
                    this.browsing_data.busy = false;
                    if result.is_ok() {
                        this.data_route(false, cx);
                        this.browsing_data.controls[16].focus(window);
                    } else if let Err(error) = result {
                        eprintln!("Cannot delete browsing data: {error}");
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    pub(crate) fn open_browsing_data(
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
        let location = format!(
            "{PRIVACY}?profile={}",
            serde_json::to_string(&profile).unwrap()
        );
        let tab = self.edit_browser(|b| b.new_tab(location, SystemTime::now()));
        self.edit_browser(|b| b.set_title(tab, "Settings - Privacy and security"));
        self.sidebar_navigation.controller.select(
            &self.browser,
            crate::sidebar_pages::SidebarPage::Space(self.browser.space().id),
        );
        self.sync_browsing_data(window, cx);
        self.browsing_data.focus.focus(window);
        window.activate_window();
        cx.notify();
    }
    pub(crate) fn render_browsing_data(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(scope) = self.data_scope() else {
            return div().into_any_element();
        };
        let dark = self.theme_is_dark();
        let c = move |v| color(dark, v);
        let state = &self.browsing_data;
        let title = div()
            .h(px(56.))
            .px(px(24.))
            .flex()
            .items_center()
            .gap(px(16.))
            .child(icons::ionicon("menu-outline", 18., 1.))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Settings"),
            );
        let rows = [
            (
                "Delete browsing data",
                "Delete history, cookies, cache, and more",
            ),
            ("Privacy Guide", "Review key privacy and security controls"),
            ("Third-party cookies", "Third-party cookies are allowed"),
            ("Security", "Safe Browsing and other security settings"),
            (
                "Site settings",
                "Controls what information sites can use and show",
            ),
        ];
        let page = div()
            .id("browsing-data-page")
            .debug_selector(|| "browsing-data-page".into())
            .absolute()
            .inset_0()
            .whitespace_normal()
            .bg(c(0x202124))
            .text_color(c(0xe8eaed))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .line_height(px(20.))
            .key_context("BrowsingData")
            .track_focus(&state.focus)
            .on_action(cx.listener(|this, _: &Cancel, w, cx| this.data_cancel(w, cx)))
            .on_action(cx.listener(|this, _: &Next, w, _| this.data_focus(false, w)))
            .on_action(cx.listener(|this, _: &Previous, w, _| this.data_focus(true, w)))
            .on_action(cx.listener(|this, _: &Activate, w, cx| this.data_activate(w, cx)))
            .child(title)
            .child(
                div()
                    .mx_auto()
                    .mt(px(28.))
                    .w(px(680.))
                    .max_w_full()
                    .child(div().mb(px(14.)).child("Privacy and security"))
                    .child(
                        div()
                            .rounded(px(8.))
                            .overflow_hidden()
                            .bg(c(0x292a2d))
                            .children(rows.into_iter().enumerate().map(
                                |(i, (label, description))| {
                                    div()
                                        .id(("privacy-row", i))
                                        .when(i == 0, |s| {
                                            s.debug_selector(|| "delete-browsing-data-row".into())
                                                .track_focus(&state.controls[16])
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.data_route(true, cx);
                                                    this.browsing_data.controls[18].focus(window);
                                                }))
                                        })
                                        .h(px(65.))
                                        .px(px(20.))
                                        .flex()
                                        .items_center()
                                        .gap(px(20.))
                                        .when(i > 0, |s| s.border_t_1().border_color(c(0x3c4043)))
                                        .child(icons::ionicon(
                                            if i == 0 {
                                                "trash-outline"
                                            } else if i == 4 {
                                                "options-outline"
                                            } else {
                                                "shield-checkmark-outline"
                                            },
                                            20.,
                                            1.,
                                        ))
                                        .child(div().flex_1().child(label).child(
                                            div().text_color(c(0xbdc1c6)).child(description),
                                        ))
                                        .child(icons::ionicon("chevron-forward-outline", 16., 1.))
                                },
                            )),
                    ),
            );
        if !self.data_dialog() {
            return page.into_any_element();
        }
        let controls = &state.controls;
        let busy = state.busy;
        let chips = div()
            .mt(px(18.))
            .mb(px(20.))
            .flex()
            .gap(px(5.))
            .children((0..5).map(|i| {
                let scope = scope.clone();
                let selected = i < 4 && state.range == Range::ALL[i];
                let text = if i == 4 {
                    if matches!(state.range, Range::FourWeeks | Range::AllTime) {
                        state.range.label()
                    } else {
                        "More"
                    }
                } else {
                    Range::ALL[i].label()
                };
                div()
                    .id(("data-range", i))
                    .debug_selector(move || format!("data-range-{i}"))
                    .track_focus(&controls[i])
                    .h(px(28.))
                    .px(px(8.))
                    .border_1()
                    .border_color(c(0x0086bd))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .when(selected, |s| s.border_color(c(0x005780)).bg(c(0x005780)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.data_scope().as_ref() != Some(&scope) || this.browsing_data.busy {
                            return;
                        }
                        this.browsing_data.controls[i].focus(window);
                        if i == 4 {
                            this.browsing_data.more = !this.browsing_data.more;
                        } else {
                            this.browsing_data.range = Range::ALL[i];
                            this.browsing_data.more = false;
                        }
                        cx.notify();
                    }))
                    .when(selected, |s| {
                        s.child(icons::control_sized(Icon::Check, 12.))
                    })
                    .child(text)
                    .when(i == 4, |s| {
                        s.child(icons::ionicon("chevron-down-outline", 10., 1.))
                    })
            }));
        let visible = if state.expanded {
            (0..7).collect::<Vec<_>>()
        } else {
            state.primary.clone()
        };
        let categories = div()
            .rounded(px(14.))
            .overflow_hidden()
            .bg(c(0x323336))
            .child(div().py(px(10.)).children(visible.into_iter().map(|i| {
                let checked = state.mask & (1 << i) != 0;
                let scope = scope.clone();
                let profile = scope.profile;
                div()
                    .id(("data-category", i))
                    .debug_selector(move || format!("data-category-{i}"))
                    .track_focus(&controls[5 + i])
                    .h(px(52.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(20.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.data_scope().as_ref() == Some(&scope) && !this.browsing_data.busy {
                            this.browsing_data.mask ^= 1 << i;
                            this.browsing_data.controls[5 + i].focus(window);
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .size(px(16.))
                            .rounded(px(1.))
                            .border_1()
                            .border_color(c(if checked { 0xa8c7fa } else { 0x9aa0a6 }))
                            .when(checked, |s| {
                                s.bg(c(0xa8c7fa))
                                    .text_color(c(0x202124))
                                    .child(icons::control_sized(Icon::Check, 14.))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(LABELS[i]),
                            )
                            .child(
                                div()
                                    .h(px(20.))
                                    .text_color(c(0xbdc1c6))
                                    .child(self.browsing_data_counter(profile, i)),
                            ),
                    )
            })))
            .when(!state.expanded, |s| {
                s.child(
                    div()
                        .id("data-show-more")
                        .debug_selector(|| "data-show-more".into())
                        .track_focus(&controls[12])
                        .h(px(48.))
                        .border_t_1()
                        .border_color(c(0x292a2d))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.browsing_data.busy {
                                this.browsing_data.expanded = true;
                                cx.notify();
                            }
                        }))
                        .child("Show more")
                        .child(icons::ionicon("chevron-down-outline", 12., 1.)),
                )
            });
        let body=div().id("data-scroll").track_scroll(&state.scroll).overflow_y_scroll().max_h(px(if state.expanded{400.}else{372.})).child(categories).child(div().id("data-manage").debug_selector(||"data-manage".into()).track_focus(&controls[13]).mt(px(12.)).h(px(84.)).rounded(px(14.)).bg(c(0x323336)).px(px(20.)).flex().items_center().gap(px(20.)).cursor_pointer().on_click(cx.listener(|this,_,_,cx|{if !this.browsing_data.busy{this.browsing_data.manage=true;cx.notify();}})).child(div().flex_1().min_w_0().child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Manage other data")).child("Search history and passwords can be deleted in their management settings")).child(icons::ionicon("chevron-forward-outline",14.,1.)));
        let cancel = button("Cancel", 76., false, dark)
            .id("data-cancel")
            .debug_selector(|| "data-cancel".into())
            .track_focus(&controls[18])
            .on_click(cx.listener(|this, _, w, cx| this.data_cancel(w, cx)));
        let delete_scope = scope.clone();
        let delete = button("Delete from this device", 175., true, dark)
            .id("data-delete")
            .debug_selector(|| "data-delete".into())
            .track_focus(&controls[19])
            .when(state.mask == 0 || busy, |s| s.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.data_scope().as_ref() == Some(&delete_scope) {
                    this.accept_browsing_data(cx);
                }
            }));
        let dialog = div()
            .id("data-dialog")
            .debug_selector(|| "data-dialog".into())
            .w(px(512.))
            .max_w_full()
            .rounded(px(8.))
            .bg(c(0x292a2d))
            .shadow_lg()
            .flex()
            .flex_col()
            .pt(px(20.))
            .child(
                div()
                    .px(px(20.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Delete browsing data"),
                    )
                    .child(chips)
                    .child(body),
            )
            .child(
                div()
                    .h(px(76.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .when(state.expanded, |s| s.border_t_1().border_color(c(0x3c4043)))
                    .child(cancel)
                    .child(delete),
            );
        let mut overlay = div()
            .occlude()
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.6))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(!state.manage, |o| o.child(dialog));
        if state.more {
            overlay = overlay.child(
                div()
                    .id("data-more-dismiss")
                    .debug_selector(|| "data-more-dismiss".into())
                    .occlude()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, w, cx| {
                        cx.stop_propagation();
                        this.browsing_data.more = false;
                        this.browsing_data.controls[4].focus(w);
                        cx.notify();
                    })),
            );
            overlay = overlay.child(
                div()
                    .occlude()
                    .absolute()
                    .top(gpui::relative(0.5))
                    .mt(px(91.
                        - if state.expanded {
                            582. / 2.
                        } else {
                            (346. + 52. * state.primary.len() as f32) / 2.
                        }))
                    .left(gpui::relative(0.5))
                    .ml(px(80.))
                    .w(px(128.))
                    .p(px(8.))
                    .rounded(px(8.))
                    .bg(c(0x1f1f1f))
                    .shadow_lg()
                    .children(
                        [Range::FourWeeks, Range::AllTime]
                            .into_iter()
                            .enumerate()
                            .map(|(i, r)| {
                                div()
                                    .id(("data-more-range", i))
                                    .debug_selector(move || format!("data-more-range-{i}"))
                                    .track_focus(&controls[14 + i])
                                    .h(px(32.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(c(0x3c4043)))
                                    .child(r.label())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.browsing_data.busy {
                                            this.browsing_data.range = r;
                                            this.browsing_data.more = false;
                                            cx.notify();
                                        }
                                    }))
                            }),
                    ),
            );
        }
        if state.manage {
            let profile = scope.profile;
            let engine = self.profile.handle.read_settings(|b| {
                b.user_profile(profile)
                    .unwrap()
                    .settings
                    .search
                    .default_name()
                    .to_string()
            });
            let search_description=format!("Your search engine is {engine}. See their instructions for deleting your search history, if applicable.");
            overlay = overlay.child(
                div()
                    .occlude()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(512.))
                            .rounded(px(8.))
                            .bg(c(0x292a2d))
                            .px(px(20.))
                            .pt(px(18.))
                            .pb(px(20.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(32.))
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(
                                        div()
                                            .id("data-other-back")
                                            .debug_selector(|| "data-other-back".into())
                                            .track_focus(&controls[16])
                                            .size(px(32.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .child(icons::control(Icon::Back))
                                            .on_click(cx.listener(|this, _, w, cx| {
                                                this.data_cancel(w, cx)
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(16.))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("Other data"),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(12.))
                                    .child("To delete this data, go to its management settings"),
                            )
                            .child(
                                div()
                                    .mt(px(16.))
                                    .rounded(px(14.))
                                    .overflow_hidden()
                                    .bg(c(0x323336))
                                    .child(
                                        div()
                                            .id("data-other-passwords")
                                            .debug_selector(|| "data-other-passwords".into())
                                            .track_focus(&controls[17])
                                            .h(px(72.))
                                            .px(px(16.))
                                            .flex()
                                            .items_center()
                                            .gap(px(20.))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                this.open_password_manager(profile, w, cx)
                                            }))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(
                                                        div()
                                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                                            .child("Passwords and passkeys"),
                                                    )
                                                    .child("Manage in Password Manager"),
                                            )
                                            .child(icons::control_sized(Icon::External, 16.)),
                                    )
                                    .when(engine != "Google", |rows| {
                                        rows.child(
                                            div()
                                                .min_h(px(94.))
                                                .px(px(16.))
                                                .py(px(16.))
                                                .border_t_1()
                                                .border_color(c(0x292a2d))
                                                .child(
                                                    div()
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .child("Search history"),
                                                )
                                                .child(search_description),
                                        )
                                    }),
                            )
                            .child(
                                div().mt(px(20.)).flex().justify_end().child(
                                    button("Done", 67., false, dark)
                                        .id("data-other-done")
                                        .debug_selector(|| "data-other-done".into())
                                        .track_focus(&controls[18])
                                        .on_click(cx.listener(|this, _, w, cx| {
                                            cx.stop_propagation();
                                            this.data_cancel(w, cx)
                                        })),
                                ),
                            ),
                    ),
            );
        }
        page.child(overlay).into_any_element()
    }
    fn browsing_data_counter(&self, profile: UserProfileId, category: usize) -> String {
        // Chromium owns site/cache/form/settings/app counts; Echo owns its history and download metadata.
        if !matches!(category, 0 | 3) {
            return self
                .browsing_data
                .counters
                .as_ref()
                .and_then(|c| c.snapshot()[category].clone())
                .unwrap_or_default();
        }
        self.profile.handle.read_settings(|b| match category {
            0 => {
                let since = self.browsing_data.range.since(SystemTime::now());
                let sites = b
                    .user_profile(profile)
                    .into_iter()
                    .flat_map(|p| &p.history)
                    .filter(|e| since.is_none_or(|s| e.visited_at >= s))
                    .filter_map(|e| {
                        url::Url::parse(&e.url)
                            .ok()
                            .and_then(|u| u.host_str().map(str::to_string))
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                match sites.len() {
                    0 => "None".into(),
                    1 => format!("From {}", sites.first().unwrap()),
                    n => format!(
                        "From {} + {} site{}",
                        sites.first().unwrap(),
                        n - 1,
                        if n == 2 { "" } else { "s" }
                    ),
                }
            }
            3 => {
                let n = b.browsing_data_download_count(
                    profile,
                    self.browsing_data.range.since(SystemTime::now()),
                );
                if n == 0 {
                    "None".into()
                } else {
                    n.to_string()
                }
            }
            _ => String::new(),
        })
    }
}
fn color(dark: bool, v: u32) -> gpui::Rgba {
    rgb(if dark {
        match v {
            0x323336 => 0x333438,
            0xe8eaed => 0xe3e3e3,
            0xa8c7fa => 0xaec6f6,
            0x0086bd => 0x367bb2,
            0x005780 => 0x004a76,
            _ => v,
        }
    } else {
        match v {
            0x202124 | 0x1f1f1f => 0xffffff,
            0x292a2d => 0xffffff,
            0x323336 => 0xf1f3f4,
            0xe8eaed => 0x202124,
            0xbdc1c6 => 0x5f6368,
            0xa8c7fa => 0x2656c9,
            0x9aa0a6 => 0x757775,
            0x0086bd => 0xaec6f6,
            0x005780 => 0xe8f0fe,
            0x3c4043 => 0xdadce0,
            _ => v,
        }
    })
}
fn button(label: &'static str, width: f32, primary: bool, dark: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(label)
        .h(px(36.))
        .w(px(width))
        .rounded_full()
        .border_1()
        .border_color(color(dark, 0x0086bd))
        .when(primary, |s| s.border_0().bg(color(dark, 0xa8c7fa)))
        .text_color(color(dark, if primary { 0x202124 } else { 0xa8c7fa }))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(label)
}

pub(crate) fn open_from_settings(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    cx: &mut gpui::App,
) {
    cx.defer(move |cx| open_from_settings_now(profile, owner, selected, true, cx));
}
fn open_from_settings_now(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    create: bool,
    cx: &mut gpui::App,
) {
    if crate::quit::is_quitting() {
        return;
    }
    let preferred = owner
        .and_then(|owner| owner.upgrade())
        .map(|owner| owner.read(cx).main_window);
    let target = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<BrowserWindow>())
        .filter(|window| {
            window.read(cx).is_ok_and(|w| {
                !w.browser.is_private()
                    && !w.browser.is_blank()
                    && w.transient.is_none()
                    && w.peek_owner.is_none()
                    && w.profile.handle.same_workspace(&profile)
            })
        })
        .min_by_key(|window| {
            if preferred.is_some_and(|p| p.window_id() == window.window_id()) {
                0
            } else {
                1
            }
        });
    if let Some(target) = target {
        let _ = target.update(cx, |this, window, cx| {
            this.open_browsing_data(selected, window, cx)
        });
    } else if create {
        crate::new_window::open(false, cx);
        cx.defer(move |cx| open_from_settings_now(profile, None, selected, false, cx));
    }
}
