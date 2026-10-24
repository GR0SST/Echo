//! Arc applies the selected interval before asking whether to extend it to all
//! profiles. Escape means "Only this Profile", rather than cancelling the edit.
use super::*;
use gpui::{Animation, AnimationExt};

gpui::actions!(archive_scope, [AllProfiles, OnlyThisProfile]);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", AllProfiles, Some("ArchiveScope")),
        gpui::KeyBinding::new("escape", OnlyThisProfile, Some("ArchiveScope")),
    ]);
}

pub(super) struct ArchiveScope {
    profile: UserProfileId,
    name: String,
    interval: Duration,
}

impl SettingsWindow {
    #[cfg(test)]
    pub(crate) fn archive_scope_is_open(&self) -> bool { self.archive_scope.is_some() }
    pub(super) fn choose_archive(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(seconds) = [43200, 86400, 604800, 2592000].get(index).copied() else { return; };
        let interval = Duration::from_secs(seconds);
        let id = self.selected_profile;
        self.archive_generation = self.archive_generation.wrapping_add(1);
        self.archive_closing = None;
        self.archive_scope = self.profile.edit_settings(|browser| {
            let several = browser.user_profiles().count() > 1;
            let profile = browser.user_profile_mut(id)?;
            if profile.archive_after == Some(interval) { return None; }
            profile.archive_after = Some(interval);
            several.then(|| ArchiveScope { profile: id, name: profile.name().to_owned(), interval })
        });
        cx.notify();
    }

    pub(super) fn finish_archive_scope(&mut self, all: bool, cx: &mut Context<Self>) {
        let Some(scope) = self.archive_scope.take() else { return; };
        if all {
            self.profile.edit_settings(|browser| {
                // A retained prompt cannot extend a deleted profile's setting or
                // overwrite a more recent interval chosen in another window.
                if !browser.user_profile(scope.profile).is_some_and(|p| p.archive_after == Some(scope.interval)) { return; }
                let ids: Vec<_> = browser.user_profiles().map(|p| p.id()).collect();
                for id in ids {
                    if let Some(profile) = browser.user_profile_mut(id) {
                        profile.archive_after = Some(scope.interval);
                    }
                }
            });
        }
        self.archive_generation = self.archive_generation.wrapping_add(1);
        if !crate::native::prefers_reduced_motion() {
            self.archive_closing = Some(scope);
            let generation = self.archive_generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                let _ = this.update(cx, |this, cx| {
                    if this.archive_generation == generation {
                        this.archive_closing = None;
                        cx.notify();
                    }
                });
            }).detach();
        }
        cx.notify();
    }

    pub(super) fn archive_scope_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let closing = self.archive_scope.is_none();
        let Some(scope) = self.archive_scope.as_ref().or(self.archive_closing.as_ref()) else { return div().into_any_element(); };
        let interval = match scope.interval.as_secs() {
            43200 => "12 hours", 86400 => "24 hours", 604800 => "7 days", _ => "30 days",
        };
        let reduced = crate::native::prefers_reduced_motion();
        let button = |selector: &'static str, title: &'static str, width: f32, all: bool| {
            div().id(selector).debug_selector(move || selector.into())
                .w(px(width)).h(px(36.)).rounded(px(6.))
                .border_1().border_color(rgb(if all { 0x5955ff } else { 0x615a9d }))
                .bg(rgb(if all { 0x3239fb } else { 0x302f63 }))
                .hover(move |s| s.bg(rgb(if all { 0x2c33e2 } else { 0x454473 })))
                .flex().items_center().justify_center().gap(px(8.))
                .text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM).text_color(gpui::white())
                .child(title)
                .child(div().h(px(20.)).px(px(4.)).rounded(px(3.)).bg(gpui::hsla(0.,0.,1.,0.18))
                    .when(all, |s| s.w(px(28.)).border_1().border_color(gpui::hsla(0.,0.,1.,0.3)))
                    .flex().items_center().justify_center().text_size(px(10.)).font_weight(gpui::FontWeight::BOLD)
                    .child(if all { icons::control_sized(Icon::Enter, 18.) } else { div().child("ESC").into_any_element() }))
                .on_click(cx.listener(move |this, _, _, cx| this.finish_archive_scope(all, cx)))
        };
        div().id("archive-scope-backdrop").debug_selector(|| "archive-scope-backdrop".into())
            .absolute().inset_0().bg(gpui::hsla(0.,0.,0.,0.27))
            .flex().items_center().justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_,_,cx| cx.stop_propagation())
            .on_mouse_down(gpui::MouseButton::Right, |_,_,cx| cx.stop_propagation())
            .child(div().id("archive-scope-dialog").debug_selector(|| "archive-scope-dialog".into())
                .relative().w(px(450.)).h(px(228.)).rounded(px(18.)).bg(rgb(0x141c31))
                .child(div().absolute().left(px(38.)).right(px(28.)).top(px(54.))
                    .text_size(px(17.)).line_height(px(22.)).font_weight(gpui::FontWeight::BOLD).text_color(gpui::white())
                    .child(format!("Echo will now archive your tabs after {interval}")))
                .child(div().absolute().left(px(38.)).right(px(28.)).top(px(90.))
                    .text_size(px(14.)).line_height(px(18.)).text_color(rgb(0xb8b8c0))
                    .child(format!("Would you like to apply this change to all of your Profiles, or only your {} Profile?", scope.name)))
                .child(div().absolute().right(px(28.)).bottom(px(28.)).flex().gap(px(8.))
                    .child(button("archive-scope-only", "Only this Profile", 164., false))
                    .child(button("archive-scope-all", "All Profiles", 124., true)))
                .with_animation(if closing { "archive-scope-exit-drop" } else { "archive-scope-entry-drop" },
                    Animation::new(if reduced { Duration::ZERO } else { Duration::from_millis(if closing { 200 } else { 300 }) }),
                    move |card, t| card.top(px(-32. * if closing { t } else { 1. - gpui::ease_in_out(t) }))))
            .with_animation(if closing { "archive-scope-exit-fade" } else { "archive-scope-entry-fade" },
                Animation::new(if reduced { Duration::ZERO } else { Duration::from_millis(if closing { 200 } else { 133 }) }),
                move |panel, t| panel.opacity(if closing { 1. - t } else { t }))
            .into_any_element()
    }
}
