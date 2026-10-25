//! The Command Bar's observed search-engine disclosure, with Echo's own policy.
use super::*;
impl BrowserWindow {
    pub(crate) fn render_suggestions_notice(&self, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.theme_is_dark();
        let ink = if dark { 1. } else { 0. };
        let button = |id: &'static str, width: f32| div().id(id).debug_selector(move || id.into())
            .w(px(width)).h(px(38.)).flex_none().rounded(px(6.)).border_1().border_color(hsla(0.,0.,ink,0.12))
            .bg(if dark { gpui::rgb(0x302f63).into() } else { gpui::white() }).text_color(hsla(0.,0.,ink,0.92))
            .shadow(vec![gpui::BoxShadow { color: hsla(0.,0.,0.,0.15), offset: point(px(0.),px(1.)), blur_radius: px(0.), spread_radius: px(0.) }])
            .flex().items_center().justify_center().text_size(px(13.)).cursor_pointer();
        div().debug_selector(|| "suggestions-privacy".into())
            .h(px(60.)).flex_none().flex().items_center().gap(px(15.)).pl(px(15.)).pr(px(9.))
            .bg(if dark { gpui::black().into() } else { gpui::white() }).rounded_b(px(COMMAND_BAR_RADIUS))
            .text_color(hsla(0.,0.,ink,0.92))
            .child(div().size(px(14.)).flex_none().text_color(hsla(0.,0.,ink,0.5)).child(crate::icons::control_sized(crate::icons::Icon::Info,14.)))
            .child(div().flex_1().min_w_0().text_size(px(13.)).line_height(px(18.))
                .child(div().font_weight(gpui::FontWeight::BOLD).child("We talk to your search engine to help us suggest results."))
                .child(div().flex().text_color(hsla(0.,0.,ink,0.5)).child("Learn more about your data on Echo ")
                    .child(div().id("suggestions-privacy-link").debug_selector(|| "suggestions-privacy-link".into()).underline().cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| { this.close_command_bar_with_focus(PopupFocus::OnActivation,cx); cx.dispatch_action(&crate::menus::PrivacyPolicy); })).child("here."))))
            .child(div().flex().gap(px(8.)).flex_none()
                .child(button("suggestions-search-settings",132.).on_click(cx.listener(|this, _, _, cx| this.search_settings_from_command(cx))).child("Search Settings…"))
                .child(button("dismiss-suggestions-privacy",72.).on_click(cx.listener(|this, _, _, cx| {
                    this.edit_browser(|browser| browser.active_user_profile_mut().settings.search.notice_dismissed = true);
                    if let Some(bar) = &mut this.command_bar { bar.suggestions_privacy = false; }
                    cx.notify();
                })).child("Dismiss")))
            .into_any_element()
    }
}
