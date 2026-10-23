//! A focused Command Bar editor for tab/group and Space symbols.
use crate::window::{BrowserWindow, Target};
use browser_core::{SpaceId, TabId};
use gpui::{div, prelude::*, px, AnyElement, Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconTarget { Tab(TabId), Space(SpaceId) }

pub(crate) fn glyph(icon: &str) -> AnyElement {
    div().size(px(18.)).flex_none().flex().items_center().justify_center().text_size(px(16.)).line_height(px(18.)).child(icon.to_string()).into_any_element()
}

impl BrowserWindow {
    pub(crate) fn edit_icon(&mut self, target: IconTarget, cx: &mut Context<Self>) {
        let icon = match target {
            IconTarget::Tab(id) => self.browser.item_icon(id),
            IconTarget::Space(id) => self.browser.spaces.iter().find(|space| space.id == id).and_then(|space| space.custom_icon.as_deref()),
        }.unwrap_or_default().to_string();
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Icon(target);
            bar.selected = 0;
            bar.input.update(cx, |input, cx| { input.set_placeholder("Enter one symbol or emoji…", cx); input.set_text(icon, cx); });
        }
        cx.notify();
    }
}
