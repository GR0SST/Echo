//! Apply audio actions immediately, independently of the window's redraw rate.
use crate::window::BrowserWindow;
use browser_core::TabId;
use gpui::Context;

impl BrowserWindow {
    pub(crate) fn tab_audible(&self, tab: TabId) -> bool {
        self.pages.as_ref().is_some_and(|pages| pages.borrow().state(tab).audible)
    }

    pub(crate) fn show_tab_audio(&self, tab: &browser_core::Tab) -> bool {
        tab.muted || self.tab_audible(tab.id)
    }

    pub(crate) fn set_tab_muted(&mut self, tab: TabId, muted: bool, cx: &mut Context<Self>) {
        self.edit_browser(|browser| browser.set_tab_muted(tab, muted));
        if let Some(pages) = &self.pages { pages.borrow_mut().sync_audio(&self.browser); }
        cx.notify();
    }

    pub(crate) fn set_all_tabs_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        self.edit_browser(|browser| browser.set_all_tabs_muted(muted));
        if let Some(pages) = &self.pages { pages.borrow_mut().sync_audio(&self.browser); }
        cx.notify();
    }
}
