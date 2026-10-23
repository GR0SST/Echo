//! A focused Command Bar editor for tab/group and Space symbols.
use crate::window::{BrowserWindow, Target};
use browser_core::{SpaceId, TabId};
use gpui::{div, prelude::*, px, AnyElement, Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconTarget { Tab(TabId), Space(SpaceId), Folder(browser_core::folders::FolderId) }

pub(crate) fn glyph(icon: &str) -> AnyElement {
    div().size(px(18.)).flex_none().flex().items_center().justify_center().text_size(px(16.)).line_height(px(18.)).child(icon.to_string()).into_any_element()
}

/// Browser controls use fixed vector geometry, never font-dependent glyphs.
pub(crate) fn disclosure(collapsed: bool) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let color = window.text_style().color;
        let points = if collapsed { [(6.75, 4.5), (11.25, 9.), (6.75, 13.5)] }
            else { [(4.5, 6.75), (9., 11.25), (13.5, 6.75)] };
        let points = points.map(|(x, y)| bounds.origin + gpui::point(px(x), px(y)));
        for segment in points.windows(2) {
            let mut path = gpui::PathBuilder::stroke(px(1.8));
            path.move_to(segment[0]);
            path.line_to(segment[1]);
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
        // Round each segment's caps and the shared join at the same stroke radius.
        for point in points {
            let mut dot = gpui::fill(gpui::Bounds::new(point - gpui::point(px(0.9), px(0.9)), gpui::size(px(1.8), px(1.8))), color);
            dot.corner_radii = px(0.9).into();
            window.paint_quad(dot);
        }
    }).size(px(18.)).flex_none().into_any_element()
}

pub(crate) fn more() -> AnyElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        for x in [5., 9., 13.] {
            let mut dot = gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(px(x - 1.2), px(7.8)), gpui::size(px(2.4), px(2.4))), window.text_style().color);
            dot.corner_radii = px(1.2).into();
            window.paint_quad(dot);
        }
    }).size(px(18.)).flex_none().into_any_element()
}

impl BrowserWindow {
    pub(crate) fn edit_icon(&mut self, target: IconTarget, cx: &mut Context<Self>) {
        let icon = match target {
            IconTarget::Folder(id) => self.browser.folder(id).and_then(|folder| folder.custom_icon.as_deref()),
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
