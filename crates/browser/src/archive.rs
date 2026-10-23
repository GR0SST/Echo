//! Searchable archive controls, presented in the browser's command overlay.
use crate::window::{BrowserWindow, Suggestion, Target};
use browser_core::{ArchiveReason, SpaceId, TabId};
use gpui::{div, hsla, prelude::*, AnyElement, Context};
use std::time::SystemTime;

impl BrowserWindow {
    pub(crate) fn archive_suggestions(
        &self,
        space: Option<SpaceId>,
        text: &str,
    ) -> Vec<Suggestion> {
        let query = text.trim().to_lowercase();
        let now = SystemTime::now();
        self.browser
            .archive
            .iter()
            .rev()
            .filter(|entry| space.is_none_or(|space| entry.space == space))
            .filter(|entry| {
                entry.tab.title.to_lowercase().contains(&query)
                    || entry.tab.url.to_lowercase().contains(&query)
                    || entry.split.as_ref().is_some_and(|split| split.view.title.as_ref().is_some_and(|title| title.to_lowercase().contains(&query)) || split.panes.iter().any(|tab| tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)))
            })
            .take(8)
            .map(|entry| {
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
                    title: entry.split.as_ref().map_or_else(|| entry.tab.title.clone(), |split| split.view.title.clone().unwrap_or_else(|| std::iter::once(&entry.tab).chain(&split.panes).map(|tab| tab.title.clone()).collect::<Vec<_>>().join(" | "))),
                    detail: if entry.split.is_some() { format!("{space} · Split View · {reason} · {age}") } else { format!("{space} · {reason} · {age}") },
                }
            })
            .collect()
    }

    pub(crate) fn edit_archive_view(&mut self, space: Option<SpaceId>, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Archive(space);
            bar.selected = 0;
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
        self.browser.delete_archived(id);
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
        div()
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
                            .when(space == selected, |this| this.bg(hsla(0.0, 0.0, 0.0, 0.08)))
                            .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.12)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(bar) = &mut this.command_bar {
                                    bar.target = Target::Archive(space);
                                    bar.selected = 0;
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
                        .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.06)))
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_clear_archive(cx)))
                        .child("Clear Archive…"),
                )
            })
            .into_any_element()
    }
}
