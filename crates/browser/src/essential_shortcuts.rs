//! The compact Help cheat sheet. Keycaps describe Echo's default bindings.
use gpui::{
    div, point, prelude::*, px, rgb, size, App, Bounds, Context, FocusHandle, Focusable,
    KeyBinding, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions,
};

gpui::actions!(shortcuts_help, [EssentialShortcuts, CloseShortcuts]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseShortcuts, Some("EssentialShortcuts")),
        KeyBinding::new("cmd-w", CloseShortcuts, Some("EssentialShortcuts")),
    ]);
    cx.on_action(|_: &EssentialShortcuts, cx| cx.defer(open));
}

pub(crate) fn open(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<ShortcutsWindow>() {
            let _ = handle.update(cx, |view, window, cx| {
                cx.on_next_frame(window, |view: &mut ShortcutsWindow, window, _| {
                    window.activate_window();
                    view.focus.focus(window);
                });
                window.activate_window();
                view.focus.focus(window);
            });
            return;
        }
    }
    let mut bounds = Bounds::centered(None, size(px(475.), px(462.)), cx);
    bounds.origin.x = px(f32::from(bounds.origin.x).round());
    bounds.origin.y = px(f32::from(bounds.origin.y).round());
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            kind: WindowKind::Floating,
            titlebar: Some(TitlebarOptions {
                title: Some("Essential Keyboard Shortcuts".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            is_resizable: false,
            ..Default::default()
        },
        |window, cx| {
            cx.new(|cx| {
                crate::native::float_utility_window(window);
                cx.on_next_frame(window, |view: &mut ShortcutsWindow, window, _| {
                    window.activate_window();
                    view.focus.focus(window);
                });
                let focus = cx.focus_handle();
                focus.focus(window);
                ShortcutsWindow { focus }
            })
        },
    );
}

pub(crate) struct ShortcutsWindow {
    focus: FocusHandle,
}
impl Focusable for ShortcutsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for ShortcutsWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows: [(&str, &str, &[&str]); 5] = [
            (
                "Open a New Tab",
                "Open the Command Bar to create a new tab",
                &["⌘", "T"],
            ),
            (
                "Switch to a Recent Tab",
                "Hold this to quickly switch between recent tabs",
                &["⌃", "TAB"],
            ),
            (
                "Copy Page URL",
                "Copy the current tab’s URL without trackers",
                &["⇧", "⌘", "C"],
            ),
            (
                "Add Split View",
                "View multiple websites side-by-side",
                &["⌃", "⇧", "="],
            ),
            (
                "Open Little Echo",
                "Use a smaller browser window for quick searches",
                &["⌥", "⌘", "N"],
            ),
        ];
        div()
            .id("essential-shortcuts-window")
            .key_context("EssentialShortcuts")
            .track_focus(&self.focus)
            .on_action(|_: &CloseShortcuts, window, _| window.remove_window())
            .font_family(".SystemUIFont")
            .size_full()
            .relative()
            .bg(rgb(0x151c30))
            .text_color(rgb(0xffffff))
            .child(
                div()
                    .absolute()
                    .left(px(30.))
                    .top(px(54.))
                    .font_family(".AppleSystemUIFontRounded")
                    .text_size(px(19.75))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Essential Keyboard Shortcuts"),
            )
            .child(
                div()
                    .id("shortcuts-view-all")
                    .debug_selector(|| "shortcuts-view-all".into())
                    .absolute()
                    .left(px(349.))
                    .top(px(55.))
                    .w(px(96.))
                    .h(px(29.))
                    .rounded(px(4.))
                    .bg(rgb(0x282d3f))
                    .border_1()
                    .border_color(rgb(0x383e50))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child("View All")
                    .child(crate::icons::control_sized(
                        crate::icons::Icon::External,
                        12.,
                    ))
                    .on_click(|_, window, cx| {
                        window.remove_window();
                        cx.defer(crate::settings::open_shortcuts);
                    }),
            )
            .children(
                rows.into_iter()
                    .enumerate()
                    .map(|(index, (title, description, keys))| {
                        let top = 100. + index as f32 * 73.;
                        div()
                            .absolute()
                            .left(px(30.))
                            .top(px(top))
                            .w(px(415.))
                            .h(px(56.))
                            .when(index < 4, |row| {
                                row.border_b_1().border_color(rgb(0x212739))
                            })
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-1.))
                                    .text_size(px(14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(19.))
                                    .text_size(px(12.))
                                    .text_color(rgb(0xb7b9c5))
                                    .child(description),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right_0()
                                    .top(px(6.))
                                    .flex()
                                    .gap(px(4.))
                                    .children(keys.iter().map(|key| {
                                        div()
                                            .w(px(28.))
                                            .h(px(28.))
                                            .rounded(px(4.))
                                            .bg(gpui::linear_gradient(
                                                180.,
                                                gpui::linear_color_stop(rgb(0xcfdef8), 0.),
                                                gpui::linear_color_stop(rgb(0xbfd1f0), 1.),
                                            ))
                                            .border_2()
                                            .border_color(rgb(0x0075f7))
                                            .shadow(vec![gpui::BoxShadow {
                                                color: rgb(0x0677f6).into(),
                                                offset: point(px(0.), px(3.)),
                                                blur_radius: px(0.),
                                                spread_radius: px(0.),
                                            }])
                                            .text_color(rgb(0x0075f7))
                                            .text_size(px(if *key == "TAB" { 10. } else { 14. }))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child((*key).to_string())
                                    })),
                            )
                    }),
            )
    }
}
