//! Arc-shaped local settings, in an independent GPUI native window.
pub(crate) mod max;
use crate::{
    icons::{self, Icon},
    profile::ProfileHandle,
    window::BrowserWindow,
};
use browser_core::{
    settings::{SearchEngine, Toggle},
    UserProfileId,
};
use gpui::{
    div, point, prelude::*, px, rgb, size, AnyElement, App, Bounds, Context, Entity, FocusHandle,
    Focusable, SharedString, Subscription, TitlebarOptions, WeakEntity, Window, WindowBounds,
    WindowOptions,
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    General,
    Profiles,
    Max,
    Links,
    Shortcuts,
    Icon,
    Advanced,
}
impl Tab {
    const ALL: [Self; 7] = [
        Self::General,
        Self::Profiles,
        Self::Max,
        Self::Links,
        Self::Shortcuts,
        Self::Icon,
        Self::Advanced,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Profiles => "Profiles",
            Self::Max => "Max",
            Self::Links => "Links",
            Self::Shortcuts => "Shortcuts",
            Self::Icon => "Icon",
            Self::Advanced => "Advanced",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::General => "settings-outline",
            Self::Profiles => "person-outline",
            Self::Max => "color-wand-outline",
            Self::Links => "browsers-outline",
            Self::Shortcuts => "keypad-outline",
            Self::Icon => "color-filter-outline",
            Self::Advanced => "options-outline",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Popup {
    Search,
    Archive,
    Documents,
    Downloads,
    LittleArchive,
}

mod archive;

pub(crate) struct SettingsWindow {
    profile: ProfileHandle,
    owner: Option<WeakEntity<BrowserWindow>>,
    pub(crate) tab: Tab,
    pub(crate) selected_profile: UserProfileId,
    focus: FocusHandle,
    popup: Option<Popup>,
    popup_selection: usize,
    archive_scope: Option<archive::ArchiveScope>,
    archive_closing: Option<archive::ArchiveScope>,
    archive_generation: u64,
    previews: bool,
    pub(crate) atc: Option<Entity<crate::atc::AtcPanel>>,
    atc_return_focus: bool,
    _atc_dismiss: Option<Subscription>,
    max: max::State,
    shortcut_search: Entity<crate::text_input::TextInput>,
    _search_changed: Subscription,
    _observe: Subscription,
}
impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub(crate) fn init(cx: &mut App) {
    archive::init(cx);
    cx.on_action(|_: &crate::menus::Settings, cx| {
        cx.defer(|cx| {
            if crate::quit::is_quitting() {
                return;
            }
            match crate::new_window::normal_source(cx) {
                Ok(normal) => {
                    let id = normal.profile.default_user_profile_id();
                    open(normal.profile, id, None, false, cx);
                }
                Err(error) => crate::show_session_error(error, cx),
            }
        });
    });
}

impl BrowserWindow {
    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) { self.open_settings_mode(false, cx); }
    pub(crate) fn open_air_traffic_control(&mut self, cx: &mut Context<Self>) { self.open_settings_mode(true, cx); }
    fn open_settings_mode(&mut self, air_traffic: bool, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() {
            return;
        }
        self.retain_normal_source(cx);
        self.close_command_bar_with_focus(crate::window::PopupFocus::OnActivation, cx);
        let owner = (!self.browser.is_private() && !self.browser.is_blank())
            .then(|| cx.entity().downgrade());
        let selected = self
            .browser
            .blank_source_user_profile_id()
            .unwrap_or(self.browser.active_user_profile_id());
        let profile = if self.browser.is_private() || self.browser.is_blank() {
            match crate::new_window::normal_source(cx) {
                Ok(normal) => normal.profile,
                Err(error) => {
                    crate::show_session_error(error, cx);
                    return;
                }
            }
        } else {
            self.profile.handle.clone()
        };
        cx.defer(move |cx| open(profile, selected, owner, air_traffic, cx));
    }
}

fn open(
    profile: ProfileHandle,
    selected: UserProfileId,
    owner: Option<WeakEntity<BrowserWindow>>,
    air_traffic: bool,
    cx: &mut App,
) {
    if crate::quit::is_quitting() {
        return;
    }
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<SettingsWindow>() {
            let same = handle
                .read(cx)
                .is_ok_and(|view| view.profile.same_workspace(&profile));
            if same {
                let _ = handle.update(cx, |view, window, cx| {
                    if owner.is_some() {
                        view.owner = owner.clone();
                    }
                    window.activate_window();
                    view.focus.focus(window);
                    if air_traffic { view.present_atc(window, cx); }
                });
                return;
            }
        }
    }
    let bounds = Bounds::centered(None, size(px(684.), px(650.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("General".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        move |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window);
                let selected_profile =
                    if profile.settings_snapshot().user_profile(selected).is_some() {
                        selected
                    } else {
                        profile.default_user_profile_id()
                    };
                let observe = profile.observe_settings(cx);
                let shortcut_search = cx.new(|cx| {
                    crate::text_input::TextInput::new(
                        "Type a feature name or shortcut (like “Command T”)",
                        cx,
                    )
                });
                let search_changed = cx.subscribe(
                    &shortcut_search,
                    |_, _, _: &crate::text_input::Changed, cx| cx.notify(),
                );
                let max = max::State::new(&profile, cx);
                let mut view = SettingsWindow {
                    max,
                    profile,
                    owner,
                    tab: Tab::General,
                    selected_profile,
                    focus,
                    popup: None,
                    popup_selection: 0,
                    archive_scope: None,
                    archive_closing: None,
                    archive_generation: 0,
                    previews: false,
                    atc: None,
                    atc_return_focus: false,
                    _atc_dismiss: None,
                    shortcut_search,
                    _search_changed: search_changed,
                    _observe: observe,
                };
                if air_traffic { view.present_atc(window, cx); }
                view
            })
        },
    );
    if let Ok(handle) = opened {
        // Let the old Command Bar's native close/activation finish first.
        cx.spawn(async move |cx| {
            let _ = handle.update(cx, |view, window, cx| {
                window.activate_window();
                if let Some(panel) = &view.atc { panel.read(cx).focus_handle(cx).focus(window); }
                else { view.focus.focus(window); }
            });
        }).detach();
        cx.activate(true);
    }
}

