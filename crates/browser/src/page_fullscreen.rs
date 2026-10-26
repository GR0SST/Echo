//! Document fullscreen owns only its requesting page, window transition and toast.
use crate::{pages::PageIdentity, window::BrowserWindow};
use gpui::{Context, Window};
use std::{rc::Rc, time::Duration};

pub(crate) const INSTRUCTION: &str = "To exit full screen, press ⎋.";
pub(crate) struct State {
    identity: PageIdentity,
    token: Rc<()>,
    restore_window: bool,
    observed_native: bool,
    toast: Option<u64>,
}
impl BrowserWindow {
    pub(crate) fn page_fullscreen_changed(
        &mut self,
        identity: PageIdentity,
        entered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !entered {
            if self
                .page_fullscreen
                .as_ref()
                .is_some_and(|state| state.identity == identity)
            {
                self.finish_page_fullscreen(false, window, cx);
            }
            return;
        }
        if self
            .page_fullscreen
            .as_ref()
            .is_some_and(|state| state.identity == identity)
        {
            return;
        }
        let allowed = self.native_page_content_visible()
            && !crate::quit::is_quitting()
            && self.browser.space().active_tab == Some(identity.tab)
            && self.pages.as_ref().is_some_and(|pages| {
                let pages = pages.borrow();
                pages.accepts_identity(identity)
                    && pages.is_visible(identity.tab)
                    && pages.state(identity.tab).crashed.is_none()
            });
        if !allowed {
            if let Some(pages) = &self.pages {
                pages.borrow_mut().exit_fullscreen(identity);
            }
            return;
        }
        self.finish_page_fullscreen(true, window, cx);
        self.close_popups_for_window_operation(false, cx);
        let restore_window = !window.is_fullscreen();
        let token = Rc::new(());
        self.page_fullscreen = Some(State {
            identity,
            token: token.clone(),
            restore_window,
            observed_native: !restore_window,
            toast: None,
        });
        if let Some(pages) = &self.pages {
            pages.borrow_mut().set_fullscreen_tab(Some(identity.tab));
        }
        if restore_window {
            window.toggle_fullscreen();
        }
        cx.notify();
        // AppKit's transition must establish the display-sized owner before
        // the existing toast component measures its top-right anchor.
        cx.spawn(async move |owner, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1000))
                .await;
            let _ = owner.update(cx, |view, cx| {
                if view.page_fullscreen.as_ref().is_some_and(|state| {
                    state.identity == identity && Rc::ptr_eq(&state.token, &token)
                }) {
                    view.show_toast(INSTRUCTION.into(), cx);
                    if let Some(state) = &mut view.page_fullscreen {
                        state.toast = Some(view.toast_generation);
                    }
                }
            });
        })
        .detach();
    }
    pub(crate) fn sync_page_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &mut self.page_fullscreen else {
            return;
        };
        state.observed_native |= window.is_fullscreen();
        let current = if let Some(pages) = &self.pages {
            let Ok(pages) = pages.try_borrow() else {
                cx.on_next_frame(window, |_, _, cx| cx.notify());
                return;
            };
            pages.accepts_identity(state.identity)
                && pages.is_visible(state.identity.tab)
                && pages.state(state.identity.tab).crashed.is_none()
        } else {
            false
        };
        let valid = self.browser.space().active_tab == Some(state.identity.tab) && current;
        let native_exit = state.observed_native && !window.is_fullscreen();
        if !valid
            || native_exit
            || !self.native_page_content_visible()
            || crate::quit::is_quitting()
        {
            self.finish_page_fullscreen(true, window, cx);
        }
    }
    fn finish_page_fullscreen(
        &mut self,
        request_exit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.page_fullscreen.take() else {
            return;
        };
        if let Some(pages) = &self.pages {
            let mut pages = pages.borrow_mut();
            if request_exit {
                pages.exit_fullscreen(state.identity);
            }
            pages.set_fullscreen_tab(None);
        }
        if state.toast == Some(self.toast_generation) {
            self.retire_toast(cx);
        }
        if state.restore_window && window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        cx.notify();
    }
}
