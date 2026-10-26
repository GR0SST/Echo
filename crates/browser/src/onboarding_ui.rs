//! First-run setup drawn like Arc's: a split card over the intro's drifting
//! color field. The white left panel holds Back, the title, the text, Next
//! and Skip; the tinted right panel, behind a scalloped edge, holds each
//! step's controls. Geometry is measured from Arc at a 1000 × 626 point
//! window and scales with the window.
use super::*;
use crate::theme_editor::{chevron, color, mode_icon, preset_color};
use browser_core::themes::{ThemeAppearance, ThemeColor};
use gpui::{Bounds, ImageFormat, MouseButton, Pixels, Rgba, Stateful, Div, img, point};
use std::sync::OnceLock;

/// Arc's onboarding blue.
const ACCENT: u32 = 0x3239fc;
const INK: u32 = 0x000000;
const TEXT: u32 = 0x6e6e6e;
const TILE: u32 = 0xf0f0f0;
/// The reference window, in points.
const REFERENCE: (f32, f32) = (1000., 626.);
const CARD: (f32, f32) = (754., 427.5);
/// The left panel's width; the right panel starts at its scalloped edge.
const SPLIT: f32 = 280.;
const PAD: f32 = 39.;

/// Each app's own icon and brand color, in `FAVORITES` order.
const APPS: [(&[u8], ImageFormat, u32); 9] = [
    (include_bytes!("../assets/onboarding/x.png"), ImageFormat::Png, 0x000000),
    (include_bytes!("../assets/onboarding/gmail.png"), ImageFormat::Png, 0xea4335),
    (include_bytes!("../assets/onboarding/calendar.png"), ImageFormat::Png, 0x4285f4),
    (include_bytes!("../assets/onboarding/spotify.png"), ImageFormat::Png, 0x1ed760),
    (include_bytes!("../assets/onboarding/youtube.png"), ImageFormat::Png, 0xff0000),
    (include_bytes!("../assets/onboarding/notion.png"), ImageFormat::Png, 0x000000),
    (include_bytes!("../assets/onboarding/slack.png"), ImageFormat::Png, 0xe01e5a),
    (include_bytes!("../assets/onboarding/figma.svg"), ImageFormat::Svg, 0xf24e1e),
    (include_bytes!("../assets/onboarding/outlook.png"), ImageFormat::Png, 0x0078d4),
];

