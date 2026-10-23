//! Incognito windows have independent tabs and no saved session.
use crate::window::{BrowserWindow, Target};
use browser_core::Section;
use gpui::{div, hsla, prelude::*, px, AnyElement, Context, Window};

impl BrowserWindow {
    pub(crate) fn open_private_command_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_command_bar(Target::NewTab, "", window, cx);
    }

    pub(crate) fn render_private_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let space = self.browser.space();
        div().id("incognito-tabs").debug_selector(|| "incognito-tabs".into())
            .flex().flex_col().size_full().px(px(7.)).gap(px(3.))
            .text_color(hsla(0., 0., 1., 0.85))
            .child(div().h(px(30.)).flex_none().flex().items_center().pl(px(10.)).gap(px(4.))
                .text_size(px(11.)).text_color(hsla(0., 0., 1., 0.45))
                .child(crate::icons::control_sized(crate::icons::Icon::Incognito, 14.))
                .child("Incognito"))
            .child(div().id("new-private-tab").debug_selector(|| "new-tab".into())
                .h(px(38.)).flex_none().rounded(px(10.)).flex().items_center().pl(px(9.)).gap(px(9.))
                .text_size(px(14.)).text_color(hsla(0., 0., 1., 0.65))
                .hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
                .on_click(cx.listener(|this, _, window, cx| this.open_private_command_bar(window, cx)))
                .child(crate::icons::control(crate::icons::Icon::Plus)).child("New Tab"))
            .child(div().id("private-today-list").debug_selector(|| "today-list".into())
                .flex().flex_col().flex_1().min_h_0().overflow_y_scroll().gap(px(3.))
                .children(space.today.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).map(|tab| {
                    let active = space.active_tab.is_some_and(|id| self.browser.item_anchor(id) == tab.id);
                    self.render_tab(tab, Section::Today, active, 0., cx)
                })))
            .into_any_element()
    }
}
