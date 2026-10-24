//! Browser actions from an owned native CEF context menu.
use crate::{pages::PageIdentity, window::BrowserWindow};
use engine::page_context::{Action, OpenTarget, Request};
use gpui::{ClipboardItem, Context, Window};
use std::time::SystemTime;

impl BrowserWindow {
    pub(crate) fn complete_context_image(
        &mut self,
        identity: PageIdentity,
        request: u64,
        page_url: &str,
        png: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        let Some((token, captured, url, started)) = &self.pending_context_image else {
            return;
        };
        if *token != request || *captured != identity {
            return;
        }
        let valid = self.page_content_visible()
            && url == page_url
            && started.elapsed() < std::time::Duration::from_secs(5)
            && self.browser.space().active_tab == Some(identity.tab)
            && self
                .browser
                .all_tabs()
                .any(|tab| tab.id == identity.tab && tab.url == *url);
        self.pending_context_image = None;
        if valid {
            cx.write_to_clipboard(ClipboardItem::new_image(&gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                png,
            )));
        }
    }
    pub(crate) fn confirm_page_context(
        &mut self,
        identity: PageIdentity,
        request: Request,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A context menu is input, never metadata that may follow a transfer.
        // Its document may have navigated while the native menu tracked input.
        if !self.page_content_visible()
            || self.browser.space().active_tab != Some(identity.tab)
            || !self
                .browser
                .all_tabs()
                .any(|tab| tab.id == identity.tab && tab.url == request.page_url)
            || self.browser.user_profile_id_for_tab(identity.tab) != Some(identity.profile)
        {
            return;
        }
        match request.action {
            Action::CopyImage(url) => {
                self.pending_quote = None;
                self.quote_request = self.quote_request.wrapping_add(1);
                let token = self.quote_request;
                self.pending_context_image = Some((
                    token,
                    identity,
                    request.page_url.clone(),
                    std::time::Instant::now(),
                ));
                if !self.pages.as_ref().is_some_and(|pages| {
                    pages.borrow_mut().copy_context_image(
                        identity.tab,
                        token,
                        &request.page_url,
                        &url,
                    )
                }) {
                    self.pending_context_image = None;
                }
            }
            Action::Copy(text) => {
                self.pending_quote = None;
                self.pending_context_image = None;
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            Action::Search(text) => {
                if text.is_empty() {
                    return;
                }
                let url = self
                    .browser
                    .active_user_profile()
                    .settings
                    .search
                    .engine
                    .search(&text)
                    .url()
                    .to_owned();
                self.edit_browser(|browser| {
                    let id = browser.new_tab(url, SystemTime::now());
                    browser.set_title(id, text);
                });
            }
            Action::Open {
                url,
                target: OpenTarget::BackgroundTab,
            } => {
                let id = self.edit_browser(|browser| {
                    let mut previous = browser.window_selection();
                    let id = browser.new_tab(&url, SystemTime::now());
                    browser.set_title(id, crate::window::display_url(&url));
                    browser.edit_in_window(&mut previous, |_| {});
                    id
                });
                self.show_background_tab(id, cx);
            }
            Action::Open {
                url,
                target: OpenTarget::Split,
            } => {
                if browser_core::omnibox::is_internal_url(&url) || !self.can_add_split(identity.tab)
                {
                    return;
                }
                self.close_command_bar(cx);
                self.begin_split(identity.tab, browser_core::splits::Placement::Right, cx);
                let Some(draft) = self.split_draft.take() else {
                    return;
                };
                self.edit_browser(|browser| {
                    browser.navigate(draft.pane, &url);
                    browser.set_title(draft.pane, crate::window::display_url(&url));
                });
            }
            Action::Open {
                url,
                target: OpenTarget::Little,
            } => {
                if !self.browser.is_private() {
                    self.open_little_url(Some(url), cx);
                }
            }
            Action::Open {
                url,
                target: OpenTarget::Window,
            } => self.new_window_for_link(url, cx),
            Action::Open {
                url,
                target: OpenTarget::Incognito,
            } => {
                self.retain_normal_source(cx);
                cx.defer(move |cx| crate::open_incognito_url(url, cx));
            }
            Action::Print => self.print_page(cx),
            Action::ViewSource => self.view_source(cx),
            // The existing Elements command opens the authenticated local
            // frontend and its picker. Revealing the original clicked node,
            // including iframe nodes across the dock resize, remains pending.
            Action::Inspect { .. } => self.run_devtools(crate::devtools::Command::Elements, cx),
        }
        if let Some(pages) = &self.pages {
            pages
                .borrow_mut()
                .sync(&self.browser, self.page_content_visible());
        }
        cx.notify();
    }
}
