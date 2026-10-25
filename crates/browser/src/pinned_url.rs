//! The observed Edit Pinned URL form reuses the scoped, native-page overlay host.
use super::*;

impl BrowserWindow {
    pub(crate) fn open_pinned_url_editor(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = self.browser.all_tabs().find(|page| page.id == tab).and_then(|page| page.pinned_url.clone()) else { return; };
        let target = Target::EditPinnedUrl { tab, profile: self.browser.active_user_profile_id(), space: self.browser.space().id };
        self.open_command_bar(target, &url, window, cx);
    }

    fn pinned_url_input(&self, bar: &CommandBar, cx: &App) -> Option<String> {
        let Target::EditPinnedUrl { tab, profile, space } = bar.target else { return None; };
        if self.browser.active_user_profile_id() != profile || self.browser.space().id != space { return None; }
        let page = self.browser.space().pinned.iter().chain(&self.browser.active_user_profile().favorites).find(|page| page.id == tab)?;
        if page.pinned_url != bar.pinned_url_source { return None; }
        match omnibox::resolve(bar.input.read(cx).text()) { Some(Resolved::Url(url)) => Some(url), _ => None }
    }

    pub(crate) fn save_pinned_url(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = self.command_bar.as_ref() else { return; };
        let Some(url) = self.pinned_url_input(bar, cx) else { self.close_command_bar(cx); return; };
        let Target::EditPinnedUrl { tab, profile, space } = bar.target else { return; };
        let original = bar.pinned_url_source.clone();
        let applied = self.edit_profile(|browser| {
            if !browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == profile
                && entry.pinned.iter().any(|page| page.id == tab && page.pinned_url == original))
                && !browser.user_profile(profile).is_some_and(|entry| entry.favorites.iter().any(|page| page.id == tab && page.pinned_url == original)) { return false; }
            browser.edit_pinned_url(tab, &url)
        });
        self.close_command_bar(cx);
        if applied {
            if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        }
        cx.notify();
    }

    pub(crate) fn render_pinned_url_editor(&self, bar: &CommandBar, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.theme_is_dark();
        let palette = crate::material::DialogPalette::new(dark);
        let ink = if dark { 0xffffff } else { 0x1c1e23 };
        let secondary = if dark { 0xbac0cf } else { 0x3d4466 };
        div().id("pinned-url-backdrop").debug_selector(|| "pinned-url-backdrop".into()).size_full()
            .rounded(px(WINDOW_CORNER_RADIUS)).bg(hsla(0., 0., 0., if dark { 0.55 } else { 0.18 }))
            .flex().justify_center().pt(px(((height - 265.) * 0.5).floor().max(16.)))
            .child(div().id("pinned-url-editor").debug_selector(|| "pinned-url-editor".into())
                .key_context(COMMAND_BAR_CONTEXT)
                .on_action(cx.listener(|this, _: &Confirm, _, cx| this.save_pinned_url(cx)))
                .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_command_bar(cx)))
                .relative().w(px(394.)).h(px(265.)).rounded(px(16.)).border_1()
                .border_color(hsla(0.,0.,if dark {1.} else {0.},0.25)).bg(palette.surface).shadow(vec![gpui::BoxShadow { color: hsla(0.,0.,0.,0.3), offset: point(px(0.),px(12.)), blur_radius: px(28.), spread_radius: px(-3.) }])
                .font_family(".SystemUIFont").text_color(gpui::rgb(ink))
                .child(div().absolute().left(px(38.)).top(px(53.)).text_size(px(18.)).line_height(px(22.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Edit Pinned URL"))
                .child(div().absolute().left(px(38.)).top(px(90.)).w(px(318.)).text_size(px(14.)).line_height(px(18.)).text_color(gpui::rgb(secondary)).child("This is what opens first when you click this tab or its favicon."))
                .child(div().id("pinned-url-input").debug_selector(|| "pinned-url-input".into())
                    .absolute().left(px(38.)).top(px(142.)).w(px(318.)).h(px(38.)).rounded(px(6.)).border_1()
                    .border_color(gpui::rgb(if dark {0x5b606e} else {0xe7e7e7}))
                    .bg(if dark { gpui::rgb(0x383e4f).into() } else { gpui::white() })
                    .px(px(8.)).flex().items_center().text_size(px(14.)).line_height(px(20.)).child(bar.input.clone()))
                .child(div().absolute().right(px(27.)).bottom(px(26.)).h(px(38.)).flex().gap(px(8.))
                    .child(div().id("pinned-url-cancel").debug_selector(|| "pinned-url-cancel".into())
                        .w(px(110.)).h_full().rounded(px(6.)).border_1().border_color(hsla(0.,0.,if dark {1.} else {0.},0.12))
                        .bg(if dark { gpui::rgb(0x302f63).into() } else { gpui::white() }).flex().items_center().justify_center().gap(px(8.)).text_size(px(13.)).cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.close_command_bar(cx))).child("Cancel")
                        .child(div().rounded(px(3.)).px(px(5.)).h(px(20.)).flex().items_center().bg(hsla(0.,0.,if dark {1.} else {0.},0.1)).text_color(gpui::rgb(if dark {0xb4b2ca} else {0x777777})).text_size(px(11.)).font_weight(gpui::FontWeight::BOLD).child("ESC")))
                    .child(div().id("pinned-url-save").debug_selector(|| "pinned-url-save".into())
                        .w(px(89.)).h_full().rounded(px(6.)).bg(gpui::rgb(0x3239fb)).text_color(gpui::white())
                        .flex().items_center().justify_center().gap(px(8.)).text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.save_pinned_url(cx))).child("Save")
                        .child(div().w(px(28.)).h(px(20.)).rounded(px(3.)).bg(hsla(0.,0.,1.,0.15)).border_1().border_color(hsla(0.,0.,1.,0.18)).flex().items_center().justify_center().child(crate::icons::control_sized(crate::icons::Icon::Enter,18.))))))
            .into_any_element()
    }
}
