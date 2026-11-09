//! Settings → Notifications (Arc's `settings/content/notifications`): the
//! profile-wide default and per-site exceptions, drawn in GPUI.
use crate::{icons, text_input::TextInput, window::BrowserWindow};
use browser_core::{TabId, UserProfileId};
use engine::{PermissionExceptions, PermissionKind as K, PermissionValue as V};
use gpui::{actions, div, prelude::*, px, rgb, AnyElement, App, Context, Entity, KeyBinding, Window};
use std::time::Duration;

pub(crate) const URL: &str = "echo://settings/content/notifications";
const CONTEXT: &str = "NotificationsPage";
actions!(notifications_page, [Cancel, Accept]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("enter", Accept, Some(CONTEXT)),
    ]);
}

/// The address for `profile`'s Notifications page.
pub(crate) fn url(profile: UserProfileId) -> String {
    format!("{URL}?profile={}", serde_json::to_string(&profile).unwrap())
}

#[derive(Default)]
pub(crate) struct State {
    scope: Option<(TabId, UserProfileId)>,
    data: Option<PermissionExceptions>,
    filter: Option<Entity<TextInput>>,
    /// The Add sheet: `true` adds to the allowed list.
    add: Option<(bool, Entity<TextInput>)>,
    /// The origin whose ⋮ menu is open.
    menu: Option<String>,
}

/// `https://host:port` from what a person types: `example.com`, a full URL…
pub(crate) fn site_origin(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() || input.contains(char::is_whitespace) {
        return None;
    }
    let url = if input.contains("://") { url::Url::parse(input).ok()? } else { url::Url::parse(&format!("https://{input}")).ok()? };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(|h| !h.contains('.') && h != "localhost") {
        return None;
    }
    Some(format!("{}://{}:{}", url.scheme(), url.host_str()?, url.port_or_known_default()?))
}

