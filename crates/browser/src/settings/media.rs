//! Per-profile Library folder access uses the existing Advanced settings rows.
use super::*;
use browser_core::media::{FolderGrant, MediaFolder};
impl SettingsWindow {
    pub(super) fn media_permissions(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.selected_profile;
        let access = self
            .profile
            .read_settings(|browser| {
                browser
                    .user_profile(profile)
                    .map(|p| p.settings.media.clone())
            })
            .unwrap_or_default();
        self.settings_group()
            .child(
                div()
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(6.))
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Library Media access"),
            )
            .children(
                MediaFolder::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, folder)| {
                        let granted = matches!(access.grant(folder), FolderGrant::Granted(_));
                        div()
                            .when(index > 0, |v| v.child(self.settings_rule()))
                            .child(
                                div()
                                    .id(("settings-media", index))
                                    .debug_selector(move || {
                                        format!("settings-media-{}", folder.label()).into()
                                    })
                                    .h(px(40.))
                                    .px(px(16.))
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .cursor_pointer()
                                    .child(icons::control_sized(Icon::Folder, 16.))
                                    .child(div().flex_1().child(folder.label()))
                                    .child(self.settings_switch(granted))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.toggle_media_folder(profile, folder, window, cx)
                                    })),
                            )
                    }),
            )
            .into_any_element()
    }
    fn toggle_media_folder(
        &mut self,
        profile: UserProfileId,
        folder: MediaFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let access = self.profile.media_access(profile).unwrap_or_default();
        if matches!(access.grant(folder), FolderGrant::Granted(_)) {
            self.profile.edit_settings(|browser| {
                if let Some(p) = browser.user_profile_mut(profile) {
                    p.settings.media.revoke(folder);
                }
            });
            cx.notify();
            return;
        }
        let Some(token) = self.profile.edit_settings(|browser| {
            browser
                .user_profile_mut(profile)?
                .settings
                .media
                .begin(folder, true)
        }) else {
            return;
        };
        let prepared = crate::media_access::prepare(crate::native::view_of(window), folder, cx);
        cx.spawn(async move |owner, cx| {
            let result = prepared.choose(cx).await;
            let _ = owner.update(cx, |this, cx| {
                this.profile.edit_settings(|browser| {
                    if let Some(p) = browser.user_profile_mut(profile) {
                        p.settings
                            .media
                            .complete(folder, token, result.unwrap_or(None));
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }
}
