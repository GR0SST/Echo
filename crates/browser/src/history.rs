//! Browser-owned history page, rendered in GPUI rather than Chromium WebUI.
use crate::text_input::{Changed, TextInput};
use crate::window::BrowserWindow;
use browser_core::{history::HistoryEntry, TabId};
use gpui::{
    actions, div, hsla, prelude::*, px, AnyElement, App, Context, Entity, Focusable, KeyBinding,
    Subscription, Window,
};
use std::time::SystemTime;

pub(crate) const HISTORY_URL: &str = "browser://history";
const CONTEXT: &str = "HistoryPage";
actions!(history, [ViewHistory, OpenFirstResult, CancelClear]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", OpenFirstResult, Some(CONTEXT)),
        KeyBinding::new("escape", CancelClear, Some(CONTEXT)),
    ]);
}

pub(crate) struct HistoryPage {
    pub input: Entity<TextInput>,
    confirm_clear: bool,
    limit: usize,
    last_tab: Option<TabId>,
    _changed: Subscription,
}

impl HistoryPage {
    pub fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let input = cx.new(|cx| TextInput::new("Search history…", cx));
        let changed = cx.subscribe(&input, |this, _, _: &Changed, cx| {
            this.history_page.limit = 100;
            cx.notify();
        });
        Self {
            input,
            confirm_clear: false,
            limit: 100,
            last_tab: None,
            _changed: changed,
        }
    }
}

impl BrowserWindow {
    pub(crate) fn history_is_open(&self) -> bool {
        self.browser
            .active_tab()
            .is_some_and(|tab| tab.url.eq_ignore_ascii_case(HISTORY_URL))
    }

    pub(crate) fn focus_history_search(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.history_is_open() {
            return false;
        }
        self.history_page
            .input
            .read(cx)
            .focus_handle(cx)
            .focus(window);
        self.history_page
            .input
            .update(cx, |input, cx| input.select_all_text(cx));
        true
    }

