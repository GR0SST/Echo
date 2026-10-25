//! Media route consent and native-page ownership. File access is never triggered
//! by construction, startup, a thumbnail timer or an unknown grant.
use crate::{library::LibrarySection, sidebar_pages::SidebarPage, window::BrowserWindow};
use browser_core::media::MediaFolder;
use gpui::Context;
impl BrowserWindow {
    pub(crate) fn media_route_active(&self) -> bool {
        !self.browser.is_private()
            && !self.browser.is_blank()
            && self.sidebar_navigation.controller.selected() == SidebarPage::Library
            && self.sidebar_navigation.library_section == LibrarySection::Media
    }
    pub(crate) fn begin_media_access(
        &mut self,
        explicit: Option<MediaFolder>,
        cx: &mut Context<Self>,
    ) {
        if !self.media_route_active() || self.sidebar_navigation.media_prompt_active {
            return;
        }
        self.sidebar_navigation.media_prompt_active = true;
        let profile = self.browser.active_user_profile_id();
        let main = self.main_window;
        cx.spawn(async move |owner, cx| {
            for folder in explicit
                .map(|f| vec![f])
                .unwrap_or_else(|| MediaFolder::ALL.to_vec())
            {
                let token = owner
                    .update(cx, |this, _| {
                        if !this.media_route_active()
                            || this.browser.active_user_profile_id() != profile
                        {
                            return None;
                        }
                        this.profile.handle.edit_settings(|browser| {
                            browser
                                .user_profile_mut(profile)?
                                .settings
                                .media
                                .begin(folder, explicit.is_some())
                        })
                    })
                    .ok()
                    .flatten();
                let Some(token) = token else { continue };
                let Ok(prepared) = main.update(cx, |_, window, cx| {
                    crate::media_access::prepare(crate::native::view_of(window), folder, cx)
                }) else {
                    break;
                };
                let result = prepared.choose(cx).await;
                let _ = owner.update(cx, |this, cx| {
                    if !this.media_route_active()
                        || this.browser.active_user_profile_id() != profile
                    {
                        return;
                    }
                    this.profile.handle.edit_settings(|browser| {
                        if let Some(p) = browser.user_profile_mut(profile) {
                            p.settings
                                .media
                                .complete(folder, token, result.unwrap_or(None));
                        }
                    });
                    this.refresh_profile_projection();
                    cx.notify();
                });
            }
            let _ = owner.update(cx, |this, cx| {
                this.sidebar_navigation.media_prompt_active = false;
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn undo_media_trash(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(media) = self.sidebar_navigation.media.clone() else {
            return false;
        };
        let profile = self.browser.active_user_profile_id();
        media.update(cx, |media, cx| media.undo_trash(profile, cx))
    }
    pub(crate) fn sync_media_selection(&mut self, selected: bool, cx: &mut Context<Self>) {
        self.sidebar_navigation.media_selected = selected;
        self.sidebar_navigation.preview_ui_focus = None;
        self.sidebar_navigation.preview_native_focus = None;
        self.sync_navigation_visibility();
        cx.notify();
    }
}
