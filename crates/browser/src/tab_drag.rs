//! Sidebar drags carry a logical item, including complete Split Views.
use crate::window::{BrowserWindow, Target};
use browser_core::{folders::FolderId, Section, SpaceId, Tab, TabId};
use gpui::{div, hsla, prelude::*, px, Context, Window};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone)]
pub(crate) struct TabDrag {
    pub id: TabId,
    title: String,
    icon: Option<String>,
    favicon: Option<Vec<u8>>,
    split: bool,
}

impl BrowserWindow {
    pub(crate) fn drop_tab_on_section(
        &mut self,
        tab: TabId,
        space: SpaceId,
        section: Section,
        cx: &mut Context<Self>,
    ) {
        let active = self.browser.space().active_tab;
        let pages = self.browser.related_tabs(tab);
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
        if let Some(active) = active.filter(|tab| pages.contains(tab)) {
            self.browser.activate_tab(active, SystemTime::now());
        } else {
            self.browser.activate_sidebar_item(tab, SystemTime::now());
        }
        match section {
            Section::Pinned => self.scroll_to_pinned_item(self.browser.pinned_item(tab), cx),
            Section::Today => self.scroll_to_today_start(cx),
        }
        cx.notify();
    }

    pub(crate) fn drop_tab_in_favorites(&mut self, tab: TabId, cx: &mut Context<Self>) {
        let active = self.browser.space().active_tab;
        let pages = self.browser.related_tabs(tab);
        if self.browser.favorite_tab(tab) {
            if let Some(active) = active.filter(|tab| pages.contains(tab)) {
                self.browser.activate_tab(active, SystemTime::now());
            } else {
                self.browser.activate_sidebar_item(tab, SystemTime::now());
            }
            cx.notify();
        }
    }

    pub(crate) fn tab_drag(&self, tab: &Tab) -> TabDrag {
        TabDrag {
            id: self.browser.item_anchor(tab.id),
            title: self.browser.item_title(tab.id),
            icon: self.browser.item_icon(tab.id).map(str::to_string),
            favicon: self.browser.favicon(&tab.url).map(|bytes| bytes.to_vec()),
            split: self.browser.split_for(tab.id).is_some(),
        }
    }

    pub(crate) fn drop_tab_in_folder(
        &mut self,
        tab: TabId,
        folder: FolderId,
        cx: &mut Context<Self>,
    ) {
        if let Some(space) = self.browser.folder_space(folder) {
            self.confirm_folder_destination(Target::MoveToFolder(tab), space, Some(folder), cx);
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
