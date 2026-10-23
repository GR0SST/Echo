//! Split View creation and pane chrome. Page surfaces remain owned by CEF.
use crate::window::{BrowserWindow, Command, Suggestion, Target};
use browser_core::{omnibox, splits::{Axis, Placement, MAX_SPLIT_PANES}, Tab, TabId};
use gpui::{div, hsla, prelude::*, px, AnyElement, Context};
use std::time::SystemTime;

impl BrowserWindow {
    pub(crate) fn duplicate_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        self.close_find(cx);
        if self.browser.duplicate_item(id, SystemTime::now()).is_none() {
            self.show_toast("You can have up to 12 Favorites".into(), cx);
        }
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, true); }
        cx.notify();
    }

    pub(crate) fn can_add_split(&self, anchor: TabId) -> bool {
        self.browser.all_tabs().any(|tab| tab.id == anchor && !omnibox::is_internal_url(&tab.url))
            && self.browser.split_for(anchor).is_none_or(|split| split.panes.len() < MAX_SPLIT_PANES)
    }

    pub(crate) fn split_suggestions(&self, anchor: TabId, text: &str) -> Vec<Suggestion> {
        if !self.can_add_split(anchor) { return Vec::new(); }
        let query = text.trim().to_lowercase();
        let mut rows: Vec<_> = omnibox::resolve(text).filter(|resolved| !omnibox::is_internal_url(resolved.url())).map(Suggestion::Load).into_iter().collect();
        let favorites = self.browser.favorites.iter().map(|tab| ("Favorites", tab));
        rows.extend(favorites.chain(self.browser.spaces.iter().flat_map(|space| space.pinned.iter().chain(&space.today).map(move |tab| (space.name.as_str(), tab))))
            .filter(|(_, tab)| tab.id != anchor && self.browser.split_for(tab.id).is_none() && !omnibox::is_internal_url(&tab.url))
            .filter(|(_, tab)| query.is_empty() || tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)
                || tab.page_title.as_ref().is_some_and(|title| title.to_lowercase().contains(&query)))
            .take(8).map(|(space, tab)| Suggestion::SplitTab { id: tab.id, title: tab.title.clone(), space: space.into() }));
        rows
    }

    pub(crate) fn edit_split(&mut self, placement: Option<Placement>, cx: &mut Context<Self>) {
        let Some(anchor) = self.browser.space().active_tab else { self.close_command_bar(cx); return; };
        if !self.can_add_split(anchor) {
            self.close_command_bar(cx);
            self.show_toast("Cannot Add New Pane".into(), cx);
            return;
        }
        let placement = placement.unwrap_or_else(|| if self.browser.active_split().is_some_and(|split| split.axis == Axis::Vertical) { Placement::Below } else { Placement::Right });
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Split { anchor, placement };
            bar.selected = 0;
            bar.input.update(cx, |input, cx| { input.set_text("", cx); input.set_placeholder("Add a page to Split View…", cx); });
        }
        cx.notify();
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
        if self.browser.close_split_pane(id, SystemTime::now()) {
            self.close_find(cx);
            if let Some(pages) = &self.pages { let mut pages = pages.borrow_mut(); pages.discard(id); pages.sync(&self.browser, true); }
            cx.notify();
        }
    }

    pub(crate) fn render_item_icon(&self, tab: &Tab, hue: f32) -> AnyElement {
        if let Some(split) = self.browser.split_for(tab.id) {
            div().flex().flex_none().gap(px(2.)).children(split.panes.iter().filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == *id)).take(2).map(|tab| self.render_favicon(tab, hue))).into_any_element()
        } else { self.render_favicon(tab, hue).into_any_element() }
    }

    pub(crate) fn render_split(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(split) = self.browser.active_split() else { return div().into_any_element(); };
        let horizontal = split.axis == Axis::Horizontal;
        let panes = div().w_full().flex_1().min_h_0().flex().gap(px(crate::split_layout::GAP as f32))
            .when(!horizontal, |this| this.flex_col())
            .children(split.panes.iter().zip(&split.weights).filter_map(|(id, weight)| {
                let tab = self.browser.all_tabs().find(|tab| tab.id == *id)?;
                let id = *id;
                let key = id.get() as usize;
                let active = self.browser.space().active_tab == Some(id);
                let mut pane = div().id(("pane", key)).min_w_0().min_h_0().flex_basis(px(0.)).flex().flex_col().overflow_hidden();
                pane.style().flex_grow = Some(*weight);
                Some(pane
                    .child(div().id(("pane-header", key)).debug_selector(move || format!("pane-header-{key}"))
                        .h(px(crate::split_layout::HEADER_HEIGHT as f32)).flex_none().flex().items_center().gap_1().px_2().text_xs()
                        .bg(hsla(0., 0., 0., if active { 0.10 } else { 0.04 }))
                        .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| { cx.stop_propagation(); this.open_pane_menu(id, event.position, window, cx); }))
                        .on_click(cx.listener(move |this, _, _, cx| { this.browser.activate_tab(id, SystemTime::now()); if let Some(pages) = &this.pages { pages.borrow_mut().sync(&this.browser, true); } cx.notify(); }))
                        .child(self.render_favicon(tab, self.browser.space().hue / 360.))
                        .child(div().flex_1().min_w_0().truncate().child(tab.title.clone()))
                        .child(div().id(("pane-menu", key)).debug_selector(move || format!("pane-menu-{key}")).px_1().rounded_sm()
                            .hover(|this| this.bg(hsla(0., 0., 0., 0.1)))
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| { cx.stop_propagation(); this.open_pane_menu(id, event.position(), window, cx); })).child("⋯"))
                        .child(div().id(("pane-separate", key)).debug_selector(move || format!("pane-separate-{key}")).px_1().rounded_sm()
                            .hover(|this| this.bg(hsla(0., 0., 0., 0.1)))
                            .on_click(cx.listener(move |this, _, _, cx| { cx.stop_propagation(); if this.browser.separate_split_pane(id) { this.browser.activate_tab(id, SystemTime::now()); } else { this.show_toast("You can have up to 12 Favorites".into(), cx); } cx.notify(); })).child("↗"))
                        .child(div().id(("pane-close", key)).debug_selector(move || format!("pane-close-{key}")).px_1().rounded_sm()
                            .hover(|this| this.bg(hsla(0., 0., 0., 0.1)))
                            .on_click(cx.listener(move |this, _, _, cx| { cx.stop_propagation(); this.close_split_pane(id, cx); })).child("×")))
                    .child(div().flex_1().min_h_0().when(self.pages.is_none() || omnibox::is_internal_url(&tab.url), |this| this.flex().items_center().justify_center().child(div().max_w_full().truncate().child(tab.url.clone())))))
            }));
        div().size_full().flex().flex_col()
            .child(div().h(px(crate::split_layout::TITLE_HEIGHT as f32)).flex_none().flex().items_center().justify_center().text_xs()
                .child(div().max_w_full().truncate().child(self.browser.item_title(split.anchor))))
            .child(panes).into_any_element()
    }
}