fn group() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(0x3e3f41))
        .bg(rgb(0x2c2e30))
        .px(px(9.))
}
fn row() -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .min_h(px(35.))
        .gap(px(10.))
}
fn rule() -> gpui::Div {
    div().h(px(1.)).w_full().bg(rgb(0x3e3f41))
}
fn button(label: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    div()
        .id(label.clone())
        .debug_selector(|| format!("settings-button-{label}"))
        .flex()
        .items_center()
        .justify_center()
        .min_h(px(24.))
        .flex_none()
        .px(px(12.))
        .rounded(px(5.))
        .bg(rgb(0x393b3d))
        .hover(|s| s.bg(rgb(0x484a4a)))
        .child(label)
}
fn switch(enabled: bool) -> AnyElement {
    div()
        .w(px(36.))
        .h(px(16.))
        .rounded_full()
        .bg(rgb(if enabled { 0x1678ff } else { 0x494b4b }))
        .child(
            div()
                .relative()
                .left(px(if enabled { 16. } else { 2. }))
                .top(px(1.))
                .size(px(14.))
                .rounded_full()
                .bg(rgb(if enabled { 0xc9dafa } else { 0xdfdfdf })),
        )
        .into_any_element()
}

fn popup_chevrons() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            for points in [
                [(3., 5.5), (5.5, 3.), (8., 5.5)],
                [(3., 10.), (5.5, 12.5), (8., 10.)],
            ] {
                let mut path = gpui::PathBuilder::stroke(px(1.4));
                path.move_to(bounds.origin + point(px(points[0].0), px(points[0].1)));
                for (x, y) in &points[1..] {
                    path.line_to(bounds.origin + point(px(*x), px(*y)));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, window.text_style().color);
                }
            }
        },
    )
    .w(px(11.))
    .h(px(16.))
    .into_any_element()
}

