//! Split View creation and pane chrome. Page surfaces remain owned by CEF.
use crate::window::{BrowserWindow, Command, Suggestion, Target};
use browser_core::{Browser, omnibox, splits::{Axis, Placement, MAX_SPLIT_PANES}, Tab, TabId};
use gpui::{div, hsla, prelude::*, px, AnyElement, Context};
use std::time::SystemTime;

#[derive(Clone)]
struct DividerDrag {
    split: browser_core::splits::SplitId,
    axis: Axis,
    panes: Vec<TabId>,
    weights: Vec<f32>,
    index: usize,
    offset: std::rc::Rc<std::cell::Cell<f32>>,
}

impl gpui::Render for DividerDrag {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement { div().size_0() }
}

pub(crate) const EMPTY_PANE_URL: &str = "browser://newtab";

#[derive(Clone)]
pub(crate) struct SplitDraft {
    pub pane: TabId,
    pub anchor: TabId,
    copied: bool,
    original: Option<browser_core::splits::SplitView>,
    previous_space: browser_core::SpaceId,
    previous_active: Option<TabId>,
}

impl SplitDraft {
    pub(crate) fn owns_selection(&self, browser: &Browser) -> bool {
        browser.space().active_tab.is_some_and(|tab| tab == self.pane
            || (self.copied && browser.related_tabs(self.anchor).contains(&tab)))
    }

    pub(crate) fn rollback(&self, browser: &mut Browser) {
        browser.discard_split_draft(self.pane, self.copied.then_some(self.anchor), SystemTime::now());
        if let Some(original) = &self.original {
            browser.set_split_axis(original.anchor, original.axis);
            browser.set_split_weights(original.anchor, &original.weights);
        }
        if let Some(index) = browser.spaces.iter().position(|space| space.id == self.previous_space) { browser.switch_space(index); }
        if let Some(active) = self.previous_active { browser.activate_tab(active, SystemTime::now()); }
    }
}

impl BrowserWindow {
    pub(crate) fn expand_current_split(&mut self, cx: &mut Context<Self>) {
        if self.split_draft.is_some() { return; }
        let Some(split) = self.browser.active_split() else { return; };
        let split_id = split.id;
        let pane = split.active;
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        // Command Bar actions may run in a child window. Read the main
        // viewport after its event handler has released the window borrow.
        cx.defer(move |cx| {
            let Ok(size) = main.update(cx, |_, window, _| window.viewport_size()) else { return; };
            let _ = browser.update(cx, |this, cx| {
                let Some(split) = this.browser.active_split().filter(|split| split.id == split_id && split.active == pane) else { return; };
                let index = split.panes.iter().position(|id| *id == pane).unwrap();
                let extent = if split.axis == Axis::Horizontal {
                    f64::from(f32::from(size.width)) - f64::from(this.sidebar_width() * this.sidebar_amount()
                        + crate::pages::CONTENT_INSET * (2. - this.sidebar_amount()))
                } else { f64::from(f32::from(size.height)) - f64::from(2. * crate::pages::CONTENT_INSET) };
                let weights = crate::split_layout::expanded_weights(extent, split.panes.len(), index);
                this.edit_browser(|browser| browser.set_split_weights(pane, &weights));
                if let Some(pages) = &this.pages { pages.borrow_mut().sync(&this.browser, this.page_content_visible()); }
                cx.notify();
            });
        });
    }

