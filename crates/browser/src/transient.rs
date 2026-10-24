//! Window-local quick lookups share website storage, never the owner's tabs.
use std::time::SystemTime;
use browser_core::{SpaceId, UserProfileId};
use gpui::{actions, div, point, prelude::*, px, size, AnyElement, App, Bounds, Context,
    Entity, Focusable, KeyBinding, Subscription, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions};
use crate::{icons::{self, Icon}, native, new_window::{self, NormalProfile}, pages::{Pages, StorageSelection},
    profile::ProfileHandle, window::{BrowserWindow, Overlay, OverlayKind, PopupFocus, Target}};

pub(crate) const TOOLBAR_HEIGHT: f32 = 46.;
actions!(transient, [NewLittleWindow, OpenInSpace, DismissPicker, NextSpace, PreviousSpace, ConfirmSpace]);

pub(crate) struct Transient {
    pub source: NormalProfile,
    pub space: SpaceId,
    pub picker: Option<Picker>,
}
pub(crate) struct Picker {
    input: Entity<crate::text_input::TextInput>,
    selected: usize,
    _changed: Subscription,
    pub overlay: Option<WindowHandle<Overlay>>,
}

/// Little's resting address shows the hostname; editing/copying retains the URL.
pub(crate) fn lookup_host(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return crate::window::display_url(url); };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") { return crate::window::display_url(url); }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') { return authority.split_once(']').map_or(authority, |(host, _)| &authority[..host.len()+1]).to_owned(); }
    let host = authority.rsplit_once(':').filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit())).map_or(authority, |(host, _)| host);
    host.strip_prefix("www.").unwrap_or(host).to_owned()
}

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", DismissPicker, Some("TransientPicker")),
        KeyBinding::new("down", NextSpace, Some("TransientPicker")),
        KeyBinding::new("up", PreviousSpace, Some("TransientPicker")),
        KeyBinding::new("enter", ConfirmSpace, Some("TransientPicker")),
    ]);
}

pub(crate) fn open_external_urls(urls: Vec<String>, cx: &mut App) {
    for url in urls {
        let web_url = url.split_once("://").is_some_and(|(scheme, rest)|
            !rest.is_empty() && (scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")));
        if web_url { open(Some(url), cx); }
    }
}

pub(crate) fn open(url: Option<String>, cx: &mut App) {
    let source = match new_window::normal_source(cx) {
        Ok(source) => source,
        Err(error) => { crate::show_session_error(error, cx); return; }
    };
    let projection = source.profile.projection(&source.profile.selection());
    let space = projection.space().id;
    let profile = projection.active_user_profile_id();
    open_with(source, space, profile, url, cx);
}

fn open_with(source: NormalProfile, space: SpaceId, profile: UserProfileId, url: Option<String>, cx: &mut App) {
    if crate::quit::is_quitting() { return; }
    let bounds = Bounds::centered(None, size(px(1000.), px(760.)), cx);
    let empty = url.is_none();
    let handle = cx.open_window(WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some("Little Echo".into()), appears_transparent: true,
            traffic_light_position: Some(point(px(10.), px(17.))), }),
        ..Default::default()
    }, move |window, cx| {
        let mut source = source;
        if source.pages.is_none() {
            source.pages = crate::start_engine(window, &crate::profile_dir().join("Chromium"), false)
                .map(Pages::new).map(|pages| std::rc::Rc::new(std::cell::RefCell::new(pages)));
        }
        let pages = source.pages.as_ref().and_then(|pages| pages.borrow().independent_in_storage(
            native::view_of(window).unwrap_or(std::ptr::null_mut()), &StorageSelection::for_profile(profile)));
        let mut browser = source.profile.blank_browser(profile);
        let projection = source.profile.projection(&source.profile.selection());
        if let Some(space) = projection.spaces.iter().find(|candidate| candidate.id == space) {
            browser.spaces[0].hue = space.hue;
            browser.spaces[0].theme = space.theme.clone();
        }
        if let Some(url) = url { browser.new_tab(url, SystemTime::now()); }
        let profile = ProfileHandle::new_with_shared_history(browser, &source.profile, cx);
        let selection = profile.selection();
        cx.set_global(source.clone());
        cx.new(|cx| {
            let mut view = BrowserWindow::with_profile_pages(profile, Some(selection), pages, window, cx);
            if let Some(pages) = &view.pages { pages.borrow_mut().square_top(); }
            view.sidebar_visible = false;
            view.transient = Some(Transient { source, space, picker: None });
            view.focus(window);
            view
        })
    });
    cx.activate(true);
    if let Ok(handle) = handle {
        cx.spawn(async move |cx| {
            let _ = handle.update(cx, |view, window, cx| {
                window.activate_window();
                if empty { view.open_command_bar(Target::NewTab, "", window, cx); }
            });
        }).detach();
    }
}

