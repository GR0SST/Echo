//! The selected/recording/conflict states inspected in Arc's Shortcuts tab.
use super::*;
use crate::shortcuts::{self as runtime, CATALOG};
pub(super) struct State {
    pub selected: Option<String>,
    pub armed: bool,
    pub focus: FocusHandle,
    _interceptor: Subscription,
}
impl State {
    pub fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let weak = cx.entity().downgrade();
        let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            if !view.read(cx).shortcuts.armed || !view.read(cx).shortcuts.focus.is_focused(window) {
                return;
            }
            cx.stop_propagation();
            view.update(cx, |this, cx| {
                this.record_shortcut(&event.keystroke, window, cx)
            });
        });
        Self {
            selected: None,
            armed: false,
            focus: cx.focus_handle(),
            _interceptor: interceptor,
        }
    }
}
impl SettingsWindow {
    fn shortcut_entries(&self, cx: &App) -> Vec<(String, String)> {
        let mut entries = CATALOG.clone();
        self.profile.read_settings(|b| {
            entries.extend(b.spaces.iter().take(9).enumerate().map(|(index, space)| {
                (format!("Go to “{}”", space.name), format!("⌃{}", index + 1))
            }))
        });
        let settings = self.profile.settings_snapshot().settings.shortcuts;
        entries.sort_by_key(|(id, _)| (!settings.bindings.contains_key(id), id.to_lowercase()));
        let query = self.shortcut_search.read(cx).text().trim().to_lowercase();
        entries
            .into_iter()
            .filter(|(id, key)| {
                query.is_empty()
                    || format!(
                        "{id} {key} {}",
                        runtime::display(
                            settings.effective(id, runtime::default_key(key).as_deref())
                        )
                    )
                    .to_lowercase()
                    .contains(&query)
            })
            .collect()
    }
    fn shortcut_value(&self, id: &str, default: &str) -> Option<String> {
        let default = runtime::default_key(default);
        self.profile
            .settings_snapshot()
            .settings
            .shortcuts
            .effective(id, default.as_deref())
            .map(str::to_owned)
    }
    fn shortcut_conflict(&self, id: &str, value: Option<&str>) -> Option<String> {
        let value = value?;
        let mut entries = CATALOG.clone();
        self.profile.read_settings(|browser| {
            entries.extend(
                browser
                    .spaces
                    .iter()
                    .take(9)
                    .enumerate()
                    .map(|(index, space)| {
                        (format!("Go to “{}”", space.name), format!("⌃{}", index + 1))
                    }),
            )
        });
        entries
            .iter()
            .find(|(other, default)| {
                other != id && self.shortcut_value(other, default).as_deref() == Some(value)
            })
            .map(|(id, _)| id.clone())
    }
    fn edit_shortcut(&mut self, id: &str, value: Option<String>, cx: &mut Context<Self>) {
        self.profile
            .edit_settings(|browser| browser.settings.shortcuts.set(id, value));
        runtime::apply(&self.profile.settings_snapshot().settings.shortcuts, cx);
        self.shortcuts.armed = false;
        cx.notify();
    }
    fn reset_shortcut(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        self.profile.edit_settings(|b| {
            if let Some(id) = id {
                b.settings.shortcuts.reset(id)
            } else {
                b.settings.shortcuts.bindings.clear()
            }
        });
        runtime::apply(&self.profile.settings_snapshot().settings.shortcuts, cx);
        self.shortcuts.armed = false;
        cx.notify();
    }
    #[cfg(test)]
    pub(crate) fn shortcut_query(&self, cx: &App) -> String {
        self.shortcut_search.read(cx).text().to_owned()
    }
    #[cfg(test)]
    pub(crate) fn shortcut_has_conflict(&self, id: &str) -> bool {
        let default = CATALOG.iter().find(|(label, _)| label == id).unwrap();
        self.shortcut_conflict(id, self.shortcut_value(id, &default.1).as_deref())
            .is_some()
    }
    fn record_shortcut(
        &mut self,
        key: &gpui::Keystroke,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if key.key == "escape" {
            self.shortcuts.armed = false;
            cx.notify();
            return;
        }
        let Some(id) = self.shortcuts.selected.clone() else {
            return;
        };
        if matches!(key.key.as_str(), "backspace" | "delete")
            && key.modifiers == gpui::Modifiers::default()
        {
            self.edit_shortcut(&id, None, cx);
            return;
        }
        if matches!(
            key.key.as_str(),
            "shift" | "control" | "alt" | "cmd" | "super" | "fn" | "meta"
        ) {
            return;
        }
        let modified = key.modifiers.platform || key.modifiers.control || key.modifiers.alt;
        let function = key
            .key
            .strip_prefix('f')
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
        if modified || function {
            self.edit_shortcut(&id, Some(key.unparse()), cx);
        }
    }
    pub(super) fn shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let entries = self.shortcut_entries(cx);
        let settings = self.profile.settings_snapshot().settings.shortcuts;
        let selected = self
            .shortcuts
            .selected
            .as_ref()
            .and_then(|id| CATALOG.iter().find(|(label, _)| label == id))
            .cloned();
        let details = if let Some((id, default)) = selected {
            let value = self.shortcut_value(&id, &default);
            let text = runtime::display(value.as_deref());
            let conflict = self.shortcut_conflict(&id, value.as_deref());
            let conflict_tip = conflict
                .as_ref()
                .map(|other| format!("{text} is already in use by “{other}”"));
            let supported = runtime::supported(&id, runtime::default_key(&default).as_deref(), cx);
            let reset_id = id.clone();
            div()
                .flex()
                .flex_col()
                .w(px(200.))
                .min_w_0()
                .h_full()
                .pb(px(1.))
                .child(
                    div()
                        .id("shortcut-keycaps")
                        .debug_selector(|| "shortcut-keycaps".into())
                        .when_some(conflict_tip, |keys, tip| {
                            keys.tooltip(move |_, cx| {
                                cx.new(|_| ShortcutTooltip(tip.clone())).into()
                            })
                        })
                        .flex()
                        .h(px(64.))
                        .gap(px(4.))
                        .children(text.chars().map(|key| keycap(key, conflict.is_some()))),
                )
                .child(
                    div()
                        .mt(px(24.))
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(id.clone()),
                )
                .when(id == "Blank Window", |panel| {
                    panel.child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .child("Creates a new blank window."),
                    )
                })
                .when(id == "Duplicate", |panel| {
                    panel.child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .child("Duplicates selected tabs."),
                    )
                })
                .when(id == "Add Split View", |panel| {
                    panel.child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .child("Adds a new page to your current Split View."),
                    )
                })
                .child(div().flex_1())
                .child(
                    self.settings_button("Reset Shortcut to Default")
                        .id("reset-one-shortcut")
                        .debug_selector(|| "reset-one-shortcut".into())
                        .w(px(172.))
                        .when(!supported, |b| b.opacity(0.5))
                        .when(supported, |b| {
                            b.cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.reset_shortcut(Some(&reset_id), cx)
                                }))
                        }),
                )
                .into_any_element()
        } else {
            div().w(px(200.)).min_w_0().flex().flex_col().gap(px(10.)).h_full()
                .child(div().flex().gap(px(4.)).text_color(rgb(0x1686ff)).child(div().size(px(64.)).rounded(px(12.)).bg(rgb(0x75acff)).border_2().border_color(rgb(0x0788ff)).flex().items_center().justify_center().child(icons::control_sized(Icon::Command,40.))).child(div().size(px(64.)).rounded(px(12.)).bg(rgb(0x75acff)).border_2().border_color(rgb(0x0788ff)).flex().items_center().justify_center().child(icons::ionicon("happy-outline",42.,1.))))
                .child(div().mt(px(12.)).font_weight(gpui::FontWeight::BOLD).child("Custom Shortcuts"))
                .child(div().text_size(px(11.)).line_height(px(14.)).text_color(colors.rgb(0x929494)).child("Change shortcuts for your favorite actions, and even choose whether Echo or a website takes priority."))
                .child(div().flex_1()).child(self.settings_button("Reset All Shortcuts").id("reset-all-shortcuts").debug_selector(||"reset-all-shortcuts".into()).px(px(8.)).cursor_pointer().on_click(cx.listener(|this,_,_,cx|this.reset_shortcut(None,cx)))).into_any_element()
        };
        let mut last_group = None;
        let rows = entries
            .into_iter()
            .map(|(id, default)| {
                let custom = settings.bindings.contains_key(&id);
                let heading = if last_group != Some(custom) {
                    last_group = Some(custom);
                    Some(if custom {
                        "Customized Shortcuts"
                    } else {
                        "Default Shortcuts"
                    })
                } else {
                    None
                };
                let value = self.shortcut_value(&id, &default);
                let supported =
                    runtime::supported(&id, runtime::default_key(&default).as_deref(), cx);
                let conflict = self.shortcut_conflict(&id, value.as_deref());
                let selected = self.shortcuts.selected.as_ref() == Some(&id);
                let select = id.clone();
                let record = id.clone();
                let selector = format!("shortcut-record-{id}");
                let recording = selected && self.shortcuts.armed;
                let row_tip = conflict.as_ref().map(|other| {
                    format!(
                        "{} is already in use by “{other}”",
                        runtime::display(value.as_deref())
                    )
                });
                div()
                    .flex_none()
                    .when_some(heading, |row, text| {
                        row.child(
                            div()
                                .h(px(28.))
                                .mb(px(10.))
                                .flex()
                                .items_center()
                                .text_size(px(11.))
                                .text_color(colors.rgb(0x929494))
                                .child(text),
                        )
                    })
                    .child(
                        div()
                            .h(px(48.))
                            .flex_none()
                            .border_b_1()
                            .border_color(colors.rgb(0x3e3f41))
                            .relative()
                            .child(
                                div()
                                    .absolute()
                                    .top(px(4.))
                                    .left_0()
                                    .right_0()
                                    .h(px(40.))
                                    .rounded(px(8.))
                                    .when(selected, |row| {
                                        row.bg(if conflict.is_some() {
                                            rgb(if colors.light { 0xfbc6c6 } else { 0x553133 })
                                        } else {
                                            colors.rgb(0x263342)
                                        })
                                    }),
                            )
                            .child(
                                div()
                                    .relative()
                                    .size_full()
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .id(gpui::ElementId::Name(
                                                format!("shortcut-select-{id}").into(),
                                            ))
                                            .debug_selector(move || {
                                                format!("shortcut-select-{select}")
                                            })
                                            .flex_1()
                                            .min_w_0()
                                            .cursor_pointer()
                                            .on_click(cx.listener({
                                                let id = id.clone();
                                                move |this, _, _, cx| {
                                                    this.shortcuts.selected = Some(id.clone());
                                                    this.shortcuts.armed = false;
                                                    cx.notify();
                                                }
                                            }))
                                            .child(id),
                                    )
                                    .when(conflict.is_some(), |row| row.child(conflict_marker()))
                                    .child(
                                        div()
                                            .id(gpui::ElementId::Name(selector.clone().into()))
                                            .debug_selector(move || selector.clone())
                                            .w(px(64.))
                                            .h(px(24.))
                                            .rounded(px(5.))
                                            .border_1()
                                            .border_color(rgb(0xbdbdbd))
                                            .bg(gpui::linear_gradient(
                                                180.,
                                                gpui::linear_color_stop(rgb(0xf7f7f7), 0.),
                                                gpui::linear_color_stop(rgb(0xffffff), 1.),
                                            ))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.))
                                            .text_color(rgb(0x747474))
                                            .child(runtime::display(value.as_deref()))
                                            .when_some(row_tip, |key, tip| {
                                                key.tooltip(move |_, cx| {
                                                    cx.new(|_| ShortcutTooltip(tip.clone())).into()
                                                })
                                            })
                                            .when(selected, |key| {
                                                key.track_focus(&self.shortcuts.focus)
                                            })
                                            .when(recording, |key| {
                                                key.border_2().border_color(rgb(0x1686ff))
                                            })
                                            .when(!supported, |key| key.opacity(0.5))
                                            .when(supported, |key| {
                                                key.cursor_pointer().on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.shortcuts.selected =
                                                            Some(record.clone());
                                                        this.shortcuts.armed = true;
                                                        this.shortcuts.focus.focus(window);
                                                        cx.notify();
                                                    },
                                                ))
                                            }),
                                    ),
                            ),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .w(px(556.))
            .flex()
            .flex_col()
            .gap(px(30.))
            .child(
                div().flex().justify_center().child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .p(px(6.))
                        .rounded(px(6.))
                        .bg(colors.rgb(0x5b575f))
                        .when(colors.light, |callout| {
                            callout.border_1().border_color(rgb(0xe7e7e7)).shadow_sm()
                        })
                        .child("Start with the basics")
                        .child(
                            self.settings_button("Learn the Essential Shortcuts")
                                .id("learn-essential-shortcuts")
                                .debug_selector(|| "learn-essential-shortcuts".into())
                                .bg(rgb(0x147bff))
                                .text_color(rgb(0xffffff))
                                .cursor_pointer()
                                .on_click(|_, _, cx| cx.defer(crate::essential_shortcuts::open)),
                        ),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .h(px(450.))
                    .child(
                        self.settings_group()
                            .when(colors.light, |list| list.bg(rgb(0xffffff)))
                            .w(px(340.))
                            .flex_none()
                            .px_0()
                            .child(
                                div()
                                    .id("shortcut-search")
                                    .debug_selector(|| "shortcut-search".into())
                                    .h(px(40.))
                                    .flex_none()
                                    .px(px(16.))
                                    .bg(colors.rgb(0x3b3e3e))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_color(colors.rgb(0xa6a8a8))
                                    .child(icons::control_sized(Icon::Search, 14.))
                                    .child(self.shortcut_search.clone()),
                            )
                            .child(
                                div()
                                    .id("settings-shortcuts-list")
                                    .min_h_0()
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .px(px(15.))
                                    .children(rows),
                            ),
                    )
                    .child(details),
            )
            .into_any_element()
    }
}

