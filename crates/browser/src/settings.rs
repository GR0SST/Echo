//! Arc-shaped local settings, in an independent GPUI native window.
pub(crate) mod max;
mod max_art;
use crate::{
    icons::{self, Icon},
    profile::ProfileHandle,
    window::BrowserWindow,
};
use browser_core::{
    settings::Toggle,
    UserProfileId,
};
use gpui::{
    div, point, prelude::*, px, rgb, size, AnyElement, App, Bounds, Context, Entity, FocusHandle,
    Focusable, SharedString, Subscription, TitlebarOptions, WeakEntity, Window, WindowBounds,
    WindowOptions, WindowHandle,
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
    fn icon(self) -> AnyElement {
        let name = match self {
            Self::General => "settings-outline",
            Self::Profiles => "person-outline",
            Self::Max => "color-wand-outline",
            Self::Links => "browsers-outline",
            Self::Shortcuts => return icons::keyboard(22.),
            Self::Icon => "color-filter-outline",
            Self::Advanced => "options-outline",
        };
        icons::ionicon(name, 22., 1.)
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
mod appearance;
mod app_icon;
mod previews;
mod profiles;
mod shortcuts;
pub(crate) mod shortcuts_menu;

pub(crate) struct SettingsWindow {
    colors: appearance::Colors,
    _appearance: Subscription,
    profile: ProfileHandle,
    owner: Option<WeakEntity<BrowserWindow>>,
    pub(crate) tab: Tab,
    pub(crate) selected_profile: UserProfileId,
    focus: FocusHandle,
    popup: Option<Popup>,
    popup_selection: usize,
    popup_keyboard: bool,
    search_popup_owner: Option<UserProfileId>,
    search_popup_providers: Vec<browser_core::omnibox::search_settings::SearchProvider>,
    archive_scope: Option<archive::ArchiveScope>,
    archive_closing: Option<archive::ArchiveScope>,
    archive_generation: u64,
    previews: bool,
    profile_editor: Option<profiles::Editor>,
    pub(crate) atc: Option<Entity<crate::atc::AtcPanel>>,
    atc_return_focus: bool,
    _atc_dismiss: Option<Subscription>,
    max: max::State,
    shortcuts: shortcuts::State,
    shortcuts_menu: Option<WindowHandle<shortcuts_menu::FooterMenu>>,
    shortcuts_menu_pending: bool,
    shortcut_search: Entity<crate::text_input::TextInput>,
    _search_changed: Subscription,
    _observe: Subscription,
    _extensions_observe: Option<Subscription>,
}

impl SettingsWindow {
    pub(crate) fn browser_owner(&self, cx: &App) -> Option<gpui::WindowHandle<BrowserWindow>> {
        let owner = self.owner.as_ref()?.upgrade()?;
        owner.read(cx).main_window.downcast::<BrowserWindow>()
    }
}
impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub(crate) fn init(cx: &mut App) {
    archive::init(cx);
    profiles::init(cx);
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
                let extensions_observe=owner.as_ref().and_then(|owner|owner.upgrade()).map(|owner|cx.observe(&owner,|_,_,cx|cx.notify()));
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
                let shortcuts = shortcuts::State::new(cx);
                let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
                cx.on_release(|view, cx| view.close_shortcuts_menu(cx)).detach();
                let mut view = SettingsWindow {
                    colors: appearance::Colors::from_appearance(window.appearance()),
                    _appearance: appearance,
                    max,
                    profile,
                    owner,
                    tab: Tab::General,
                    selected_profile,
                    focus,
                    popup: None,
                    popup_selection: 0,
                    popup_keyboard: false,
                    search_popup_owner: None,
                    search_popup_providers: Vec::new(),
                    archive_scope: None,
                    archive_closing: None,
                    archive_generation: 0,
                    previews: false,
                    profile_editor: None,
                    atc: None,
                    atc_return_focus: false,
                    _atc_dismiss: None,
                    shortcuts,
                    shortcuts_menu: None,
                    shortcuts_menu_pending: false,
                    shortcut_search,
                    _search_changed: search_changed,
                    _observe: observe,
                    _extensions_observe: extensions_observe,
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
    button_palette(label, appearance::Colors { light: false })
}
fn button_palette(label: impl Into<SharedString>, colors: appearance::Colors) -> gpui::Stateful<gpui::Div> {
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
        .bg(colors.rgb(0x393b3d))
        .when(colors.light, |button| button.text_color(rgb(0x424242)))
        .hover(move |s| s.bg(colors.rgb(0x484a4a)))
        .child(label)
}
fn switch(enabled: bool) -> AnyElement {
    div()
        .w(px(36.))
        .h(px(16.))
        .rounded_full()
        .bg(rgb(if enabled { 0x107cfc } else { 0x494b4b }))
        .child(
            div()
                .relative()
                .left(px(if enabled { 14. } else { 1. }))
                .top(px(1.))
                .w(px(21.)).h(px(14.))
                .rounded_full()
                .bg(rgb(if enabled { 0xdfebff } else { 0xdfdfdf })),
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
        self.close_shortcuts_menu(cx);
        self.shortcuts.armed = false;
        self.tab = tab;
        self.popup = None;
        self.previews = false;
        if self.profile_editor.take().is_some() { self.focus.focus(window); }
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
            .child(self.settings_switch(enabled))
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
            .font_weight(gpui::FontWeight::NORMAL)
            .min_h(px(match action {
                "Search Settings…" => 44.,
                "Previews Settings…" => 40.,
                "Profile Settings…" => 47.,
                _ => 35.,
            }))
            .child(div().flex_1().min_w_0().child(label))
            .child(
                self.settings_button(action)
                    .when(tab.is_none() && action != "Previews Settings…" && action != "Air Traffic Control…" && action != "Search Settings…", |s| {
                        s.opacity(0.5)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(tab) = tab {
                            this.change_tab(tab, window, cx);
                        } else if action == "Air Traffic Control…" {
                            this.present_atc(window, cx);
                        } else if action == "Search Settings…" {
                            crate::search_settings::open_from_settings(this.profile.clone(),this.owner.clone(),this.selected_profile,cx);
                        } else if action == "Previews Settings…" {
                            this.popup = None;
                            this.previews = true;
                            this.focus.focus(window);
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }
    fn general(&self, cx: &mut Context<Self>) -> AnyElement {
        div().w(px(556.)).flex().flex_col().gap(px(8.))
            .child(self.settings_group().child(self.toggle_row("Warn before quitting", Toggle::WarnBeforeQuitting, cx)))
            .child(self.settings_group().child(self.action_row("Previews", "Previews Settings…", None, cx)).child(self.settings_rule())
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
                    self.settings_button(value)
                        .id(SharedString::from(format!("popup-{label}")))
                        .debug_selector(|| format!("settings-popup-{label}"))
                        .w(px(width))
                        .justify_between()
                        .child(popup_chevrons())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if popup == Popup::Search {
                                this.search_popup_owner=Some(this.selected_profile);
                                this.search_popup_providers=this.profile.read_settings(|b|b.user_profile(this.selected_profile)
                                    .map(|p|p.settings.search.providers.iter().filter(|p|p.active&&p.show_in_engine_list).cloned().collect()).unwrap_or_default());
                            }
                            this.popup = if this.popup == Some(popup) {
                                None
                            } else {
                                Some(popup)
                            };
                            this.popup_keyboard = false;
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
        let password_owner=self.selected_profile;
        let colors = self.colors;
        let browser = self.profile.settings_snapshot();
        let profile = browser
            .user_profile(self.selected_profile)
            .unwrap_or(browser.active_user_profile());
        let selected = profile.id();
        let can_delete = selected != browser.default_user_profile_id() && !browser.spaces.iter().any(|space| space.user_profile_id == selected);
        let list = self.settings_group().when(colors.light, |list| list.bg(gpui::rgb(0xffffff)))
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
                    .text_color(colors.rgb(0x929494))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Your Profiles"),
            )
            .child(self.settings_rule())
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
                            .relative()
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.))
                            .when(id == selected, |row| row.child(div().absolute().left_0().right_0().top(px(5.)).h(px(30.)).rounded(px(6.)).bg(colors.rgb(0x263342))))
                            .child(div().absolute().left(px(6.)).right_0().bottom_0().h(px(1.)).bg(colors.rgb(0x3e3f41)))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .truncate()
                                    .child(profile.name().to_string()),
                            )
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(11.))
                                    .text_color(colors.rgb(0x999b9b))
                                    .child(count),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_profile = id;
                                this.popup = None;
                                cx.notify();
                            }))
                    })),
            )
            .child(div().h(px(31.)).border_t_1().border_color(colors.rgb(0x3e3f41)).flex().items_center()
                .child(div().id("add-profile").debug_selector(|| "settings-add-profile".into()).w(px(31.)).h(px(30.)).flex().items_center().justify_center().cursor_pointer()
                    .child(icons::control_sized(Icon::Plus,13.)).on_click(cx.listener(|this,_,window,cx| this.begin_profile_creation(window,cx))))
                .child(div().w(px(1.)).h(px(14.)).bg(colors.rgb(0x3e3f41)))
                .child(div().id("delete-profile").debug_selector(|| "settings-delete-profile".into()).w(px(31.)).h(px(30.)).flex().items_center().justify_center()
                    .when(!can_delete,|s|s.opacity(0.35)).when(can_delete,|s|s.cursor_pointer())
                    .child(icons::control_sized(Icon::Minus,13.)).on_click(cx.listener(move|this,_,window,cx|{if can_delete {this.begin_profile_editor(profiles::Operation::Delete,window,cx);}})))
                .child(div().w(px(1.)).h(px(14.)).bg(colors.rgb(0x3e3f41)))
                .child(div().id("rename-profile").debug_selector(|| "settings-rename-profile".into()).w(px(36.)).h(px(30.)).flex().items_center().justify_center().cursor_pointer()
                    .child(profiles::rename_control()).on_click(cx.listener(|this,_,window,cx|this.begin_profile_editor(profiles::Operation::Rename,window,cx)))));

        let fields = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                self.settings_group()
                    .child(self.popup_row(
                        "Search engine",
                        profile.settings.search.default_name().to_owned(),
                        Popup::Search,
                        137.,
                        cx,
                    ))
                    .child(self.settings_rule())
                    .child(self.action_row("Manage search", "Search Settings…", None, cx))
                    .child(self.settings_rule())
                    .child(
                        row()
                            .min_h(px(36.))
                            .id("search-suggestions")
                            .debug_selector(|| "settings-search-suggestions".into())
                            .child("Include search engine suggestions")
                            .child(self.settings_switch(profile.settings.search.suggestions))
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
                self.settings_group()
                    .child(self.popup_row(
                        "Archive tabs after",
                        archive_label(profile.archive_after),
                        Popup::Archive,
                        154.,
                        cx,
                    ))
                    .child(self.settings_rule())
                    .child(self.popup_row(
                        "New documents",
                        profile.settings.document_app.clone(),
                        Popup::Documents,
                        150.,
                        cx,
                    ))
                    .child(self.settings_rule())
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
                    .text_color(colors.rgb(0x929494))
                    .line_height(px(12.))
                    .child("Your Data and Settings"),
            )
            .child(
                self.settings_group().py(px(3.)).children(
                    [
                        ("Privacy and Security", Icon::Lock, 0x198deb),
                        ("Passwords", Icon::SiteControls, 0x27b943),
                        ("Credit Cards", Icon::Library, 0xe2bc13),
                        ("Notifications", Icon::Info, 0xec8b25),
                        ("Clear Browsing Data", Icon::Close, 0xf34747),
                    ]
                    .into_iter()
                    .map(|(label, icon, color)| {
                        row().id(label)
                            .when(label=="Passwords",|row|row.debug_selector(||"settings-passwords".into()).cursor_pointer().on_click(cx.listener(move|this,_,_,cx|{
                                if this.selected_profile==password_owner && this.profile.read_settings(|b|b.user_profile(password_owner).is_some()) {crate::password_manager::open_from_settings(this.profile.clone(),this.owner.clone(),password_owner,cx);}
                            })))
                            .when(label=="Clear Browsing Data",|row|row.debug_selector(||"settings-clear-browsing-data".into()).cursor_pointer().on_click(cx.listener(move|this,_,_,cx|{
                                if this.selected_profile==password_owner && this.profile.read_settings(|b|b.user_profile(password_owner).is_some()) {crate::browsing_data::open_from_settings(this.profile.clone(),this.owner.clone(),password_owner,cx);}
                            })))
                            .min_h(px(34.))
                            .justify_start()
                            .gap(px(8.))
                            .child(
                                div()
                                    .size(px(24.))
                                    .rounded(px(4.))
                                    .bg(colors.rgb(color))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(colors.rgb(0xffffff))
                                    .child(icons::control(icon)),
                            )
                            .child(div().flex_1().min_w_0().child(label))
                            .child(
                                div()
                                    .text_color(colors.rgb(0x2794f4))
                                    .child(icons::control(Icon::Forward)),
                            )
                    }),
                ),
            );
        div().w(px(556.)).flex().flex_col().gap(px(20.)).child("Profiles help keep your data separate across Spaces — like history, logins, cookies, and extensions. You can use any Profile across one or more Spaces.")
            .child(div().flex().gap(px(10.)).child(list).child(fields)).into_any_element()
    }
    fn links(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let settings = self.profile.read_settings(|b| b.settings.clone());
        let archive = match settings.little_window_archive {
            browser_core::settings::LittleWindowArchive::OneHour => "1 hour",
            browser_core::settings::LittleWindowArchive::SixHours => "6 hours (default)",
            browser_core::settings::LittleWindowArchive::TwelveHours => "12 hours",
        };
        div().flex().flex_col()
            .child(div().h(px(134.)).overflow_hidden().bg(colors.rgb(0xd28586)).px(px(60.)).pt(px(46.))
                .child(div().h(px(130.)).rounded_t(px(8.)).bg(colors.rgb(0xe6d1cc)).border_1().border_color(colors.rgb(0xbcb7bd))
                    .child(div().h(px(25.)).bg(colors.rgb(0xbab9c5)).rounded_t(px(8.)).flex().items_center().justify_between().px(px(10.)).text_size(px(10.)).text_color(colors.rgb(0x555761)).child("Work").child("echo.local").child("Open in Work"))))
            .child(div().px(px(64.)).py(px(24.)).flex().flex_col().gap(px(24.))
                .child(div().px(px(10.)).child("Little Echo is a smaller, simpler Echo window. It’s perfect for quick lookups and reading funny tweets your friends send you."))
                .child(self.settings_group().child(self.toggle_row("Open Little Echo when I press ⌥⌘N in any app", Toggle::LittleWindowShortcut, cx)).child(self.settings_rule())
                    .child(self.toggle_row("Open Little Echo when clicking on links with ⌥⌘ held", Toggle::LittleWindowCommandClick, cx)).child(self.settings_rule())
                    .child(self.toggle_row("Links from other apps open in Little Echo", Toggle::LittleWindowLinks, cx)).child(self.settings_rule())
                    .child(self.popup_row("Archive Little Echos after:", archive, Popup::LittleArchive, 150., cx)))
                .child(self.settings_rule())
                .child(div().px(px(10.)).child("Peek is a way to preview a link without opening a new tab. It’s perfect for quickly looking at a link from an email, news article, or tweet."))
                .child(self.settings_group().child(self.toggle_row("Open a Peek window when clicking on links with Shift held", Toggle::PeekCommandShiftClick, cx)).child(self.settings_rule())
                    .child(self.toggle_row("Open a Peek window when clicking on links to other sites", Toggle::PeekPinned, cx))
                    .child(div().pb(px(6.)).italic().text_color(colors.rgb(0x929494)).child("Note: This only affects Favorites and Pinned tabs.")))
                .child(self.settings_rule()).child(self.action_row("Choose where links open inside Echo.", "Air Traffic Control…", None, cx)))
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
        let colors = self.colors;
        let enabled = self.profile.read_settings(|b| b.settings.enabled(toggle));
        row()
            .id(SharedString::from(format!("toggle-{toggle:?}")))
            .debug_selector(|| format!("settings-toggle-{toggle:?}"))
            .min_h(px(40.))
            .focusable()
            .tab_stop(true)
            .child(
                div()
                    .size(px(20.))
                    .rounded(px(4.))
                    .bg(colors.rgb(color))
                    .text_color(colors.rgb(0xffffff))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::control_sized(icon, 16.)),
            )
            .child(
                div()
                    .flex_1()
                    .child(label)
            )
            .child(self.settings_switch(enabled))
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
                    Icon::SoundEffects,
                    0x0091ff,
                ),
                (
                    "Haptic feedback when reordering tabs",
                    Toggle::HapticFeedback,
                    Icon::HapticFeedback,
                    0x0091ff,
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
            .children(sections.into_iter().enumerate().map(|(group_index,rows)| {
                self.settings_group().when(group_index==0 && !self.colors.light,|group|group.border_color(rgb(0x414244))).child(
                    // AppKit's reference border overlays the two 40 px feedback rows.
                    div().flex().flex_col().when(group_index==0,|rows|rows.my(px(-1.)))
                        .children(rows.into_iter().enumerate().map(
                            |(index,(label,toggle,icon,color))|div()
                                .when(index>0,|s|s.child(self.settings_rule().when(group_index==0 && !self.colors.light,|rule|rule.bg(rgb(0x414244)))))
                                .child(self.advanced_row(label,toggle,icon,color,cx))
                        ))
                )
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(self.settings_button("More Settings…").id("settings-more").debug_selector(||"settings-more".into()).cursor_pointer().on_click(cx.listener(|this,_,window,cx|this.open_more_settings(window,cx))))
                    .child(self.settings_button("Reset Translated-site Prompts").id("settings-reset-translation").debug_selector(||"settings-reset-translation".into()).cursor_pointer().on_click(cx.listener(|this,_,_,cx|this.reset_translation_prompts(cx))))
                    .child(div().flex_1())
                    .child(self.settings_button("Export Data").id("settings-export-data").debug_selector(||"settings-export-data".into()).cursor_pointer().on_click(cx.listener(|this,_,window,cx|this.export_data(window,cx)))),
            )
            .into_any_element()
    }
    fn open_more_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        let owner=self.owner.clone();let profile=self.profile.clone();
        window.remove_window();
        cx.defer(move |cx| {
            let find_host=|cx: &mut App| cx.windows().into_iter()
                .filter_map(|handle|handle.downcast::<BrowserWindow>()).find_map(|handle| {
                    let matches=handle.read(cx).is_ok_and(|view|!view.browser.is_private() && !view.browser.is_blank() && view.profile.handle.same_workspace(&profile));
                    if matches { handle.update(cx,|_,_,cx|cx.entity()).ok() } else { None }
                });
            let mut host=owner.and_then(|owner|owner.upgrade()).filter(|owner| {
                let view=owner.read(cx); !view.browser.is_private() && !view.browser.is_blank() && view.profile.handle.same_workspace(&profile)
            }).or_else(||find_host(cx));
            if host.is_none() { crate::new_window::open(false,cx);host=find_host(cx); }
            if let Some(host)=host {
                let main=host.update(cx,|view,cx| {
                    view.edit_browser(|browser| { browser.new_tab("browser://settings/",std::time::SystemTime::now()); });
                    if let Some(pages)=&view.pages { pages.borrow_mut().request_ui_focus(); }
                    cx.notify();view.main_window
                });
                let _=main.update(cx,|_,window,_|window.activate_window());
            }
        });
    }

    fn reset_translation_prompts(&self, cx: &mut Context<Self>) {
        if self.profile.read_settings(|model|model.user_profile(self.selected_profile).is_none()) { return; }
        if let Some(owner)=self.owner.as_ref().and_then(|owner|owner.upgrade()) {
            let selected=self.selected_profile;
            let accepted=owner.update(cx,|view,_|view.pages.as_ref().is_some_and(|pages|pages.borrow_mut().reset_translation_prompts(selected)));
            eprintln!("[settings] translation prompt reset accepted={accepted}");
        }
    }

    fn export_data(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.activate_window();
        let markdown=self.profile.read_settings(|browser| browser.export_sidebar_markdown());
        let directory=dirs::download_dir().unwrap_or_else(crate::profile_dir);
        let choice=crate::download_dialog::prepare_exact(crate::native::view_of(window),&directory,"Echo Data Export.md",cx);
        cx.spawn(async move |_,cx| {
            if let Ok(Some(path))=choice.choose(cx).await {
                let result=cx.background_executor().spawn(async move { std::fs::write(path,markdown) }).await;
                if let Err(error)=result { eprintln!("Sidebar export failed: {error}"); }
            }
        }).detach();
    }

    fn popup_options(&self, popup: Popup) -> Vec<(String, bool)> {
        let browser = self.profile.settings_snapshot();
        let profile = browser
            .user_profile(self.selected_profile)
            .unwrap_or(browser.active_user_profile());
        match popup {
            Popup::Search => self.search_popup_providers.iter()
                .map(|p| (p.name.clone(), profile.settings.search.default_provider().is_some_and(|selected|selected.id==p.id)))
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
        let colors = self.colors;
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
            .font_weight(gpui::FontWeight::NORMAL)
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
            .border_color(colors.rgb(0x636565))
            .bg(colors.rgb(0x343636))
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
                            .hover(move |s| s.bg(colors.rgb(0x1769c4)).text_color(rgb(0xffffff)))
                            .when(self.popup_keyboard && index == self.popup_selection, |s| s.debug_selector(|| "settings-popup-keyboard-selection".into()).bg(colors.rgb(0x087dff)).text_color(rgb(0xffffff)))
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
        let search_id=self.search_popup_providers.get(index).map(|p|p.id).filter(|_|self.search_popup_owner==Some(id));
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
                Popup::Search => if let Some(id)=search_id {let _=profile.settings.search.make_default(id);},
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
        self.colors = appearance::Colors::from_appearance(window.appearance());
        let colors = self.colors;
        self.validate_profile_editor(window, cx);
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
            Tab::Icon => self.icon(cx),
            Tab::Advanced => self.advanced(cx),
        };
        div()
            .id("settings-window")
            .debug_selector(|| "settings-window".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.rgb(0x292b2d))
            .text_color(colors.rgb(0xc4c5c5))
            .font_family(".SystemUIFont")
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_size(px(13.))
            .line_height(px(16.))
            .track_focus(&self.focus)
            .key_context(if self.profile_editor.is_some() { "ProfileSettingsEditor" } else if self.archive_scope.is_some() { "ArchiveScope" } else { "SettingsWindow" })
            .on_action(cx.listener(|this, _: &profiles::Accept, window, cx| this.accept_profile_editor(window,cx)))
            .on_action(cx.listener(|this, _: &profiles::Cancel, window, cx| this.cancel_profile_editor(window,cx)))
            .on_action(cx.listener(|this, _: &profiles::Next, window, cx| this.move_profile_editor_focus(false,window,cx)))
            .on_action(cx.listener(|this, _: &profiles::Previous, window, cx| this.move_profile_editor_focus(true,window,cx)))
            .on_action(cx.listener(|this, _: &archive::AllProfiles, _, cx| this.finish_archive_scope(true, cx)))
            .on_action(cx.listener(|this, _: &archive::OnlyThisProfile, _, cx| this.finish_archive_scope(false, cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.profile_editor.is_some() {
                    if event.keystroke.key == "w" && event.keystroke.modifiers.platform { window.remove_window(); cx.stop_propagation(); }
                    return;
                }
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
                            this.popup_keyboard = true;
                            this.popup_selection = (this.popup_selection + 1) % count;
                            cx.notify();
                            cx.stop_propagation();
                            return;
                        }
                        "up" => {
                            this.popup_keyboard = true;
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
                    .bg(colors.rgb(0x303334))
                    .border_b_1()
                    .border_color(colors.rgb(0x393b3b))
                    .child(
                        div()
                            .h(px(28.))
                            .flex()
                            .justify_center()
                            .items_center()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(colors.rgb(0xacafb0))
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
                                    .text_color(colors.rgb(if self.tab == tab {
                                        0x007aff
                                    } else {
                                        0x989a9a
                                    }))
                                    .when(self.tab == tab, |s| {
                                        s.bg(colors.rgb(0x363838)).border_1().border_color(colors.rgb(0x181919))
                                    })
                                    .child(tab.icon())
                                    .child(tab.label())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.change_tab(tab, window, cx)
                                    }))
                            })),
                    ),
            )
            .child(
                div()
                    .id(("settings-scroll", self.tab as usize)).debug_selector(|| "settings-scroll".into())
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
            .when(self.profile_editor.is_some(), |s| s.child(self.profile_editor_panel(cx)))
            .when(self.previews, |s| s.child(self.previews_panel(cx)))
            .when(self.archive_scope.is_some() || self.archive_closing.is_some(), |s| s.child(self.archive_scope_panel(cx)))
            .when_some(self.atc.clone(), |s, panel| s.child(div().absolute().inset_0().bg(gpui::hsla(0., 0., 0., 0.35))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().absolute().top(px(88.)).left(gpui::relative(0.5)).ml(px(-255.)).child(panel))))
    }
}

