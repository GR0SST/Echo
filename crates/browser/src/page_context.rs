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
            || self.pages.as_ref().is_none_or(|pages| pages.borrow().page_document_epoch(identity.tab) != Some(request.document_epoch))
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
            Action::Media { target, operation } => {
                if let Some(pages) = &self.pages { pages.borrow_mut().apply_context_media(identity, request.document_epoch, &target, operation); }
            }
            Action::SaveAs(url) => self.request_save_download(identity, request.document_epoch, url, cx),
            Action::Speak(text) => self.queue_context_speech(identity, request.document_epoch, request.page_url, Some(text), cx),
            Action::StopSpeaking => self.queue_context_speech(identity, request.document_epoch, request.page_url, None, cx),
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
                self.preload_background_tab(id);
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
            Action::SavePage => self.save_page(cx),
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

    fn queue_context_speech(&self, identity: PageIdentity, epoch: u64, url: String, text: Option<String>, cx: &mut Context<Self>) {
        let main = self.main_window;
        cx.spawn(async move |owner, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(8)).await;
            let accepted = cx.update(|cx| main.update(cx, |_, window, cx| owner.read_with(cx, |view, _| {
                window.is_window_active() && view.page_content_visible()
                    && view.browser.active_tab().is_some_and(|tab| tab.id == identity.tab && tab.url == url)
                    && view.browser.user_profile_id_for_tab(identity.tab) == Some(identity.profile)
                    && view.pages.as_ref().is_some_and(|pages| {
                        let pages = pages.borrow();
                        pages.media_identity(identity.tab) == Some(identity)
                            && pages.page_document_epoch(identity.tab) == Some(epoch)
                    })
            }).unwrap_or(false)).unwrap_or(false)).unwrap_or(false);
            if !accepted { return; }
            // No application, window, page-store or engine borrow crosses the
            // platform speech operation, which may re-enter AppKit.
            #[cfg(not(test))]
            match text { Some(text) => { engine::speech::speak(&text); }, None => engine::speech::stop() }
            #[cfg(test)]
            let _ = cx.update(|cx| {
                if !cx.has_global::<SpeechRequests>() { cx.set_global(SpeechRequests::default()); }
                cx.global_mut::<SpeechRequests>().0.push(text);
            });
        }).detach();
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct SpeechRequests(pub Vec<Option<String>>);
#[cfg(test)]
impl gpui::Global for SpeechRequests {}