    pub(crate) fn sync_history_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_is_open() {
            let tab = self.browser.space().active_tab;
            if self.history_page.last_tab != tab {
                self.history_page.last_tab = tab;
                self.focus_history_search(window, cx);
            }
        } else {
            self.history_page.last_tab = None;
            self.history_page.confirm_clear = false;
        }
    }

    pub(crate) fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existing = self.browser.favorites.iter()
            .chain(&self.browser.space().pinned)
            .chain(&self.browser.space().today)
            .find(|tab| tab.url.eq_ignore_ascii_case(HISTORY_URL))
            .map(|tab| tab.id);
        let now = SystemTime::now();
        if let Some(id) = existing {
            self.browser.activate_tab(id, now);
        } else {
            let id = self.browser.new_tab(HISTORY_URL, now);
            self.browser.set_title(id, "History");
        }
        self.history_page.confirm_clear = false;
        self.focus_history_search(window, cx);
        cx.notify();
    }

    pub(crate) fn history_from_command(&mut self, cx: &mut Context<Self>) {
        self.close_command_bar(cx);
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = browser.update(cx, |this, cx| this.open_history(window, cx));
            });
        });
    }

    fn filtered_history(&self, cx: &App) -> Vec<&HistoryEntry> {
        let query = self
            .history_page
            .input
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        self.browser
            .history
            .iter()
            .filter(|entry| {
                entry.title.to_lowercase().contains(&query)
                    || entry.url.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn open_history_url(
        &mut self,
        url: &str,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.browser.new_tab(url, SystemTime::now());
        self.browser.set_title(id, title);
        self.focus(window);
        cx.notify();
    }

    pub(crate) fn render_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let entries = self.filtered_history(cx);
        let count = entries.len();
        let rows: Vec<_> = entries
            .into_iter()
            .take(self.history_page.limit)
            .enumerate()
            .map(|(index, entry)| {
                let (url, title) = (entry.url.clone(), entry.title.clone());
                let remove = url.clone();
                let seconds = SystemTime::now()
                    .duration_since(entry.visited_at)
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
                div()
                    .id(("history-visit", index))
                    .debug_selector(move || format!("history-visit-{index}"))
                    .flex()
                    .items_center()
                    .gap_3()
                    .w_full()
                    .h(px(64.0))
                    .px_3()
                    .rounded_lg()
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.04)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_history_url(&url, &title, window, cx)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().truncate().text_sm().child(entry.title.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                                    .child(entry.url.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                            .child(age),
                    )
                    .child(
                        div()
                            .id(("history-remove", index))
                            .debug_selector(move || format!("history-remove-{index}"))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.08)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.browser.delete_history_entry(&remove);
                                cx.notify();
                            }))
                            .child("×"),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .id("history-page")
            .debug_selector(|| "history-page".into())
            .key_context(CONTEXT)
            .size_full()
            .min_h_0()
            .max_w(px(960.0))
            .py_8()
            .flex()
            .flex_col()
            .gap_4()
            .text_color(hsla(0.0, 0.0, 0.15, 1.0))
            .on_action(cx.listener(|this, _: &OpenFirstResult, window, cx| {
                let first = this
                    .filtered_history(cx)
                    .first()
                    .map(|entry| (entry.url.clone(), entry.title.clone()));
                if let Some((url, title)) = first {
                    this.open_history_url(&url, &title, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CancelClear, _, cx| {
                this.history_page.confirm_clear = false;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_2xl().child("History"))
                    .when(!self.browser.history.is_empty(), |this| {
                        this.child(
                            div()
                                .id("clear-history")
                                .debug_selector(|| "clear-history".into())
                                .px_3()
                                .py_2()
                                .rounded_lg()
                                .text_sm()
                                .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.05)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.history_page.confirm_clear = true;
                                    this.focus_history_search(window, cx);
                                    cx.notify();
                                }))
                                .child("Clear history…"),
                        )
                    }),
            )
            .child(
                div()
                    .id("history-search")
                    .debug_selector(|| "history-search".into())
                    .h(px(40.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_lg()
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.15))
                    .text_sm()
                    .child(self.history_page.input.clone()),
            )
            .when(self.history_page.confirm_clear, |this| {
                this.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(hsla(0.0, 0.0, 0.0, 0.04))
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_sm()
                        .child(
                            div()
                                .flex_1()
                                .child("Permanently delete all browsing history?"),
                        )
                        .child(
                            div()
                                .id("history-clear-cancel")
                                .debug_selector(|| "history-clear-cancel".into())
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.08)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history_page.confirm_clear = false;
                                    cx.notify();
                                }))
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id("history-clear-confirm")
                                .debug_selector(|| "history-clear-confirm".into())
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(hsla(0.0, 0.65, 0.45, 1.0))
                                .text_color(gpui::white())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.browser.history.clear();
                                    this.history_page.confirm_clear = false;
                                    this.history_page
                                        .input
                                        .update(cx, |input, cx| input.set_text("", cx));
                                    cx.notify();
                                }))
                                .child("Delete history"),
                        ),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                    .child(format!(
                        "{count} {}",
                        if count == 1 { "page" } else { "pages" }
                    )),
            )
            .child(
                div()
                    .id("history-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows)
                    .when(count == 0, |this| {
                        this.child(
                            div()
                                .py_8()
                                .text_sm()
                                .text_color(hsla(0.0, 0.0, 0.45, 1.0))
                                .child(if self.browser.history.is_empty() {
                                    "No browsing history yet."
                                } else {
                                    "No pages match your search."
                                }),
                        )
                    })
                    .when(count > self.history_page.limit, |this| {
                        this.child(
                            div()
                                .id("history-more")
                                .debug_selector(|| "history-more".into())
                                .px_3()
                                .py_2()
                                .rounded_lg()
                                .text_sm()
                                .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.04)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history_page.limit += 100;
                                    cx.notify();
                                }))
                                .child("Show more"),
                        )
                    }),
            )
            .into_any_element()
    }
}
