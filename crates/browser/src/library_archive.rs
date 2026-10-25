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
    div, prelude::*, px, Context, Entity, Focusable, Subscription, WeakEntity, Window,
};
use std::{collections::HashSet, time::SystemTime};

#[derive(Clone, Copy)]
pub(crate) enum ArchiveAction { Restore, Delete }

pub(crate) struct ArchiveLibrary {
    owner: WeakEntity<BrowserWindow>,
    input: Entity<TextInput>,
    filters: bool,
    hovered: Option<TabId>,
    reason: Option<ArchiveReason>,
    little_arc: bool,
    spaces: HashSet<SpaceId>,
    colors: LibraryColors,
    _subscriptions: Vec<Subscription>,
}
impl ArchiveLibrary {
    pub(crate) fn new(
        owner: WeakEntity<BrowserWindow>,
        colors: LibraryColors,
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
            hovered: None,
            reason: None,
            little_arc: false,
            spaces: HashSet::new(),
            colors,
            _subscriptions: subscriptions,
        }
    }
    pub(crate) fn set_colors(&mut self, colors: LibraryColors) {
        self.colors = colors;
    }
}
impl Render for ArchiveLibrary {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inventory = self
            .owner
            .read_with(cx, |view, _| {
                (
                    view.browser
                        .archive
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                    view.browser
                        .spaces
                        .iter()
                        .map(|space| (space.id, space.name.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .ok();
        let colors = self.colors;
        let dark = colors.text.l > 0.5;
        let chip_fill = if dark { gpui::hsla(0., 0., 1., 0.055) } else { gpui::hsla(0., 0., 0., 0.055) };
        let chip_selected = if dark { gpui::hsla(0., 0., 1., 0.45) } else { gpui::hsla(0., 0., 0., 0.45) };
        let chip_selected_ink = if dark { gpui::black() } else { gpui::white() };
        let Some((mut entries, spaces)) = inventory else {
            return div().into_any_element();
        };
        let query = self.input.read(cx).text().trim().to_lowercase();
        let filter = browser_core::archive_query::ArchiveFilter {
            query,
            spaces: self.spaces.clone(),
            reason: self.reason,
            little_arc: self.little_arc,
        };
        entries.retain(|entry| filter.matches_global(entry));
        let empty = entries.is_empty();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.archived_at));
        let has_filters = self.little_arc || self.reason.is_some() || !self.spaces.is_empty();
        let mut list = div()
            .id("library-archive-rows")
            .debug_selector(|| "library-archive-rows".into())
            .absolute()
            .left(px(8.))
            .right(px(8.))
            .top(px(if self.filters { 245. } else if has_filters { 100. } else { 58. }))
            .bottom_0()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        let today = Local::now().date_naive();
        let mut previous = None;
        for entry in entries {
            let profile = entry.user_profile_id;
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
            let archived_at = entry.archived_at;
            let group = gpui::SharedString::from(format!("archive-hover-{}", id.get()));
            let mut actions = div().absolute().right(px(12.)).top(px(12.)).h(px(32.))
                .flex().gap(px(2.)).invisible().group_hover(group.clone(), |style| style.visible());
            for (action, name, icon) in [
                (ArchiveAction::Delete, "Delete", "trash-outline"),
                (ArchiveAction::Restore, "Restore", "arrow-undo-outline"),
            ] {
                let owner = self.owner.clone();
                let url = url.clone();
                actions = actions.child(div()
                    .id((gpui::SharedString::from(format!("archive-{name}")), id.get() as usize))
                    .debug_selector(move || format!("library-archive-{}-{}", name.to_lowercase(), id.get()))
                    .size(px(32.)).rounded(px(8.)).flex().items_center().justify_center()
                    .cursor_pointer().text_color(colors.muted).hover(|style| style.bg(colors.hover).text_color(colors.text))
                    .tooltip(move |_, cx| cx.new(|_| ArchiveTooltip(name)).into())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = owner.update(cx, |view, cx| view.apply_library_archive_action(profile, id, &url, archived_at, action, cx));
                    })
                    .child(icons::ionicon(icon, 16., 1.)));
            }
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
                    .relative().group(group.clone())
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered { this.hovered = Some(id); }
                        else if this.hovered == Some(id) { this.hovered = None; }
                        cx.notify();
                    }))
                    .h(px(56.))
                    .flex_none()
                    .rounded(px(10.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
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
                            .debug_selector(move || format!("library-archive-text-{}", id.get()))
                            .overflow_hidden()
                            .when(self.hovered == Some(id), |style| style.mr(px(76.)))
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
                                    .truncate().w_full().min_w_0().overflow_hidden()
                                    .text_size(px(12.))
                                    .line_height(px(15.))
                                    .text_color(colors.muted)
                                    .child(crate::sidebar_title::SidebarTitle::new(subtitle)),
                            ),
                    )
                    .child(actions),
            );
        }
        if empty && !filter.query.is_empty() {
            list = div().id("library-archive-empty-list").absolute().left(px(8.)).right(px(8.)).top(px(64.)).bottom(px(10.))
                .flex().items_center().justify_center().child(
                    div().debug_selector(|| "library-archive-empty".into()).w_full().flex().flex_col().items_center()
                        .text_color(colors.muted)
                        .child(div().size(px(56.)).child(crate::library::navigation_icon(crate::library::LibrarySection::ArchivedTabs, 56. / 24.)))
                        .child(div().mt(px(12.)).max_w(px(200.)).text_center().text_size(px(14.)).line_height(px(17.)).font_weight(gpui::FontWeight::MEDIUM)
                            .child(format!("No results found for “{}”", self.input.read(cx).text().trim().replace('-', "-\u{200b}"))))
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
                        .top(px(17.))
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
            let mut chips = div().flex().flex_wrap().gap(px(6.)).child(
                div().id("archive-little-arc").debug_selector(|| "library-archive-little-arc".into())
                    .px(px(12.)).h(px(32.)).rounded(px(12.))
                    .bg(if self.little_arc { chip_selected } else { chip_fill })
                    .text_color(if self.little_arc { chip_selected_ink } else { colors.text })
                    .flex().items_center().font_weight(gpui::FontWeight::SEMIBOLD).cursor_pointer().child("Little Arc")
                    .on_click(cx.listener(|this, _, _, cx| { this.little_arc = !this.little_arc; cx.notify(); }))
            );
            for (index, (id, name)) in spaces.into_iter().enumerate() {
                chips = chips.child(
                    div()
                        .id(("archive-space", index))
                        .debug_selector(move || format!("library-archive-space-{index}"))
                        .px(px(12.))
                        .h(px(32.))
                        .rounded(px(12.))
                        .bg(if self.spaces.contains(&id) { chip_selected } else { chip_fill })
                        .text_color(if self.spaces.contains(&id) { chip_selected_ink } else { colors.text })
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
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
                    .top(px(61.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .pl(px(4.))
                            .text_size(px(12.))
                            .line_height(px(16.))
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
                                    .bg(if self.reason == Some(reason) { chip_selected } else { chip_fill })
                                    .text_color(if self.reason == Some(reason) { chip_selected_ink } else { colors.text })
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
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
                            .line_height(px(16.))
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
        if !self.filters && has_filters {
            let chip = |id: &'static str, name: String| div().id(id).debug_selector(move || id.into())
                .flex_none().h(px(32.)).px(px(8.)).rounded(px(12.)).bg(colors.selected)
                .flex().items_center().gap(px(4.)).cursor_pointer().text_size(px(13.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(icons::ionicon("close-circle", 14., 1.)).child(name);
            let mut summary = div().id("archive-active-filters").absolute().top(px(52.)).left(px(8.)).right(px(8.))
                .flex().gap(px(6.)).overflow_x_scroll();
            if self.little_arc {
                summary = summary.child(chip("archive-remove-little", "Little Arc".into())
                    .on_click(cx.listener(|this, _, _, cx| { this.little_arc = false; cx.notify(); })));
            }
            if let Some(reason) = self.reason {
                summary = summary.child(chip("archive-remove-reason", match reason {
                    ArchiveReason::Automatic => "Auto-Closed", ArchiveReason::Manual => "Manually Closed", ArchiveReason::Unknown => "Archived",
                }.into()).on_click(cx.listener(|this, _, _, cx| { this.reason = None; cx.notify(); })));
            }
            for (index, (id, name)) in self.owner.read_with(cx, |view, _| view.browser.spaces.iter()
                .filter(|space| self.spaces.contains(&space.id)).map(|space| (space.id, space.name.clone())).collect::<Vec<_>>()).unwrap_or_default().into_iter().enumerate() {
                summary = summary.child(div().id(("archive-remove-space", index)).h(px(32.)).px(px(8.)).rounded(px(12.))
                    .bg(colors.selected).flex().items_center().gap(px(4.)).cursor_pointer().text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD).child(icons::ionicon("close-circle", 14., 1.)).child(name)
                    .on_click(cx.listener(move |this, _, _, cx| { this.spaces.remove(&id); cx.notify(); })));
            }
            column = column.child(summary);
        }
        div()
            .debug_selector(|| "library-archive".into())
            .relative()
            .size_full()
            .text_size(px(14.))
            .text_color(colors.text)
            .child(column.child(list))

            .into_any_element()
    }
}
impl BrowserWindow {
    pub(crate) fn apply_library_archive_action(
        &mut self, profile: UserProfileId, id: TabId, url: &str, archived_at: SystemTime,
        action: ArchiveAction, cx: &mut Context<Self>,
    ) {
        if self.sidebar_navigation.controller.selected() != crate::sidebar_pages::SidebarPage::Library
            || self.sidebar_navigation.library_section != crate::library::LibrarySection::ArchivedTabs
            || !self.browser.archive.iter().any(|entry| entry.tab.id == id && entry.user_profile_id == profile
                && entry.tab.url == url && entry.archived_at == archived_at) { return; }
        match action {
            ArchiveAction::Restore => {
                if self.edit_browser(|browser| browser.restore_archived_in_background(id, profile, SystemTime::now())).is_some() {
                    let space = self.browser.spaces.iter().find(|space| space.pinned.iter().chain(&space.today).any(|tab| tab.id == id)).map(|space| space.name.clone()).unwrap_or_default();
                    self.show_reveal_toast(format!("Tab restored to {space}. Click to view."), id, cx);
                }
            }
            ArchiveAction::Delete => { self.edit_browser(|browser| browser.delete_archived(id)); }
        }
        cx.notify();
    }
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
            || !self.browser.archive.iter().any(|entry| {
                entry.user_profile_id == profile && entry.tab.id == id && entry.tab.url == url
            })
        {
            return;
        }
        self.open_peek_for_profile(profile, url.to_owned(), window, cx);
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

struct ArchiveTooltip(&'static str);
impl Render for ArchiveTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // GPUI anchors at pointer + (1, 1); Arc's visible pill starts at (9, 12).
        div().pl(px(8.)).pt(px(11.)).child(
            div().px(px(10.)).py(px(6.)).rounded_full().bg(gpui::rgb(0x0b0a48))
                .text_color(gpui::white()).text_size(px(12.)).line_height(px(16.))
                .font_weight(gpui::FontWeight::MEDIUM).child(self.0)
        )
    }
}
