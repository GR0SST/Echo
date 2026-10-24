//! Local preview preferences, measured from Arc's 422 × 370 settings sheet.
use super::*;

pub(crate) const HELP_URL: &str = "https://resources.arc.net/hc/en-us/articles/19335284431639-Previews-Glance-Top-Sites";

impl SettingsWindow {
    #[cfg(test)]
    pub(crate) fn previews_open(&self) -> bool { self.previews }

    pub(super) fn previews_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.profile.read_settings(|b| b.settings.clone());
        div().absolute().inset_0().bg(gpui::rgba(0x00000046))
            .flex().items_center().justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().debug_selector(|| "settings-previews-sheet".into())
                .w(px(422.)).h(px(370.)).relative().flex_none()
                .rounded(px(17.)).border_1().border_color(rgb(0x050914))
                .bg(rgb(0x161c2f)).shadow_lg()
                .child(div().absolute().left(px(24.)).right(px(24.)).top(px(24.))
                    .text_size(px(13.)).line_height(px(16.)).font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(0xffffff)).child("Show Echo Previews:"))
                .child(div().absolute().left(px(24.)).right(px(24.)).top(px(50.))
                    .text_size(px(12.)).line_height(px(14.)).text_color(rgb(0xc4c5cd))
                    .child("Save a click! Hover your mouse over an Echo Folder, Pinned, or Favorited tab to quickly preview what you need from that site!")
                    .child(div().id("previews-learn-more").debug_selector(|| "settings-previews-learn-more".into())
                        .text_color(rgb(0x2499ff)).cursor_pointer().child("Learn more")
                        .on_click(|_, _, cx| cx.open_url(HELP_URL))))
                .child(div().absolute().left(px(24.)).right(px(24.)).top(px(106.))
                    .flex().flex_col().gap(px(8.))
                    .children([
                        ("Echo Folders", Toggle::PreviewFolders),
                        ("Google Calendar", Toggle::PreviewGoogleCalendar),
                        ("Google Mail", Toggle::PreviewGoogleMail),
                        ("Outlook Calendar", Toggle::PreviewOutlookCalendar),
                        ("Outlook Mail", Toggle::PreviewOutlookMail),
                        ("Recent pages on Linear, Notion, and Figma", Toggle::PreviewRecentPages),
                        ("Notion Calendar", Toggle::PreviewNotionCalendar),
                    ].into_iter().map(|(label, toggle)| {
                        div().id(SharedString::from(format!("preview-{toggle:?}")))
                            .debug_selector(move || format!("settings-preview-{toggle:?}"))
                            .h(px(20.)).flex_none().flex().items_center().gap(px(12.))
                            .text_color(rgb(0xe8e8ef)).cursor_pointer()
                            .child(div().size(px(16.)).flex_none().rounded(px(4.))
                                .bg(rgb(if settings.enabled(toggle) { 0x1684ff } else { 0x555968 }))
                                .when(settings.enabled(toggle), |s| s.child(icons::control_sized(Icon::Check, 16.))))
                            .child(label).on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle, cx)))
                    })))
                .child(div().absolute().right(px(24.)).bottom(px(24.))
                    .child(button("Done").id("previews-done").debug_selector(|| "settings-previews-done".into())
                        .w(px(92.)).h(px(36.)).gap(px(6.)).justify_center()
                        .bg(rgb(0x3339f1)).text_color(rgb(0xffffff)).cursor_pointer()
                        .child(div().w(px(28.)).h(px(22.)).rounded(px(4.))
                            .bg(gpui::rgba(0xffffff16)).border_1().border_color(gpui::rgba(0xffffff26))
                            .flex().items_center().justify_center().child(icons::control(Icon::Enter)))
                        .on_click(cx.listener(|this, _, _, cx| { this.previews = false; cx.notify(); })))))
            .into_any_element()
    }
}
