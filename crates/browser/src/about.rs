//! Product information uses Echo's own icon and dependency acknowledgements.
use gpui::{
    div, point, prelude::*, px, rgb, size, App, Bounds, Context, FocusHandle, Focusable,
    KeyBinding, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

gpui::actions!(about, [CloseAbout]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseAbout, Some("About")),
        KeyBinding::new("cmd-w", CloseAbout, Some("About")),
    ]);
    cx.on_action(|_: &crate::menus::About, cx| cx.defer(open));
    cx.on_action(|_: &crate::menus::PrivacyPolicy, cx| open_resource("Privacy.html", cx));
}

pub(crate) fn open(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    for handle in cx.windows() {
        if let Some(handle) = handle.downcast::<AboutWindow>() {
            let _ = handle.update(cx, |view, window, cx| {
                cx.on_next_frame(window, |view: &mut AboutWindow, window, _| {
                    window.activate_window();
                    view.focus.focus(window);
                });
                window.activate_window();
                view.focus.focus(window);
            });
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(360.), px(262.)), cx);
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("About Echo".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        |window, cx| {
            cx.new(|cx| {
                cx.on_next_frame(window, |view: &mut AboutWindow, window, _| {
                    window.activate_window();
                    view.focus.focus(window);
                });
                let focus = cx.focus_handle();
                focus.focus(window);
                AboutWindow { focus }
            })
        },
    );
}

pub(crate) struct AboutWindow {
    focus: FocusHandle,
}
impl Focusable for AboutWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for AboutWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let label = |top: f32, text: String| {
            div()
                .absolute()
                .top(px(top))
                .w_full()
                .text_center()
                .text_size(px(11.))
                .child(text)
        };
        div()
            .id("about-window")
            .key_context("About")
            .track_focus(&self.focus)
            .on_action(|_: &CloseAbout, window, _| window.remove_window())
            .font_family(".SystemUIFont")
            .size_full()
            .relative()
            .bg(rgb(0x292b2d))
            .text_color(rgb(0xefefef))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .w_full()
                    .h(px(32.))
                    .bg(rgb(0x313335)),
            )
            .child(
                gpui::img(std::sync::Arc::new(gpui::Image::from_bytes(
                    gpui::ImageFormat::Png,
                    include_bytes!("../assets/echo-icon.png").to_vec(),
                )))
                .absolute()
                .left(px(148.75))
                .top(px(48.75))
                .size(px(62.5)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(118.))
                    .w_full()
                    .text_center()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Echo"),
            )
            .child(label(
                142.,
                format!("Version {} ({})", option_env!("ECHO_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")), option_env!("ECHO_BUILD_NUMBER").unwrap_or("1")),
            ))
            .child(label(
                156.,
                format!("Chromium Engine Version {}", engine::chromium_version()),
            ))
            .child(label(178., "Copyright © Echo contributors".into()))
            .child(
                div()
                    .absolute()
                    .top(px(202.))
                    .left(px(10.))
                    .w(px(340.))
                    .text_center()
                    .text_size(px(11.))
                    .line_height(px(13.))
                    .child(acknowledgements()),
            )
    }
}

fn acknowledgements() -> gpui::InteractiveText {
    let text = "Echo is made possible by the Chromium open source project,\nChromium’s open source software, as well as other open\nsource software.";
    let chromium = text.find("Chromium").unwrap();
    let software = text.find("open source software").unwrap();
    let other = text.find("open\nsource software").unwrap();
    let ranges = vec![
        chromium..chromium + "Chromium".len(),
        software..software + "open source software".len(),
        other..other + "open\nsource software".len(),
    ];
    let styled = gpui::StyledText::new(text).with_highlights(ranges.iter().cloned().map(|range| {
        (
            range,
            gpui::HighlightStyle {
                color: Some(rgb(0x87b9ef).into()),
                underline: Some(gpui::UnderlineStyle {
                    thickness: px(1.),
                    color: None,
                    wavy: false,
                }),
                ..Default::default()
            },
        )
    }));
    gpui::InteractiveText::new("about-acknowledgements", styled).on_click(ranges, |index, _, cx| {
        if index == 0 {
            cx.open_url("https://www.chromium.org/");
        } else {
            open_resource(
                if index == 1 {
                    "ChromiumCredits.html"
                } else {
                    "Acknowledgements.html"
                },
                cx,
            );
        }
    })
}

fn open_resource(name: &str, cx: &App) {
    if let Some(path) = crate::resources::document(name) {
        if let Ok(url) = url::Url::from_file_path(path) {
            cx.open_url(url.as_str());
        }
    }
}