impl BrowserWindow {
    fn notifications_scope(&self) -> Option<(TabId, UserProfileId)> {
        if self.browser.is_private() || self.browser.is_blank() || self.browser.active_split().is_some() {
            return None;
        }
        let tab = self.browser.active_tab()?;
        let url = url::Url::parse(&tab.url).ok()?;
        if url.scheme() != "echo" || url.host_str() != Some("settings") || url.path() != "/content/notifications" {
            return None;
        }
        let profile = match url.query_pairs().find(|(k, _)| k == "profile") {
            Some((_, v)) => serde_json::from_str(&v).ok()?,
            None => self.browser.active_user_profile_id(),
        };
        self.profile.handle.read_settings(|b| b.user_profile(profile).is_some()).then_some((tab.id, profile))
    }
    #[cfg(test)]
    pub(crate) fn notifications_add_open(&self) -> bool {
        self.notifications_page.add.is_some()
    }
    pub(crate) fn notifications_page_is_open(&self) -> bool {
        self.notifications_scope().is_some()
    }
    pub(crate) fn open_notifications_page(&mut self, profile: UserProfileId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.profile.handle.read_settings(|b| b.user_profile(profile).is_some()) {
            return;
        }
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        let tab = self.edit_browser(|b| b.new_tab(url(profile), std::time::SystemTime::now()));
        self.edit_browser(|b| b.set_title(tab, "Settings - Notifications"));
        self.sidebar_navigation.controller.select(&self.browser, crate::sidebar_pages::SidebarPage::Space(self.browser.space().id));
        self.sync_notifications_page(cx);
        window.activate_window();
        cx.notify();
    }
    pub(crate) fn sync_notifications_page(&mut self, cx: &mut Context<Self>) {
        let scope = self.notifications_scope();
        if self.notifications_page.scope == scope {
            return;
        }
        let filter = self.notifications_page.filter.take().unwrap_or_else(|| {
            let input = cx.new(|cx| TextInput::new("Filter sites on page", cx));
            cx.observe(&input, |_, _, cx| cx.notify()).detach();
            input
        });
        filter.update(cx, |input, cx| input.set_text("", cx));
        self.notifications_page = State { scope, filter: Some(filter), ..Default::default() };
        let Some(scope) = scope else { return };
        self.refresh_notifications(cx);
        // Sites can change their permission from a page prompt while this is open.
        cx.spawn(async move |owner, cx| loop {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let open = owner
                .update(cx, |view, cx| {
                    if view.notifications_page.scope != Some(scope) || crate::quit::is_quitting() {
                        return false;
                    }
                    view.refresh_notifications(cx);
                    true
                })
                .unwrap_or(false);
            if !open {
                break;
            }
        })
        .detach();
    }
    fn refresh_notifications(&mut self, cx: &mut Context<Self>) {
        let Some((_, profile)) = self.notifications_page.scope else { return };
        let data = self.pages.as_ref().and_then(|pages| pages.borrow().permission_exceptions(profile, K::Notifications));
        if data != self.notifications_page.data {
            self.notifications_page.data = data;
            cx.notify();
        }
    }
    fn set_notification_default(&mut self, value: V, cx: &mut Context<Self>) {
        let Some((_, profile)) = self.notifications_page.scope else { return };
        if let Some(pages) = &self.pages {
            pages.borrow_mut().set_default_permission(profile, K::Notifications, value);
        }
        self.refresh_notifications(cx);
    }
    fn set_notification_site(&mut self, origin: &str, value: V, cx: &mut Context<Self>) {
        let Some((_, profile)) = self.notifications_page.scope else { return };
        if let Some(pages) = &self.pages {
            pages.borrow_mut().set_permission_exception(profile, origin, K::Notifications, value);
        }
        self.notifications_page.menu = None;
        self.refresh_notifications(cx);
        cx.notify();
    }
    fn open_notification_add(&mut self, allow: bool, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| TextInput::new("[*.]example.com", cx));
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        gpui::Focusable::focus_handle(input.read(cx), cx).focus(window);
        self.notifications_page.add = Some((allow, input));
        self.notifications_page.menu = None;
        cx.notify();
    }
    fn accept_notification_add(&mut self, cx: &mut Context<Self>) {
        let Some((allow, input)) = &self.notifications_page.add else { return };
        let Some(origin) = site_origin(input.read(cx).text()) else { return };
        let value = if *allow { V::Allow } else { V::Block };
        self.notifications_page.add = None;
        self.set_notification_site(&origin, value, cx);
    }
    fn cancel_notifications(&mut self, cx: &mut Context<Self>) {
        self.notifications_page.add = None;
        self.notifications_page.menu = None;
        cx.notify();
    }

    pub(crate) fn render_notifications_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.notifications_page;
        let data = state.data.clone().unwrap_or_default();
        let filter = state.filter.as_ref().map(|f| f.read(cx).text().trim().to_lowercase()).unwrap_or_default();
        let shown = |origins: &[String]| origins.iter().filter(|o| filter.is_empty() || o.to_lowercase().contains(&filter)).cloned().collect::<Vec<_>>();
        let muted = rgb(0xbdc1c6);
        let ask = data.default != V::Block;
        let radio = |id: &'static str, selected: bool, icon: &'static str, label: &'static str, value: V| {
            div()
                .id(id)
                .debug_selector(move || format!("notifications-{id}"))
                .h(px(48.))
                .flex()
                .items_center()
                .gap(px(16.))
                .border_t_1()
                .border_color(rgb(0x3c4043))
                .cursor_pointer()
                .child(
                    div().size(px(16.)).rounded_full().border_2().border_color(if selected { rgb(0x8ab4f8) } else { rgb(0x9aa0a6) }).flex().items_center().justify_center()
                        .when(selected, |dot| dot.child(div().size(px(8.)).rounded_full().bg(rgb(0x8ab4f8)))),
                )
                .child(icons::ionicon(icon, 18., 1.))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.set_notification_default(value, cx)))
        };
        let list = |allow: bool, title: &'static str, origins: Vec<String>| {
            let add_id = if allow { "notifications-add-allowed" } else { "notifications-add-blocked" };
            div()
                .mt(px(20.))
                .child(
                    div().h(px(40.)).flex().items_center().justify_between().child(title).child(
                        div()
                            .id(add_id)
                            .debug_selector(move || add_id.into())
                            .px(px(15.))
                            .h(px(32.))
                            .rounded(px(16.))
                            .border_1()
                            .border_color(rgb(0x8ab4f8))
                            .text_color(rgb(0x8ab4f8))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child("Add")
                            .on_click(cx.listener(move |this, _, window, cx| this.open_notification_add(allow, window, cx))),
                    ),
                )
                .when(origins.is_empty(), |list| list.child(div().h(px(40.)).pl(px(30.)).flex().items_center().text_color(muted).child("No sites added")))
                .children(origins.into_iter().map(|origin| {
                    let menu_open = self.notifications_page.menu.as_deref() == Some(origin.as_str());
                    let toggle = origin.clone();
                    div()
                        .relative()
                        .h(px(48.))
                        .pl(px(30.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(icons::ionicon("globe-outline", 16., 1.))
                        .child(div().flex_1().min_w_0().truncate().child(origin.clone()))
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!("notifications-menu-{origin}")))
                                .debug_selector({ let o = origin.clone(); move || format!("notifications-menu-{o}") })
                                .size(px(32.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|b| b.bg(rgb(0x3c4043)))
                                .child(icons::ionicon("ellipsis-vertical-sharp", 16., 1.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let open = this.notifications_page.menu.as_deref() == Some(toggle.as_str());
                                    this.notifications_page.menu = (!open).then(|| toggle.clone());
                                    cx.notify();
                                })),
                        )
                        .when(menu_open, |row| {
                            let choices = [(if allow { "Block" } else { "Allow" }, if allow { V::Block } else { V::Allow }), ("Remove", V::Default)];
                            row.child(
                                div().absolute().top(px(40.)).right(px(0.)).w(px(160.)).py(px(8.)).rounded(px(4.)).bg(rgb(0x3c3c3c)).shadow_lg().occlude()
                                    .children(choices.into_iter().map(|(label, value)| {
                                        let origin = origin.clone();
                                        div()
                                            .id(gpui::SharedString::from(format!("notifications-{label}-{origin}")))
                                            .debug_selector({ let o = origin.clone(); move || format!("notifications-{label}-{o}") })
                                            .h(px(32.))
                                            .px(px(16.))
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .hover(|r| r.bg(rgb(0x4a4a4a)))
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.set_notification_site(&origin, value, cx)
                                            }))
                                    })),
                            )
                        })
                }))
        };
        let back_profile = state.scope.map(|(_, p)| p);
        let page = div()
            .id("notifications-page")
            .debug_selector(|| "notifications-page".into())
            .size_full()
            .overflow_y_scroll()
            .on_click(cx.listener(|this, _, _, cx| {
                if this.notifications_page.menu.take().is_some() {
                    cx.notify();
                }
            }))
            .child(
                div().w_full().max_w(px(680.)).mx_auto().mt(px(18.)).mb(px(40.)).rounded(px(8.)).bg(rgb(0x292a2d)).px(px(20.)).pb(px(20.))
                    .child(
                        div().h(px(56.)).flex().items_center().gap(px(16.)).font_weight(gpui::FontWeight::MEDIUM)
                            .child(
                                div().id("notifications-back").debug_selector(|| "notifications-back".into()).size(px(32.)).rounded_full().flex().items_center().justify_center().cursor_pointer().hover(|b| b.bg(rgb(0x3c4043)))
                                    .child(icons::ionicon("arrow-back", 18., 1.))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let (Some(tab), Some(profile)) = (this.browser.active_tab().map(|t| t.id), back_profile) else { return };
                                        let url = format!("{}?profile={}", crate::browsing_data::PRIVACY, serde_json::to_string(&profile).unwrap());
                                        this.edit_browser(|b| b.navigate(tab, url));
                                        cx.notify();
                                    })),
                            )
                            .child(div().flex_1().child("Notifications"))
                            .children(state.filter.clone().map(|filter| div().w(px(160.)).h(px(32.)).flex().items_center().gap(px(6.)).font_weight(gpui::FontWeight::NORMAL).border_b_1().border_color(rgb(0x9aa0a6)).child(icons::ionicon("filter", 14., 1.)).child(filter))),
                    )
                    .child(div().pt(px(8.)).child("Sites usually send notifications to let you know about breaking news or chat messages."))
                    .child(div().mt(px(16.)).font_weight(gpui::FontWeight::MEDIUM).child("Default behavior"))
                    .child(div().mb(px(8.)).text_color(muted).child("Sites automatically follow this setting when you visit them"))
                    .child(radio("ask", ask, "notifications-outline", "Sites can ask to send notifications", V::Ask))
                    .child(radio("block", !ask, "notifications-off-outline", "Don’t allow sites to send notifications", V::Block))
                    .child(div().mt(px(20.)).font_weight(gpui::FontWeight::MEDIUM).child("Customized behaviors"))
                    .child(div().text_color(muted).child("Sites listed below follow a custom setting instead of the default"))
                    .child(list(false, "Not allowed to send notifications", shown(&data.blocked)))
                    .child(list(true, "Allowed to send notifications", shown(&data.allowed))),
            );
        let sheet = state.add.as_ref().map(|(_, input)| {
            let valid = site_origin(input.read(cx).text()).is_some();
            div().absolute().inset_0().bg(gpui::rgba(0x00000099)).flex().items_center().justify_center().occlude()
                .child(
                    div().id("notifications-add-sheet").debug_selector(|| "notifications-add-sheet".into()).w(px(512.)).rounded(px(8.)).bg(rgb(0x292a2d)).p(px(20.)).flex().flex_col().gap(px(16.))
                        .child(div().text_size(px(16.)).child("Add a site"))
                        .child(div().flex().flex_col().gap(px(4.)).child(div().text_color(muted).child("Site"))
                            .child(div().h(px(36.)).px(px(8.)).rounded(px(4.)).bg(rgb(0x202124)).border_b_1().border_color(rgb(0x8ab4f8)).flex().items_center().child(input.clone())))
                        .child(div().flex().justify_end().gap(px(8.))
                            .child(div().id("notifications-add-cancel").debug_selector(|| "notifications-add-cancel".into()).px(px(15.)).h(px(32.)).rounded(px(16.)).border_1().border_color(rgb(0x5f6368)).text_color(rgb(0x8ab4f8)).flex().items_center().cursor_pointer().child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_notifications(cx))))
                            .child(div().id("notifications-add-confirm").debug_selector(|| "notifications-add-confirm".into()).px(px(15.)).h(px(32.)).rounded(px(16.)).bg(rgb(0x8ab4f8)).text_color(rgb(0x062e6f)).flex().items_center().cursor_pointer().when(!valid, |b| b.opacity(0.4)).child("Add")
                                .on_click(cx.listener(|this, _, _, cx| this.accept_notification_add(cx))))),
                )
        });
        // Keys reach the page and the Add sheet's field alike.
        let content = div()
            .relative()
            .size_full()
            .text_size(px(13.))
            .line_height(px(20.))
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.cancel_notifications(cx)))
            .on_action(cx.listener(|this, _: &Accept, _, cx| this.accept_notification_add(cx)))
            .child(page)
            .children(sheet);
        self.settings_shell(crate::settings_shell::Section::Privacy, content, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::site_origin;
    #[test]
    fn typed_sites_become_exact_origins() {
        assert_eq!(site_origin("example.com").as_deref(), Some("https://example.com:443"));
        assert_eq!(site_origin("http://news.example:8080/a").as_deref(), Some("http://news.example:8080"));
        assert_eq!(site_origin("  "), None);
        assert_eq!(site_origin("not a site"), None);
        assert_eq!(site_origin("ftp://example.com"), None);
        assert_eq!(site_origin("intranet"), None);
    }
}

/// Settings' Notifications row: open the page in the profile's browser window,
/// creating one if none is open.
pub(crate) fn open_from_settings(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    cx: &mut App,
) {
    cx.defer(move |cx| open_from_settings_now(profile, owner, selected, true, cx));
}
fn open_from_settings_now(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    create: bool,
    cx: &mut App,
) {
    if crate::quit::is_quitting() {
        return;
    }
    let preferred = owner.and_then(|owner| owner.upgrade()).map(|owner| owner.read(cx).main_window);
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
        .min_by_key(|window| if preferred.is_some_and(|p| p.window_id() == window.window_id()) { 0 } else { 1 });
    if let Some(target) = target {
        let _ = target.update(cx, |this, window, cx| this.open_notifications_page(selected, window, cx));
    } else if create {
        crate::new_window::open(false, cx);
        cx.defer(move |cx| open_from_settings_now(profile, None, selected, false, cx));
    }
}