struct ShortcutTooltip(String);
impl Render for ShortcutTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(28.))
            .px(px(10.))
            .rounded(px(14.))
            .bg(rgb(0x303030))
            .border_1()
            .border_color(rgb(0x545454))
            .flex()
            .items_center()
            .text_color(gpui::white())
            .text_size(px(11.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(self.0.clone())
    }
}

fn keycap(key: char, conflict: bool) -> gpui::Div {
    let (outer_top, outer_bottom, inner_top, inner_bottom, ink, edge) = if conflict {
        (0xde4043, 0xcb3a3c, 0xfad7d7, 0xf3c7c7, 0xfe6263, 0xfdecec)
    } else {
        (0x0f6fdf, 0x0d65ca, 0xb1ccf5, 0x92b4e7, 0x0077ff, 0xd1e0fd)
    };
    div()
        .relative()
        .w(px(64.))
        .h(px(64.))
        .min_w_0()
        .flex_shrink()
        .rounded(px(12.))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(rgb(outer_top), 0.),
            gpui::linear_color_stop(rgb(outer_bottom), 1.),
        ))
        .child(
            div()
                .absolute()
                .left(px(4.))
                .right(px(4.))
                .top(px(4.))
                .h(px(51.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(edge))
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(rgb(inner_top), 0.),
                    gpui::linear_color_stop(rgb(inner_bottom), 1.),
                ))
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(ink))
                .font_family(".AppleSystemUIFontRounded")
                .text_size(px(32.))
                .line_height(px(38.))
                .font_weight(gpui::FontWeight::BOLD)
                .child(key.to_string()),
        )
}

fn conflict_marker() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let mut circle = gpui::fill(bounds, rgb(0xff454d));
            circle.corner_radii = px(6.).into();
            window.paint_quad(circle);
            for (x, y, w, h) in [(5.4, 2.5, 1.2, 4.5), (5.4, 8.2, 1.2, 1.2)] {
                let mut mark = gpui::fill(
                    gpui::Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h))),
                    rgb(0x553133),
                );
                mark.corner_radii = px(0.6).into();
                window.paint_quad(mark);
            }
        },
    )
    .size(px(12.))
    .into_any_element()
}
