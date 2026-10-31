//! Standalone Today models share regular profile storage and history only.
use browser_core::{Section, UserProfileId};
use gpui::{div, point, prelude::*, px, size, App, Bounds, Context, TitlebarOptions, WindowBounds, WindowOptions};
use crate::{native, new_window, pages::Pages, profile::ProfileHandle, window::{self, BrowserWindow, Command, Target}};

pub(crate) fn open(cx: &mut App) {
    if crate::quit::is_quitting() { return; }
    let normal = match new_window::normal_source(cx) {
        Ok(normal) => normal,
        Err(error) => { crate::show_session_error(error, cx); return; }
    };
    let source = normal.profile.default_user_profile_id();
    open_for_source(source, cx);
}

fn open_for_source(source:UserProfileId,cx:&mut App){open_for_source_with_url(source,None,cx);}
pub(crate) fn open_url_for_profile(source:UserProfileId,url:String,cx:&mut App){open_for_source_with_url(source,Some(url),cx);}
fn open_for_source_with_url(source: UserProfileId, url:Option<String>, cx: &mut App) {
    if crate::quit::is_quitting() { return; }
    let normal = match new_window::normal_source(cx) {
        Ok(normal) => normal,
        Err(error) => { crate::show_session_error(error, cx); return; }
    };
    if !normal.profile.read_settings(|b|b.user_profile(source).is_some()){return;}
    let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
    let handle = cx.open_window(WindowOptions {
        window_min_size: window::window_min_size(),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some(crate::APP_NAME.into()), appears_transparent: true,
            traffic_light_position: Some(point(px(window::TRAFFIC_LIGHT_X), px(window::TRAFFIC_LIGHT_Y))), }),
        ..Default::default()
    }, move |window, cx| {
        let mut normal = normal;
        // A private-only launch still needs a retained regular context. Keep that
        // source alive; the Blank itself always gets an independent page store.
        if normal.pages.is_none() {
            normal.pages = crate::start_engine(window, &crate::profile_dir().join("Chromium"), false)
                .map(Pages::new).map(|pages| std::rc::Rc::new(std::cell::RefCell::new(pages)));
        }
        let pages = normal.pages.as_ref().and_then(|pages| pages.borrow()
            .independent_in_storage(native::view_of(window).unwrap_or(std::ptr::null_mut()), &crate::pages::StorageSelection::for_profile(source)));
        let profile = ProfileHandle::new_with_shared_history(normal.profile.blank_browser(source), &normal.profile, cx);
        cx.set_global(normal);
        cx.new(|cx| BrowserWindow::with_profile_pages(profile, None, pages, window, cx))
    }).unwrap();
    cx.activate(true);
    cx.spawn(async move |cx| {
        let _ = handle.update(cx, |view, window, cx| {
            window.activate_window();
            if let Some(url)=url {view.edit_browser(|b|b.new_tab(url,std::time::SystemTime::now()));view.focus(window);cx.notify();}
            else {view.open_command_bar(Target::NewTab, "", window, cx);}
        });
    }).detach();
}

/// A standalone window for a page dragged out of `space`: it keeps that
/// Space's name, icon and theme but has no Space switching, Pinned or
/// Favorites. The caller moves the page in.
pub(crate) fn open_torn(source: UserProfileId, space: &browser_core::Space, bounds: Bounds<gpui::Pixels>, cx: &mut App) -> Option<gpui::WindowHandle<BrowserWindow>> {
    if crate::quit::is_quitting() { return None; }
    let normal = new_window::normal_source(cx).ok()?;
    if !normal.profile.read_settings(|b| b.user_profile(source).is_some()) { return None; }
    let (name, icon, hue, theme) = (space.name.clone(), space.custom_icon.clone(), space.hue, space.theme.clone());
    let handle = cx.open_window(WindowOptions {
        window_min_size: window::window_min_size(),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some(crate::APP_NAME.into()), appears_transparent: true,
            traffic_light_position: Some(point(px(window::TRAFFIC_LIGHT_X), px(window::TRAFFIC_LIGHT_Y))), }),
        ..Default::default()
    }, move |window, cx| {
        let mut normal = normal;
        if normal.pages.is_none() {
            normal.pages = crate::start_engine(window, &crate::profile_dir().join("Chromium"), false)
                .map(Pages::new).map(|pages| std::rc::Rc::new(std::cell::RefCell::new(pages)));
        }
        let pages = normal.pages.as_ref().and_then(|pages| pages.borrow()
            .independent_in_storage(native::view_of(window).unwrap_or(std::ptr::null_mut()), &crate::pages::StorageSelection::for_profile(source)));
        let mut model = normal.profile.blank_browser(source);
        if let Some(blank) = model.spaces.first_mut() {
            blank.name = name; blank.custom_icon = icon; blank.hue = hue; blank.theme = theme;
        }
        let profile = ProfileHandle::new_with_shared_history(model, &normal.profile, cx);
        cx.set_global(normal);
        cx.new(|cx| {
            let mut view = BrowserWindow::with_profile_pages(profile, None, pages, window, cx);
            view.torn_space = true;
            view
        })
    }).ok()?;
    cx.activate(true);
    Some(handle)
}

