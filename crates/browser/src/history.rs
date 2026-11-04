//! Browser-owned history page, rendered in GPUI rather than Chromium WebUI.
use crate::text_input::{Changed, TextInput};
use crate::window::BrowserWindow;
use browser_core::{history::HistoryEntry, TabId, UserProfileId};
use chrono::{DateTime, Local, NaiveDate};
use gpui::{
    actions, div, prelude::*, px, rgb, rgba, AnyElement, App, Context, Entity, FontWeight,
    Focusable, KeyBinding, Rgba, Subscription, Window,
};
use std::collections::HashSet;
use std::time::SystemTime;

pub(crate) const HISTORY_URL: &str = "echo://history";
const CONTEXT: &str = "HistoryPage";
actions!(history, [ViewHistory, OpenFirstResult, CancelClear]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", OpenFirstResult, Some(CONTEXT)),
        KeyBinding::new("escape", CancelClear, Some(CONTEXT)),
    ]);
}

pub(crate) struct HistoryPage {
    pub input: Entity<TextInput>,
    confirm_clear: Option<UserProfileId>,
    /// URLs ticked in the list, with the profile they were ticked for.
    selected: HashSet<String>,
    selected_owner: Option<UserProfileId>,
    confirm_selected: bool,
    /// The URL whose row menu is open.
    menu: Option<String>,
    limit: usize,
    last_tab: Option<TabId>,
    _changed: Subscription,
}

impl HistoryPage {
    pub fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let input = cx.new(|cx| TextInput::new("Search history…", cx));
        let changed = cx.subscribe(&input, |this, _, _: &Changed, cx| {
            this.history_page.limit = 100;
            cx.notify();
        });
        Self {
            input,
            confirm_clear: None,
            selected: HashSet::new(),
            selected_owner: None,
            confirm_selected: false,
            menu: None,
            limit: 100,
            last_tab: None,
            _changed: changed,
        }
    }
}

impl BrowserWindow {
    #[cfg(test)]
    pub(crate) fn history_selection_len(&self) -> usize { self.history_page.selected.len() }
    #[cfg(test)]
    pub(crate) fn history_row_menu_is_open(&self) -> bool { self.history_page.menu.is_some() }

    pub(crate) fn history_is_open(&self) -> bool {
        self.browser.active_split().is_none() && self.browser
            .active_tab()
            .is_some_and(|tab| tab.url.eq_ignore_ascii_case(HISTORY_URL))
    }

    pub(crate) fn focus_history_search(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.history_is_open() {
            return false;
        }
        self.history_page
            .input
            .read(cx)
            .focus_handle(cx)
            .focus(window);
        self.history_page
            .input
            .update(cx, |input, cx| input.select_all_text(cx));
        true
    }

    pub(crate) fn invalidate_history_confirmation(&mut self) {
        if self.history_page.confirm_clear.is_some_and(|owner| owner != self.browser.active_user_profile_id()) {
            self.history_page.confirm_clear = None;
        }
        if self.history_page.selected_owner.is_some_and(|owner| owner != self.browser.active_user_profile_id()) {
            self.clear_history_selection();
        }
    }

    fn clear_history_selection(&mut self) {
        self.history_page.selected.clear();
        self.history_page.selected_owner = None;
        self.history_page.confirm_selected = false;
    }