fn rgba(hex: u32) -> Rgba {
    gpui::rgb(hex)
}
/// `b` laid over `a` at `t`.
fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: 1. }
}
fn white() -> Rgba {
    rgba(0xffffff)
}
/// The background's clock, shared by every setup step so the field keeps
/// drifting across them. It starts past the intro's brightening.
fn field_time() -> f32 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    5. + START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32()
}
fn app_image(index: usize) -> Arc<gpui::Image> {
    thread_local! {
        static IMAGES: std::cell::RefCell<Vec<Arc<gpui::Image>>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    IMAGES.with(|images| {
        let mut images = images.borrow_mut();
        if images.is_empty() {
            *images = APPS
                .iter()
                .map(|(bytes, format, _)| Arc::new(gpui::Image::from_bytes(*format, bytes.to_vec())))
                .collect();
        }
        images[index].clone()
    })
}
/// The theme color's position in the picker field: hue by angle, saturation
/// by distance from the center, as in the theme editor.
fn color_offset(c: ThemeColor, radius: f32) -> (f32, f32) {
    let (h, s, _) = c.hsv();
    let a = (h - 90.).to_radians();
    (a.cos() * s * radius, a.sin() * s * radius)
}

impl SetupDialog {
    /// Buttons and links follow the picked theme, like Arc's.
    fn accent(&self) -> Rgba {
        if !self.theme_picked {
            return rgba(ACCENT);
        }
        let (h, s, _) = self.theme.colors[0].hsv();
        color(ThemeColor::from_hsv(h, s.max(0.55), 0.62))
    }

    fn emit_theme(&mut self, cx: &mut Context<Self>) {
        self.theme_picked = true;
        cx.emit(Event::Theme(self.theme.clone()));
        cx.notify();
    }

    fn onboarding_title(&self) -> &str {
        match self.step {
            // Arc breaks its titles by hand.
            Step::Onboard(SetupStep::Import) => "Get us up\nto speed.",
            Step::Onboard(SetupStep::Favorites) => "Choose the\napps you use\nmost.",
            Step::Onboard(SetupStep::Theme) => "Add some flair\nwith a theme.",
            _ => self.title(),
        }
    }

    pub(super) fn render_onboarding(
        &mut self,
        _dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let size = window.viewport_size();
        let (w, h) = (f32::from(size.width), f32::from(size.height));
        // Baking finishes with a notify, which renders and bakes the next.
        self.bake_field(field_time(), w / h.max(1.), window, cx);
        let field = self.field.clone();
        let s = (w / REFERENCE.0).min(h / REFERENCE.1).clamp(0.5, 1.5);
        let u = move |v: f32| px(v * s);
        let accent = self.accent();
        let panel = mix(white(), accent, 0.127);
        let link = mix(white(), accent, 0.65);
        let enabled = self.can_next();
        let skip = matches!(
            self.step,
            Step::Onboard(SetupStep::Favorites | SetupStep::Theme) | Step::Passwords
        );
        let next_label = match self.step {
            Step::Reading => "Importing…",
            _ => "Next",
        };
        let errors = self.error.clone().or_else(|| {
            self.report
                .as_ref()
                .filter(|r| !r.issues.is_empty() && matches!(self.step, Step::Passwords | Step::Bookmarks))
                .map(|r| r.issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>().join("\n"))
        });

        let left = div()
            .absolute()
            .left_0()
            .top_0()
            .w(u(SPLIT))
            .h_full()
            .rounded_l(u(16.))
            .bg(white())
            .when(self.has_back(), |l| {
                l.child(
                    div()
                        .id("setup-back")
                        .debug_selector(|| "setup-back".into())
                        .absolute()
                        .left(u(PAD))
                        .top(u(37.))
                        .flex()
                        .items_center()
                        .gap(u(4.))
                        .text_size(u(11.))
                        .text_color(link)
                        .cursor_pointer()
                        .child(icons::control_sized(Icon::Back, 11. * s))
                        .child("Back")
                        .on_click(cx.listener(|this, _, _, cx| this.retreat(cx))),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(u(PAD))
                    .top(u(if self.has_back() { 65.5 } else { 36.5 }))
                    .w(u(205.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .debug_selector(|| "onboarding-title".into())
                            .w(u(235.))
                            .text_size(u(27.))
                            .line_height(u(28.))
                            .font_weight(gpui::FontWeight::BLACK)
                            .text_color(rgba(INK))
                            .children(
                                self.onboarding_title()
                                    .split('\n')
                                    .map(|line| div().child(line.to_owned())),
                            ),
                    )
                    .child(
                        div()
                            .mt(u(20.))
                            .text_size(u(12.5))
                            .line_height(u(17.5))
                            .text_color(rgba(TEXT))
                            .flex()
                            .flex_col()
                            .gap(u(12.))
                            .children(
                                self.description()
                                    .split("\n\n")
                                    .filter(|t| !t.is_empty())
                                    .map(|t| div().child(t.to_owned())),
                            ),
                    )
                    .when_some(errors, |s, error| {
                        s.child(
                            div()
                                .mt(u(10.))
                                .max_h(u(80.))
                                .overflow_hidden()
                                .text_size(u(10.5))
                                .text_color(rgba(0xd9304f))
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .id("setup-next")
                    .debug_selector(|| "setup-next".into())
                    .absolute()
                    .left(u(PAD))
                    .top(u(if skip { 330. } else { 357.5 }))
                    .w(u(200.))
                    .h(u(30.5))
                    .rounded(u(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(accent)
                    .text_size(u(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(gpui::white().opacity(if enabled { 1. } else { 0.5 }))
                    .when(enabled, |s| {
                        s.cursor_pointer().hover(move |s| s.bg(mix(accent, rgba(0), 0.12)))
                    })
                    .child(next_label)
                    .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
            )
            .when(skip, |s| {
                s.child(
                    div()
                        .id("setup-later")
                        .debug_selector(|| "setup-later".into())
                        .absolute()
                        .left(u(PAD))
                        .top(u(375.))
                        .w(u(200.))
                        .text_center()
                        .text_size(u(11.))
                        .text_color(link)
                        .cursor_pointer()
                        .child("Skip for now")
                        .on_click(cx.listener(|this, _, _, cx| this.later(cx))),
                )
            });

        // Arc's scalloped edge: bumps of the right panel along the split.
        let scallops = (0..((CARD.1 - 10.75) / 13.5) as usize + 1).fold(
            div().absolute().left(u(SPLIT - 3.)).top_0().w(u(6.)).h_full(),
            |edge, i| {
                edge.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(u(i as f32 * 13.5 + 2.75))
                        .w(u(6.))
                        .h(u(8.))
                        .rounded_full()
                        .bg(panel),
                )
            },
        );
        let right = div()
            .id("onboarding-right")
            .absolute()
            .left(u(SPLIT))
            .top_0()
            .right_0()
            .h_full()
            .rounded_r(u(16.))
            .overflow_hidden()
            .bg(panel)
            .child(self.right_panel(s, accent, cx));

        let card = div()
            .id("onboarding-card")
            .debug_selector(|| "onboarding-card".into())
            .absolute()
            .left(px((w - CARD.0 * s) / 2.))
            .top(px((h - CARD.1 * s) / 2.))
            .w(u(CARD.0))
            .h(u(CARD.1))
            .rounded(u(16.))
            .bg(panel)
            .shadow(vec![gpui::BoxShadow {
                color: hsla(0.67, 0.6, 0.15, 0.25),
                offset: point(px(0.), u(10.)),
                blur_radius: u(40.),
                spread_radius: px(0.),
            }])
            .child(right)
            .child(left)
            .child(scallops);

        div()
            .id("local-import-sheet")
            .debug_selector(|| "local-import-sheet".into())
            .key_context("LocalSetup")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .font_family(".SystemUIFont")
            .on_action(cx.listener(|this, _: &Next, _, cx| this.next(cx)))
            .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                if paths.paths().len() == 1 {
                    this.load_password_file(paths.paths()[0].clone(), cx);
                }
            }))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(10.))
                    .bg(rgba(0x4a3ce6))
                    .when_some(field, |b, field| {
                        b.child(
                            img(field)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(gpui::ObjectFit::Fill)
                                .rounded(px(10.)),
                        )
                    }),
            )
            .child(crate::welcome::window_buttons(
                div()
                    .id("setup-window-close")
                    .debug_selector(|| "setup-window-close".into())
                    .on_click(|_, _, cx| crate::quit::request(cx)),
                div()
                    .id("setup-window-minimize")
                    .debug_selector(|| "setup-window-minimize".into())
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(Event::Window(crate::menus::WindowOperation::Minimize))
                    })),
                Some(
                    div()
                        .id("setup-window-zoom")
                        .debug_selector(|| "setup-window-zoom".into())
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(Event::Window(crate::menus::WindowOperation::Zoom))
                        })),
                ),
            ))
            .child(card)
    }

    fn right_panel(&mut self, s: f32, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        match self.step {
            Step::Onboard(SetupStep::Import) => self.import_choices(s, accent, cx),
            Step::Onboard(SetupStep::Favorites) => self.app_grid(s, cx),
            Step::Onboard(SetupStep::Theme) => self.theme_picker(s, cx),
            Step::Passwords => self.password_drop(s, accent, cx),
            Step::Bookmarks => {
                let rows = self
                    .report
                    .clone()
                    .map(|r| self.bookmark_rows(&r.data.bookmarks, "", 0, cx))
                    .unwrap_or_default();
                div()
                    .id("onboarding-bookmarks")
                    .absolute()
                    .left(px(40. * s))
                    .right(px(40. * s))
                    .top(px(50. * s))
                    .bottom(px(50. * s))
                    .p(px(12. * s))
                    .rounded(px(9. * s))
                    .bg(white())
                    .overflow_y_scroll()
                    .text_color(rgba(0x333333))
                    .children(rows)
                    .into_any_element()
            }
            _ => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12. * s))
                .text_color(rgba(TEXT))
                .child(match self.step {
                    Step::Reading => "Bringing everything over…",
                    Step::SafariAccess => "Waiting for Full Disk Access…",
                    _ => "",
                })
                .into_any_element(),
        }
    }

    fn radio(&self, selected: bool, s: f32, accent: Rgba) -> Div {
        div()
            .flex_none()
            .size(px(17.5 * s))
            .rounded_full()
            .border(px(1.5 * s))
            .border_color(if selected { accent } else { rgba(0xbebebe) })
            .flex()
            .items_center()
            .justify_center()
            .when(selected, |c| c.child(div().size(px(9. * s)).rounded_full().bg(accent)))
    }

    fn option_card(&self, id: String, selected: bool, s: f32, accent: Rgba) -> Stateful<Div> {
        let selector = id.clone();
        div()
            .id(gpui::SharedString::from(id))
            .debug_selector(move || selector.clone())
            .flex_none()
            .w(px(215. * s))
            .min_h(px(71.5 * s))
            .rounded(px(9. * s))
            .overflow_hidden()
            .bg(white())
            .border(px(if selected { 2. } else { 1. } * s))
            .border_color(if selected { accent } else { rgba(0xe2e2e8) })
            .shadow(vec![gpui::BoxShadow {
                color: hsla(0.67, 0.4, 0.3, 0.12),
                offset: point(px(0.), px(2. * s)),
                blur_radius: px(6. * s),
                spread_radius: px(0.),
            }])
            .cursor_pointer()
            .flex()
    }

    fn import_choices(&mut self, s: f32, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("onboarding-sources")
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .h(px(377. * s))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(17.5 * s))
            .pt(px(63. * s))
            .pb(px(20. * s));
        if !self.sources_ready {
            list = list.child(
                div().text_size(px(11. * s)).text_color(rgba(TEXT)).child("Looking for browsers…"),
            );
        }
        let label = |text: String| {
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(11. * s))
                .line_height(px(14. * s))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgba(TEXT))
                .child(text)
        };
        for (i, source) in self.sources.iter().enumerate() {
            let selected = self.source == Some(i);
            let tint = self
                .artwork
                .tints
                .get(&source.browser)
                .copied()
                .unwrap_or(rgba(0x8fa3c8));
            list = list.child(
                self.option_card(format!("import-source-{i}"), selected, s, accent)
                    .h(px(71.5 * s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(15. * s))
                            .pl(px(18.5 * s))
                            .child(self.radio(selected, s, accent))
                            .child(label(source.browser.name().to_owned())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(72. * s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(gpui::linear_gradient(
                                160.,
                                gpui::linear_color_stop(mix(white(), tint, 0.22), 0.),
                                gpui::linear_color_stop(mix(white(), tint, 0.4), 1.),
                            ))
                            .child(self.artwork.browsers.get(&source.browser).map_or_else(
                                || icons::ionicon("globe-outline", 30. * s, 1.),
                                |icon| img(icon.clone()).size(px(36. * s)).into_any_element(),
                            )),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.source = Some(i);
                        this.no_import = false;
                        cx.notify();
                    })),
            );
        }
        let none = self.no_import;
        list.child(
            self.option_card("import-none".into(), none, s, accent)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(15. * s))
                        .px(px(18.5 * s))
                        .py(px(14. * s))
                        .child(self.radio(none, s, accent))
                        .child(label("No, I want to type in all my passwords again.".into())),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.no_import = true;
                    this.source = None;
                    cx.notify();
                })),
        )
        .into_any_element()
    }

    /// A browser window with the app grid in its sidebar.
    fn app_grid(&mut self, s: f32, cx: &mut Context<Self>) -> AnyElement {
        let u = move |v: f32| px(v * s);
        let mut sidebar = div()
            .absolute()
            .left(u(106.))
            .top(u(26.))
            .w(u(294.5))
            .bottom_0()
            .rounded_tl(u(15.))
            .bg(white());
        for i in 0..3 {
            sidebar = sidebar.child(
                div()
                    .absolute()
                    .left(u(23.5 + 30.5 * i as f32))
                    .top(u(33.))
                    .size(u(18.))
                    .rounded_full()
                    .bg(rgba(TILE)),
            );
        }
        for i in 0..FAVORITES.len() {
            let selected = self.selected_favorites.contains(&i);
            let tint = rgba(APPS[i].2);
            sidebar = sidebar.child(
                div()
                    .id(("setup-favorite", i))
                    .debug_selector(move || format!("setup-favorite-{i}"))
                    .absolute()
                    .left(u(23.5 + 83. * (i % 3) as f32))
                    .top(u(74. + 83. * (i / 3) as f32))
                    .size(u(73.))
                    .rounded(u(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(selected, |t| {
                        t.bg(gpui::linear_gradient(
                            160.,
                            gpui::linear_color_stop(mix(white(), tint, 0.08), 0.),
                            gpui::linear_color_stop(mix(white(), tint, 0.2), 1.),
                        ))
                        .border(u(1.5))
                        .border_color(mix(white(), tint, 0.5))
                    })
                    .when(!selected, |t| t.bg(rgba(TILE)).hover(|t| t.bg(rgba(0xe8e8e8))))
                    .child(img(app_image(i)).size(u(32.)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.selected_favorites.insert(i) {
                            this.selected_favorites.remove(&i);
                        }
                        cx.emit(Event::Favorite(i));
                        cx.notify();
                    })),
            );
        }
        for top in [337., 390.5] {
            sidebar = sidebar.child(
                div()
                    .absolute()
                    .left(u(23.5))
                    .top(u(top))
                    .w(u(236.))
                    .h(u(42.5))
                    .rounded_full()
                    .bg(rgba(TILE)),
            );
        }
        div()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(u(400.5))
                    .top(u(26.))
                    .right_0()
                    .bottom_0()
                    .bg(rgba(0xf2f2f2)),
            )
            .child(sidebar)
            .into_any_element()
    }

    fn set_theme_color(&mut self, c: ThemeColor, cx: &mut Context<Self>) {
        self.theme.colors[0] = c;
        self.emit_theme(cx);
    }

    fn pick_theme_at(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.theme_field.get() else { return };
        let d = position - bounds.center();
        let radius = f32::from(bounds.size.width) * 0.42;
        let (x, y) = (f32::from(d.x), f32::from(d.y));
        let hue = (y.atan2(x).to_degrees() + 90.).rem_euclid(360.);
        let saturation = (x.hypot(y) / radius).clamp(0., 1.);
        self.set_theme_color(ThemeColor::from_hsv(hue, saturation, 0.95), cx);
    }

    fn set_intensity_at(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.theme_wave.get() else { return };
        let x = f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width).max(1.);
        self.theme.intensity = x.clamp(0., 1.);
        self.emit_theme(cx);
    }

    /// Arc's compact theme picker.
    fn theme_picker(&mut self, s: f32, cx: &mut Context<Self>) -> AnyElement {
        let u = move |v: f32| px(v * s);
        let picked = self.theme_picked;
        let colors = self.theme.colors.clone();
        let field_bounds = self.theme_field.clone();
        let field = div()
            .id("theme-pick-field")
            .debug_selector(|| "theme-pick-field".into())
            .absolute()
            .left(u(5.))
            .top(u(5.))
            .w(u(219.))
            .h(u(219.))
            .rounded(u(8.))
            .overflow_hidden()
            .bg(rgba(0xf0eff1))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| this.pick_theme_at(e.position, cx)),
            )
            .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    this.pick_theme_at(e.position, cx);
                }
            }))
            .child(
                gpui::canvas(
                    move |bounds, _, _| field_bounds.set(Some(bounds)),
                    move |bounds, _, window, _| {
                        let step = 4. * s;
                        let n = (f32::from(bounds.size.width) / step) as usize;
                        for y in 1..n {
                            for x in 1..n {
                                let p = bounds.origin + point(px(x as f32 * step), px(y as f32 * step));
                                window.paint_quad(
                                    gpui::fill(Bounds::new(p, gpui::size(px(1.), px(1.))), hsla(0., 0., 0., 0.12))
                                        .corner_radii(px(0.5)),
                                );
                            }
                        }
                        if picked {
                            let radius = f32::from(bounds.size.width) * 0.42;
                            for (index, c) in colors.iter().enumerate() {
                                let (x, y) = color_offset(*c, radius);
                                let r = if index == 0 { 11. } else { 7. } * s;
                                let center = bounds.center() + point(px(x), px(y));
                                window.paint_quad(gpui::quad(
                                    Bounds::new(center - point(px(r), px(r)), gpui::size(px(2. * r), px(2. * r))),
                                    px(r),
                                    color(*c),
                                    px(2.5 * s),
                                    white(),
                                    gpui::BorderStyle::Solid,
                                ));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(!picked, |f| {
                f.child(
                    div()
                        .absolute()
                        .left_0()
                        .w_full()
                        .top(u(104.))
                        .text_center()
                        .text_size(u(9.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(0x2a2a2a))
                        .child("Click to pick a color"),
                )
            });
        let modes = [ThemeAppearance::Automatic, ThemeAppearance::Light, ThemeAppearance::Dark]
            .into_iter()
            .enumerate()
            .fold(div(), |row, (i, mode)| {
                let name = match mode {
                    ThemeAppearance::Automatic => "automatic",
                    ThemeAppearance::Light => "light",
                    ThemeAppearance::Dark => "dark",
                };
                let current = self.appearance == mode;
                row.child(
                    div()
                        .id(name)
                        .debug_selector(move || format!("setup-mode-{name}"))
                        .absolute()
                        .left(u(78.5 + 25.75 * i as f32))
                        .top(u(20.5))
                        .size(u(20.))
                        .rounded(u(5.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(hsla(0., 0., 0., if current { 0.7 } else { 0.45 }))
                        .when(current, |b| b.bg(hsla(0., 0., 0., 0.08)))
                        .hover(|b| b.bg(hsla(0., 0., 0., 0.06)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.appearance = mode;
                            cx.emit(Event::Appearance(mode));
                            cx.notify();
                        }))
                        .child(mode_icon(mode)),
                )
            });
        let count = self.theme.colors.len();
        let adjust = [(false, "−"), (true, "+")].into_iter().enumerate().fold(div(), |row, (i, (add, glyph))| {
            let enabled = picked && if add { count < 3 } else { count > 1 };
            row.child(
                div()
                    .id(if add { "setup-theme-add" } else { "setup-theme-remove" })
                    .debug_selector(move || (if add { "setup-theme-add" } else { "setup-theme-remove" }).into())
                    .absolute()
                    .left(u(92. + 26. * i as f32))
                    .top(u(196.))
                    .size(u(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(u(13.))
                    .text_color(hsla(0., 0., 0., if enabled { 0.55 } else { 0.2 }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(enabled, |b| {
                        b.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                            if add {
                                // A companion color a third of the wheel away.
                                let (h, sat, v) = this.theme.colors[0].hsv();
                                let n = this.theme.colors.len() as f32;
                                this.theme.colors.push(ThemeColor::from_hsv(h + 120. * n, sat, v));
                            } else {
                                this.theme.colors.pop();
                            }
                            this.emit_theme(cx);
                        }))
                    })
                    .child(glyph),
            )
        });
        let page = self.theme_page;
        let mut palette = div()
            .absolute()
            .left_0()
            .top(u(236.))
            .w_full()
            .h(u(18.))
            .child(
                div()
                    .id("setup-theme-prev")
                    .debug_selector(|| "setup-theme-prev".into())
                    .absolute()
                    .left(u(6.))
                    .top(u(0.))
                    .size(u(18.))
                    .text_color(hsla(0., 0., 0., if page == 0 { 0.2 } else { 0.55 }))
                    .when(page > 0, |b| {
                        b.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                            this.theme_page -= 1;
                            cx.notify();
                        }))
                    })
                    .child(chevron(false)),
            )
            .child(
                div()
                    .id("setup-theme-next")
                    .debug_selector(|| "setup-theme-next".into())
                    .absolute()
                    .left(u(205.5))
                    .top(u(0.))
                    .size(u(18.))
                    .text_color(hsla(0., 0., 0., if page == 3 { 0.2 } else { 0.55 }))
                    .when(page < 3, |b| {
                        b.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                            this.theme_page += 1;
                            cx.notify();
                        }))
                    })
                    .child(chevron(true)),
            );
        for i in 0..9 {
            let c = preset_color(page, i);
            palette = palette.child(
                div()
                    .id(("setup-theme", i))
                    .debug_selector(move || format!("setup-theme-{i}"))
                    .absolute()
                    .left(u(30.5 + 19. * i as f32))
                    .top(u(1.5))
                    .size(u(15.))
                    .rounded_full()
                    .border(u(1.))
                    .border_color(hsla(0., 0., 0., 0.08))
                    .bg(color(c))
                    .cursor_pointer()
                    .hover(|b| b.border_color(hsla(0., 0., 0., 0.3)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_theme_color(c, cx))),
            );
        }
        let wave_bounds = self.theme_wave.clone();
        let intensity = self.theme.intensity;
        let wave = div()
            .id("setup-theme-wave")
            .debug_selector(|| "setup-theme-wave".into())
            .absolute()
            .left(u(11.5))
            .top(u(276.5))
            .w(u(143.5))
            .h(u(30.))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| this.set_intensity_at(e.position, cx)),
            )
            .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    this.set_intensity_at(e.position, cx);
                }
            }))
            .child(
                gpui::canvas(
                    move |bounds, _, _| wave_bounds.set(Some(bounds)),
                    move |bounds, _, window, _| {
                        let width = f32::from(bounds.size.width);
                        let amplitude = (2. + intensity * 7.) * s;
                        let mut path = gpui::PathBuilder::stroke(px(4.5 * s));
                        for i in 0..=(width as usize) {
                            let x = i as f32;
                            let p = bounds.origin
                                + point(px(x), px(15. * s + (x / (20. * s) * std::f32::consts::TAU).sin() * amplitude));
                            if i == 0 { path.move_to(p) } else { path.line_to(p) }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, hsla(0., 0., 0., 0.13));
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(picked, |b| {
                b.child(
                    div()
                        .absolute()
                        .top(u(2.))
                        .left(u(intensity * 133.5))
                        .w(u(10.))
                        .h(u(26.))
                        .rounded(u(5.))
                        .bg(white())
                        .shadow(vec![gpui::BoxShadow {
                            color: hsla(0., 0., 0., 0.25),
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(3. * s),
                            spread_radius: px(0.),
                        }]),
                )
            });
        let noise = self.theme.noise;
        let dial = div()
            .id("setup-theme-grain")
            .debug_selector(|| "setup-theme-grain".into())
            .absolute()
            .left(u(169.))
            .top(u(268.5))
            .size(u(46.))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                // Each click steps the grain around the dial.
                this.theme.noise = ((this.theme.noise * 4.).round() + 1.) % 5. / 4.;
                this.emit_theme(cx);
            }))
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let c = bounds.center();
                        for i in 0..24 {
                            let a = i as f32 / 24. * std::f32::consts::TAU;
                            let lit = (i as f32 / 24.) < noise;
                            let r = 1.3 * s;
                            let p = c + point(px(a.sin() * 21. * s - r), px(-a.cos() * 21. * s - r));
                            window.paint_quad(
                                gpui::fill(Bounds::new(p, gpui::size(px(2. * r), px(2. * r))), hsla(0., 0., 0., if lit { 0.45 } else { 0.18 }))
                                    .corner_radii(px(r)),
                            );
                        }
                        let r = 13. * s;
                        window.paint_quad(gpui::quad(
                            Bounds::new(c - point(px(r), px(r)), gpui::size(px(2. * r), px(2. * r))),
                            px(r),
                            hsla(0., 0., 1., 1.),
                            px(1.),
                            hsla(0., 0., 0., 0.14),
                            gpui::BorderStyle::Solid,
                        ));
                    },
                )
                .absolute()
                .size_full(),
            );
        div()
            .size_full()
            .child(
                div()
                    .id("onboarding-theme-picker")
                    .absolute()
                    .left(u(121.5))
                    .top(u(50.))
                    .w(u(229.))
                    .h(u(327.))
                    .rounded(u(12.))
                    .bg(white())
                    .shadow(vec![gpui::BoxShadow {
                        color: hsla(0.67, 0.3, 0.2, 0.18),
                        offset: point(px(0.), u(4.)),
                        blur_radius: u(14.),
                        spread_radius: px(0.),
                    }])
                    .child(field)
                    .child(modes)
                    .child(adjust)
                    .child(palette)
                    .child(wave)
                    .child(dial),
            )
            .into_any_element()
    }

    fn password_drop(&mut self, s: f32, accent: Rgba, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("import-password-file")
                    .debug_selector(|| "import-password-file".into())
                    .w(px(215. * s))
                    .h(px(120. * s))
                    .rounded(px(12. * s))
                    .bg(white())
                    .border(px(1.5 * s))
                    .border_color(mix(white(), accent, 0.45))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(11. * s))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgba(TEXT))
                    .child(if self.password_file { "Passwords.csv" } else { "Drag or click here to upload" })
                    .on_click(cx.listener(|this, _, _, cx| this.choose_passwords(cx))),
            )
            .into_any_element()
    }
}
