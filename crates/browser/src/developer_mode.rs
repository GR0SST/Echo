//! The site-scoped developer toolbar; engine actions use the existing page owner.
use super::*;
impl BrowserWindow {
    pub(crate) fn page_toolbar_shown(&self) -> bool { self.toolbar_shown() || self.developer_mode_enabled() }
    pub(crate) fn page_toolbar_height(&self) -> f32 { if self.developer_mode_enabled() { 40. } else { TOP_TOOLBAR_HEIGHT } }
    pub(super) fn render_developer_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let url = self.browser.active_tab().map(|tab| tab.url.trim_end_matches('/').to_owned()).unwrap_or_default();
        let button = |id: &'static str, icon: &'static str| div().id(id).debug_selector(move ||id.into())
            .w(px(28.)).h(px(28.)).flex_none().rounded(px(5.)).flex().items_center().justify_center()
            .cursor_pointer().hover(|button|button.bg(hsla(0.,0.,1.,0.08))).child(if icon=="extension-puzzle-outline" { extension_icon() } else { crate::icons::ionicon(icon,16.,1.) });
        let divider = |margin| div().mx(px(margin)).w(px(1.)).h(px(18.)).bg(hsla(0.,0.,1.,0.1));
        div().debug_selector(||"developer-toolbar".into()).absolute().top_0().left_0().right_0().h(px(40.))
            .flex().items_center().px(px(4.)).bg(gpui::rgb(0x3e43cf)).text_color(gpui::rgb(0xe8e7ff))
            .child(gpui::canvas(|_,_,_|(), |bounds,_,window,_| {
                for x in (-40..f32::from(bounds.size.width) as i32+40).step_by(44) {
                    let mut path=gpui::PathBuilder::stroke(px(15.));
                    path.move_to(bounds.origin+point(px(x as f32),px(0.)));
                    path.line_to(bounds.origin+point(px(x as f32+40.),px(40.)));
                    if let Ok(path)=path.build(){window.paint_path(path,hsla(0.,0.,1.,0.025));}
                }
            }).absolute().inset_0())
            .child(button("developer-site-controls","information-outline").w(px(36.)).mr(px(2.))
                .on_click(cx.listener(|this,event: &gpui::ClickEvent,window,cx| {
                    this.site_controls_anchor.set(Some(Bounds::new(event.position()-point(px(14.),px(14.)),size(px(28.),px(28.)))));
                    this.toggle_site_controls(window,cx);
                })))
            .child(div().id("developer-address").debug_selector(||"developer-address".into()).flex_1().min_w_0().overflow_hidden()
                .font_family("Menlo").text_size(px(11.)).font_weight(gpui::FontWeight::SEMIBOLD).child(url)
                .on_click(cx.listener(|this,_,window,cx|this.on_edit_current_url(&EditCurrentUrl,window,cx))))
            .child(button("developer-copy","link-outline").on_click(cx.listener(|this,_,_,cx|this.copy_url(false,cx))))
            .child(divider(14.))
            // Portrait capture retains its separately tracked implementation gap.
            .child(button("developer-portrait","scan-outline"))
            .child(button("developer-capture","camera-outline").on_click(cx.listener(|this,_,_,cx|this.capture_page(cx))))
            .child(divider(7.))
            .child(button("developer-console","terminal-outline").on_click(cx.listener(|this,_,_,cx|this.run_devtools(crate::devtools::Command::Console,cx))))
            .child(button("developer-network","globe-outline").on_click(cx.listener(|this,_,_,cx|this.run_devtools(crate::devtools::Command::Network,cx))))
            .child(button("developer-inspect","locate-outline").on_click(cx.listener(|this,_,_,cx|this.run_devtools(crate::devtools::Command::Elements,cx))))
            .child(divider(7.))
            .child(button("developer-extensions","extension-puzzle-outline").on_click(cx.listener(|this,_,window,cx|this.toggle_site_controls(window,cx))))
            .child(button("developer-split","browsers-outline").on_click(cx.listener(|this,_,_,cx|this.edit_split(None,cx))))
            .into_any_element()
    }
}

fn extension_icon() -> AnyElement {
    canvas(|_,_,_|(),|bounds,_,window,_| {
        let at=|x,y|bounds.origin+point(px(x),px(y));
        let mut path=gpui::PathBuilder::stroke(px(1.2));
        path.move_to(at(3.,5.));path.line_to(at(6.,5.));path.line_to(at(6.,3.));
        path.curve_to(at(10.,3.),at(8.,0.));path.line_to(at(10.,5.));path.line_to(at(13.,5.));
        path.line_to(at(13.,8.));path.line_to(at(15.,8.));path.curve_to(at(15.,12.),at(18.,10.));
        path.line_to(at(13.,12.));path.line_to(at(13.,15.));path.line_to(at(10.,15.));
        path.line_to(at(10.,13.));path.curve_to(at(6.,13.),at(8.,10.));path.line_to(at(6.,15.));
        path.line_to(at(3.,15.));path.line_to(at(3.,12.));path.line_to(at(1.,12.));
        path.curve_to(at(1.,8.),at(-2.,10.));path.line_to(at(3.,8.));path.close();
        if let Ok(path)=path.build(){window.paint_path(path,window.text_style().color);}
    }).size(px(18.)).into_any_element()
}
