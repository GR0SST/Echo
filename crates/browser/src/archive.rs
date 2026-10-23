//! Searchable archive controls, presented in the browser's command overlay.
use crate::window::{BrowserWindow, Suggestion, Target};
use browser_core::{ArchiveReason, SpaceId, TabId};
use gpui::{div, hsla, prelude::*, AnyElement, Context};
use std::time::SystemTime;
use chrono::{DateTime, Local, NaiveDate};

fn date_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today { "Today".into() }
    else if Some(day) == today.pred_opt() { "Yesterday".into() }
    else { day.format("%b %-d, %Y").to_string() }
}

impl BrowserWindow {
    pub(crate) fn archive_relative(&mut self, id: TabId, relative: browser_core::bulk_archive::Relative, cx: &mut Context<Self>) {
        let removed: Vec<_> = self.browser.tabs_to_archive(id, relative).into_iter().flat_map(|id| self.browser.related_tabs(id)).collect();
        let count = self.edit_browser(|browser| browser.archive_relative(id, relative, SystemTime::now()));
        if let Some(pages) = &self.pages {
            let mut pages = pages.borrow_mut();
            for id in removed { pages.discard(id); }
            pages.sync(&self.browser, true);
        }
        if count > 0 { self.after_tab_removal(cx); }
        if count > 0 && !(self.browser.is_blank() && self.browser.all_tabs().next().is_none()) { self.show_toast(format!("Archived {count} {}", if count == 1 { "tab" } else { "tabs" }), cx); }
        cx.notify();
    }