impl BrowserWindow {
    pub(crate) fn blank_window(&mut self, cx: &mut Context<Self>) {
    if crate::quit::is_quitting() { return; }
        self.refresh_profile_projection();
        self.retain_normal_source(cx);
        self.close_command_bar(cx);
        // Default-source private behavior is verified. The originating named
        // profile for a private-to-Blank launch remains unverified; do not infer
        // a named identity from the ephemeral private model.
        let source = if self.browser.is_private() {
            match new_window::normal_source(cx) {
                Ok(normal) => normal.profile.default_user_profile_id(),
                Err(error) => { crate::show_session_error(error, cx); return; }
            }
        } else {
            self.browser.blank_source_user_profile_id().unwrap_or(self.browser.active_user_profile_id())
        };
        cx.defer(move |cx| open_for_source(source, cx));
    }

    /// Space/profile editing in Blank has no verified reference flow yet.
    pub(crate) fn blank_allows_command(&self, command: Command) -> bool {
        !self.browser.is_blank() || !matches!(command,
            Command::NewFolder | Command::NewFolderWithTab | Command::MoveToFolder | Command::PinToFolder(_)
            | Command::CollapseAllFolders | Command::ExpandAllFolders | Command::RenameFolder(_) | Command::ExpandFolder(_)
            | Command::NewSpace | Command::RenameSpace | Command::CustomizeSpaceIcon | Command::ResetSpace
            | Command::ToggleSpaceHeader | Command::ToggleFavoritesOrdering | Command::TogglePin | Command::ToggleFavorite
            | Command::TogglePinnedSection | Command::FocusSpace(_) | Command::MoveTab { .. })
    }

    /// Only successful removals call this: an initial empty model or cancelled
    /// split draft must remain open. Count real tabs, including split secondaries.
    pub(crate) fn after_tab_removal(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_blank() && self.browser.all_tabs().next().is_none() {
            self.close_window(cx);
        }
    }

    pub(crate) fn clear_today_tabs(&mut self, cx: &mut Context<Self>) {
        let before = self.browser.all_tabs().count();
        self.edit_browser(|browser| browser.clear_today(std::time::SystemTime::now()));
        if self.browser.all_tabs().count() < before {
            if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, true); }
            self.after_tab_removal(cx);
            // Clearing the last Blank tab closes the host; do not create a
            // notification child after its parent has been scheduled to close.
            if !(self.browser.is_blank() && self.browser.all_tabs().next().is_none()) {
                self.show_toast("Cleared Tabs! Use ⌘Z to undo.".into(), cx);
            }
        }
    }

    pub(crate) fn render_blank_tabs(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let space = self.browser.space();
        div().id("blank-tabs").debug_selector(|| "blank-tabs".into())
            .flex().flex_col().size_full().px(px(7.)).gap(px(3.))
            // Profile selection UI remains unsupported until its native flow is inspected.
            .child(div().debug_selector(|| "blank-profile".into()).h(px(30.)).flex_none()
                .flex().items_center().gap(px(6.)).pl(px(10.)).text_size(px(11.))
                // A window torn from a Space is titled by that Space; it cannot switch.
                .when(self.torn_space, |label| label.font_weight(gpui::FontWeight::SEMIBOLD)
                    .when_some(space.custom_icon.clone(), |label, icon| label.child(div().size(px(14.)).flex_none().flex().items_center().justify_center().child(crate::icons::glyph(&icon)))))
                .child(if self.torn_space { space.name.clone() } else { self.browser.active_user_profile().name().to_owned() }))
            .child(div().id("new-blank-tab").debug_selector(|| "new-tab".into())
                .h(px(38.)).flex_none().rounded(px(10.)).flex().items_center().pl(px(9.)).gap(px(9.))
                .text_size(px(14.)).hover(|style| style.bg(gpui::hsla(0., 0., 1., 0.12)))
                .on_click(cx.listener(|this, _, window, cx| this.open_command_bar(Target::NewTab, "", window, cx)))
                .child(crate::icons::control(crate::icons::Icon::Plus)).child("New Tab"))
            .child(div().id("blank-today-list").debug_selector(|| "today-list".into())
                .flex().flex_col().flex_1().min_h_0().overflow_y_scroll().gap(px(3.))
                .children(space.today.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).map(|tab| {
                    let active = space.active_tab.is_some_and(|id| self.browser.item_anchor(id) == tab.id);
                    self.render_tab(tab, Section::Today, active, 0., cx).w_full()
                }))).into_any_element()
    }
}
