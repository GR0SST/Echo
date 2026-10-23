//! Sidebar drags carry a logical item, including complete Split Views.
use crate::window::{BrowserWindow, Target};
use browser_core::{folders::FolderId, Section, SpaceId, Tab, TabId};
use gpui::{div, hsla, prelude::*, px, Context, Window};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone)]
pub(crate) struct TabDrag {
    pub id: TabId,
    focus: TabId,
    title: String,
    icon: Option<String>,
    favicon: Option<Vec<u8>>,
    split: bool,
}

impl BrowserWindow {
    pub(crate) fn drop_tab_on_space(
        &mut self,
        drag: &TabDrag,
        space: SpaceId,
        cx: &mut Context<Self>,
    ) {
        let tab = drag.id;
        self.cancel_space_drag_hover();
        let section = if self.browser.is_favorite(tab)
            || self
                .browser
                .spaces
                .iter()
                .any(|space| space.pinned.iter().any(|entry| entry.id == tab))
        {
            Section::Pinned
        } else {
            Section::Today
        };
        self.drop_tab_on_section(drag, space, section, cx);
    }

    pub(crate) fn drop_tab_relative(
        &mut self,
        drag: &TabDrag,
        target: TabId,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        let tab = drag.id;
        if !self.browser.move_tab_relative(tab, target, after) {
            return;
        }
        if let Some(index) = self.browser.spaces.iter().position(|space| {
            space
                .pinned
                .iter()
                .chain(&space.today)
                .any(|entry| entry.id == tab)
        }) {
            self.switch_space(index, cx);
        }
        self.activate_dragged_tab(drag);
        if self
            .browser
            .space()
            .pinned
            .iter()
            .any(|entry| entry.id == tab)
        {
            self.scroll_to_pinned_item(self.browser.pinned_item(tab), cx);
        } else {
            self.scroll_to_today_item(tab, cx);
        }
        cx.notify();
    }

    pub(crate) fn drop_tab_at_favorite(
        &mut self,
        drag: &TabDrag,
        target: TabId,
        cx: &mut Context<Self>,
    ) {
        let tab = drag.id;
        let Some(destination) = self
            .browser
            .favorites
            .iter()
            .filter(|tab| !self.browser.is_split_secondary(tab.id))
            .position(|tab| tab.id == target)
        else {
            return;
        };
        if !self.browser.is_favorite(tab) {
            self.drop_tab_in_favorites(drag, cx);
        }
        if self.browser.is_favorite(tab) && self.browser.move_favorite(tab, destination) {
            cx.notify();
        }
    }

    pub(crate) fn drop_tab_on_section(
        &mut self,
        drag: &TabDrag,
        space: SpaceId,
        section: Section,
        cx: &mut Context<Self>,
    ) {
        let tab = drag.id;
        let changed = match section {
            Section::Pinned => self.browser.move_item_to_pinned_root(tab, space),
            Section::Today => self.browser.move_tab(tab, space, section),
        };
        if !changed {
            return;
        }
        if let Some(index) = self
            .browser
            .spaces
            .iter()
            .position(|entry| entry.id == space)
        {
            self.switch_space(index, cx);
        }
        self.activate_dragged_tab(drag);
        match section {
            Section::Pinned => self.scroll_to_pinned_item(self.browser.pinned_item(tab), cx),
            Section::Today => self.scroll_to_today_start(cx),
        }
        cx.notify();
    }

    pub(crate) fn drop_tab_in_favorites(&mut self, drag: &TabDrag, cx: &mut Context<Self>) {
        if self.browser.favorite_tab(drag.id) {
            self.activate_dragged_tab(drag);
            cx.notify();
        }
    }

    fn activate_dragged_tab(&mut self, drag: &TabDrag) {
        if self.browser.related_tabs(drag.id).contains(&drag.focus) {
            self.browser.activate_tab(drag.focus, SystemTime::now());
        } else {
            self.browser
                .activate_sidebar_item(drag.id, SystemTime::now());
        }
    }

    pub(crate) fn tab_drag(&self, tab: &Tab) -> TabDrag {
        let pages = self.browser.related_tabs(tab.id);
        let focus = self
            .browser
            .space()
            .active_tab
            .filter(|id| pages.contains(id))
            .unwrap_or_else(|| {
                self.browser
                    .split_for(tab.id)
                    .map_or(tab.id, |split| split.active)
            });
        TabDrag {
            id: self.browser.item_anchor(tab.id),
            focus,
            title: self.browser.item_title(tab.id),
            icon: self.browser.item_icon(tab.id).map(str::to_string),
            favicon: self.browser.favicon(&tab.url).map(|bytes| bytes.to_vec()),
            split: self.browser.split_for(tab.id).is_some(),
        }
    }

    pub(crate) fn drop_tab_in_folder(
        &mut self,
        drag: &TabDrag,
        folder: FolderId,
        cx: &mut Context<Self>,
    ) {
        if let Some(space) = self.browser.folder_space(folder) {
            if self.confirm_folder_destination(
                Target::MoveToFolder(drag.id),
                space,
                Some(folder),
                cx,
            ) {
                self.activate_dragged_tab(drag);
                cx.notify();
            }
        }
    }
}

impl Render for TabDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let icon = if let Some(icon) = &self.icon {
            crate::icons::glyph(icon)
        } else if self.split {
            crate::icons::glyph("▥")
        } else if let Some(bytes) = &self.favicon {
            gpui::img(Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                bytes.clone(),
            )))
            .size(px(18.))
            .flex_none()
            .into_any_element()
        } else {
            crate::icons::glyph("↗")
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(34.))
            .max_w(px(240.))
            .px_2()
            .rounded_lg()
            .shadow_md()
            .bg(hsla(0., 0., 0.98, 0.96))
            .text_color(hsla(0., 0., 0.1, 1.))
            .text_sm()
            .child(icon)
            .child(div().min_w_0().truncate().child(self.title.clone()))
    }
}