/// Help's View All opens the existing local Shortcuts settings section.
pub(crate) fn open_shortcuts(cx: &mut App) { open_section(Tab::Shortcuts,cx); }

pub(crate) fn open_section(section:Tab,cx: &mut App) {
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
                    view.change_tab(section, window, cx);
                    window.activate_window();
                    view.focus.focus(window);
                    cx.notify();
                }
            });
        }
    }
}

impl BrowserWindow {
    pub(crate) fn more_settings_is_open(&self) -> bool {
        self.browser.active_tab().is_some_and(|tab|url::Url::parse(&tab.url).is_ok_and(|url|
            url.scheme()=="browser" && url.host_str()==Some("settings") && matches!(url.path(),""|"/") && url.query().is_none()))
    }
    pub(crate) fn render_more_settings_root(&self) -> AnyElement {
        div().debug_selector(||"more-settings-page".into()).size_full().bg(rgb(0x202124)).text_color(rgb(0xe8eaed))
            .font_family(".SystemUIFont").flex().flex_col()
            .child(div().h(px(56.)).flex_none().px(px(26.)).flex().items_center().gap(px(14.))
                .child(icons::ionicon("menu-outline",18.,1.)).child(div().text_size(px(22.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Settings"))
                .child(div().flex_1()).child(icons::ionicon("search-outline",20.,1.)))
            // Arc's account/synchronization section is intentionally outside Echo's local profile surface.
            .child(div().mx_auto().mt(px(75.)).w_full().max_w(px(720.)).px(px(20.))
                .child(div().h(px(64.)).px(px(20.)).rounded(px(8.)).bg(rgb(0x292a2d)).flex().items_center().gap(px(16.))
                    .child(icons::ionicon("person-circle-outline",32.,1.))
                    .child(div().text_size(px(14.)).child(self.browser.active_user_profile().name().to_owned()))
                    .child(div().flex_1())))
            .into_any_element()
    }
}