impl SettingsWindow {
    fn begin_profile_creation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile = self.profile.clone();
        let owner = self.owner.clone();
        window.remove_window();
        cx.defer(move |cx| {
            if crate::quit::is_quitting() {
                return;
            }
            if let Some(owner) = owner.and_then(|owner| owner.upgrade()) {
                owner.update(cx, |owner, cx| {
                    owner.open_user_profile_creation(
                        crate::user_profile_dialog::CreationSource::Settings,
                        cx,
                    )
                });
                return;
            }
            let find_host = |cx: &App| {
                cx.windows()
                    .into_iter()
                    .filter_map(|w| w.downcast::<BrowserWindow>())
                    .find(|handle| {
                        handle.read(cx).is_ok_and(|owner| {
                            !owner.browser.is_private()
                                && !owner.browser.is_blank()
                                && owner.profile.handle.same_workspace(&profile)
                        })
                    })
            };
            let mut host = find_host(cx);
            if host.is_none() {
                crate::new_window::open(false, cx);
                host = find_host(cx);
            }
            if let Some(host) = host {
                let _ = host.update(cx, |owner, _, cx| {
                    owner.open_user_profile_creation(
                        crate::user_profile_dialog::CreationSource::Settings,
                        cx,
                    )
                });
            }
        });
    }

    fn present_atc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.change_tab(Tab::Links, window, cx);
        if self.atc.is_none() {
            let panel = cx.new(|cx| crate::atc::AtcPanel::new(self.profile.clone(), cx));
            self._atc_dismiss = Some(cx.subscribe(&panel, |this, _, _: &crate::atc::Dismiss, cx| {
                this.atc = None;
                this.atc_return_focus = true;
                cx.notify();
            }));
            self.atc = Some(panel);
        }
        if let Some(panel) = &self.atc { panel.read(cx).focus_handle(cx).focus(window); }
        cx.notify();
    }
    fn change_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.popup = None;
        self.previews = false;
        window.set_window_title(tab.label());
        cx.notify();
    }
    fn toggle(&mut self, toggle: Toggle, cx: &mut Context<Self>) {
        self.profile.edit_settings(|browser| {
            let enabled = browser.settings.enabled(toggle);
            browser.settings.set_enabled(toggle, !enabled);
        });
        cx.notify();
    }
    fn toggle_row(
        &self,
        label: &'static str,
        toggle: Toggle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.profile.read_settings(|b| b.settings.enabled(toggle));
        row()
            .when(
                matches!(
                    toggle,
                    Toggle::LittleWindowShortcut
                        | Toggle::LittleWindowLinks
                        | Toggle::LittleWindowCommandClick
                        | Toggle::PeekPinned
                        | Toggle::PeekCommandShiftClick
                ),
                |s| s.min_h(px(37.)),
            )
            .id(SharedString::from(format!("toggle-{toggle:?}")))
            .debug_selector(|| format!("settings-toggle-{toggle:?}"))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle, cx)))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                    this.toggle(toggle, cx);
                    cx.stop_propagation();
                }
            }))
            .child(div().flex_1().min_w_0().child(label))
            .child(switch(enabled))
            .into_any_element()
    }
    fn action_row(
        &self,
        label: &'static str,
        action: &'static str,
        tab: Option<Tab>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        row()
            .min_h(px(match action {
                "Search Settings…" => 44.,
                "Previews Settings…" => 40.,
                "Profile Settings…" => 47.,
                _ => 35.,
            }))
            .child(div().flex_1().min_w_0().child(label))
            .child(
                button(action)
                    .when(tab.is_none() && action != "Previews Settings…" && action != "Air Traffic Control…", |s| {
                        s.opacity(0.5)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(tab) = tab {
                            this.change_tab(tab, window, cx);
                        } else if action == "Air Traffic Control…" {
                            this.present_atc(window, cx);
                        } else if action == "Previews Settings…" {
                            this.previews = true;
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }
    fn general(&self, cx: &mut Context<Self>) -> AnyElement {
        div().w(px(556.)).flex().flex_col().gap(px(8.))
            .child(group().child(self.toggle_row("Automatically update my Echo", Toggle::AutomaticUpdates, cx)).child(rule()).child(self.toggle_row("Warn before quitting", Toggle::WarnBeforeQuitting, cx)))
            .child(group().child(self.action_row("Previews", "Previews Settings…", None, cx)).child(rule())
                .child(self.action_row("Manage your default search engine, Auto-Archive, and downloads in Profile Settings.", "Profile Settings…", Some(Tab::Profiles), cx)))
            .into_any_element()
    }
    fn popup_row(
        &self,
        label: &'static str,
        value: impl Into<SharedString>,
        popup: Popup,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        row()
            .min_h(px(match popup {
                Popup::Search => 42.,
                Popup::Archive | Popup::Downloads => 43.,
                Popup::Documents => 44.,
                Popup::LittleArchive => 39.,
            }))
            .child(div().flex_1().min_w_0().child(label))
            .child(
                div().w(px(178.)).flex_none().flex().justify_center().child(
                    button(value)
                        .id(SharedString::from(format!("popup-{label}")))
                        .debug_selector(|| format!("settings-popup-{label}"))
                        .w(px(width))
                        .justify_between()
                        .child(popup_chevrons())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.popup = if this.popup == Some(popup) {
                                None
                            } else {
                                Some(popup)
                            };
                            this.popup_selection = this
                                .popup_options(popup)
                                .iter()
                                .position(|(_, selected)| *selected)
                                .unwrap_or(0);
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
    fn profiles(&self, cx: &mut Context<Self>) -> AnyElement {
        let browser = self.profile.settings_snapshot();
        let profile = browser
            .user_profile(self.selected_profile)
            .unwrap_or(browser.active_user_profile());
        let selected = profile.id();
        let list = group()
            .w(px(206.))
            .px_0()
            .h(px(478.))
            .child(
                div()
                    .px(px(16.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(0x929494))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Your Profiles"),
            )
            .child(rule())
            .child(
                div()
                    .id("settings-profiles-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(16.))
                    .py(px(9.))
                    .children(browser.user_profiles().map(|profile| {
                        let id = profile.id();
                        let count = browser
                            .spaces
                            .iter()
                            .filter(|s| s.user_profile_id == id)
                            .count();
                        let count = match count {
                            0 => "Not used".into(),
                            1 => "1 Space".into(),
                            n => format!("{n} Spaces"),
                        };
                        div()
                            .id(SharedString::from(format!("profile-{id:?}")))
                            .debug_selector(|| format!("settings-profile-{}", profile.name()))
                            .h(px(40.))
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.))
                            .border_b_1()
                            .border_color(rgb(0x3e3f41))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(profile.name().to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x999b9b))
                                    .child(count),
                            )
                            .when(id == selected, |s| s.bg(rgb(0x263342)).rounded(px(6.)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_profile = id;
                                this.popup = None;
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .h(px(31.))
                    .border_t_1()
                    .border_color(rgb(0x3e3f41))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(8.))
                    .child(
                        div()
                            .id("add-profile")
                            .debug_selector(|| "settings-add-profile".into())
                            .child(icons::control(Icon::Plus))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.begin_profile_creation(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x777979))
                            .child(icons::control(Icon::Minus)),
                    ),
            );
        let fields = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                group()
                    .child(self.popup_row(
                        "Search engine",
                        profile.settings.search.engine.label(),
                        Popup::Search,
                        137.,
                        cx,
                    ))
                    .child(rule())
                    .child(self.action_row("Manage search", "Search Settings…", None, cx))
                    .child(rule())
                    .child(
                        row()
                            .min_h(px(36.))
                            .id("search-suggestions")
                            .debug_selector(|| "settings-search-suggestions".into())
                            .child("Include search engine suggestions")
                            .child(switch(profile.settings.search.suggestions))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.profile.edit_settings(|b| {
                                    if let Some(p) = b.user_profile_mut(selected) {
                                        p.settings.search.suggestions =
                                            !p.settings.search.suggestions;
                                    }
                                });
                                cx.notify();
                            })),
                    ),
            )
            .child(
                group()
                    .child(self.popup_row(
                        "Archive tabs after",
                        archive_label(profile.archive_after),
                        Popup::Archive,
                        154.,
                        cx,
                    ))
                    .child(rule())
                    .child(self.popup_row(
                        "New documents",
                        profile.settings.document_app.clone(),
                        Popup::Documents,
                        150.,
                        cx,
                    ))
                    .child(rule())
                    .child(self.popup_row(
                        "Download location",
                        profile.settings.download_location.clone(),
                        Popup::Downloads,
                        138.,
                        cx,
                    )),
            )
            .child(
                div()
                    .pl(px(10.))
                    .pt(px(4.))
                    .text_size(px(11.))
                    .text_color(rgb(0x929494))
                    .line_height(px(12.))
                    .child("Your Data and Settings"),
            )
            .child(
                group().py(px(3.)).children(
                    [
                        ("Privacy and Security", Icon::Lock, 0x198deb),
                        ("Passwords", Icon::SiteControls, 0x27b943),
                        ("Credit Cards", Icon::Library, 0xe2bc13),
                        ("Notifications", Icon::Info, 0xec8b25),
                        ("Clear Browsing Data", Icon::Close, 0xf34747),
                    ]
                    .into_iter()
                    .map(|(label, icon, color)| {
                        row()
                            .min_h(px(34.))
                            .justify_start()
                            .gap(px(8.))
                            .child(
                                div()
                                    .size(px(24.))
                                    .rounded(px(4.))
                                    .bg(rgb(color))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(0xffffff))
                                    .child(icons::control(icon)),
                            )
                            .child(div().flex_1().min_w_0().child(label))
                            .child(
                                div()
                                    .text_color(rgb(0x2794f4))
                                    .child(icons::control(Icon::Forward)),
                            )
                    }),
                ),
            );
        div().w(px(556.)).flex().flex_col().gap(px(20.)).child("Profiles help keep your data separate across Spaces — like history, logins, cookies, and extensions. You can use any Profile across one or more Spaces.")
            .child(div().flex().gap(px(10.)).child(list).child(fields)).into_any_element()
    }
    fn links(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.profile.read_settings(|b| b.settings.clone());
        let archive = match settings.little_window_archive {
            browser_core::settings::LittleWindowArchive::OneHour => "1 hour",
            browser_core::settings::LittleWindowArchive::SixHours => "6 hours (default)",
            browser_core::settings::LittleWindowArchive::TwelveHours => "12 hours",
        };
        div().flex().flex_col()
            .child(div().h(px(134.)).overflow_hidden().bg(rgb(0xd28586)).px(px(60.)).pt(px(46.))
                .child(div().h(px(130.)).rounded_t(px(8.)).bg(rgb(0xe6d1cc)).border_1().border_color(rgb(0xbcb7bd))
                    .child(div().h(px(25.)).bg(rgb(0xbab9c5)).rounded_t(px(8.)).flex().items_center().justify_between().px(px(10.)).text_size(px(10.)).text_color(rgb(0x555761)).child("Work").child("echo.local").child("Open in Work"))))
            .child(div().px(px(64.)).py(px(24.)).flex().flex_col().gap(px(24.))
                .child(div().px(px(10.)).child("Little Echo is a smaller, simpler Echo window. It’s perfect for quick lookups and reading funny tweets your friends send you."))
                .child(group().child(self.toggle_row("Open Little Echo when I press ⌥⌘N in any app", Toggle::LittleWindowShortcut, cx)).child(rule())
                    .child(self.toggle_row("Open Little Echo when clicking on links with ⌥⌘ held", Toggle::LittleWindowCommandClick, cx)).child(rule())
                    .child(self.toggle_row("Links from other apps open in Little Echo", Toggle::LittleWindowLinks, cx)).child(rule())
                    .child(self.popup_row("Archive Little Echos after:", archive, Popup::LittleArchive, 150., cx)))
                .child(rule())
                .child(div().px(px(10.)).child("Peek is a way to preview a link without opening a new tab. It’s perfect for quickly looking at a link from an email, news article, or tweet."))
                .child(group().child(self.toggle_row("Open a Peek window when clicking on links with Shift held", Toggle::PeekCommandShiftClick, cx)).child(rule())
                    .child(self.toggle_row("Open a Peek window when clicking on links to other sites", Toggle::PeekPinned, cx))
                    .child(div().pb(px(6.)).italic().text_color(rgb(0x929494)).child("Note: This only affects Favorites and Pinned tabs.")))
                .child(rule()).child(self.action_row("Choose where links open inside Echo.", "Air Traffic Control…", None, cx)))
            .into_any_element()
    }
    fn advanced_row(
        &self,
        label: &'static str,
        toggle: Toggle,
        icon: Icon,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.profile.read_settings(|b| b.settings.enabled(toggle));
        row()
            .id(SharedString::from(format!("toggle-{toggle:?}")))
            .debug_selector(|| format!("settings-toggle-{toggle:?}"))
            .min_h(px(if toggle == Toggle::SharedQuotes {
                50.
            } else {
                40.
            }))
            .focusable()
            .tab_stop(true)
            .child(
                div()
                    .size(px(20.))
                    .rounded(px(4.))
                    .bg(rgb(color))
                    .text_color(rgb(0xffffff))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::control_sized(icon, 16.)),
            )
            .child(
                div()
                    .flex_1()
                    .child(label)
                    .when(toggle == Toggle::SharedQuotes, |s| {
                        s.child(
                            div().text_size(px(10.)).text_color(rgb(0x858787)).child(
                                "If disabled, you can still copy URL as a quote using ⌃⇧⌘C.",
                            ),
                        )
                    }),
            )
            .child(switch(enabled))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle, cx)))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                    this.toggle(toggle, cx);
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }
    fn advanced(&self, cx: &mut Context<Self>) -> AnyElement {
        let sections = [
            vec![
                (
                    "Play Echo sound effects",
                    Toggle::SoundEffects,
                    Icon::Mute,
                    0x0095ff,
                ),
                (
                    "Haptic feedback when reordering tabs",
                    Toggle::HapticFeedback,
                    Icon::Info,
                    0x0095ff,
                ),
            ],
            vec![
                (
                    "When opening Echo, restore windows from previous session",
                    Toggle::RestoreWindows,
                    Icon::Restore,
                    0x23ce4b,
                ),
                (
                    "Allow window dragging from the top of webpages",
                    Toggle::DragWebpages,
                    Icon::NewTab,
                    0x23ce4b,
                ),
                (
                    "Show full URL when Toolbar is enabled",
                    Toggle::FullToolbarUrl,
                    Icon::SiteControls,
                    0xff434d,
                ),
                (
                    "Enable Shared Quotes when highlighting text.",
                    Toggle::SharedQuotes,
                    Icon::Share,
                    0xff434d,
                ),
            ],
            vec![
                (
                    "Enable Picture in Picture when you leave a video tab",
                    Toggle::AutomaticPictureInPicture,
                    Icon::Split,
                    0xd326eb,
                ),
                (
                    "Allow websites to get your theme data",
                    Toggle::ThemeData,
                    Icon::Tidy,
                    0xd326eb,
                ),
                (
                    "Enable Boosts on websites you visit",
                    Toggle::Boosts,
                    Icon::Boost,
                    0xd326eb,
                ),
            ],
        ];
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(sections.into_iter().map(|rows| {
                group().children(rows.into_iter().enumerate().map(
                    |(index, (label, toggle, icon, color))| {
                        div()
                            .when(index > 0, |s| s.child(rule()))
                            .child(self.advanced_row(label, toggle, icon, color, cx))
                    },
                ))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("More Settings…").opacity(0.5))
                    .child(button("Reset Translated-site Prompts").opacity(0.5))
                    .child(div().flex_1())
                    .child(button("Export Data").opacity(0.5)),
            )
            .into_any_element()
    }
    fn icon(&self) -> AnyElement {
        // Arc's referral icons and artwork are deliberately not redistributed.
        group()
            .px(px(16.))
            .py(px(24.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .size(px(20.))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(0x5b53ff))
                            .bg(rgb(0xffffff))
                            .child(
                                div()
                                    .m(px(3.))
                                    .size(px(10.))
                                    .rounded_full()
                                    .bg(rgb(0x5b53ff)),
                            ),
                    )
                    .child(
                        gpui::img(std::sync::Arc::new(gpui::Image::from_bytes(
                            gpui::ImageFormat::Png,
                            include_bytes!("../assets/echo-icon.png").to_vec(),
                        )))
                        .size(px(52.)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child("The Original Echo")
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(0x1590ff))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("CURRENT ICON"),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        static CATALOG: std::sync::LazyLock<Vec<(String, String)>> =
            std::sync::LazyLock::new(|| {
                serde_json::from_str(include_str!("../assets/settings-shortcuts.json"))
                    .expect("embedded settings shortcut catalog")
            });
        let query = self.shortcut_search.read(cx).text().trim().to_lowercase();
        let mut entries = CATALOG.clone();
        self.profile.read_settings(|b| {
            entries.extend(b.spaces.iter().take(9).enumerate().map(|(index, space)| {
                (format!("Go to “{}”", space.name), format!("⌃{}", index + 1))
            }));
        });
        entries.sort_by_key(|(label, _)| label.to_lowercase());
        let entries = entries.into_iter().filter(|(label, key)| {
            query.is_empty() || format!("{label} {key}").to_lowercase().contains(&query)
        });
        div().w(px(556.)).flex().flex_col().gap(px(30.))
            .child(div().flex().justify_center().child(div().flex().items_center().gap(px(10.)).p(px(6.)).rounded(px(6.)).bg(rgb(0x5b575f)).child("Start with the basics").child(button("Learn the Essential Shortcuts").id("learn-essential-shortcuts").debug_selector(|| "learn-essential-shortcuts".into()).bg(rgb(0x147bff)).cursor_pointer().on_click(|_, _, cx| cx.defer(crate::essential_shortcuts::open)))))
            .child(div().flex().gap(px(16.)).h(px(450.))
                .child(group().w(px(340.)).flex_none().px_0()
                    .child(div().h(px(40.)).flex_none().px(px(16.)).bg(rgb(0x3b3e3e)).flex().items_center().gap(px(6.)).text_color(rgb(0xa6a8a8)).child(icons::control_sized(Icon::Search, 14.)).child(self.shortcut_search.clone()))
                    .child(div().h(px(28.)).flex_none().px(px(16.)).flex().items_center().text_size(px(11.)).text_color(rgb(0x929494)).child("Default Shortcuts"))
                    .child(div().id("settings-shortcuts-list").min_h_0().flex_1().overflow_y_scroll().px(px(24.)).children(entries.map(|(label, key)| {
                        row().h(px(48.)).flex_none().border_b_1().border_color(rgb(0x3e3f41)).child(div().flex_1().min_w_0().child(label)).child(button(key).w(px(64.)).opacity(0.5))
                    }))))
                .child(div().w(px(200.)).min_w_0().flex_none().flex().flex_col().gap(px(10.))
                    .child(div().flex().gap(px(4.)).text_color(rgb(0x1686ff)).child(div().size(px(64.)).rounded(px(12.)).bg(rgb(0x75acff)).border_2().border_color(rgb(0x0788ff)).flex().items_center().justify_center().child(icons::control_sized(Icon::Command, 40.))).child(div().size(px(64.)).rounded(px(12.)).bg(rgb(0x75acff)).border_2().border_color(rgb(0x0788ff)).flex().items_center().justify_center().child(icons::ionicon("happy-outline", 42., 1.))))
                    .child(div().mt(px(12.)).font_weight(gpui::FontWeight::BOLD).child("Custom Shortcuts"))
                    .child(div().text_size(px(11.)).line_height(px(14.)).text_color(rgb(0x929494)).child("Change shortcuts for your favorite actions, and even choose whether Echo or a website takes priority."))
                    .child(div().flex_1()).child(div().flex().gap(px(4.)).child(button("Reset All Shortcuts").px(px(8.)).opacity(0.5)).child(div().size(px(24.)).rounded_full().bg(rgb(0x393b3d)).child(icons::more()).opacity(0.5)))))
            .into_any_element()
    }
    fn previews_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.profile.read_settings(|b| b.settings.clone());
        div().absolute().inset_0().bg(gpui::rgba(0x00000050)).flex().items_center().justify_center()
            .child(div().w(px(422.)).p(px(24.)).rounded(px(17.)).border_1().border_color(rgb(0x050914)).bg(rgb(0x151c30)).shadow_lg().flex().flex_col().gap(px(8.))
                .child(div().font_weight(gpui::FontWeight::BOLD).text_color(rgb(0xffffff)).child("Show Echo Previews:"))
                .child("Save a click. Hover your mouse over an Echo Folder, Pinned, or Favorited tab to quickly preview what you need from that site!")
                .children([("Echo Folders", Toggle::PreviewFolders), ("Google Calendar", Toggle::PreviewGoogleCalendar), ("Google Mail", Toggle::PreviewGoogleMail), ("Outlook Calendar", Toggle::PreviewOutlookCalendar), ("Outlook Mail", Toggle::PreviewOutlookMail), ("Recent pages on Linear, Notion, and Figma", Toggle::PreviewRecentPages), ("Notion Calendar", Toggle::PreviewNotionCalendar)].into_iter().map(|(label, toggle)| {
                    div().id(SharedString::from(format!("preview-{toggle:?}"))).debug_selector(|| format!("settings-preview-{toggle:?}")).h(px(20.)).flex().items_center().gap(px(12.))
                        .child(div().size(px(16.)).rounded(px(4.)).bg(rgb(if settings.enabled(toggle) { 0x1684ff } else { 0x555968 })).when(settings.enabled(toggle), |s| s.child(icons::control_sized(Icon::Check, 16.))))
                        .child(label).on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle, cx)))
                }))
                .child(div().flex().justify_end().mt(px(16.)).child(button("Done").h(px(36.)).bg(rgb(0x4037f7)).text_color(rgb(0xffffff)).child(icons::control(Icon::Enter)).on_click(cx.listener(|this, _, _, cx| { this.previews = false; cx.notify(); }))))).into_any_element()
    }
    fn popup_options(&self, popup: Popup) -> Vec<(String, bool)> {
        let browser = self.profile.settings_snapshot();
        let profile = browser
            .user_profile(self.selected_profile)
            .unwrap_or(browser.active_user_profile());
        match popup {
            Popup::Search => SearchEngine::ALL
                .into_iter()
                .map(|e| (e.label().into(), e == profile.settings.search.engine))
                .collect(),
            Popup::Archive => [43200, 86400, 604800, 2592000]
                .into_iter()
                .map(|s| {
                    (
                        archive_label(Some(Duration::from_secs(s))).into(),
                        profile.archive_after == Some(Duration::from_secs(s)),
                    )
                })
                .collect(),
            Popup::Documents => [
                "—",
                "Confluence Page",
                "Google Doc",
                "Notion Page",
                "Word Doc",
            ]
            .into_iter()
            .map(|s| (s.into(), profile.settings.document_app == s))
            .collect(),
            Popup::Downloads => [
                "Downloads",
                "Other…",
                "Ask me where to save files every time",
            ]
            .into_iter()
            .map(|s| {
                (
                    s.into(),
                    if s == "Ask me where to save files every time" {
                        profile.settings.ask_for_download_location
                    } else {
                        profile.settings.download_location == s
                    },
                )
            })
            .collect(),
            Popup::LittleArchive => ["1 hour", "6 hours (default)", "12 hours"]
                .into_iter()
                .enumerate()
                .map(|(i, s)| {
                    (
                        s.into(),
                        i == browser.settings.little_window_archive as usize,
                    )
                })
                .collect(),
        }
    }
    fn popup_menu(&self, popup: Popup, cx: &mut Context<Self>) -> AnyElement {
        let options = self.popup_options(popup);
        let selected_index = options
            .iter()
            .position(|(_, selected)| *selected)
            .unwrap_or(0);
        let center = match popup {
            Popup::Search => 182.,
            Popup::Archive => 316.,
            Popup::Documents => 361.,
            Popup::Downloads => 406.,
            Popup::LittleArchive => 436.,
        };
        let top = center - selected_index as f32 * 24. - 16.;
        let width = match popup {
            Popup::Search => 137.,
            Popup::Archive => 154.,
            Popup::Documents => 150.,
            Popup::Downloads => 284.,
            Popup::LittleArchive => 150.,
        };
        let left = match popup {
            Popup::Search => 439.,
            Popup::Archive => 431.,
            Popup::Documents => 432.,
            Popup::Downloads => 438.,
            Popup::LittleArchive => 423.,
        };
        div()
            .id("settings-popup-menu")
            .flex()
            .flex_col()
            .on_click(|_, _, cx| cx.stop_propagation())
            .absolute()
            .top(px(top))
            .left(px(left))
            .w(px(width))
            .p(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(0x636565))
            .bg(rgb(0x343636))
            .shadow_lg()
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(index, (label, selected))| {
                        div()
                            .id(("settings-choice", index))
                            .debug_selector(|| format!("settings-choice-{index}"))
                            .h(px(24.))
                            .px(px(6.))
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .hover(|s| s.bg(rgb(0x1769c4)))
                            .when(index == self.popup_selection, |s| s.bg(rgb(0x087dff)))
                            .child(div().w(px(16.)).when(selected, |s| {
                                s.child(icons::control_sized(Icon::Check, 14.))
                            }))
                            .child(label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose(popup, index, window, cx);
                            }))
                    }),
            )
            .into_any_element()
    }
    fn choose(&mut self, popup: Popup, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if popup == Popup::Archive {
            self.choose_archive(index, cx);
            self.popup = None;
            self.focus.focus(window);
            cx.notify();
            return;
        }
        let id = self.selected_profile;
        self.profile.edit_settings(|browser| {
            if popup == Popup::LittleArchive {
                use browser_core::settings::LittleWindowArchive::*;
                browser.settings.little_window_archive = [OneHour, SixHours, TwelveHours][index];
                return;
            }
            let Some(profile) = browser.user_profile_mut(id) else {
                return;
            };
            match popup {
                Popup::Search => profile.settings.search.engine = SearchEngine::ALL[index],
                Popup::Archive => unreachable!("archive changes include the profile-scope question"),
                Popup::Documents => {
                    profile.settings.document_app = [
                        "—",
                        "Confluence Page",
                        "Google Doc",
                        "Notion Page",
                        "Word Doc",
                    ][index]
                        .into()
                }
                Popup::Downloads => match index {
                    0 => profile.settings.download_location = "Downloads".into(),
                    2 => {
                        profile.settings.ask_for_download_location =
                            !profile.settings.ask_for_download_location
                    }
                    _ => {}
                },
                Popup::LittleArchive => unreachable!(),
            }
        });
        self.popup = None;
        if popup == Popup::Downloads && index == 1 {
            let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: None,
            });
            cx.spawn(async move |this, cx| {
                if let Ok(Ok(Some(paths))) = chosen.await {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = this.update(cx, |this, cx| {
                            this.profile.edit_settings(|b| {
                                if let Some(p) = b.user_profile_mut(id) {
                                    p.settings.download_location =
                                        path.to_string_lossy().into_owned();
                                }
                            });
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }
}
fn archive_label(duration: Option<Duration>) -> &'static str {
    match duration.map(|d| d.as_secs()) {
        Some(43200) => "12 hours (default)",
        Some(86400) => "24 hours",
        Some(604800) => "7 days",
        Some(2592000) => "30 days",
        _ => "—",
    }
}
impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.atc_return_focus { self.atc_return_focus = false; self.focus.focus(window); }
        let browser = self.profile.settings_snapshot();
        if browser.user_profile(self.selected_profile).is_none() {
            self.selected_profile = browser.default_user_profile_id();
        }
        let content = match self.tab {
            Tab::General => self.general(cx),
            Tab::Profiles => self.profiles(cx),
            Tab::Max => self.max(cx),
            Tab::Links => self.links(cx),
            Tab::Shortcuts => self.shortcuts(cx),
            Tab::Icon => self.icon(),
            Tab::Advanced => self.advanced(cx),
        };
        div()
            .id("settings-window")
            .debug_selector(|| "settings-window".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x292b2d))
            .text_color(rgb(0xc4c5c5))
            .font_family(".SystemUIFont")
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_size(px(13.))
            .line_height(px(16.))
            .track_focus(&self.focus)
            .key_context(if self.archive_scope.is_some() { "ArchiveScope" } else { "SettingsWindow" })
            .on_action(cx.listener(|this, _: &archive::AllProfiles, _, cx| this.finish_archive_scope(true, cx)))
            .on_action(cx.listener(|this, _: &archive::OnlyThisProfile, _, cx| this.finish_archive_scope(false, cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.archive_scope.is_some() || this.archive_closing.is_some() {
                    match event.keystroke.key.as_str() {
                        "enter" => this.finish_archive_scope(true, cx),
                        "escape" => this.finish_archive_scope(false, cx),
                        "w" if event.keystroke.modifiers.platform => window.remove_window(),
                        _ => {},
                    }
                    cx.stop_propagation();
                    return;
                }
                if let Some(popup) = this.popup {
                    let count = this.popup_options(popup).len();
                    match event.keystroke.key.as_str() {
                        "down" => {
                            this.popup_selection = (this.popup_selection + 1) % count;
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        "up" => {
                            this.popup_selection = (this.popup_selection + count - 1) % count;
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        "enter" => {
                            this.choose(popup, this.popup_selection, window, cx);
                            cx.stop_propagation();
                            return;
                        }
                        _ => {}
                    }
                }
                if this.previews && event.keystroke.key == "enter" {
                    this.previews = false;
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                if self::Tab::Max == this.tab && event.keystroke.key == "enter" && this.max.key.focus_handle(cx).is_focused(window) {
                    this.max_save_key(cx); cx.stop_propagation(); return;
                }
                match event.keystroke.key.as_str() {
                    "tab" => {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev();
                        } else {
                            window.focus_next();
                        }
                        cx.stop_propagation();
                    }
                    "escape" => {
                        if this.popup.take().is_some() || this.previews {
                            this.previews = false;
                            cx.notify();
                        } else {
                            window.remove_window();
                        }
                    }
                    "w" if event.keystroke.modifiers.platform => window.remove_window(),
                    _ => {}
                }
            }))
            .on_action(
                cx.listener(|_, _: &crate::window::CloseTab, window, _| window.remove_window()),
            )
            .on_action(
                cx.listener(|_, _: &crate::menus::Settings, window, _| window.activate_window()),
            )
            .child(
                div()
                    .h(px(88.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .bg(rgb(0x303334))
                    .border_b_1()
                    .border_color(rgb(0x393b3b))
                    .child(
                        div()
                            .h(px(28.))
                            .flex()
                            .justify_center()
                            .items_center()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xacafb0))
                            .child(self.tab.label()),
                    )
                    .child(
                        div()
                            .mt(px(4.))
                            .flex()
                            .justify_center()
                            .gap(px(4.))
                            .children(Tab::ALL.into_iter().map(|tab| {
                                div()
                                    .id(tab.label())
                                    .debug_selector(|| format!("settings-tab-{}", tab.label()))
                                    .w(px(if tab == Tab::Shortcuts { 65. } else { 56. }))
                                    .h(px(49.))
                                    .rounded(px(6.))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(3.))
                                    .text_size(px(11.))
                                    .text_color(rgb(if self.tab == tab {
                                        0x0788ff
                                    } else {
                                        0x989a9a
                                    }))
                                    .when(self.tab == tab, |s| {
                                        s.bg(rgb(0x363838)).border_1().border_color(rgb(0x181919))
                                    })
                                    .child(icons::ionicon(tab.icon(), 22., 1.))
                                    .child(tab.label())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.change_tab(tab, window, cx)
                                    }))
                            })),
                    ),
            )
            .child(
                div()
                    .id("settings-scroll").debug_selector(|| "settings-scroll".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(!matches!(self.tab, Tab::Links | Tab::Max), |s| {
                        s.px(px(64.)).pt(px(20.)).pb(px(12.))
                    })
                    .child(content),
            )
            .when_some(self.popup, |s, popup| {
                s.child(
                    div()
                        .id("settings-popup-backdrop")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.popup = None;
                            cx.notify();
                        }))
                        .child(self.popup_menu(popup, cx)),
                )
            })
            .when(self.previews, |s| s.child(self.previews_panel(cx)))
            .when(self.archive_scope.is_some() || self.archive_closing.is_some(), |s| s.child(self.archive_scope_panel(cx)))
            .when_some(self.atc.clone(), |s, panel| s.child(div().absolute().inset_0().bg(gpui::hsla(0., 0., 0., 0.35))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().absolute().top(px(88.)).left(gpui::relative(0.5)).ml(px(-255.)).child(panel))))
    }
}

/// Help's View All opens the existing local Shortcuts settings section.
pub(crate) fn open_shortcuts(cx: &mut App) {
    let source = match crate::new_window::normal_source(cx) {
        Ok(source) => source,
        Err(error) => { crate::show_session_error(error, cx); return; }
    };
    let profile = source.profile;
    open(profile.clone(), profile.default_user_profile_id(), None, false, cx);
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<SettingsWindow>() {
            let _ = handle.update(cx, |view, window, cx| {
                if view.profile.same_workspace(&profile) {
                    view.change_tab(Tab::Shortcuts, window, cx);
                    window.activate_window();
                    view.focus.focus(window);
                    cx.notify();
                }
            });
        }
    }
}