impl BrowserWindow {
    pub(crate) fn new_little_window(&mut self, cx: &mut Context<Self>) {
        self.retain_normal_source(cx);
        self.close_command_bar(cx);
        let (source, space, profile) = if let Some(transient) = &self.transient {
            (transient.source.clone(), transient.space, self.browser.active_user_profile_id())
        } else if !self.browser.is_private() && !self.browser.is_blank() {
            (NormalProfile { profile: self.profile.handle.clone(), pages: self.pages.clone() },
                self.browser.space().id, self.browser.active_user_profile_id())
        } else { cx.defer(|cx| open(None, cx)); return; };
        cx.defer(move |cx| open_with(source, space, profile, None, cx));
    }

    pub(crate) fn render_transient_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let transient = self.transient.as_ref().expect("compact chrome");
        let projection = transient.source.profile.projection(&transient.source.profile.selection());
        let space = projection.spaces.iter().find(|space| space.id == transient.space).unwrap_or(projection.space());
        let title = self.browser.active_tab().map(|tab| lookup_host(&tab.url)).unwrap_or_default();
        let button = |id: &'static str| div().id(id).debug_selector(move || id.into())
            .h(px(30.)).flex_none().flex().items_center().justify_center().rounded(px(6.))
            .hover(|style| style.bg(gpui::hsla(0., 0., 1., 0.15)));
        div().flex().flex_col().size_full().text_color(self.theme_colors().foreground)
            .child(div().h(px(TOOLBAR_HEIGHT)).flex_none().flex().items_center().gap(px(8.)).pr(px(8.))
                .child(div().w(px(75.)).h_full().flex_none().child(self.render_traffic_lights(cx)))
                .child(div().relative().flex_1().min_w_0().h(px(32.)).rounded(px(6.))
                    .bg(gpui::hsla(0., 0., 1., 0.13)).flex().items_center().px(px(8.))
                    .child(div().text_size(px(11.)).flex_none().child(self.browser.active_user_profile().name().to_owned()))
                    .child(div().id("little-address").debug_selector(|| "little-address".into()).flex_1().min_w_0()
                        .text_size(px(13.)).text_center().truncate().child(title)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let target = this.browser.active_tab().map(|tab| (Target::Tab(tab.id), tab.url.clone()));
                            if let Some((target, url)) = target { this.open_command_bar(target, &url, window, cx); }
                        })))
                    .child(button("little-copy-url").w(px(28.)).child(icons::control(Icon::Link))
                        .on_click(cx.listener(|this, _, _, cx| this.copy_url(false, cx)))))
                .child(div().flex_none().h(px(32.)).flex().items_center().rounded(px(6.))
                    .bg(gpui::hsla(0., 0., 0., 0.15)).text_size(px(13.))
                    .child(button("little-open-in-space").px(px(10.)).gap(px(7.))
                        .child("Open in").child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(space.name.clone()))
                        .child(div().text_size(px(11.)).rounded(px(3.)).bg(gpui::hsla(0., 0., 1., 0.08)).child("⌘O"))
                        .on_click(cx.listener(|this, _, _, cx| this.promote_transient(None, cx))))
                    .child(div().h(px(18.)).w(px(1.)).bg(gpui::hsla(0., 0., 1., 0.15)))
                    .child(button("little-space-picker").w(px(28.)).child(icons::chevron(false))
                        .on_click(cx.listener(|this, _, window, cx| this.toggle_transient_picker(window, cx))))))
            .child(div().debug_selector(|| "transient-page".into()).relative().flex_1().min_h_0().w_full()
                .bg(gpui::white())
                .when(self.pages.is_none(), |surface| surface.child(self.browser.active_tab().map(|tab| tab.url.clone()).unwrap_or_default()))
                .children(self.render_page_viewport_canvas()))
            .into_any_element()
    }

    pub(crate) fn promote_transient(&mut self, destination: Option<SpaceId>, cx: &mut Context<Self>) {
        let Some(transient) = &self.transient else { return; };
        let Some(tab) = self.browser.active_tab().cloned() else { return; };
        let source = transient.source.clone();
        let space = destination.unwrap_or(transient.space);
        let roots: Vec<_> = cx.windows().into_iter().filter_map(|w| w.downcast::<BrowserWindow>()).collect();
        let target = roots.into_iter().find(|handle| handle.read(cx).is_ok_and(|view|
            view.transient.is_none() && view.profile.handle.same_workspace(&source.profile)));
        self.close_transient_picker(cx);
        let lookup = cx.weak_entity();
        cx.defer(move |cx| {
            let target = target.or_else(|| {
                new_window::open(false, cx);
                cx.windows().into_iter().filter_map(|w| w.downcast::<BrowserWindow>()).find(|handle|
                    handle.read(cx).is_ok_and(|view| view.transient.is_none() && view.profile.handle.same_workspace(&source.profile)))
            });
            if let Some(target) = target {
                let moved = target.update(cx, |view, window, cx| {
                    view.refresh_profile_projection();
                    if !view.browser.spaces.iter().any(|candidate| candidate.id == space) { return false; }
                    view.edit_browser(|browser| {
                        if let Some(index) = browser.spaces.iter().position(|candidate| candidate.id == space) {
                            browser.switch_space(index);
                            let id = browser.new_tab(tab.url.clone(), SystemTime::now());
                            browser.set_title(id, tab.title.clone());
                        }
                    });
                    window.activate_window();
                    view.focus(window);
                    cx.notify();
                    true
                }).unwrap_or(false);
                if moved { let _ = lookup.update(cx, |this, cx| this.close_window(cx)); }
            }
        });
    }

    pub(crate) fn close_transient_picker(&mut self, cx: &mut Context<Self>) {
        self.close_transient_picker_with_focus(PopupFocus::Restore, cx);
    }

    pub(crate) fn close_transient_picker_with_focus(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        let picker = self.transient.as_mut().and_then(|state| state.picker.take());
        if let Some(picker) = picker { self.close_popup_overlay(picker.overlay, focus, true, cx); cx.notify(); }
    }

    fn toggle_transient_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.transient.as_ref().is_some_and(|state| state.picker.is_some()) { self.close_transient_picker(cx); return; }
        self.close_command_bar(cx);
        if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
        let input = cx.new(|cx| crate::text_input::TextInput::new("Search Spaces…", cx));
        let changed = cx.observe(&input, |this, _, cx| {
            if let Some(picker) = this.transient.as_mut().and_then(|state| state.picker.as_mut()) { picker.selected = 0; }
            cx.notify();
        });
        self.transient.as_mut().unwrap().picker = Some(Picker { input: input.clone(), selected: 0, _changed: changed, overlay: None });
        let bounds = window.bounds();
        let parent = self.native_view;
        let browser = cx.weak_entity();
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.transient.as_ref().is_some_and(|state| state.picker.is_some())).unwrap_or(false) { return; }
            if let Ok(overlay) = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None, kind: WindowKind::Normal,
                is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            }, |window, cx| {
                input.read(cx).focus_handle(cx).focus(window);
                if let (Some(parent), Some(child)) = (parent, native::view_of(window)) { native::attach_overlay(parent, child, false); }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::TransientPicker, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(picker) = this.transient.as_mut().and_then(|state| state.picker.as_mut()) { picker.overlay = Some(overlay); }
                });
            }
        });
        cx.notify();
    }

    fn transient_spaces(&self, cx: &App) -> Vec<(SpaceId, String, String, f32)> {
        let Some(state) = &self.transient else { return Vec::new(); };
        let Some(picker) = &state.picker else { return Vec::new(); };
        let query = picker.input.read(cx).text().to_lowercase();
        let projection = state.source.profile.projection(&state.source.profile.selection());
        let mut spaces: Vec<_> = projection.spaces.iter().filter(|space| space.name.to_lowercase().contains(&query)).collect();
        spaces.sort_by_key(|space| space.id != state.space);
        spaces.into_iter().map(|space| (space.id, space.name.clone(), space.custom_icon.clone().unwrap_or_default(), space.hue)).collect()
    }

    fn select_transient_space(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.transient_spaces(cx).len();
        if let Some(picker) = self.transient.as_mut().and_then(|state| state.picker.as_mut()) {
            if count > 0 { picker.selected = (picker.selected as isize + delta).rem_euclid(count as isize) as usize; }
        }
        cx.notify();
    }

    pub(crate) fn render_transient_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = &self.transient else { return div().into_any_element(); };
        let Some(picker) = &state.picker else { return div().into_any_element(); };
        let rows = self.transient_spaces(cx);
        let focus = picker.input.read(cx).focus_handle(cx);
        div().id("transient-picker-backdrop").size_full().key_context("TransientPicker").track_focus(&focus)
            .on_action(cx.listener(|this, _: &DismissPicker, _, cx| this.close_transient_picker(cx)))
            .on_action(cx.listener(|this, _: &NextSpace, _, cx| this.select_transient_space(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousSpace, _, cx| this.select_transient_space(-1, cx)))
            .on_action(cx.listener(|this, _: &ConfirmSpace, _, cx| {
                let rows = this.transient_spaces(cx);
                let selected = this.transient.as_ref().and_then(|state| state.picker.as_ref()).map_or(0, |picker| picker.selected);
                if let Some((space, ..)) = rows.get(selected) { this.promote_transient(Some(*space), cx); }
            }))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.close_transient_picker(cx)))
            .child(crate::panel::shell("transient-space-picker").absolute().right(px(4.)).top(px(4.)).w(px(244.))
                .rounded(px(9.)).flex().flex_col().overflow_hidden()
                .child(self.theme_surface(true))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().flex_none().h(px(46.)).flex().items_center().pl(px(14.)).pr(px(8.)).gap(px(10.))
                    .text_size(px(13.)).child(icons::control_sized(Icon::Search, 14.))
                    .child(div().flex_1().min_w_0().child(picker.input.clone()))
                    .child(div().id("close-space-picker").size(px(28.)).rounded(px(6.)).flex().items_center().justify_center()
                        .hover(|style| style.bg(gpui::hsla(0., 0., 1., 0.1)))
                        .child(icons::chevron(true))
                        .on_click(cx.listener(|this, _, _, cx| this.close_transient_picker(cx)))))
                .child(div().id("transient-spaces").max_h(px(350.)).overflow_y_scroll().flex().flex_col()
                    .children(rows.into_iter().enumerate().map(|(index, (space, name, icon, hue))| {
                        div().id(("transient-space", index)).debug_selector(move || format!("transient-space-{index}"))
                            .h(px(46.)).flex_none().flex().items_center().px(px(10.)).gap(px(16.))
                            .bg(gpui::hsla(0., 0., 1., if picker.selected == index { 0.12 } else { 0. }))
                            .hover(|style| style.bg(gpui::hsla(0., 0., 1., 0.16)))
                            .text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(div().size(px(26.)).flex_none().rounded(px(5.)).flex().items_center().justify_center()
                                .bg(crate::window::tint(hue / 360., 0.5))
                                .child(if icon.is_empty() { div().size(px(8.)).rounded_full().bg(gpui::white()).into_any_element() } else { icons::glyph(&icon) }))
                            .child(div().flex_1().min_w_0().truncate().child(name))
                            .on_click(cx.listener(move |this, _, _, cx| this.promote_transient(Some(space), cx)))
                    }))))
            .into_any_element()
    }
}
