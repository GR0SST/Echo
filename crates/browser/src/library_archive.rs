//! Window-local Archive list and inline filters measured in Arc's Library.
use crate::{
    icons,
    library::LibraryColors,
    text_input::{Changed, TextInput},
    window::BrowserWindow,
};
use browser_core::{ArchiveReason, SpaceId, TabId, UserProfileId};
use chrono::{DateTime, Local};
use gpui::{
    div, prelude::*, px, Context, Entity, Focusable, Hsla, Subscription, WeakEntity, Window,
};
use std::collections::HashSet;

pub(crate) struct ArchiveLibrary {
    owner: WeakEntity<BrowserWindow>,
    input: Entity<TextInput>,
    filters: bool,
    reason: Option<ArchiveReason>,
    spaces: HashSet<SpaceId>,
    profile: Option<UserProfileId>,
    colors: LibraryColors,
    preview: Hsla,
    _subscriptions: Vec<Subscription>,
}
impl ArchiveLibrary {
    pub(crate) fn new(
        owner: WeakEntity<BrowserWindow>,
        colors: LibraryColors,
        preview: Hsla,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("Search Archive…", cx));
        let mut subscriptions = vec![cx.subscribe(&input, |_, _, _: &Changed, cx| cx.notify())];
        if let Some(view) = owner.upgrade() {
            subscriptions.push(cx.observe(&view, |_, _, cx| cx.notify()));
        }
        Self {
            owner,
            input,
            filters: false,
            reason: None,
            spaces: HashSet::new(),
            profile: None,
            colors,
            preview,
            _subscriptions: subscriptions,
        }
    }
    pub(crate) fn set_colors(&mut self, colors: LibraryColors, preview: Hsla) {
        self.colors = colors;
        self.preview = preview;
    }
}
impl Render for ArchiveLibrary {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inventory = self
            .owner
            .read_with(cx, |view, _| {
                let profile = view.browser.active_user_profile_id();
                (
                    profile,
                    view.browser
                        .archive
                        .iter()
                        .filter(|entry| entry.user_profile_id == profile)
                        .cloned()
                        .collect::<Vec<_>>(),
                    view.browser
                        .spaces
                        .iter()
                        .filter(|space| space.user_profile_id == profile)
                        .map(|space| (space.id, space.name.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .ok();
        let colors = self.colors;
        let Some((profile, mut entries, spaces)) = inventory else {
            return div().into_any_element();
        };
        if self.profile != Some(profile) {
            if self.profile.is_some() {
                self.input.update(cx, |input, cx| input.set_text("", cx));
            }
            self.profile = Some(profile);
            self.spaces.clear();
            self.reason = None;
        }
        let query = self.input.read(cx).text().trim().to_lowercase();
        let filter = browser_core::archive_query::ArchiveFilter {
            query,
            spaces: self.spaces.clone(),
            reason: self.reason,
        };
        entries.retain(|entry| filter.matches(entry, profile));
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.archived_at));
        let mut list = div()
            .id("library-archive-rows")
            .debug_selector(|| "library-archive-rows".into())
            .absolute()
            .left(px(8.))
            .right(px(8.))
            .top(px(if self.filters { 242. } else { 58. }))
            .bottom_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        let today = Local::now().date_naive();
        let mut previous = None;
        for entry in entries {
            let day = DateTime::<Local>::from(entry.archived_at).date_naive();
            if previous != Some(day) {
                list = list.child(
                    div()
                        .h(px(24.))
                        .flex_none()
                        .px(px(4.))
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(colors.muted)
                        .child(crate::archive::date_label(day, today)),
                );
                previous = Some(day);
            }
            let favicon = self.owner.read_with(cx, |view, _| {
                (entry.tab.custom_icon.is_some() || view.browser.favicon(profile, &entry.tab.url).is_some())
                    .then(|| view.render_favicon_for_profile(&entry.tab, profile, view.theme_uses_light_ink_for(view.browser.space().id)))
            }).ok().flatten();
            let owner = self.owner.clone();
            let id = entry.tab.id;
            let url = entry.tab.url.clone();
            let captured_url = url.clone();
            let subtitle = url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .unwrap_or(&url)
                .trim_end_matches('/')
                .to_owned();
            let title = entry
                .split
                .as_ref()
                .and_then(|split| split.view.title.clone())
                .unwrap_or(entry.tab.title.clone());
            list = list.child(
                div()
                    .id(("archive-row", id.get() as usize))
                    .debug_selector(move || format!("library-archive-row-{}", id.get()))
                    .h(px(56.))
                    .flex_none()
                    .rounded(px(10.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            view.open_library_archived_tab(profile, id, &captured_url, window, cx)
                        });
                    })
                    .child(
                        div()
                            .size(px(24.))
                            .flex_none()
                            .rounded(px(7.))
                            .bg(colors.hover).flex().items_center().justify_center().children(favicon),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.))
                                    .line_height(px(17.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(crate::sidebar_title::SidebarTitle::new(title)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .line_height(px(15.))
                                    .text_color(colors.muted)
                                    .child(subtitle),
                            ),
                    ),
            );
        }
        let input = self.input.clone();
        let mut column = div()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(px(305.))
            .text_color(colors.text);
        if self.filters {
            column = column
                .child(
                    div()
                        .absolute()
                        .left(px(12.))
                        .top(px(20.))
                        .text_size(px(14.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("Filter Archived Tabs"),
                )
                .child(
                    div()
                        .id("archive-filter-done")
                        .debug_selector(|| "archive-filter-done".into())
                        .absolute()
                        .right(px(12.))
                        .top(px(10.))
                        .w(px(58.))
                        .h(px(36.))
                        .rounded(px(12.))
                        .bg(colors.selected)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.filters = false;
                            cx.notify();
                        }))
                        .child("Done"),
                );
            let mut chips = div().flex().flex_wrap().gap(px(6.));
            for (index, (id, name)) in spaces.into_iter().enumerate() {
                chips = chips.child(
                    div()
                        .id(("archive-space", index))
                        .debug_selector(move || format!("library-archive-space-{index}"))
                        .px(px(12.))
                        .h(px(32.))
                        .rounded(px(12.))
                        .bg(if self.spaces.contains(&id) {
                            colors.selected
                        } else {
                            colors.hover
                        })
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.spaces.remove(&id) {
                                this.spaces.insert(id);
                            }
                            cx.notify();
                        }))
                        .child(name),
                );
            }
            column = column.child(
                div()
                    .absolute()
                    .left(px(8.))
                    .right(px(8.))
                    .top(px(64.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .pl(px(4.))
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(colors.muted)
                            .child("How was the tab closed?"),
                    )
                    .child(
                        div().flex().gap(px(6.)).children(
                            [
                                (ArchiveReason::Automatic, "Auto-Closed"),
                                (ArchiveReason::Manual, "Manually Closed"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (reason, label))| {
                                div()
                                    .id(("archive-reason", index))
                                    .debug_selector(move || {
                                        format!("library-archive-reason-{index}")
                                    })
                                    .px(px(12.))
                                    .h(px(32.))
                                    .rounded(px(12.))
                                    .bg(if self.reason == Some(reason) {
                                        colors.selected
                                    } else {
                                        colors.hover
                                    })
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.reason = if this.reason == Some(reason) {
                                            None
                                        } else {
                                            Some(reason)
                                        };
                                        cx.notify();
                                    }))
                                    .child(label)
                            }),
                        ),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .pl(px(4.))
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(colors.muted)
                            .child("Where was the tab?"),
                    )
                    .child(chips),
            );
        } else {
            column = column
                .child(
                    div()
                        .id("library-archive-search")
                        .debug_selector(|| "library-archive-search".into())
                        .absolute()
                        .left(px(8.))
                        .top(px(10.))
                        .w(px(213.))
                        .h(px(36.))
                        .rounded(px(10.))
                        .bg(colors.selected)
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                            input.read(cx).focus_handle(cx).focus(window);
                            window.prevent_default();
                        })
                        .child(icons::control(icons::Icon::Search))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(self.input.clone()),
                        ),
                )
                .child(
                    div()
                        .id("library-archive-filter")
                        .debug_selector(|| "library-archive-filter".into())
                        .absolute()
                        .right(px(8.))
                        .top(px(10.))
                        .w(px(72.))
                        .h(px(36.))
                        .rounded(px(12.))
                        .bg(colors.hover)
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(3.))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.filters = true;
                            cx.notify();
                        }))
                        .child(archive_filter_icon())
                        .child("Filter"),
                );
        }
        div()
            .debug_selector(|| "library-archive".into())
            .relative()
            .size_full()
            .text_size(px(14.))
            .text_color(colors.text)
            .child(column.child(list))
            .child(crate::library::media::render_preview_at(
                self.preview,
                None,
                305.,
            ))
            .into_any_element()
    }
}
impl BrowserWindow {
    pub(crate) fn open_library_archived_tab(
        &mut self,
        profile: UserProfileId,
        id: TabId,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_navigation.controller.selected() != crate::sidebar_pages::SidebarPage::Library
            || self.sidebar_navigation.library_section != crate::library::LibrarySection::ArchivedTabs
            || self.browser.active_user_profile_id() != profile
            || !self.browser.archive.iter().any(|entry| {
                entry.user_profile_id == profile && entry.tab.id == id && entry.tab.url == url
            })
        {
            return;
        }
        self.open_peek(url.to_owned(), window, cx);
    }
}

// Three decreasing strokes in the reference's 14 px circular outline.
fn archive_filter_icon() -> gpui::AnyElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        let color = window.text_style().color;
        let circle = gpui::Bounds::new(bounds.origin + gpui::point(px(1.), px(1.)), gpui::size(px(14.), px(14.)));
        window.paint_quad(gpui::quad(circle, px(7.), gpui::transparent_black(), px(1.), color, gpui::BorderStyle::Solid));
        for (y, width) in [(5., 8.), (8., 5.), (11., 2.)] {
            let mut stroke = gpui::PathBuilder::stroke(px(1.));
            stroke.move_to(bounds.origin + gpui::point(px(8. - width / 2.), px(y)));
            stroke.line_to(bounds.origin + gpui::point(px(8. + width / 2.), px(y)));
            if let Ok(stroke) = stroke.build() { window.paint_path(stroke, color); }
        }
    }).size(px(16.)).flex_none().into_any_element()
}
