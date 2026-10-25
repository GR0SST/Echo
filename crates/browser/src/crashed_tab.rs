//! Recoverable renderer failure belongs to the retained page, never its profile.
use crate::window::BrowserWindow;
use browser_core::TabId;
use gpui::{div, prelude::*, px, rgb, AnyElement, Context};

impl BrowserWindow {
    pub(crate) fn render_crashed_tab(
        &self,
        tab: TabId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let code = self.pages.as_ref()?.borrow().state(tab).crashed?;
        let space = self.browser.space().id;
        let profile = self.browser.active_user_profile_id();
        Some(
            div()
                .debug_selector(move || format!("crashed-tab-{}", tab.get()))
                .absolute()
                .inset_0()
                .bg(gpui::white())
                .text_color(gpui::black().opacity(0.6))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .child("Tab crashed. Maybe ")
                        .child(
                            div()
                                .id(("reload-crashed-tab", tab.get() as usize))
                                .debug_selector(move || format!("reload-crashed-tab-{}", tab.get()))
                                .cursor_pointer()
                                .text_color(rgb(0x2f39ff))
                                .hover(|style| style.underline())
                                .child("reload")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.browser.space().id != space
                                        || this.browser.active_user_profile_id() != profile
                                        || this.browser.user_profile_id_for_tab(tab)
                                            != Some(profile)
                                    {
                                        return;
                                    }
                                    let Some(pages) = this.pages.clone() else {
                                        return;
                                    };
                                    if pages.borrow().state(tab).crashed != Some(code) {
                                        return;
                                    }
                                    this.edit_browser(|browser| {
                                        browser.activate_tab(tab, std::time::SystemTime::now())
                                    });
                                    pages.borrow_mut().reload(tab);
                                    cx.notify();
                                })),
                        )
                        .child(" the page?"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .child(format!("Error code: {code}")),
                )
                .into_any_element(),
        )
    }
}
