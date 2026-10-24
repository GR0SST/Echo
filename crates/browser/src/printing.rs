//! Native Print is a detached effect: the OS dialog may enter nested event loops.
use super::{BrowserWindow, PopupFocus};
use gpui::Context;
use std::time::Duration;

impl BrowserWindow {
    pub(crate) fn print_available(&self) -> bool {
        self.native_page_content_visible()
            && self.browser.active_tab().is_some_and(|tab| {
                !browser_core::omnibox::is_internal_url(&tab.url)
                    && self
                        .pages
                        .as_ref()
                        .is_some_and(|pages| pages.borrow().can_print(tab.id))
            })
    }
    pub(crate) fn print_page(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if crate::quit::is_quitting() || !self.print_available() {
            return;
        }
        let tab = self.browser.active_tab().unwrap();
        let (tab, profile, url) = (
            tab.id,
            self.browser.active_user_profile_id(),
            tab.url.clone(),
        );
        let Some((owner, mut effect)) = self.pages.as_ref().and_then(|pages| {
            let mut pages = pages.borrow_mut();
            Some((
                pages.capture_ui_page_focus_owner(tab)?,
                pages.prepare_print(tab)?,
            ))
        }) else {
            return;
        };
        self.print_generation = self
            .print_generation
            .checked_add(1)
            .expect("Print generation exhausted");
        let generation = self.print_generation;
        // AppKit delivers the parent key-window notification asynchronously
        // after an owned popup is removed. Allow that handoff a bounded grace
        // period, without activating an unrelated or inactive host ourselves.
        let activation_deadline = (self.command_bar.is_some()
            || self.find_bar.is_some()
            || self.user_profile_dialog.is_some()
            || self.tab_menu.is_some()
            || self.site_controls.is_some()
            || self.new_menu.is_some()
            || self.floating_sidebar.is_some())
        .then(|| cx.background_executor().now() + Duration::from_millis(250));
        self.close_user_profile_dialog_with_focus(PopupFocus::Restore, cx);
        self.close_site_controls_with_focus(PopupFocus::Restore, cx);
        self.close_new_menu_with_focus(PopupFocus::Restore, cx);
        self.close_command_bar_with_focus(PopupFocus::Restore, cx);
        self.close_find_with_focus(PopupFocus::Restore, cx);
        self.close_tab_menu_with_focus(PopupFocus::Restore, cx);
        self.close_floating_sidebar(cx);
        let main = self.main_window;
        cx.spawn(async move |this, cx| {
            let mut activation_observed = false;
            loop {
                // Also lets queued overlay removal/reactivation finish before
                // a native dialog attaches to its original main window.
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let wait_for_activation = !activation_observed
                    && activation_deadline
                        .is_some_and(|deadline| cx.background_executor().now() < deadline);
                let ready = cx
                    .update(|cx| {
                        if crate::quit::is_quitting()
                            || !cx
                                .windows()
                                .iter()
                                .any(|window| window.window_id() == main.window_id())
                        {
                            return None;
                        }
                        main.update(cx, |_, window, cx| {
                            this.update(cx, |this, _| {
                                this.refresh_profile_projection();
                                if this.print_generation != generation
                                    || !this.native_page_content_visible()
                                    || this.browser.active_user_profile_id() != profile
                                    || !this.browser.active_tab().is_some_and(|current| {
                                        current.id == tab && current.url == url
                                    })
                                    || !this.pages.as_ref().is_some_and(|pages| {
                                        let pages = pages.borrow();
                                        pages.can_print(tab)
                                            && pages.accepts_ui_page_focus_owner(&owner)
                                    })
                                {
                                    return None;
                                }
                                // A newer popup intent supersedes this queued
                                // request; never open a native dialog over it.
                                if this.command_bar.is_some()
                                    || this.find_bar.is_some()
                                    || this.user_profile_dialog.is_some()
                                    || this.tab_menu.is_some()
                                    || this.site_controls.is_some()
                                    || this.new_menu.is_some()
                                {
                                    return None;
                                }
                                if this.popup_focus_pending > 0 {
                                    return Some(false);
                                }
                                if window.is_window_active() {
                                    Some(true)
                                } else if wait_for_activation {
                                    Some(false)
                                } else {
                                    None
                                }
                            })
                            .ok()
                            .flatten()
                        })
                        .ok()
                        .flatten()
                    })
                    .ok()
                    .flatten();
                match ready {
                    None => break,
                    Some(false) => continue,
                    Some(true) => activation_observed = true,
                }
                // Every App/window/Pages/model/engine borrow ended above.
                // A busy CEF pump is retried with the same retained request;
                // changed selection, closed host or quit cancels it.
                match effect() {
                    engine::PrintDispatch::Busy => continue,
                    engine::PrintDispatch::Submitted | engine::PrintDispatch::Unavailable => break,
                }
            }
        })
        .detach();
    }
}