    pub(crate) fn sync_history_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.invalidate_history_confirmation();
        if self.history_is_open() {
            let tab = self.browser.space().active_tab;
            if self.history_page.last_tab != tab {
                self.history_page.last_tab = tab;
                self.focus_history_search(window, cx);
            }
        } else {
            self.history_page.last_tab = None;
            self.history_page.confirm_clear = None;
            self.history_page.menu = None;
            self.clear_history_selection();
        }
    }

    /// A painted confirmation remains bound to its original data owner even
    /// when a Space is reassigned before the next frame or event drain.
    pub(crate) fn confirm_history_clear(&mut self, owner: UserProfileId, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if self.history_page.confirm_clear != Some(owner) || owner != self.browser.active_user_profile_id() { return; }
        self.history_page.confirm_clear = None;
        self.edit_browser(|browser| {
            if browser.active_user_profile_id() == owner {
                if let Some(profile) = browser.user_profile_mut(owner) { profile.history.clear(); }
            }
        });
        self.history_page.input.update(cx, |input, cx| input.set_text("", cx));
        cx.notify();
    }

    pub(crate) fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab_menu(cx);
        let existing = self.browser.active_user_profile().favorites.iter()
            .chain(&self.browser.space().pinned)
            .chain(&self.browser.space().today)
            .find(|tab| tab.url.eq_ignore_ascii_case(HISTORY_URL))
            .map(|tab| tab.id);
        let now = SystemTime::now();
        if let Some(id) = existing {
            self.edit_browser(|browser| browser.activate_tab(id, now));
        } else {
            let id = self.edit_browser(|browser| browser.new_tab(HISTORY_URL, now));
            self.edit_browser(|browser| browser.set_title(id, "History"));
        }
        self.history_page.confirm_clear = None;
        self.focus_history_search(window, cx);
        cx.notify();
    }

    pub(crate) fn history_from_command(&mut self, cx: &mut Context<Self>) {
        self.close_command_bar(cx);
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = browser.update(cx, |this, cx| this.open_history(window, cx));
            });
        });
    }

    fn filtered_history(&self, cx: &App) -> Vec<&HistoryEntry> {
        let query = self
            .history_page
            .input
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        let mut entries: Vec<_> = self.browser.active_user_profile().history
            .iter()
            .filter(|entry| {
                entry.title.to_lowercase().contains(&query)
                    || entry.url.to_lowercase().contains(&query)
            })
            .collect();
        // Day cards need newest-first order, and Enter opens the first row shown.
        entries.sort_by(|a, b| b.visited_at.cmp(&a.visited_at));
        entries
    }

    fn delete_selected_history(&mut self, owner: UserProfileId, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if owner != self.browser.active_user_profile_id() || self.history_page.selected_owner != Some(owner) { return; }
        let urls: Vec<_> = self.history_page.selected.iter().cloned().collect();
        self.edit_browser(|browser| {
            for url in &urls { browser.delete_history_entry(owner, url); }
        });
        self.clear_history_selection();
        cx.notify();
    }

    fn open_history_url(
        &mut self,
        url: &str,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.edit_browser(|browser| browser.new_tab(url, SystemTime::now()));
        self.edit_browser(|browser| browser.set_title(id, title));
        self.focus(window);
        cx.notify();
    }

    fn render_history_row(&self, index: usize, entry: &HistoryEntry, owner: UserProfileId, c: Palette, cx: &mut Context<Self>) -> AnyElement {
        let (url, title) = (entry.url.clone(), entry.title.clone());
        let visited: DateTime<Local> = entry.visited_at.into();
        let domain = host(&entry.url);
        let selected = self.history_page.selected.contains(&entry.url);
        let menu_open = self.history_page.menu.as_deref() == Some(entry.url.as_str());
        let toggle = entry.url.clone();
        let open_menu = entry.url.clone();
        let remove = entry.url.clone();
        let site = domain.clone();
        div()
            .id(("history-visit", index))
            .debug_selector(move || format!("history-visit-{index}"))
            .flex()
            .items_center()
            .w_full()
            .h(px(36.0))
            .pl(px(14.0))
            .cursor_pointer()
            .when(selected, |row| row.bg(c.selected_row))
            .when(!selected, |row| row.hover(|s| s.bg(c.hover)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_history_url(&url, &title, window, cx)
            }))
            .child(
                div()
                    .id(("history-select", index))
                    .debug_selector(move || format!("history-select-{index}"))
                    .flex_none()
                    .mx(px(10.0))
                    .size(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(2.0))
                    .when(selected, |b| b.bg(c.accent).text_color(c.on_accent))
                    .when(!selected, |b| b.border_2().border_color(c.checkbox))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.refresh_profile_projection();
                        if this.browser.active_user_profile_id() != owner { return; }
                        let page = &mut this.history_page;
                        if !page.selected.remove(&toggle) { page.selected.insert(toggle.clone()); }
                        page.selected_owner = (!page.selected.is_empty()).then_some(owner);
                        page.confirm_selected = false;
                        cx.notify();
                    }))
                    .when(selected, |b| b.child(crate::icons::control_sized(crate::icons::Icon::Check, 12.))),
            )
            .child(
                div()
                    .flex_none()
                    .ml(px(6.0))
                    .min_w(px(96.0))
                    .text_size(px(12.0))
                    .text_color(c.time)
                    .child(visited.format("%-I:%M %p").to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pl(px(5.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(div().flex_none().size(px(24.0)).flex().items_center().justify_center()
                        .child(self.render_command_page_icon(&entry.url, None, c.dark)))
                    .child(div().ml(px(8.0)).min_w_0().truncate().text_size(px(13.0)).font_weight(FontWeight::MEDIUM).child(entry.title.clone()))
                    .child(div().flex_none().max_w(px(280.0)).ml(px(8.0)).truncate().text_size(px(12.0)).text_color(c.secondary).child(domain)),
            )
            .child(
                div()
                    .id(("history-menu", index))
                    .debug_selector(move || format!("history-menu-{index}"))
                    .flex_none()
                    .relative()
                    .mr(px(20.0))
                    .size(px(24.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.0))
                    .rounded_full()
                    .hover(|s| s.bg(c.hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let page = &mut this.history_page;
                        page.menu = if page.menu.as_deref() == Some(open_menu.as_str()) { None } else { Some(open_menu.clone()) };
                        cx.notify();
                    }))
                    .children((0..3).map(|_| div().size(px(2.0)).rounded_full().bg(c.dots)))
                    .when(menu_open, |this| this.child(gpui::deferred(
                        gpui::anchored()
                            .position_mode(gpui::AnchoredPositionMode::Local)
                            .position(gpui::point(px(-168.0), px(24.0)))
                            .snap_to_window_with_margin(px(8.0))
                            .child(
                                div()
                                    .id("history-row-menu")
                                    .occlude()
                                    .w(px(192.0))
                                    .py(px(8.0))
                                    .rounded(px(8.0))
                                    .bg(c.card)
                                    .border_1()
                                    .border_color(c.separator)
                                    .shadow_lg()
                                    .text_size(px(13.0))
                                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                        this.history_page.menu = None;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .id(("history-from-site", index))
                                            .debug_selector(move || format!("history-from-site-{index}"))
                                            .h(px(32.0)).px(px(16.0)).flex().items_center()
                                            .hover(|s| s.bg(c.hover))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.history_page.menu = None;
                                                this.history_page.input.update(cx, |input, cx| input.set_text(site.clone(), cx));
                                                cx.notify();
                                            }))
                                            .child("More from this site"),
                                    )
                                    .child(
                                        div()
                                            .id(("history-remove", index))
                                            .debug_selector(move || format!("history-remove-{index}"))
                                            .h(px(32.0)).px(px(16.0)).flex().items_center()
                                            .hover(|s| s.bg(c.hover))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.history_page.menu = None;
                                                this.refresh_profile_projection();
                                                if this.browser.active_user_profile_id() != owner { return; }
                                                this.edit_browser(|browser| browser.delete_history_entry(owner, &remove));
                                                this.history_page.selected.remove(&remove);
                                                cx.notify();
                                            }))
                                            .child("Remove from history"),
                                    ),
                            ),
                    ).with_priority(1))),
            )
            .into_any_element()
    }

    pub(crate) fn render_history(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let owner = self.browser.active_user_profile_id();
        let c = Palette::new(self.theme_is_dark());
        let entries = self.filtered_history(cx);
        let count = entries.len();
        let today = Local::now().date_naive();
        let mut cards: Vec<(NaiveDate, Vec<AnyElement>)> = Vec::new();
        for (index, entry) in entries.iter().take(self.history_page.limit).enumerate() {
            let day = DateTime::<Local>::from(entry.visited_at).date_naive();
            let row = self.render_history_row(index, entry, owner, c, cx);
            match cards.last_mut() {
                Some((current, rows)) if *current == day => rows.push(row),
                _ => cards.push((day, vec![row])),
            }
        }
        let has_history = !self.browser.active_user_profile().history.is_empty();
        let selection = self.history_page.selected.len();
        let search_focused = self.history_page.input.read(cx).focus_handle(cx).is_focused(window);
        // Chrome folds its side bar away when the page gets narrow.
        let page_width = f32::from(window.viewport_size().width) - self.sidebar_width() * self.sidebar_amount();
        let wide = page_width >= 900.0;
        let clear = |cx: &mut Context<Self>| {
            div()
                .id("clear-history")
                .debug_selector(|| "clear-history".into())
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.refresh_profile_projection();
                    if this.browser.active_user_profile_id() != owner { return; }
                    this.history_page.confirm_clear = Some(owner);
                    this.focus_history_search(window, cx);
                    cx.notify();
                }))
        };
        let toolbar = if selection > 0 {
            div()
                .flex_none()
                .h(px(56.0))
                .px(px(24.0))
                .flex()
                .items_center()
                .gap(px(16.0))
                .bg(c.accent)
                .text_color(c.on_accent)
                .child(div().flex_1().text_size(px(16.0)).child(format!("{selection} selected")))
                .child(
                    div()
                        .id("history-selection-cancel")
                        .debug_selector(|| "history-selection-cancel".into())
                        .px(px(16.0)).h(px(32.0)).flex().items_center().rounded(px(16.0))
                        .text_size(px(13.0)).font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_history_selection();
                            cx.notify();
                        }))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id("history-delete-selected")
                        .debug_selector(|| "history-delete-selected".into())
                        .px(px(16.0)).h(px(32.0)).flex().items_center().rounded(px(16.0))
                        .bg(c.on_accent).text_color(c.accent)
                        .text_size(px(13.0)).font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.history_page.confirm_selected = true;
                            cx.notify();
                        }))
                        .child("Delete"),
                )
        } else {
            div()
                .flex_none()
                .h(px(56.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_none()
                        .when(wide, |title| title.w(px(256.0)))
                        .pl(px(24.0))
                        .pr(px(16.0))
                        .text_size(px(22.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child("History"),
                )
                .child(
                    // The field starts at the card column's edge, as in Chrome.
                    div().flex_1().min_w_0().px(px(20.0)).flex().justify_center().child(
                        div().w_full().max_w(px(968.0)).child(
                            div()
                                .id("history-search")
                                .debug_selector(|| "history-search".into())
                                .w_full()
                                .max_w(px(700.0))
                                .h(px(44.0))
                                .px(px(14.0))
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .rounded(px(22.0))
                                .bg(c.field)
                                .border_2()
                                .border_color(if search_focused { c.ring } else { c.field })
                                .text_size(px(13.0))
                                // The whole field is the target, including its icon and padding.
                                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                    this.history_page.input.read(cx).focus_handle(cx).focus(window);
                                }))
                                .child(div().flex_none().text_color(c.secondary).child(crate::icons::control_sized(crate::icons::Icon::Search, 16.)))
                                .child(self.history_page.input.clone()),
                        ),
                    ),
                )
                .when(!wide && has_history, |bar| bar.child(
                    clear(cx).flex_none().mr(px(24.0)).text_size(px(13.0)).text_color(c.accent).child("Delete history"),
                ))
        };
        let confirm = |message: &'static str, cancel: AnyElement, accept: AnyElement| {
            div()
                .w_full()
                .max_w(px(960.0))
                .mb(px(16.0))
                .p(px(16.0))
                .rounded(px(12.0))
                .bg(c.card)
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .child(div().flex_1().child(message))
                .child(cancel)
                .child(accept)
        };
        let dialog_button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(32.0))
                .px(px(16.0))
                .flex()
                .items_center()
                .rounded(px(16.0))
                .cursor_pointer()
                .font_weight(FontWeight::MEDIUM)
                .when(primary, |b| b.bg(c.accent).text_color(c.on_accent))
                .when(!primary, |b| b.text_color(c.accent))
                .child(label)
        };
        let clear_confirm = (self.history_page.confirm_clear == Some(owner)).then(|| confirm(
            "Permanently delete all browsing history?",
            dialog_button("history-clear-cancel", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.history_page.confirm_clear = None;
                    cx.notify();
                }))
                .into_any_element(),
            dialog_button("history-clear-confirm", "Delete history", true)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm_history_clear(owner, cx);
                }))
                .into_any_element(),
        ));
        let selected_confirm = (self.history_page.confirm_selected && selection > 0).then(|| confirm(
            "Remove the selected pages from your history?",
            dialog_button("history-selected-cancel", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.history_page.confirm_selected = false;
                    cx.notify();
                }))
                .into_any_element(),
            dialog_button("history-selected-confirm", "Remove", true)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.delete_selected_history(owner, cx);
                }))
                .into_any_element(),
        ));
        self.echo_page_corners(div())
            .id("history-page")
            .debug_selector(|| "history-page".into())
            .key_context(CONTEXT)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(c.page)
            .text_color(c.text)
            .on_action(cx.listener(|this, _: &OpenFirstResult, window, cx| {
                let first = this
                    .filtered_history(cx)
                    .first()
                    .map(|entry| (entry.url.clone(), entry.title.clone()));
                if let Some((url, title)) = first {
                    this.open_history_url(&url, &title, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CancelClear, _, cx| {
                this.history_page.confirm_clear = None;
                this.history_page.confirm_selected = false;
                this.history_page.menu = None;
                cx.notify();
            }))
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(wide, |page| page.child(
                        div()
                            .flex_none()
                            .w(px(256.0))
                            .pt(px(8.0))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                div()
                                    .h(px(40.0))
                                    .mr(px(2.0))
                                    .pl(px(24.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(20.0))
                                    .rounded_r(px(20.0))
                                    .bg(c.nav_selected)
                                    .text_color(c.nav_selected_text)
                                    .child(crate::icons::control_sized(crate::icons::Icon::History, 18.))
                                    .child("Echo history"),
                            )
                            .when(has_history, |nav| nav.child(
                                clear(cx)
                                    .h(px(40.0))
                                    .mr(px(2.0))
                                    .pl(px(24.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(20.0))
                                    .rounded_r(px(20.0))
                                    .hover(|s| s.bg(c.hover))
                                    .child(crate::icons::ionicon("trash-outline", 18., 1.))
                                    .child("Delete browsing history"),
                            )),
                    ))
                    .child(
                        div()
                            .id("history-list")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .pt(px(24.0))
                            .px(px(24.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .children(clear_confirm)
                            .children(selected_confirm)
                            .children(cards.into_iter().map(|(day, rows)| {
                                div()
                                    .w_full()
                                    .max_w(px(960.0))
                                    .flex_none()
                                    .mb(px(16.0))
                                    .pb(px(8.0))
                                    .rounded(px(12.0))
                                    .bg(c.card)
                                    // Chrome lifts light cards off their white page with a shadow.
                                    .when(!c.dark, |card| card.shadow(vec![
                                        gpui::BoxShadow { color: gpui::hsla(0.6, 0.06, 0.25, 0.3), offset: gpui::point(px(0.), px(1.)), blur_radius: px(2.), spread_radius: px(0.) },
                                        gpui::BoxShadow { color: gpui::hsla(0.6, 0.06, 0.25, 0.15), offset: gpui::point(px(0.), px(1.)), blur_radius: px(3.), spread_radius: px(1.) },
                                    ]))
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .pt(px(23.0))
                                            .pb(px(13.0))
                                            .px(px(24.0))
                                            .text_size(px(16.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(day_title(day, today)),
                                    )
                                    .children(rows)
                            }))
                            .when(count == 0, |this| {
                                this.child(
                                    div()
                                        .py(px(80.0))
                                        .text_size(px(16.0))
                                        .text_color(c.secondary)
                                        .child(if has_history {
                                            "No search results found"
                                        } else {
                                            "Your browsing history appears here"
                                        }),
                                )
                            })
                            .when(count > self.history_page.limit, |this| {
                                this.child(
                                    div()
                                        .id("history-more")
                                        .debug_selector(|| "history-more".into())
                                        .flex_none()
                                        .mb(px(16.0))
                                        .h(px(32.0))
                                        .px(px(16.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(16.0))
                                        .text_size(px(13.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(c.accent)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(c.hover))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.history_page.limit += 100;
                                            cx.notify();
                                        }))
                                        .child("Show more"),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}

/// Colors sampled from Chrome's own history page in its light and dark themes.
#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    page: Rgba,
    card: Rgba,
    text: Rgba,
    secondary: Rgba,
    time: Rgba,
    separator: Rgba,
    hover: Rgba,
    accent: Rgba,
    on_accent: Rgba,
    field: Rgba,
    ring: Rgba,
    checkbox: Rgba,
    dots: Rgba,
    nav_selected: Rgba,
    nav_selected_text: Rgba,
    selected_row: Rgba,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                page: rgb(0x202124),
                card: rgb(0x292a2d),
                text: rgb(0xe3e3e3),
                secondary: rgb(0xc5c7c5),
                time: rgb(0xc5c7c5),
                separator: rgb(0x37383a),
                hover: rgba(0xffffff14),
                accent: rgb(0x93b3f2),
                on_accent: rgb(0x202124),
                field: rgb(0x282828),
                ring: rgb(0xaec6f6),
                checkbox: rgb(0x8f918f),
                dots: rgb(0x9ba0a5),
                nav_selected: rgb(0x93b3f2),
                nav_selected_text: rgb(0x202124),
                selected_row: rgba(0x93b3f229),
            }
        } else {
            Self {
                dark,
                page: rgb(0xffffff),
                card: rgb(0xffffff),
                text: rgb(0x1f1f1f),
                secondary: rgb(0x444746),
                time: rgb(0x646464),
                separator: rgb(0xebebeb),
                hover: rgba(0x1f1f1f0f),
                accent: rgb(0x2656c9),
                on_accent: rgb(0xffffff),
                field: rgb(0xedeff6),
                ring: rgb(0x2656c9),
                checkbox: rgb(0x757775),
                dots: rgb(0x1f1f1f),
                nav_selected: rgb(0xe9f0fd),
                nav_selected_text: rgb(0x3265cb),
                selected_row: rgba(0x2656c91f),
            }
        }
    }
}

/// A day card's heading: "Today - Wednesday, November 4, 2026".
fn day_title(day: NaiveDate, today: NaiveDate) -> String {
    let date = day.format("%A, %B %-d, %Y");
    if day == today {
        format!("Today - {date}")
    } else if Some(day) == today.pred_opt() {
        format!("Yesterday - {date}")
    } else {
        date.to_string()
    }
}

/// The host a row shows next to its title. Non-web addresses show in full.
fn host(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else { return url.into() };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority.rsplit('@').next().unwrap_or(authority).to_string()
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn day_titles_follow_chrome_and_cross_year_boundaries() {
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let today = day(2027, 1, 1);
        assert_eq!(day_title(today, today), "Today - Friday, January 1, 2027");
        assert_eq!(day_title(day(2026, 12, 31), today), "Yesterday - Thursday, December 31, 2026");
        assert_eq!(day_title(day(2026, 11, 4), today), "Wednesday, November 4, 2026");
    }

    #[test]
    fn host_strips_only_the_address_around_it() {
        assert_eq!(host("https://user:pw@www.example.com:8080/a?b#c"), "www.example.com:8080");
        assert_eq!(host("https://example.com?x=/y"), "example.com");
        assert_eq!(host("data:text/plain,hello"), "data:text/plain,hello");
    }
}
