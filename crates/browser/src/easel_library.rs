//! Library's Easels: the profile's Easels as two columns of cards, newest
//! first, beside the retained page preview, as in Arc.
use crate::{library::LibraryColors, window::BrowserWindow};
use gpui::{div, hsla, prelude::*, px, AnyElement, Context};

/// The list column, matching Boosts.
pub(crate) const COLUMN: f32 = 304.;

impl BrowserWindow {
    pub(crate) fn render_easel_library(&self, colors: LibraryColors, cx: &mut Context<Self>) -> AnyElement {
        let open = self.open_easel_id();
        let easels: Vec<_> = self.browser.active_user_profile().easels.iter().map(|easel| (easel.id, easel.title.clone())).collect();
        let cards = easels.into_iter().enumerate().map(|(index, (id, title))| {
            let selected = open == Some(id);
            div().id(("easel-library-card", index)).debug_selector(move || format!("easel-library-card-{index}"))
                .w(px(140.)).h(px(134.)).p(px(7.)).rounded(px(16.)).bg(hsla(0., 0., 1., 0.04))
                .when(selected, |cell| cell.bg(colors.selected))
                .on_click(cx.listener(move |this, _, _, cx| this.open_easel(id, cx)))
                .child(div().size_full().rounded(px(12.)).bg(hsla(0., 0., 1., 0.06)).border_1().border_color(hsla(0., 0., 1., 0.08))
                    .hover(move |card| card.bg(colors.hover))
                    .p(px(10.)).flex().flex_col().justify_end().gap(px(4.))
                    .child(crate::easel::card_mark())
                    .child(div().max_h(px(36.)).overflow_hidden().whitespace_normal().text_size(px(14.)).line_height(px(17.))
                        .font_weight(gpui::FontWeight::BOLD).text_color(colors.text).child(title)))
        }).collect::<Vec<_>>();
        let empty = cards.is_empty();
        let mut list = div().id("easel-library-cards").debug_selector(|| "easel-library-cards".into())
            .absolute().top(px(10.)).left(px(8.)).w(px(COLUMN - 16.)).bottom_0().overflow_y_scroll();
        list.style().restrict_scroll_to_axis = Some(true);
        div().debug_selector(|| "easel-library".into()).absolute().left_0().top_0().bottom_0().w(px(COLUMN))
            .when(!empty, |column| column.child(list.child(div().flex().flex_wrap().gap(px(8.)).pb(px(12.)).children(cards))))
            .when(empty, |column| column.child(div().size_full().flex().flex_col().items_center().justify_center().gap(px(12.))
                .text_size(px(13.)).text_color(colors.muted).child("No Easels yet")
                .child(div().id("easel-library-new").debug_selector(|| "easel-library-new".into())
                    .px(px(14.)).h(px(30.)).rounded(px(8.)).bg(colors.selected).hover(move |button| button.bg(colors.hover))
                    .flex().items_center().text_color(colors.text).font_weight(gpui::FontWeight::SEMIBOLD).child("New Easel")
                    .on_click(cx.listener(|this, _, _, cx| this.new_easel(cx))))))
            .into_any_element()
    }
}
