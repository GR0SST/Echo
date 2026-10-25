//! Page sharing captures a document identity before entering the OS presenter.
use crate::window::{BrowserWindow, PopupFocus};
use gpui::{AppContext, Context};
use std::time::Duration;

impl BrowserWindow {
    pub(crate) fn can_share_page(&self) -> bool {
        crate::native::sharing::page_picker_supported()
            && self.native_page_content_visible()
            && self.browser.active_tab().is_some_and(|tab| {
                !browser_core::omnibox::is_internal_url(&tab.url)
                    && self
                        .pages
                        .as_ref()
                        .is_some_and(|pages| pages.borrow().media_identity(tab.id).is_some())
            })
    }
    pub(crate) fn share_page(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if !self.can_share_page() {
            return;
        }
        let tab = self.browser.active_tab().unwrap();
        let url = tab.url.clone();
        let id = tab.id;
        let Some((identity, epoch)) = self.pages.as_ref().and_then(|pages| {
            let pages = pages.borrow();
            Some((pages.media_identity(id)?, pages.page_document_epoch(id)?))
        }) else {
            return;
        };
        self.page_share_generation = self.page_share_generation.wrapping_add(1);
        let generation = self.page_share_generation;
        self.close_command_bar_with_focus(PopupFocus::Restore, cx);
        self.close_find_with_focus(PopupFocus::Restore, cx);
        self.close_tab_menu_with_focus(PopupFocus::Restore, cx);
        self.close_site_controls_with_focus(PopupFocus::Restore, cx);
        self.close_new_menu_with_focus(PopupFocus::Restore, cx);
        self.finish_recent_items(false, PopupFocus::Restore, cx);
        self.close_floating_sidebar(cx);
        let main = self.main_window;
        cx.spawn(async move |owner, cx| {
            let deadline = cx.background_executor().now() + Duration::from_millis(350);
            let request = loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let state = main
                    .update(cx, |_, window, cx| {
                        owner
                            .update(cx, |view, cx| {
                                if crate::quit::is_quitting()
                                    || view.page_share_generation != generation
                                    || !view
                                        .browser
                                        .active_tab()
                                        .is_some_and(|tab| tab.id == id && tab.url == url)
                                    || !view.pages.as_ref().is_some_and(|pages| {
                                        let pages = pages.borrow();
                                        pages.media_identity(id) == Some(identity)
                                            && pages.page_document_epoch(id) == Some(epoch)
                                    })
                                {
                                    return None;
                                }
                                if view.popup_focus_pending > 0 || !window.is_window_active() {
                                    return (cx.background_executor().now() < deadline)
                                        .then_some(None);
                                }
                                let amount = view.sidebar_amount();
                                let left = view.sidebar_width() * amount
                                    + crate::pages::CONTENT_INSET * (1. - amount);
                                let width = f32::from(window.viewport_size().width)
                                    - left
                                    - crate::pages::CONTENT_INSET;
                                if let Some(pages) = &view.pages {
                                    pages.borrow_mut().request_ui_focus();
                                }
                                Some(Some((
                                    crate::native::view_of(window),
                                    [left, 0., width.max(1.), 1.],
                                )))
                            })
                            .ok()
                            .flatten()
                    })
                    .ok()
                    .flatten();
                match state {
                    Some(Some(request)) => break Some(request),
                    Some(None) => continue,
                    None => break None,
                }
            };
            let Some((view, anchor)) = request else {
                return;
            };
            let previous = owner
                .update(cx, |view, _| view.share_picker.take())
                .ok()
                .flatten();
            drop(previous);
            // AppKit presentation and destruction can enter its event loop.
            // Keep both outside GPUI application/entity/page borrows.
            let mut picker = crate::native::sharing::show_page(view, anchor, &url);
            let previous = owner
                .update(cx, |view, _| {
                    std::mem::replace(&mut view.share_picker, picker.take())
                })
                .ok()
                .flatten();
            drop(previous);
            drop(picker);
        })
        .detach();
    }
}
