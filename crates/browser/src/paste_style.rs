//! Route plain-text paste to the current native responder, never a hidden page.
use super::BrowserWindow;
use gpui::Context;
use std::time::Duration;

impl BrowserWindow {
    pub(crate) fn paste_and_match_style(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if crate::quit::is_quitting() || !self.native_page_content_visible() {
            return;
        }
        let Some(tab) = self.browser.active_tab() else {
            return;
        };
        let (tab, profile, url) = (
            tab.id,
            self.browser.active_user_profile_id(),
            tab.url.clone(),
        );
        let Some((snapshot, mut effect)) = self
            .pages
            .as_ref()
            .and_then(|pages| pages.borrow_mut().prepare_native_paste())
        else {
            return;
        };
        let main = self.main_window;
        cx.spawn(async move |this, cx| {
            loop {
                let ready = main
                    .update(cx, |_, window, cx| {
                        this.update(cx, |this, _| {
                            this.refresh_profile_projection();
                            !crate::quit::is_quitting()
                                && window.is_window_active()
                                && this.native_page_content_visible()
                                && this.browser.active_user_profile_id() == profile
                                && this
                                    .browser
                                    .active_tab()
                                    .is_some_and(|current| current.id == tab && current.url == url)
                                && this.pages.as_ref().is_some_and(|pages| {
                                    pages.borrow().accepts_native_paste_focus(&snapshot)
                                })
                        })
                        .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !ready {
                    break;
                }
                // Native frame operations can reenter. All application, model,
                // Pages and engine borrows have ended before invoking CEF.
                match effect() {
                    engine::PasteDispatch::Busy => {
                        cx.background_executor()
                            .timer(Duration::from_millis(8))
                            .await
                    }
                    engine::PasteDispatch::Submitted | engine::PasteDispatch::Unavailable => break,
                }
            }
        })
        .detach();
    }
}