    pub(crate) fn archive_suggestions(
        &self,
        space: Option<SpaceId>,
        text: &str,
    ) -> Vec<Suggestion> {
        let query = text.trim().to_lowercase();
        let now = SystemTime::now();
        let reason = self.command_bar.as_ref().and_then(|bar| bar.archive_reason);
        let today = DateTime::<Local>::from(now).date_naive();
        let mut entries: Vec<_> = self.browser.archive.iter().rev()
            .filter(|entry| space.is_none_or(|space| entry.space == space))
            .filter(|entry| {
                entry.tab.title.to_lowercase().contains(&query)
                    || entry.tab.url.to_lowercase().contains(&query)
                    || entry.split.as_ref().is_some_and(|split| split.view.title.as_ref().is_some_and(|title| title.to_lowercase().contains(&query)) || split.panes.iter().any(|tab| tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)))
            })
            .filter(|entry| reason.is_none_or(|reason| entry.reason == reason)).collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.archived_at));
        let limit = self.command_bar.as_ref().map_or(8, |bar| bar.archive_limit);
        let remaining = entries.len().saturating_sub(limit);
        let mut rows: Vec<_> = entries.into_iter().take(limit).map(|entry| {
                let space = self
                    .browser
                    .spaces
                    .iter()
                    .find(|space| space.id == entry.space)
                    .map(|space| space.name.as_str())
                    .unwrap_or("Deleted Space");
                let reason = match entry.reason {
                    ArchiveReason::Manual => "Manually Closed",
                    ArchiveReason::Automatic => "Auto-Closed",
                    ArchiveReason::Unknown => "Archived",
                };
                let seconds = now
                    .duration_since(entry.archived_at)
                    .unwrap_or_default()
                    .as_secs();
                let age = if seconds < 60 {
                    "Just now".into()
                } else if seconds < 3600 {
                    format!("{}m ago", seconds / 60)
                } else if seconds < 86400 {
                    format!("{}h ago", seconds / 3600)
                } else {
                    format!("{}d ago", seconds / 86400)
                };
                Suggestion::ArchivedTab {
                    id: entry.tab.id,
                    date: date_label(DateTime::<Local>::from(entry.archived_at).date_naive(), today),
                    title: entry.split.as_ref().map_or_else(|| entry.tab.title.clone(), |split| split.view.title.clone().unwrap_or_else(|| std::iter::once(&entry.tab).chain(&split.panes).map(|tab| tab.title.clone()).collect::<Vec<_>>().join(" | "))),
                    detail: if entry.split.is_some() { format!("{space} · Split View · {reason} · {age}") } else { format!("{space} · {reason} · {age}") },
                }
            })
            .collect();
        if remaining > 0 { rows.push(Suggestion::MoreArchive { remaining }); }
        rows
    }

    pub(crate) fn edit_archive_view(&mut self, space: Option<SpaceId>, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Archive(space);
            bar.selected = 0;
            bar.archive_reason = None;
            bar.archive_limit = 8;
            bar.rows_scroll.set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
            bar.input.update(cx, |input, cx| {
                input.set_placeholder("Search Archive…", cx);
                input.set_text("", cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn confirm_clear_archive(&mut self, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::ConfirmClearArchive;
            bar.selected = 0;
            bar.input.update(cx, |input, cx| {
                input.set_placeholder("Type “Clear Archive” to confirm…", cx);
                input.set_text("", cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn delete_archive_entry(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.edit_browser(|browser| browser.delete_archived(id));
        let count = self.suggestions(cx).len();
        if let Some(bar) = &mut self.command_bar {
            bar.selected = bar.selected.min(count.saturating_sub(1));
        }
        cx.notify();
    }

    pub(crate) fn render_archive_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.browser.space().id;
        let selected = self.command_bar.as_ref().and_then(|bar| match bar.target {
            Target::Archive(space) => space,
            _ => None,
        });
        let spaces = div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .pb_2()
            .text_xs()
            .child(
                div().flex().gap_2().children(
                    [
                        (None, "All Spaces", "archive-all-spaces"),
                        (Some(current), "This Space", "archive-this-space"),
                    ]
                    .into_iter()
                    .map(|(space, label, selector)| {
                        div()
                            .id(selector)
                            .debug_selector(move || selector.into())
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .when(space == selected, |this| this.bg(hsla(0.0, 0.0, 1.0, 0.1)))
                            .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.14)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(bar) = &mut this.command_bar {
                                    bar.target = Target::Archive(space);
                                    bar.selected = 0;
                                    bar.archive_limit = 8;
                                    bar.rows_scroll.set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
                                }
                                cx.notify();
                            }))
                            .child(label)
                    }),
                ),
            )
            .when(!self.browser.archive.is_empty(), |this| {
                this.child(
                    div()
                        .id("clear-archive")
                        .debug_selector(|| "clear-archive".into())
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.08)))
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_clear_archive(cx)))
                        .child("Clear Archive…"),
                )
            })
            .into_any_element();
        let reason = self.command_bar.as_ref().and_then(|bar| bar.archive_reason);
        div().flex().flex_col().child(spaces)
            .child(div().flex().flex_wrap().gap_2().px_4().pb_2().text_xs().children([
                (None, "All", "archive-reason-all"),
                (Some(ArchiveReason::Manual), "Manually Closed", "archive-reason-manual"),
                (Some(ArchiveReason::Automatic), "Auto-Closed", "archive-reason-automatic"),
            ].into_iter().map(|(filter, label, selector)| div().id(selector).debug_selector(move || selector.into())
                .px_2().py_1().rounded_md().when(filter == reason, |this| this.bg(hsla(0., 0., 1., 0.1)))
                .hover(|this| this.bg(hsla(0., 0., 1., 0.14)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(bar) = &mut this.command_bar { bar.archive_reason = filter; bar.archive_limit = 8; bar.selected = 0; bar.rows_scroll.set_offset(gpui::point(gpui::px(0.), gpui::px(0.))); }
                    cx.notify();
                })).child(label))))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn date_groups_use_calendar_boundaries_including_new_year_and_leap_days() {
        let date = |year, month, day| NaiveDate::from_ymd_opt(year, month, day).unwrap();
        assert_eq!(date_label(date(2026, 1, 1), date(2026, 1, 1)), "Today");
        assert_eq!(date_label(date(2025, 12, 31), date(2026, 1, 1)), "Yesterday");
        assert_eq!(date_label(date(2024, 2, 29), date(2024, 3, 1)), "Yesterday");
        assert_eq!(date_label(date(2026, 3, 1), date(2026, 3, 10)), "Mar 1, 2026");
        let zone = FixedOffset::east_opt(3 * 3600).unwrap();
        let before = zone.with_ymd_and_hms(2026, 10, 22, 23, 59, 0).unwrap();
        let after = zone.with_ymd_and_hms(2026, 10, 23, 0, 1, 0).unwrap();
        assert_eq!(date_label(before.date_naive(), after.date_naive()), "Yesterday");
        assert!(after.signed_duration_since(before).num_minutes() < 24 * 60);
    }
}
