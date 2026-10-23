//! The dock carries stable Space identities through GPUI's native drag flow.
use browser_core::SpaceId;
use gpui::{div, hsla, prelude::*, px, Context, Window};

#[derive(Clone)]
pub(crate) struct SpaceDrag {
    pub id: SpaceId,
    pub workspace: crate::profile::ProfileHandle,
    pub name: String,
    pub icon: Option<String>,
    pub hue: f32,
}

impl Render for SpaceDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(32.))
            .max_w(px(180.))
            .px_2()
            .rounded_lg()
            .shadow_md()
            .bg(hsla(self.hue / 360., 0.5, 0.86, 0.96))
            .text_color(hsla(0., 0., 0.1, 1.))
            .text_sm()
            .child(crate::icons::glyph(self.icon.as_deref().unwrap_or("●")))
            .child(div().truncate().child(self.name.clone()))
    }
}
