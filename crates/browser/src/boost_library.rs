//! Profile-local Boost inventory, measured against Arc's Library column.
use crate::{
    icons,
    library::LibraryColors,
    text_input::{Changed, TextInput},
    window::BrowserWindow,
};
use browser_core::boosts::{Boost, BoostId};
use gpui::{div, prelude::*, px, Context, Entity, Focusable, Subscription, WeakEntity, Window};

pub(crate) struct BoostLibrary {
    browser: WeakEntity<BrowserWindow>,
    input: Entity<TextInput>,
    hover: Option<BoostId>,
    colors: LibraryColors,
    _subscriptions: Vec<Subscription>,
}
impl BoostLibrary {
    pub(crate) fn new(
        browser: WeakEntity<BrowserWindow>,
        colors: LibraryColors,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("Search Boosts...", cx));
        let mut subscriptions = vec![cx.subscribe(&input, |_, _, _: &Changed, cx| cx.notify())];
        if let Some(owner) = browser.upgrade() {
            subscriptions.push(cx.observe(&owner, |_, _, cx| cx.notify()));
        }
        Self {
            browser,
            input,
            hover: None,
            colors,
            _subscriptions: subscriptions,
        }
    }
    pub(crate) fn set_colors(&mut self, colors: LibraryColors) {
        self.colors = colors;
    }
}
impl Render for BoostLibrary {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.input.read(cx).text().to_lowercase();
        let profile = self
            .browser
            .read_with(cx, |view, _| view.browser.active_user_profile_id())
            .ok();
        let mut boosts = self
            .browser
            .read_with(cx, |view, _| {
                view.browser
                    .active_user_profile()
                    .boosts
                    .iter()
                    .filter(|b| {
                        b.name.to_lowercase().contains(&query) || b.domain().contains(&query)
                    })
                    .cloned()
                    .collect::<Vec<Boost>>()
            })
            .unwrap_or_default();
        boosts.sort_by(|a, b| a.domain().cmp(b.domain()));
        let colors = self.colors;
        let mut list = div()
            .id("boost-library-rows")
            .debug_selector(|| "boost-library-rows".into())
            .absolute()
            .top(px(56.))
            .left(px(8.))
            .right(px(8.))
            .bottom_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (index, boost) in boosts.into_iter().enumerate() {
            let domain = boost.domain().to_owned();
            let id = boost.id;
            let enabled = boost.enabled;
            let hover = self.hover == Some(id);
            let thumbnail_color = boost
                .appearance
                .tint
                .map(|h| gpui::hsla(h as f32 / 360., 0.35, 0.75, 1.))
                .unwrap_or(gpui::rgb(0xebebeb).into());
            let mut thumbnail = div()
                .relative()
                .size(px(44.))
                .flex_none()
                .rounded(px(9.))
                .border(px(3.))
                .border_color(gpui::white())
                .bg(thumbnail_color)
                .when(!enabled, |el| el.opacity(0.5));
            if !enabled {
                thumbnail = thumbnail.child(
                    gpui::canvas(
                        |_, _, _| (),
                        |bounds, _, window, _| {
                            let mut line = gpui::PathBuilder::stroke(px(5.));
                            line.move_to(bounds.origin);
                            line.line_to(bounds.bottom_right());
                            if let Ok(path) = line.build() {
                                window.paint_path(path, gpui::rgb(0x303040));
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                );
            }
            let owner = self.browser.clone();
            let row = div()
                .id(("boost-library-row", index))
                .debug_selector(move || format!("boost-library-row-{index}"))
                .h(px(56.))
                .w_full()
                .flex_none()
                .rounded(px(14.))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .when(hover, |el| el.bg(colors.hover))
                .on_hover(cx.listener(move |this, over, _, cx| {
                    this.hover = if *over { Some(id) } else { None };
                    cx.notify();
                }))
                .on_click(move |_, window, cx| {
                    if let Some(profile) = profile {
                        let _ = owner.update(cx, |view, cx| {
                            view.open_library_boost(profile, id, window, cx)
                        });
                    }
                })
                .child(thumbnail)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .text_color(colors.text)
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(17.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .truncate()
                                .child(boost.name),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(15.))
                                .text_color(colors.muted)
                                .truncate()
                                .child(domain),
                        ),
                );
            let owner = self.browser.clone();
            list = list.child(
                row.child(
                    div()
                        .id(("boost-library-toggle", index))
                        .debug_selector(move || format!("boost-library-toggle-{index}"))
                        .w(px(36.))
                        .h(px(24.))
                        .flex_none()
                        .rounded_full()
                        .bg(gpui::white())
                        .opacity(if hover { 1. } else { 0. })
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top(px(2.))
                                .left(px(if enabled { 14. } else { 2. }))
                                .size(px(20.))
                                .rounded_full()
                                .bg(if enabled {
                                    gpui::rgb(0x538bff)
                                } else {
                                    gpui::rgb(0xa5a5aa)
                                }),
                        )
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            if let Some(profile) = profile {
                                let _ = owner.update(cx, |view, cx| {
                                    view.edit_browser(|browser| {
                                        if let Some(boost) = browser
                                            .user_profile_mut(profile)
                                            .and_then(|profile| profile.boosts.get_mut(id))
                                        {
                                            boost.enabled = !boost.enabled;
                                        }
                                    });
                                    view.sync_boosts(cx);
                                });
                            }
                        }),
                ),
            );
        }
        let input = self.input.clone();
        div()
            .debug_selector(|| "boost-library".into())
            .relative()
            .size_full()
            .child(
                div()
                    .debug_selector(|| "boost-library-column".into())
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(304.))
                    .child(
                        div()
                            .id("boost-library-search")
                            .debug_selector(|| "boost-library-search".into())
                            .absolute()
                            .left(px(8.))
                            .top(px(10.))
                            .w(px(288.))
                            .h(px(36.))
                            .rounded(px(12.))
                            .bg(colors.selected)
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .px(px(14.))
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(colors.text)
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                input.read(cx).focus_handle(cx).focus(window);
                            })
                            .child(icons::ionicon("search-outline", 16., 1.))
                            .child(self.input.clone()),
                    )
                    .child(list),
            )
            .child(
                div()
                    .debug_selector(|| "boost-library-preview".into())
                    .absolute()
                    .left(px(305.))
                    .right_0()
                    .top(px(10.))
                    .bottom(px(10.))
                    .rounded(px(7.))
                    .bg(colors.selected),
            )
    }
}
impl BrowserWindow {
    pub(crate) fn open_library_boost(
        &mut self,
        profile: browser_core::UserProfileId,
        id: BoostId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(domain) = self
            .browser
            .user_profile(profile)
            .and_then(|profile| profile.boosts.iter().find(|boost| boost.id == id))
            .map(|boost| boost.domain().to_owned())
        else {
            return;
        };
        self.open_peek(format!("https://{domain}"), window, cx);
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |view, cx| {
                let Some(handle) = view.peek_child else {
                    return;
                };
                let Ok((page, tab)) = handle.update(cx, |page, _, cx| {
                    (cx.entity().downgrade(), page.browser.space().active_tab)
                }) else {
                    return;
                };
                if let Some(tab) = tab {
                    view.open_boost_editor_on(profile, id, tab, page, cx);
                }
            });
        });
    }
}