    pub(crate) fn duplicate_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        self.close_find(cx);
        if self.edit_browser(|browser| browser.duplicate_item(id, SystemTime::now())).is_none() {
            self.show_toast("You can have up to 12 Favorites".into(), cx);
        }
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }

    pub(crate) fn can_add_split(&self, anchor: TabId) -> bool {
        self.browser.all_tabs().any(|tab| tab.id == anchor && !omnibox::is_internal_url(&tab.url))
            && self.browser.split_for(anchor).is_none_or(|split| split.panes.len() < MAX_SPLIT_PANES)
    }

    pub(crate) fn split_suggestions(&self, anchor: TabId, text: &str) -> Vec<Suggestion> {
        if self.split_draft.is_none() && !self.can_add_split(anchor) { return Vec::new(); }
        let query = text.trim().to_lowercase();
        let mut rows: Vec<_> = omnibox::resolve(text).filter(|resolved| !omnibox::is_internal_url(resolved.url())).map(Suggestion::Load).into_iter().collect();
        let favorites = self.browser.active_user_profile().favorites.iter().map(|tab| ("Favorites", tab));
        rows.extend(favorites.chain(self.browser.spaces.iter().filter(|space| space.user_profile_id == self.browser.active_user_profile_id()).flat_map(|space| space.pinned.iter().chain(&space.today).map(move |tab| (space.name.as_str(), tab))))
            .filter(|(_, tab)| tab.id != anchor && tab.url != EMPTY_PANE_URL && self.browser.split_for(tab.id).is_none() && !omnibox::is_internal_url(&tab.url))
            .filter(|(_, tab)| query.is_empty() || tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)
                || tab.page_title.as_ref().is_some_and(|title| title.to_lowercase().contains(&query)))
            .take(6).map(|(space, tab)| Suggestion::SplitTab { id: tab.id, title: tab.title.clone(), space: space.into() }));
        if !query.is_empty() {
            let open: std::collections::HashSet<_> = self.browser.all_tabs().filter(|tab| self.browser.user_profile_id_for_tab(tab.id) == Some(self.browser.active_user_profile_id())).map(|tab| tab.url.as_str()).collect();
            rows.extend(self.browser.active_user_profile().history.iter()
                .filter(|entry| !open.contains(entry.url.as_str()))
                .filter(|entry| entry.title.to_lowercase().contains(&query) || entry.url.to_lowercase().contains(&query))
                .take(4).map(|entry| Suggestion::History { title: entry.title.clone(), url: entry.url.clone() }));
        }
        rows
    }

    pub(crate) fn edit_split(&mut self, placement: Option<Placement>, cx: &mut Context<Self>) {
        let Some(anchor) = self.browser.space().active_tab else { self.close_command_bar(cx); return; };
        let placement = placement.unwrap_or_else(|| if self.browser.active_split().is_some_and(|split| split.axis == Axis::Vertical) { Placement::Below } else { Placement::Right });
        self.begin_split(anchor, placement, cx);
    }

    pub(crate) fn begin_split(&mut self, source: TabId, placement: Placement, cx: &mut Context<Self>) {
        if !self.can_add_split(source) {
            self.close_command_bar(cx);
            self.show_toast("Cannot Add New Pane".into(), cx);
            return;
        }
        self.cancel_split_draft();
        let now = SystemTime::now();
        let previous_space = self.browser.space().id;
        let previous_active = self.browser.space().active_tab;
        let copied = self.browser.is_favorite(source) || self.browser.spaces.iter().any(|space| space.pinned.iter().any(|tab| tab.id == source));
        let original = (!copied).then(|| self.browser.split_for(source).cloned()).flatten();
        let anchor = if copied { self.edit_browser(|browser| browser.copy_item_to_today(source, now)).expect("existing split source") } else { source };
        let pane = self.edit_browser(|browser| browser.new_tab(EMPTY_PANE_URL, now));
        self.edit_browser(|browser| browser.set_title(pane, "New Tab"));
        self.edit_browser(|browser| browser.add_split(anchor, pane, placement, now)).expect("validated split source");
        self.split_draft = Some(SplitDraft { pane, anchor, copied, original, previous_space, previous_active });
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Split { anchor, placement };
            bar.selected = 0;
            bar.input.update(cx, |input, cx| { input.set_text("", cx); input.set_placeholder("Search or Enter URL…", cx); });
        }
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }

    pub(crate) fn cancel_split_draft(&mut self) {
        if let Some(draft) = self.split_draft.take() { self.edit_browser(|browser| draft.rollback(browser)); }
    }

    #[cfg(test)]
    pub(crate) fn session_json(&self) -> String {
        self.profile.session_json(&self.split_draft)
    }

    pub(crate) fn split_commands(&self) -> Vec<(Command, &'static str, &'static str, bool)> {
        let active = self.browser.active_tab().is_some_and(|tab| !omnibox::is_internal_url(&tab.url));
        let split = self.browser.active_split();
        let horizontal = split.is_some_and(|split| split.axis == Axis::Horizontal);
        let pane_count = split.map_or(0, |split| split.panes.len());
        vec![
            (Command::AddSplit(None), "Add Split View…", "add split view|add pane|new split view", active),
            (Command::AddSplit(Some(Placement::Left)), "Add Left Split…", "add left split|split left", active),
            (Command::AddSplit(Some(Placement::Right)), "Add Right Split…", "add right split|split right", active),
            (Command::AddSplit(Some(Placement::Above)), "Add Top Split…", "add top split|split above", active),
            (Command::AddSplit(Some(Placement::Below)), "Add Bottom Split…", "add bottom split|split below", active),
            (Command::CloseSplitPane, "Close Pane", "close pane|close current pane", split.is_some()),
            (Command::SplitAxis(Axis::Horizontal), "Convert to Horizontal Split", "convert to horizontal split|horizontal split", split.is_some() && !horizontal),
            (Command::SplitAxis(Axis::Vertical), "Convert to Vertical Split", "convert to vertical split|vertical split", horizontal),
            (Command::SeparateSplit, "Separate All Tabs", "separate all tabs|separate split view", split.is_some()),
            (Command::SeparatePane, "Separate Tab", "separate tab|separate pane", split.is_some()),
            (Command::ExpandCurrentSplit, "Expand Current Split", "expand current split|expand pane", split.is_some() && self.split_draft.is_none()),
            (Command::MovePane(-1), if horizontal { "Move Pane Left" } else { "Move Pane Up" }, if horizontal { "move pane left" } else { "move pane up" }, split.is_some()),
            (Command::MovePane(1), if horizontal { "Move Pane Right" } else { "Move Pane Down" }, if horizontal { "move pane right" } else { "move pane down" }, split.is_some()),
            (Command::CyclePane(1), "Focus Next Pane", "focus next pane|next pane", split.is_some()),
            (Command::CyclePane(-1), "Focus Previous Pane", "focus previous pane|previous pane", split.is_some()),
            (Command::FocusPane(0), "Focus Pane 1", "focus pane 1", pane_count > 0),
            (Command::FocusPane(1), "Focus Pane 2", "focus pane 2", pane_count > 1),
            (Command::FocusPane(2), "Focus Pane 3", "focus pane 3", pane_count > 2),
            (Command::FocusPane(3), "Focus Pane 4", "focus pane 4", pane_count > 3),
        ]
    }

    pub(crate) fn close_split_pane(&mut self, id: TabId, cx: &mut Context<Self>) {
        if self.split_draft.as_ref().is_some_and(|draft| draft.pane == id) {
            self.close_command_bar(cx);
            return;
        }
        if self.edit_browser(|browser| browser.close_split_pane(id, SystemTime::now())) {
            self.close_find(cx);
            if let Some(pages) = &self.pages { let mut pages = pages.borrow_mut(); pages.discard(id); pages.sync(&self.browser, self.page_content_visible()); }
            self.after_tab_removal(cx);
            cx.notify();
        }
    }

    pub(crate) fn render_item_icon(&self, tab: &Tab, hue: f32) -> AnyElement {
        if let Some(icon) = self.browser.item_icon(tab.id) { return div().debug_selector(|| self.sidebar_debug_name((move || format!("item-icon-{}", tab.id.get()))())).child(crate::icons::glyph(icon)).into_any_element(); }
        if let Some(split) = self.browser.split_for(tab.id) {
            div().flex().flex_none().gap(px(2.)).children(split.panes.iter().filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == *id)).take(2).map(|tab| self.render_favicon(tab, hue))).into_any_element()
        } else { self.render_favicon(tab, hue).into_any_element() }
    }

    pub(crate) fn render_sidebar_item_contents(&self, tab: &Tab, hue: f32, cx: &mut Context<Self>) -> AnyElement {
        let anchor = tab.id.get();
        let painted_space = self.browser.space().id;
        let painted_profile = self.browser.active_user_profile_id();
        if let Some(split) = self.browser.split_for(tab.id).filter(|split| split.title.is_none()) {
            let count = split.panes.len();
            let selected = self.browser.active_split().is_some_and(|active| active.id == split.id);
            let dark = self.theme_is_dark_for(painted_space);
            let hovered = self.hovered_row == Some(self.browser.item_anchor(tab.id));
            return div().debug_selector(|| self.sidebar_debug_name((move || format!("sidebar-split-{anchor}"))())).flex_1().min_w_0().flex().items_center().gap(px(4.)).children(split.panes.iter().enumerate().filter_map(|(index, id)| {
                let tab = self.browser.all_tabs().find(|tab| tab.id == *id)?;
                let id = *id;
                let close_visible = hovered;
                Some(div().id(("sidebar-pane", id.get() as usize)).debug_selector(|| self.sidebar_debug_name((move || format!("sidebar-pane-{}", id.get()))()))
                    .relative().flex_1().flex_basis(px(0.)).min_w_0().h(px(28.)).pl(px(7.)).pr(px(4.)).rounded(px(8.)).flex().items_center().gap(px(11.))
                    .overflow_hidden()
                    .when(selected, |this| this.bg(hsla(0., 0., 1., if dark { 0.075 } else { 0.12 })))
                    .hover(move |style| style.bg(hsla(0., 0., 1., if dark { 0.12 } else { 0.2 })))
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                        if this.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Library {
                            this.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(painted_space), cx);
                        }
                        if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                        this.edit_browser(|browser| browser.activate_tab(id, SystemTime::now()));
                        if event.click_count() == 2 { this.open_command_bar(Target::RenameTab(id), &this.browser.item_title(id), window, cx); }
                        cx.notify();
                    }))
                    // The inactive item has hairlines; the selected item has equal pills.
                    .when(!selected && index + 1 < count, |this| this.child(div().absolute().right_0().top(px(6.)).h(px(18.)).w(px(1.)).bg(hsla(0., 0., 0., 0.15))))
                    .child(self.render_favicon(tab, hue))
                    .when(self.show_tab_audio(tab), |this| this.child(self.render_mute_control(tab, false, cx)))
                    .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("sidebar-pane-title-{}", id.get()))())).flex_1().min_w_0().when(close_visible, |this| this.mr(px(22.))).overflow_hidden().child(crate::sidebar_title::SidebarTitle::new(tab.title.clone())))
                    .when(close_visible, |this| this.child(div().id(("sidebar-pane-close",id.get() as usize))
                        .debug_selector(|| self.sidebar_debug_name((move || format!("sidebar-pane-close-{}",id.get()))()))
                        .absolute().right(px(2.)).top(px(5.)).size(px(18.)).flex().items_center().justify_center().rounded(px(4.))
                        .hover(|style| style.bg(hsla(0.,0.,1.,0.1)))
                        .on_click(cx.listener(move |this,_,_,cx| {
                            cx.stop_propagation();
                            if !this.prepare_sidebar_target(painted_space,painted_profile) { return; }
                            this.close_split_pane(id,cx);
                        })).child(crate::icons::control_sized(crate::icons::Icon::Close,14.)))))
            })).into_any_element();
        }
        div().flex_1().min_w_0().flex().items_center().gap(px(9.))
            .child(self.render_item_icon(tab, hue))
            .when(self.show_tab_audio(tab), |this| this.child(self.render_mute_control(tab, false, cx)))
            .child(self.render_inline_tab_name(tab.id, cx).unwrap_or_else(|| div().debug_selector(|| self.sidebar_debug_name(format!("sidebar-item-title-{}", tab.id.get()))).flex_1().min_w_0().overflow_hidden().child(crate::sidebar_title::SidebarTitle::new(self.browser.item_title(tab.id))).into_any_element())).into_any_element()
    }

    pub(crate) fn render_split(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(split) = self.browser.active_split() else { return div().into_any_element(); };
        let horizontal = split.axis == Axis::Horizontal;
        let minimum = crate::split_layout::minimum_extent(split.panes.len()) as f32;
        let extent = f32::from(if horizontal { self.sidebar_navigation.normal_content_size.width } else { self.sidebar_navigation.normal_content_size.height }).max(minimum);
        let weights = crate::split_layout::constrained_weights(extent as f64, &split.weights);
        let panes = div().w_full().flex_1().min_h_0().flex().gap(px(crate::split_layout::GAP as f32))
            .when(!horizontal, |this| this.flex_col())
            .children(split.panes.iter().zip(&weights).filter_map(|(id, weight)| {
                let tab = self.browser.all_tabs().find(|tab| tab.id == *id)?;
                let id = *id;
                let key = id.get() as usize;
                let active = self.browser.space().active_tab == Some(id);
                let header_group = gpui::SharedString::from(format!("pane-header-{key}"));
                let mut pane = div().id(("pane", key)).debug_selector(|| self.sidebar_debug_name((move || format!("split-pane-{key}"))()))
                    .min_w_0().min_h_0().flex_basis(px(0.)).flex().flex_col()
                    .relative()
                    .rounded(px(crate::pages::CONTENT_CORNER_RADIUS)).overflow_hidden().shadow_sm();
                pane.style().flex_grow = Some(*weight);
                Some(pane
                    .child(div().absolute().inset_0().rounded(px(crate::pages::CONTENT_CORNER_RADIUS))
                        .border(px(crate::split_layout::BORDER as f32))
                        .border_color(hsla(self.accent(), 0.75, 0.4, if active { 1. } else { 0. })))
                    .child(div().id(("pane-header", key)).debug_selector(|| self.sidebar_debug_name((move || format!("pane-header-{key}"))()))
                        .mx(px(crate::split_layout::BORDER as f32)).mt(px(crate::split_layout::BORDER as f32))
                        .rounded_t(px(crate::pages::CONTENT_CORNER_RADIUS))
                        .h(px((crate::split_layout::HEADER_HEIGHT - crate::split_layout::BORDER) as f32)).flex_none().flex().items_center().gap_1().px_2().text_xs().relative().group(header_group.clone())
                        .bg(hsla(0., 0., 1., if active { 0.65 } else { 0.35 }))
                        .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| { cx.stop_propagation(); this.open_pane_menu(id, event.position, window, cx); }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.page_content_visible() {
                                if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                            }
                            this.edit_browser(|browser| browser.activate_tab(id, SystemTime::now()));
                            if let Some(pages) = &this.pages { pages.borrow_mut().sync(&this.browser, this.page_content_visible()); }
                            cx.notify();
                        }))
                        .child(div().id(("pane-back", key)).px_1().rounded_sm().child(crate::icons::control(crate::icons::Icon::Back))
                            .on_click(cx.listener(move |this, _, _, cx| { cx.stop_propagation(); if let Some(pages) = &this.pages { pages.borrow_mut().go_back(id); } })))
                        .child(div().id(("pane-forward", key)).px_1().rounded_sm().child(crate::icons::control(crate::icons::Icon::Forward))
                            .on_click(cx.listener(move |this, _, _, cx| { cx.stop_propagation(); if let Some(pages) = &this.pages { pages.borrow_mut().go_forward(id); } })))
                        .child(div().id(("pane-reload", key)).px_1().rounded_sm()
                            .when(!active, |this| this.invisible().group_hover(header_group.clone(), |style| style.visible())).child(crate::icons::control(crate::icons::Icon::Reload))
                            .on_click(cx.listener(move |this, _, _, cx| { cx.stop_propagation(); if let Some(pages) = &this.pages { pages.borrow_mut().reload(id); } })))
                        .child(div().id(("pane-address", key)).debug_selector(|| self.sidebar_debug_name((move || format!("pane-address-{key}"))()))
                            .absolute().left(px(if active { 94. } else { 56. })).right(px(if active { 94. } else { 56. })).top_0().h_full()
                            .when(!active, |this| this.group_hover(header_group.clone(), |style| style.left(px(94.)).right(px(94.))))
                            .flex().items_center().justify_center().gap(px(6.)).text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD)
                            .when(tab.url != EMPTY_PANE_URL, |this| this.child(crate::icons::control_sized(crate::icons::Icon::Link,14.)))
                            .child(div().min_w_0().truncate().child(if tab.url == EMPTY_PANE_URL { "Search…".into() } else { crate::window::display_url(&tab.url) }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if this.page_content_visible() {
                                    if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                                }
                                this.edit_browser(|browser| browser.activate_tab(id, SystemTime::now()));
                                if let Some(pages) = &this.pages { pages.borrow_mut().sync(&this.browser, this.page_content_visible()); }
                                let url = this.browser.active_tab().unwrap().url.clone();
                                this.open_command_bar(Target::Tab(id), &url, window, cx);
                            })))
                        .child(div().id(("pane-menu", key)).debug_selector(|| self.sidebar_debug_name((move || format!("pane-menu-{key}"))())).ml_auto().px_1().rounded_sm()
                            .when(!active, |this| this.invisible().group_hover(header_group.clone(), |style| style.visible()))
                            .hover(|this| this.bg(hsla(0., 0., 0., 0.1)))
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| { cx.stop_propagation(); this.open_pane_menu(id, event.position(), window, cx); })).child(crate::icons::control(crate::icons::Icon::Split)))
                        .child(div().id(("pane-close", key)).debug_selector(|| self.sidebar_debug_name((move || format!("pane-close-{key}"))())).px_1().rounded_sm()
                            .hover(|this| this.bg(hsla(0., 0., 0., 0.1)))
                            .on_click(cx.listener(move |this, _, _, cx| { cx.stop_propagation(); if this.split_draft.as_ref().is_some_and(|draft| draft.pane == id) { this.close_command_bar(cx); } else { this.close_split_pane(id, cx); } })).child(crate::icons::control(crate::icons::Icon::Close))))
                    .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("pane-body-{key}"))())).mx(px(crate::split_layout::BORDER as f32)).mb(px(crate::split_layout::BORDER as f32)).rounded_b(px(crate::pages::CONTENT_CORNER_RADIUS)).flex_1().min_h_0().when(tab.url == EMPTY_PANE_URL, |this| this.bg(hsla(self.browser.space().hue / 360., 0.35, 0.86, 1.))).when(self.pages.is_none() || omnibox::is_internal_url(&tab.url), |this| this.flex().items_center().justify_center().child(div().max_w_full().truncate().child(if tab.url == EMPTY_PANE_URL { String::new() } else { tab.url.clone() })))))
            }));
        let dividers = (0..split.panes.len().saturating_sub(1)).map(|index| {
            let fraction: f32 = weights[..=index].iter().sum();
            let correction = crate::split_layout::GAP as f32 * (index as f32 - fraction * (split.panes.len() - 1) as f32);
            let drag = DividerDrag { split: split.id, axis: split.axis, panes: split.panes.clone(), weights: weights.clone(), index, offset: Default::default() };
            div().id(("split-divider", index)).debug_selector(move || format!("split-divider-{index}"))
                .absolute().flex().items_center().justify_center()
                .when(horizontal, |this| this.left(gpui::relative(fraction)).ml(px(correction)).top_0().bottom_0().w(px(crate::split_layout::GAP as f32)).cursor(gpui::CursorStyle::ResizeLeftRight))
                .when(!horizontal, |this| this.top(gpui::relative(fraction)).mt(px(correction)).left_0().right_0().h(px(crate::split_layout::GAP as f32)).cursor(gpui::CursorStyle::ResizeUpDown))
                .group("split-divider")
                .child(div().rounded_full().bg(hsla(0., 0., 1., 0.45)).invisible()
                    .group_hover("split-divider", |style| style.visible())
                    .when(horizontal, |this| this.w(px(2.)).h(px(24.)))
                    .when(!horizontal, |this| this.w(px(64.)).h(px(2.))))
                .on_drag(drag, move |drag, offset, _, cx| {
                    drag.offset.set(f32::from(if horizontal { offset.x } else { offset.y }));
                    cx.new(|_| drag.clone())
                })
        });
        // Arc keeps its window frame fixed: horizontal panes overflow right;
        // stacked panes remain bottom-aligned and overflow above the window.
        div().absolute().left_0().bottom_0().size_full().flex_none()
            .when(horizontal, |this| this.min_w(px(minimum)))
            .when(!horizontal, |this| this.min_h(px(minimum)))
            .flex().flex_col().child(panes).children(dividers)
            .on_drag_move::<DividerDrag>(cx.listener(|this, event: &gpui::DragMoveEvent<DividerDrag>, _, cx| {
                let drag = event.drag(cx).clone();
                let Some(split) = this.browser.active_split().filter(|split| split.id == drag.split && split.axis == drag.axis && split.panes == drag.panes) else { return; };
                let horizontal = drag.axis == Axis::Horizontal;
                let extent = f32::from(if horizontal { event.bounds.size.width } else { event.bounds.size.height });
                let pointer = f32::from(if horizontal { event.event.position.x - event.bounds.origin.x } else { event.event.position.y - event.bounds.origin.y });
                let available = extent - crate::split_layout::GAP as f32 * (drag.weights.len() - 1) as f32;
                if available <= 0. { return; }
                let before: f32 = drag.weights[..drag.index].iter().sum();
                let pair = available * (drag.weights[drag.index] + drag.weights[drag.index + 1]);
                let minimum = (crate::split_layout::MIN_PANE_EXTENT as f32).min(pair / 2.);
                let left = (pointer - drag.offset.get() - before * available - crate::split_layout::GAP as f32 * drag.index as f32).clamp(minimum, pair - minimum);
                let anchor = split.anchor;
                let mut weights = drag.weights.clone();
                weights[drag.index] = left / available;
                weights[drag.index + 1] = (pair - left) / available;
                this.edit_browser(|browser| browser.set_split_weights(anchor, &weights));
                cx.notify();
            })).into_any_element()
    }
}
