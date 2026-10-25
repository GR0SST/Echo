//! Arc's inline Air Traffic Control sheet, inspected in Links preferences.
use crate::{
    icons::{self, Icon},
    profile::ProfileHandle,
    text_input::TextInput,
};
use browser_core::routing::{LinkDestination, LinkMatch};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Context, Entity, EventEmitter, FocusHandle,
    Focusable, SharedString, Subscription, Window, div, prelude::*, px, rgb,
};

pub(crate) struct Dismiss;
struct RouteRow {
    key: u64,
    saved: Option<u64>,
    contains: bool,
    destination: LinkDestination,
    input: Entity<TextInput>,
    _changed: Subscription,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Match(u64),
    Destination(Option<u64>),
}
pub(crate) struct AtcPanel {
    profile: ProfileHandle,
    palette: crate::material::DialogPalette,
    rows: Vec<RouteRow>,
    next_key: u64,
    popup: Option<(Menu, usize)>,
    keyboard_menu: bool,
    menu_scroll: gpui::ScrollHandle,
    focus: FocusHandle,
    _observe: Subscription,
}
impl EventEmitter<Dismiss> for AtcPanel {}
impl Focusable for AtcPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl AtcPanel {
    #[cfg(test)]
    pub(crate) fn menu_is_open(&self) -> bool {
        self.popup.is_some()
    }
    fn text_width(text: &str, window: &Window) -> f32 {
        let mut font = gpui::font(".SystemUIFont");
        font.weight = gpui::FontWeight::MEDIUM;
        window
            .text_system()
            .shape_line(
                text.to_owned().into(),
                px(13.),
                &[gpui::TextRun {
                    len: text.len(),
                    font,
                    color: gpui::white(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
            .to_f64() as f32
    }
    fn destination_width(&self, window: &Window) -> f32 {
        self.destinations()
            .iter()
            .map(|(label, _)| Self::text_width(label, window) + 36.)
            .fold(0., f32::max)
            .ceil()
            .min(355.)
    }
    pub(crate) fn new(profile: ProfileHandle, cx: &mut Context<Self>) -> Self {
        let routes = profile.read_settings(|browser| browser.settings.routing());
        let mut panel = Self {
            _observe: profile.observe_settings(cx),
            profile,
            palette: crate::material::DialogPalette::new(true),            rows: vec![],
            next_key: 0,
            popup: None,
            keyboard_menu: false,
            menu_scroll: gpui::ScrollHandle::new(),
            focus: cx.focus_handle(),
        };
        for rule in routes.rules {
            let (text, contains) = match rule.condition {
                LinkMatch::Contains(text) => (text, true),
                LinkMatch::IsEqualTo(text) => (text, false),
                _ => continue,
            };
            panel.append(Some(rule.id), &text, contains, rule.destination, cx);
        }
        panel
    }
    fn append(
        &mut self,
        saved: Option<u64>,
        text: &str,
        contains: bool,
        destination: LinkDestination,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        self.next_key += 1;
        let key = self.next_key;
        let input = cx.new(|cx| {
            let mut input = TextInput::new("echo.local", cx);
            input.set_text_at_end(text.to_owned(), cx);
            input
        });
        let changed = cx.subscribe(
            &input,
            move |this, _, _: &crate::text_input::Changed, cx| {
                this.save_row(key, cx);
            },
        );
        self.rows.push(RouteRow {
            key,
            saved,
            contains,
            destination,
            input: input.clone(),
            _changed: changed,
        });
        input
    }
    fn save_row(&mut self, key: u64, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter_mut().find(|row| row.key == key) else {
            return;
        };
        let text = row.input.read(cx).text().to_owned();
        let condition = if row.contains {
            LinkMatch::Contains(text.clone())
        } else {
            LinkMatch::IsEqualTo(text.clone())
        };
        if text.trim().is_empty() && row.saved.is_none() {
            return;
        }
        let saved = row.saved;
        let destination = row.destination;
        if self.profile.read_settings(|browser| {
            browser.settings.routing().rules.iter().any(|rule| {
                Some(rule.id) == saved
                    && rule.condition == condition
                    && rule.destination == destination
            })
        }) {
            return;
        }
        let id = self.profile.edit_settings(|browser| {
            browser.settings.edit_routing(|routes| {
                if let Some(rule) =
                    saved.and_then(|id| routes.rules.iter_mut().find(|rule| rule.id == id))
                {
                    rule.condition = condition;
                    rule.destination = destination;
                    rule.id
                } else {
                    routes.add(condition, destination)
                }
            })
        });
        row.saved = Some(id);
        cx.notify();
    }
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        let empty: Vec<_> = self
            .rows
            .iter()
            .filter(|row| row.input.read(cx).text().trim().is_empty())
            .filter_map(|row| row.saved)
            .collect();
        if !empty.is_empty() {
            self.profile.edit_settings(|browser| {
                browser
                    .settings
                    .edit_routing(|routes| routes.rules.retain(|rule| !empty.contains(&rule.id)))
            });
        }
        cx.emit(Dismiss);
    }
    fn destinations(&self) -> Vec<(String, LinkDestination)> {
        self.profile.read_settings(|browser| {
            let mut options: Vec<_> = browser
                .spaces
                .iter()
                .map(|space| {
                    let name = browser
                        .user_profile(space.user_profile_id)
                        .map_or("", |profile| profile.name());
                    (
                        format!("{} ({name})", space.name),
                        LinkDestination::Space(space.id),
                    )
                })
                .collect();
            options.push(("Most Recent Space".into(), LinkDestination::MostRecentSpace));
            options.push(("Little Echo".into(), LinkDestination::LittleWindow));
            options
        })
    }
    fn label(&self, destination: LinkDestination) -> String {
        self.destinations()
            .into_iter()
            .find(|(_, value)| *value == destination)
            .map(|(name, _)| name)
            .unwrap_or_else(|| "Most Recent Space".into())
    }
    fn open_menu(&mut self, menu: Menu, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_some_and(|(current, _)| current == menu) {
            self.popup = None;
        } else {
            let selected = match menu {
                Menu::Match(key) => usize::from(
                    self.rows
                        .iter()
                        .find(|row| row.key == key)
                        .is_some_and(|row| row.contains),
                ),
                Menu::Destination(key) => {
                    let destination = key
                        .and_then(|key| {
                            self.rows
                                .iter()
                                .find(|row| row.key == key)
                                .map(|row| row.destination)
                        })
                        .unwrap_or_else(|| {
                            self.profile.read_settings(|browser| {
                                browser.settings.routing().default_destination
                            })
                        });
                    self.destinations()
                        .iter()
                        .position(|(_, value)| *value == destination)
                        .unwrap_or(0)
                }
            };
            self.popup = Some((menu, selected));
            self.keyboard_menu = false;
            self.menu_scroll.scroll_to_item(selected);
        }
        self.focus.focus(window);
        cx.notify();
    }
    fn choose(&mut self, menu: Menu, selected: usize, cx: &mut Context<Self>) {
        match menu {
            Menu::Match(key) => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.key == key) {
                    row.contains = selected == 1;
                }
                self.save_row(key, cx);
            }
            Menu::Destination(Some(key)) => {
                if let Some((_, destination)) = self.destinations().get(selected) {
                    if let Some(row) = self.rows.iter_mut().find(|row| row.key == key) {
                        row.destination = *destination;
                    }
                    self.save_row(key, cx);
                }
            }
            Menu::Destination(None) => {
                if let Some((_, destination)) = self.destinations().get(selected) {
                    self.profile.edit_settings(|browser| {
                        browser
                            .settings
                            .edit_routing(|routes| routes.default_destination = *destination)
                    });
                }
            }
        }
        self.popup = None;
        cx.notify();
    }
    fn picker(
        &self,
        key: String,
        label: String,
        width: f32,
        menu: Menu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector(move || key.clone())
            .w(px(width))
            .h(px(24.))
            .rounded(px(5.))
            .bg(self.palette.control)
            .flex()
            .items_center()
            .justify_between()
            .pl(px(12.))
            .pr(px(7.))
            .gap(px(6.))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(chooser_arrow())
            .on_click(cx.listener(move |this, _, window, cx| this.open_menu(menu, window, cx)))
            .into_any_element()
    }
    fn render_row(
        &self,
        row: &RouteRow,
        destination_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = row.key;
        let input = row.input.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .mb(px(24.))
            .child(
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(80.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .items_center()
                            .gap(px(5.))
                            .text_color(self.palette.muted)
                            .text_size(px(14.))
                            .child(icons::control(Icon::Link))
                            .child("URL"),
                    )
                    .child(
                        self.picker(
                            format!("atc-match-{key}"),
                            if row.contains {
                                "Contains"
                            } else {
                                "Is equal to"
                            }
                            .into(),
                            111.,
                            Menu::Match(key),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("atc-input-{key}")))
                            .debug_selector(move || format!("atc-input-{key}"))
                            .flex_1()
                            .min_w_0()
                            .h(px(24.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(self.palette.border)
                            .bg(self.palette.input)
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .child(input.clone())
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                input.read(cx).focus_handle(cx).focus(window)
                            }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("atc-remove-{key}")))
                            .debug_selector(move || format!("atc-remove-{key}"))
                            .w(px(18.))
                            .h(px(24.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(self.palette.muted)
                            .text_size(px(14.))
                            .child(icons::ionicon("remove-circle", 18., 1.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(id) = this
                                    .rows
                                    .iter()
                                    .find(|row| row.key == key)
                                    .and_then(|row| row.saved)
                                {
                                    this.profile.edit_settings(|browser| {
                                        browser.settings.edit_routing(|routes| routes.remove(id))
                                    });
                                }
                                this.rows.retain(|row| row.key != key);
                                this.popup = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .pr(px(12.))
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_color(self.palette.muted)
                            .text_size(px(14.))
                            .child(icons::ionicon("return-down-forward-outline", 18., 1.))
                            .child("Open in"),
                    )
                    .child(self.picker(
                        format!("atc-destination-{key}"),
                        self.label(row.destination),
                        destination_width,
                        Menu::Destination(Some(key)),
                        cx,
                    )),
            )
            .into_any_element()
    }
    fn render_menu(
        &self,
        menu: Menu,
        selected: usize,
        destination_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let labels: Vec<String> = match menu {
            Menu::Match(_) => vec!["Is equal to".into(), "Contains".into()],
            _ => self
                .destinations()
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
        };
        let current = match menu {
            Menu::Match(key) => usize::from(
                self.rows
                    .iter()
                    .find(|row| row.key == key)
                    .is_some_and(|row| row.contains),
            ),
            Menu::Destination(key) => {
                let destination = key
                    .and_then(|key| {
                        self.rows
                            .iter()
                            .find(|row| row.key == key)
                            .map(|row| row.destination)
                    })
                    .unwrap_or_else(|| {
                        self.profile
                            .read_settings(|browser| browser.settings.routing().default_destination)
                    });
                self.destinations()
                    .iter()
                    .position(|(_, value)| *value == destination)
                    .unwrap_or(0)
            }
        };
        let row_key = match menu {
            Menu::Match(key) | Menu::Destination(Some(key)) => Some(key),
            _ => None,
        };
        let index = row_key
            .and_then(|key| self.rows.iter().position(|row| row.key == key))
            .unwrap_or(0);
        let anchor = match menu {
            Menu::Match(_) => 108. + index as f32 * 76.,
            Menu::Destination(Some(_)) => 136. + index as f32 * 76.,
            Menu::Destination(None) => 434.,
        };
        let height = (labels.len() as f32 * 24.
            + 12.
            + if matches!(menu, Menu::Destination(_)) {
                9.
            } else {
                0.
            })
        .min(420.);
        let top = (anchor - current as f32 * 24. - 6.).clamp(4., 496. - height);
        div()
            .id("atc-menu-backdrop")
            .absolute()
            .inset_0()
            .on_click(cx.listener(|this, _, _, cx| {
                this.popup = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("atc-menu")
                    .debug_selector(|| "atc-menu".into())
                    .absolute()
                    .left(px(match menu {
                        Menu::Match(_) => 101.,
                        Menu::Destination(Some(_)) => {
                            254. + (19. + 8. + 45. - destination_width) / 2. - 12.
                        }
                        Menu::Destination(None) => {
                            112. + (270. - destination_width.min(270.)) / 2. - 12.
                        }
                    }))
                    .top(px(top))
                    .w(px(match menu {
                        Menu::Match(_) => 111.,
                        _ => destination_width + 15.,
                    }))
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .track_scroll(&self.menu_scroll)
                    .py(px(6.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(if self.palette.dark { rgb(0x141822).into() } else { self.palette.border })
                    .bg(if self.palette.dark { rgb(0x343944).into() } else { self.palette.surface })
                    .shadow_lg()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .children(labels.into_iter().enumerate().map(|(index, label)| {
                        div()
                            .flex()
                            .flex_col()
                            .when(
                                matches!(menu, Menu::Destination(_))
                                    && index == self.destinations().len() - 2,
                                |s| {
                                    s.child(
                                        div().h(px(1.)).mx(px(10.)).my(px(4.)).bg(self.palette.border),
                                    )
                                },
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("atc-option-{index}")))
                                    .debug_selector(move || format!("atc-option-{index}"))
                                    .mx(px(5.))
                                    .px(px(6.))
                                    .h(px(24.))
                                    .rounded(px(5.))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .when(index == selected && self.keyboard_menu, |s| {
                                        s.bg(rgb(0x007aff)).text_color(gpui::white())
                                    })
                                    .hover(|s| s.bg(rgb(0x007aff)).text_color(gpui::white()))
                                    .child(div().w(px(14.)).when(index == current, |s| {
                                        s.child(icons::control(Icon::Check))
                                    }))
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.choose(menu, index, cx)
                                    })),
                            )
                    })),
            )
            .into_any_element()
    }
}
fn chooser_arrow() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            for (y, sign) in [(6., -1.), (10., 1.)] {
                let mut path = gpui::PathBuilder::stroke(px(1.2));
                path.move_to(bounds.origin + gpui::point(px(2.), px(y)));
                path.line_to(bounds.origin + gpui::point(px(5.), px(y + sign * 3.)));
                path.line_to(bounds.origin + gpui::point(px(8.), px(y)));
                if let Ok(path) = path.build() {
                    window.paint_path(path, window.text_style().color);
                }
            }
        },
    )
    .w(px(10.))
    .h(px(14.))
    .flex_none()
    .into_any_element()
}
impl Render for AtcPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.palette = crate::material::DialogPalette::new(matches!(window.appearance(), gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark));
        let palette = self.palette;
        let destination_width = self.destination_width(window);
        let destination = self
            .profile
            .read_settings(|browser| browser.settings.routing().default_destination);
        div()
            .id("atc-sheet")
            .debug_selector(|| "atc-sheet".into())
            .relative()
            .w(px(510.))
            .h(px(500.))
            .rounded(px(18.))
            .border_1()
            .border_color(self.palette.border)
            .bg(self.palette.surface)
            .shadow_lg()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .line_height(px(16.))
            .font_family(".SystemUIFont")
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(self.palette.text)
            .track_focus(&self.focus)
            .key_context("AirTrafficControl")
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if let Some((menu, selected)) = this.popup {
                    let count = if matches!(menu, Menu::Match(_)) {
                        2
                    } else {
                        this.destinations().len()
                    };
                    match event.keystroke.key.as_str() {
                        "escape" => this.popup = None,
                        "down" => this.popup = Some((menu, (selected + 1) % count)),
                        "up" => this.popup = Some((menu, (selected + count - 1) % count)),
                        "enter" => this.choose(menu, selected, cx),
                        _ => return,
                    }
                    this.keyboard_menu = true;
                    if let Some((_, selected)) = this.popup {
                        this.menu_scroll.scroll_to_item(selected);
                    }
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                match event.keystroke.key.as_str() {
                    "escape" => {
                        this.dismiss(cx);
                        cx.stop_propagation();
                    }
                    "enter" => {
                        this.focus.focus(window);
                        cx.stop_propagation();
                    }
                    "tab" => {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev();
                        } else {
                            window.focus_next();
                        }
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // Two native frame strokes, without changing content layout.
                        window.paint_quad(gpui::fill(bounds, palette.border).corner_radii(px(17.)));
                        window.paint_quad(
                            gpui::fill(bounds.inset(px(1.)), palette.surface).corner_radii(px(16.)),
                        );
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .h(px(83.))
                    .flex_none()
                    .mx(px(24.))
                    .pt(px(32.))
                    .pb(px(24.))
                    .border_b_1()
                    .border_color(self.palette.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(self.palette.text)
                            .relative()
                            .top(px(-1.))
                            .child("Air Traffic Control"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(28.))
                            .mr(px(4.))
                            .child(
                                div()
                                    .id("atc-new-route")
                                    .debug_selector(|| "atc-new-route".into())
                                    .w(px(122.))
                                    .h(px(28.))
                                    .rounded(px(6.))
                                    .bg(self.palette.control)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child("New Route")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.popup = None;
                                        let input = this.append(
                                            None,
                                            "",
                                            true,
                                            LinkDestination::MostRecentSpace,
                                            cx,
                                        );
                                        input.read(cx).focus_handle(cx).focus(window);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("atc-close")
                                    .debug_selector(|| "atc-close".into())
                                    .w(px(24.))
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(self.palette.muted)
                                    .child(icons::ionicon("close-outline", 20., 1.))
                                    .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .id("atc-routes")
                    .flex_1()
                    .min_h_0()
                    .mx(px(24.))
                    .pt(px(24.))
                    .overflow_y_scroll()
                    .children(
                        self.rows
                            .iter()
                            .map(|row| self.render_row(row, destination_width, cx)),
                    ),
            )
            .child(
                div()
                    .h(px(90.))
                    .flex_none()
                    .mx(px(24.))
                    .pt(px(24.))
                    .border_t_1()
                    .border_color(self.palette.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().w(px(80.)).text_right().child("Default"))
                            .child(div().w(px(270.)).flex().justify_center().child(self.picker(
                                "atc-default".into(),
                                self.label(destination),
                                destination_width.min(270.),
                                Menu::Destination(None),
                                cx,
                            ))),
                    ),
            )
            .when_some(self.popup, |s, (menu, selected)| {
                s.child(self.render_menu(menu, selected, destination_width, cx))
            })
            // Native Arc recording: 24px descent, settling over 200ms; the
            // surface fade completes before the positional easing settles.
            .with_animation(
                "atc-entry",
                Animation::new(if crate::native::prefers_reduced_motion() {
                    std::time::Duration::ZERO
                } else {
                    std::time::Duration::from_millis(200)
                })
                .with_easing(|t| 0.5 - 0.5 * (std::f32::consts::PI * t).cos()),
                |panel, delta| {
                    panel
                        .top(px(-24. * (1. - delta)))
                        .opacity((delta * 2.).min(1.))
                },
            )
    }
}
