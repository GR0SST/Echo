//! Space Style Editor, rendered in a child window above native page surfaces.
use crate::window::{BrowserWindow, Overlay, OverlayKind, PopupFocus};
use browser_core::{
    SpaceId,
    themes::{SpaceTheme, ThemeAppearance, ThemeColor},
};
use gpui::{
    AnyElement, App, Bounds, Context, FocusHandle, KeyBinding, MouseButton, Pixels, Point, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, actions,
    canvas, div, hsla, point, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

const CONTEXT: &str = "SpaceThemeEditor";
const WIDTH: f32 = 359.;
const HEIGHT: f32 = 512.;
actions!(theme_editor, [Dismiss]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("cmd-z", crate::text_input::Undo, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-z", crate::text_input::Redo, Some(CONTEXT)),
    ]);
}
#[derive(Clone, Copy)]
enum Drag {
    Color,
    Noise,
    Intensity,
}
pub(crate) struct ThemeEditor {
    pub space: SpaceId,
    pub draft: SpaceTheme,
    original: SpaceTheme,
    original_picked: bool,
    selected: usize,
    pub(crate) picked: bool,
    page: usize,
    position: Point<Pixels>,
    focus: FocusHandle,
    dragging: Option<Drag>,
    color_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    noise_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    intensity_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub overlay: Option<WindowHandle<Overlay>>,
    backdrop: bool,
}
impl ThemeEditor {
    pub(crate) fn noise_marker_visible(&self) -> bool {
        self.picked
    }
}

pub(crate) fn color(c: ThemeColor) -> gpui::Rgba {
    gpui::Rgba {
        r: c.red,
        g: c.green,
        b: c.blue,
        a: 1.,
    }
}
pub(crate) fn chevron(forward: bool) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::stroke(px(1.8));
            for (i, (x, y)) in [
                (if forward { 6. } else { 12. }, 3.),
                (if forward { 12. } else { 6. }, 9.),
                (if forward { 6. } else { 12. }, 15.),
            ]
            .into_iter()
            .enumerate()
            {
                let p = bounds.origin + point(px(x), px(y));
                if i == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .size(px(18.))
    .into_any_element()
}
pub(crate) fn mode_icon(mode: ThemeAppearance) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let ink = window.text_style().color;
            let center = bounds.center();
            match mode {
                ThemeAppearance::Light => {
                    window.paint_quad(
                        gpui::fill(
                            Bounds::new(center - point(px(4.), px(4.)), gpui::size(px(8.), px(8.))),
                            ink,
                        )
                        .corner_radii(px(4.)),
                    );
                    for i in 0..8 {
                        let a = i as f32 * std::f32::consts::TAU / 8.;
                        let mut path = gpui::PathBuilder::stroke(px(1.3));
                        path.move_to(center + point(px(a.cos() * 6.), px(a.sin() * 6.)));
                        path.line_to(center + point(px(a.cos() * 8.), px(a.sin() * 8.)));
                        if let Ok(path) = path.build() {
                            window.paint_path(path, ink);
                        }
                    }
                }
                ThemeAppearance::Dark => {
                    let mut path = gpui::PathBuilder::fill();
                    for i in 0..=24 {
                        let a = 0.2 + i as f32 / 24. * 4.8;
                        let p = center + point(px(a.cos() * 7.), px(a.sin() * 7.));
                        if i == 0 {
                            path.move_to(p);
                        } else {
                            path.line_to(p);
                        }
                    }
                    for i in 0..=24 {
                        let a = 4.7 - i as f32 / 24. * 4.7;
                        path.line_to(center + point(px(3. + a.cos() * 6.), px(-3. + a.sin() * 6.)));
                    }
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, ink);
                    }
                }
                ThemeAppearance::Automatic => {
                    for (x, y, r) in [(7., 11., 5.), (14., 4., 3.), (17., 11., 2.)] {
                        let mut path = gpui::PathBuilder::fill();
                        for (i, (dx, dy)) in [
                            (0., -r),
                            (r * 0.3, -r * 0.3),
                            (r, 0.),
                            (r * 0.3, r * 0.3),
                            (0., r),
                            (-r * 0.3, r * 0.3),
                            (-r, 0.),
                            (-r * 0.3, -r * 0.3),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let p = bounds.origin + point(px(x + dx), px(y + dy));
                            if i == 0 {
                                path.move_to(p);
                            } else {
                                path.line_to(p);
                            }
                        }
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, ink);
                        }
                    }
                }
            }
        },
    )
    .size(px(20.))
    .into_any_element()
}
impl BrowserWindow {
    pub(crate) fn open_theme_editor(
        &mut self,
        space: SpaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_theme_editor_anchored(space, None, window, cx);
    }
    pub(crate) fn open_theme_editor_anchored(
        &mut self, space: SpaceId, bottom: Option<f32>, window: &mut Window, cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() || self.browser.is_private() {
            return;
        }
        let Some(target) = self.browser.spaces.iter().find(|s| s.id == space) else {
            return;
        };
        let draft = target.theme();
        let picked = target.theme.is_some();
        self.close_theme_editor(cx);
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        self.close_site_controls(cx);
        self.close_new_menu(cx);
        self.close_user_profile_dialog(cx);
        let focus = cx.focus_handle();
        let size = window.bounds().size;
        // Main Spaces-menu entry follows the pointer; header menus supply their
        // own bottom anchor. Read the native pointer too when it is over CEF.
        let pointer_y = self.native_view.and_then(crate::native::pointer_position)
            .map_or(f32::from(window.mouse_position().y), |position| position[1]);
        let top = bottom.map_or_else(
            || (pointer_y - 256.).max(-f32::from(window.bounds().origin.y).max(0.)),
            |bottom| bottom - HEIGHT,
        );
        let library_card = self.space_menu_is_in_library().then(||
            self.sidebar_navigation.card_bounds.get(&space).and_then(|bounds| bounds.get())).flatten();
        let desired = library_card.map_or_else(|| point(
            px((self.sidebar_width() + 14.).min((f32::from(size.width) - WIDTH - 8.).max(8.))),
            px(top),
        ), |card| point(card.right() + px(12.), px(pointer_y - HEIGHT / 2.)));
        let host = window.bounds();
        let mut panel = Bounds::new(host.origin + desired, gpui::size(px(WIDTH), px(HEIGHT)));
        if let Some(work) = crate::native::display_work_area(window, cx) {
            // Arc's 9 px popover padding plus 2 px outer screen clearance.
            let inset = px(11.);
            panel.origin.x = panel.origin.x.clamp(work.left() + inset, (work.right() - px(WIDTH) - inset).max(work.left() + inset));
            panel.origin.y = panel.origin.y.clamp(work.top() + inset, (work.bottom() - px(HEIGHT) - inset).max(work.top() + inset));
        }
        let overlay_bounds = host.union(&panel);
        let position = panel.origin - overlay_bounds.origin;
        self.theme_editor = Some(ThemeEditor {
            space,
            original: draft.clone(),
            original_picked: picked,
            draft,
            selected: 0,
            picked,
            page: 0,
            position,
            focus: focus.clone(),
            dragging: None,
            color_bounds: Default::default(),
            noise_bounds: Default::default(),
            intensity_bounds: Default::default(),
            overlay: None,
            backdrop: false,
        });
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.notify();
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.theme_editor.as_ref().is_some_and(|e| e.focus == focus)
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok(current) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            // Dismiss instead of attaching a stale anchor after a window move.
            if current.origin != host.origin { let _ = browser.update(cx, |view, cx| view.close_theme_editor(cx)); return; }
            let bounds = overlay_bounds;
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                focus.focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::ThemeEditor, window, cx))
            }) {
                let _ = browser.update(cx, |this, _| {
                    if let Some(editor) = &mut this.theme_editor {
                        editor.overlay = Some(overlay);
                    }
                });
            }
        });
    }
    pub(crate) fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.close_theme_editor_with_focus(PopupFocus::Restore, cx);
    }
    pub(crate) fn close_theme_editor_with_focus(
        &mut self,
        policy: PopupFocus,
        cx: &mut Context<Self>,
    ) {
        if self.theme_editor.is_none() {
            return;
        }
        let overlay = self.take_theme_editor();
        self.close_popup_overlay(overlay, policy, false, cx);
        cx.notify();
    }
    fn undo_theme_editor(&mut self, redo: bool, cx: &mut Context<Self>) {
        self.commit_theme_editor();
        let label = if redo { self.browser.sidebar_redo_action() } else { self.browser.sidebar_undo_action() };
        if label != Some("Update Theme") { return; }
        let changed = self.edit_profile(|browser| if redo {
            browser.redo_sidebar(std::time::SystemTime::now())
        } else {
            browser.undo_sidebar(std::time::SystemTime::now())
        });
        if changed {
            if let Some(editor) = &mut self.theme_editor {
                if let Some(space) = self.browser.spaces.iter().find(|space| space.id == editor.space) {
                    editor.draft = space.theme();
                    editor.original = editor.draft.clone();
                    editor.picked = space.theme.is_some();
                    editor.original_picked = editor.picked;
                    editor.selected = editor.selected.min(editor.draft.colors.len() - 1);
                    editor.dragging = None;
                }
            }
            cx.notify();
        }
    }
    pub(crate) fn commit_theme_editor(&mut self) {
        let edit = self.theme_editor.as_mut().and_then(|editor| {
            if editor.draft == editor.original && editor.picked == editor.original_picked {
                return None;
            }
            editor.original = editor.draft.clone();
            editor.original_picked = editor.picked;
            Some((editor.space, editor.picked.then(|| editor.draft.clone())))
        });
        if let Some((space, theme)) = edit {
            self.edit_profile(|browser| match theme {
                Some(theme) => browser.update_space_theme(space, theme),
                None => browser.clear_space_theme(space),
            });
        }
    }
    pub(crate) fn take_theme_editor(&mut self) -> Option<WindowHandle<Overlay>> {
        self.commit_theme_editor();
        self.theme_editor.take()?.overlay
    }
    fn theme_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(e) = &mut self.theme_editor else {
            return;
        };
        let mut haptic = false;
        match e.dragging {
            Some(Drag::Color) => {
                if let Some(b) = e.color_bounds.get() {
                    let x = f32::from(position.x - b.center().x);
                    let y = f32::from(position.y - b.center().y);
                    let hue = (y.atan2(x).to_degrees() + 90.).rem_euclid(360.);
                    let saturation = (x.hypot(y) / 130.).clamp(0., 1.);
                    e.draft.colors[e.selected] = ThemeColor::from_hsv(hue, saturation, 0.95);
                    harmonize(&mut e.draft.colors, e.selected);
                    e.picked = true;
                }
            }
            Some(Drag::Intensity) => {
                if let Some(b) = e.intensity_bounds.get() {
                    let value = ((f32::from(position.x - b.left()) - 6.) / 212.).clamp(0., 1.);
                    // Tactile detents do not quantize the continuous preview.
                    haptic = (e.draft.intensity * 50.).round() != (value * 50.).round();
                    e.draft.intensity = value;
                }
            }
            Some(Drag::Noise) => {
                if let Some(b) = e.noise_bounds.get() {
                    let delta = position - b.center();
                    let angle = f32::from(delta.y).atan2(f32::from(delta.x));
                    let value = (angle + std::f32::consts::FRAC_PI_2)
                        .rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
                    // Match the dial's 24 visible ring positions.
                    haptic = (e.draft.noise * 24.).round() != (value * 24.).round();
                    e.draft.noise = value;
                }
            }
            None => {}
        }
        if haptic && crate::feedback::enabled(self, browser_core::settings::Toggle::HapticFeedback, cx) {
            crate::native::reorder_haptic();
        }
        cx.notify();
    }
    pub(crate) fn theme_editor_is_dark(&self) -> bool {
        self.theme_is_dark()
    }
    pub(crate) fn sync_theme_editor_backdrop(&mut self, window: &Window) {
        use crate::material::*;
        let dark = self.theme_editor_is_dark();
        let Some(editor) = &mut self.theme_editor else {
            return;
        };
        editor.backdrop = crate::native::view_of(window).is_some_and(|view| {
            crate::native::set_backdrop_configured(
                view,
                "theme-editor",
                Some([
                    f32::from(editor.position.x) as f64,
                    f32::from(editor.position.y) as f64,
                    WIDTH as f64,
                    HEIGHT as f64,
                ]),
                16.,
                BackdropConfig {
                    appearance: if dark {
                        EffectAppearance::Dark
                    } else {
                        EffectAppearance::Light
                    },
                    state: EffectState::Active,
                    blending: EffectBlending::BehindWindow,
                    emphasized: false,
                    effect_alpha: 1.,
                    material: EffectMaterial::Menu,
                },
            ) == BackdropInstallation::Installed
        });
    }
    pub(crate) fn render_theme_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(e) = &self.theme_editor else {
            return div().into_any_element();
        };
        let dark = self.theme_editor_is_dark();
        let ink = hsla(0., 0., if dark { 1. } else { 0. }, 0.85);
        let field_bounds = e.color_bounds.clone();
        let picked = e.picked;
        let colors = e.draft.colors.clone();
        let previews = colors.clone();
        let selected = e.selected;
        let field = div()
            .id("theme-color-field")
            .debug_selector(|| "theme-color-field".into())
            .relative()
            .w(px(340.))
            .h(px(340.))
            .rounded(px(6.))
            .overflow_hidden()
            .bg(hsla(0., 0., if dark { 1. } else { 0. }, 0.025))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(e) = &mut this.theme_editor {
                        if let Some(b) = e.color_bounds.get() {
                            if let Some(index) = e.draft.colors.iter().position(|c| {
                                let (h, s, _) = c.hsv();
                                let a = (h - 90.).to_radians();
                                let center = b.center()
                                    + point(px(a.cos() * s * 130.), px(a.sin() * s * 130.));
                                let d = event.position - center;
                                f32::from(d.x).hypot(f32::from(d.y)) < 12.
                            }) {
                                e.selected = index;
                            }
                        }
                        e.dragging = Some(Drag::Color);
                    }
                    this.theme_pointer(event.position, cx);
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| {
                        field_bounds.set(Some(bounds));
                    },
                    move |bounds, _, window, _| {
                        // The 400-point reference grid scales into its 340-point field.
                        for y in 1..80 {
                            for x in 1..80 {
                                let center = bounds.origin + point(px(x as f32 * 4.25), px(y as f32 * 4.25));
                                window.paint_quad(gpui::fill(
                                    Bounds::new(center, gpui::size(px(1.), px(1.))),
                                    hsla(0., 0., if dark { 1. } else { 0. }, 0.13),
                                ).corner_radii(px(0.5)));
                            }
                        }
                        if picked {
                            for (index, c) in colors.iter().enumerate() {
                                let (h, s, _) = c.hsv();
                                let a = (h - 90.).to_radians();
                                let center = bounds.center()
                                    + point(px(a.cos() * s * 130.), px(a.sin() * s * 130.));
                                let radius = if index == selected { 24. } else { 14. };
                                window.paint_quad(gpui::quad(
                                    Bounds::new(
                                        center - point(px(radius), px(radius)),
                                        gpui::size(px(radius * 2.), px(radius * 2.)),
                                    ),
                                    px(radius),
                                    color(previews[index]),
                                    px(if index == selected { 5. } else { 2. }),
                                    hsla(0., 0., 1., 0.95),
                                    gpui::BorderStyle::Solid,
                                ));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(!picked, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(160.))
                        .left_0()
                        .w_full()
                        .text_center()
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Tap to pick a color for this space"),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(24.))
                    .left_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .gap(px(8.))
                    .children(
                        [
                            ThemeAppearance::Automatic,
                            ThemeAppearance::Light,
                            ThemeAppearance::Dark,
                        ]
                        .into_iter()
                        .map(|mode| {
                            let name = match mode {
                                ThemeAppearance::Automatic => "automatic",
                                ThemeAppearance::Light => "light",
                                ThemeAppearance::Dark => "dark",
                            };
                            div()
                                .id(name)
                                .debug_selector(move || format!("theme-{name}"))
                                .size(px(32.))
                                .rounded(px(8.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(hsla(0., 0., if dark { 1. } else { 0. }, if self.theme_appearance() == mode { 0.7 } else { 0.5 }))
                                .when(self.theme_appearance() == mode, |this| {
                                    this.bg(hsla(0., 0., if dark { 1. } else { 0. }, 0.10))
                                })
                                .hover(move |s| s.bg(hsla(0., 0., if dark { 1. } else { 0. }, 0.12)))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit_profile(|browser| browser.set_browser_appearance(mode));
                                    cx.notify();
                                }))
                                .child(mode_icon(mode))
                        }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(22.))
                    .left_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .gap(px(20.))
                    .children([false, true].into_iter().map(|add| {
                        let enabled = picked && (!add || e.draft.colors.len() < 3);
                        div()
                            .id(if add {
                                "theme-add-color"
                            } else {
                                "theme-remove-color"
                            })
                            .debug_selector(move || {
                                if add {
                                    "theme-add-color".into()
                                } else {
                                    "theme-remove-color".into()
                                }
                            })
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(ink.opacity(if enabled { 0.65 } else { 0.15 }))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .when(enabled, |this| {
                                this.on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(e) = &mut this.theme_editor {
                                        if add {
                                            e.draft.colors.push(e.draft.colors[e.selected]);
                                            harmonize(&mut e.draft.colors, e.selected);
                                            // Arc keeps the existing large handle selected.
                                        } else if e.draft.colors.len() == 1 {
                                            e.picked = false;
                                        } else {
                                            e.draft.colors.remove(e.selected);
                                            e.selected = e.selected.min(e.draft.colors.len() - 1);
                                            harmonize(&mut e.draft.colors, e.selected);
                                        }
                                    }
                                    this.commit_theme_editor();
                                    cx.notify();
                                }))
                            })
                            .child(if add {
                                crate::icons::control(crate::icons::Icon::Plus)
                            } else {
                                canvas(
                                    |_, _, _| (),
                                    |b, _, w, _| {
                                        w.paint_quad(gpui::fill(
                                            Bounds::new(
                                                b.origin + point(px(4.), px(8.)),
                                                gpui::size(px(10.), px(1.5)),
                                            ),
                                            w.text_style().color,
                                        ));
                                    },
                                )
                                .size(px(18.))
                                .into_any_element()
                            })
                    })),
            );
        let palette = div()
            .h(px(64.))
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.))
            .child(
                div()
                    .id("theme-palette-previous")
                    .debug_selector(|| "theme-palette-previous".into())
                    .size(px(20.))
                    .text_color(ink.opacity(if e.page == 0 { 0.25 } else { 0.7 }))
                    .when(e.page > 0, |this| {
                        this.on_click(cx.listener(|this, _, _, cx| {
                            if let Some(e) = &mut this.theme_editor {
                                e.page -= 1;
                            }
                            cx.notify();
                        }))
                    })
                    .child(chevron(false)),
            )
            .children(
                PALETTES[e.page]
                    .iter()
                    .enumerate()
                    .map(|(index, _)| {
                        let c = preset_color(e.page, index);
                        div()
                            .id(("theme-preset", index))
                            .debug_selector(move || format!("theme-preset-{index}"))
                            .size(px(24.))
                            .rounded_full()
                            .border_2()
                            .border_color(hsla(0., 0., 1., 0.12))
                            .bg(color(c))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(e) = &mut this.theme_editor {
                                    e.draft.colors[e.selected] = c;
                                    harmonize(&mut e.draft.colors, e.selected);
                                    e.picked = true;
                                }
                                this.commit_theme_editor();
                                cx.notify();
                            }))
                    }),
            )
            .child(
                div()
                    .id("theme-palette-next")
                    .debug_selector(|| "theme-palette-next".into())
                    .size(px(20.))
                    .text_color(ink.opacity(if e.page == 3 { 0.25 } else { 0.7 }))
                    .when(e.page < 3, |this| {
                        this.on_click(cx.listener(|this, _, _, cx| {
                            if let Some(e) = &mut this.theme_editor {
                                e.page += 1;
                            }
                            cx.notify();
                        }))
                    })
                    .child(chevron(true)),
            );
        let noise_bounds = e.noise_bounds.clone();
        let intensity_bounds = e.intensity_bounds.clone();
        let noise = e.draft.noise;
        let noise_marker_visible = e.noise_marker_visible();
        let intensity = e.draft.intensity;
        let wave = if picked { intensity } else { 0. };
        let sliders = div()
            .h(px(80.))
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.))
            .child(
                div()
                    .id("theme-intensity")
                    .debug_selector(|| "theme-intensity".into())
                    .relative()
                    .w(px(224.))
                    .h(px(68.))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            // Without a picked color there is no tint to adjust.
                            let Some(e) = this.theme_editor.as_mut().filter(|e| e.picked) else { return };
                            e.dragging = Some(Drag::Intensity);
                            this.theme_pointer(event.position, cx);
                        }),
                    )
                    .child(
                        canvas(
                            move |b, _, _| intensity_bounds.set(Some(b)),
                            move |b, _, w, _| {
                                let mut path = gpui::PathBuilder::stroke(px(6.));
                                for i in 0..=224 {
                                    let p = b.origin
                                        + point(
                                            px(i as f32),
                                            px(34.
                                                + (i as f32 / 32. * std::f32::consts::TAU).sin()
                                                    * (wave * 14.)),
                                        );
                                    if i == 0 {
                                        path.move_to(p);
                                    } else {
                                        path.line_to(p);
                                    }
                                }
                                if let Ok(path) = path.build() {
                                    w.paint_path(path, hsla(0., 0., if dark { 1. } else { 0. }, 0.30));
                                }

                            },
                        )
                        .size_full(),
                    )
                    .when(picked, |this| this.child(div()
                        .debug_selector(|| "theme-intensity-thumb".into())
                        .absolute().top(px(6.)).left(px(intensity * 212. - 6.))
                        .w(px(24.)).h(px(56.)).rounded(px(12.))
                        .bg(hsla(0., 0., 1., 0.95)))),
            )
            .child(
                div()
                    .id("theme-noise")
                    .debug_selector(|| "theme-noise".into())
                    .size(px(80.))
                    .relative()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            let Some(e) = this.theme_editor.as_mut().filter(|e| e.picked) else { return };
                            e.dragging = Some(Drag::Noise);
                            this.theme_pointer(event.position, cx);
                        }),
                    )
                    .child(
                        canvas(
                            move |b, _, _| noise_bounds.set(Some(b)),
                            move |b, _, w, _| {
                                for i in 0..24 {
                                    // Dots run clockwise from noon; those up to the
                                    // current noise value light up.
                                    let a = i as f32 / 24. * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                                    let p = b.center()
                                        + point(px(a.cos() * 32. - 2.), px(a.sin() * 32. - 2.));
                                    let lit = noise_marker_visible && noise > 0. && i as f32 / 24. <= noise;
                                    w.paint_quad(
                                        gpui::fill(
                                            Bounds::new(p, gpui::size(px(4.), px(4.))),
                                            hsla(0., 0., 1., if lit { 0.6 } else { 0.15 }),
                                        )
                                        .corner_radii(px(2.)),
                                    );
                                }
                                w.paint_quad(gpui::quad(
                                    Bounds::new(
                                        b.center() - point(px(20.), px(20.)),
                                        gpui::size(px(40.), px(40.)),
                                    ),
                                    px(20.),
                                    hsla(0., 0., 1., 0.02),
                                    px(1.),
                                    hsla(0., 0., 1., 0.15),
                                    gpui::BorderStyle::Solid,
                                ));
                                if noise_marker_visible {
                                let a = noise * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                                let direction = point(px(a.cos()), px(a.sin()));
                                let start = b.center() + direction * 28.;
                                let end = b.center() + direction * 36.;
                                let mut path = gpui::PathBuilder::stroke(px(8.));
                                path.move_to(start); path.line_to(end);
                                if let Ok(path) = path.build() { w.paint_path(path, hsla(0., 0., 1., 0.95)); }
                                for p in [start, end] {
                                    w.paint_quad(gpui::fill(Bounds::new(p - point(px(4.), px(4.)), gpui::size(px(8.), px(8.))), hsla(0., 0., 1., 0.95)).corner_radii(px(4.)));
                                }
                                }
                            },
                        )
                        .size_full(),
                    )
                    .child(
                        div()
                            .id("theme-texture")
                            .debug_selector(|| "theme-texture".into())
                            .absolute()
                            .left(px(27.))
                            .top(px(27.))
                            .size(px(26.))
                            .rounded_full()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(e) = &mut this.theme_editor {
                                    e.draft.texture = e.draft.texture.next();
                                }
                                this.commit_theme_editor();
                                cx.notify();
                            })),
                    ),
            );
        div()
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&e.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_theme_editor(cx)))
            .on_action(cx.listener(|this, _: &crate::text_input::Undo, _, cx| this.undo_theme_editor(false, cx)))
            .on_action(cx.listener(|this, _: &crate::text_input::Redo, _, cx| this.undo_theme_editor(true, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_theme_editor(cx)),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                this.theme_pointer(event.position, cx)
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    let completed = this.theme_editor.as_mut().is_some_and(|e| e.dragging.take().is_some());
                    if completed { this.commit_theme_editor(); cx.notify(); }
                }),
            )
            .child(
                div()
                    .id("theme-editor")
                    .debug_selector(|| "theme-editor".into())
                    .absolute()
                    .left(e.position.x)
                    .top(e.position.y)
                    .w(px(WIDTH))
                    .h(px(HEIGHT))
                    .rounded(px(16.))
                    .bg(hsla(
                        0.,
                        0.,
                        if dark { 0.20 } else { 0.91 },
                        if e.backdrop { 0.35 } else { 0.98 },
                    ))
                    .child(div().absolute().inset_0().child(self.theme_surface_shadow(16.)))
                    .text_color(ink)
                    .px(px(9.5)).py(px(9.))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(field)
                    .child(palette)
                    .child(sliders),
            )
            .into_any_element()
    }
}

// The installed catalogs use sRGB for Brand 1–4, linear sRGB for Brand 5,
// and Display P3 for the remaining colors. Store only converted sRGB.
pub(crate) fn preset_color(page: usize, index: usize) -> ThemeColor {
    let c = PALETTES[page][index];
    match (page, index) {
        (0, 0..=3) => ThemeColor { red: c[0], green: c[1], blue: c[2] },
        (0, 4) => ThemeColor::from_linear_srgb(c),
        _ => ThemeColor::from_display_p3(c),
    }
}
const PALETTES: [[[f32; 3]; 9]; 4] = [
    [
        [0.950, 0.918, 0.894],
        [0.950, 0.608, 0.733],
        [0.651, 0.447, 0.616],
        [0.949, 0.369, 0.420],
        [1.000, 0.235, 0.097],
        [0.950, 0.840, 0.428],
        [0.451, 0.897, 0.612],
        [0.496, 0.721, 0.838],
        [0.401, 0.404, 0.527],
    ],
    [
        [0.994, 0.984, 0.981],
        [0.985, 0.917, 0.951],
        [0.924, 0.825, 0.918],
        [0.968, 0.804, 0.822],
        [0.980, 0.882, 0.834],
        [0.996, 0.977, 0.906],
        [0.894, 0.988, 0.926],
        [0.880, 0.950, 0.984],
        [0.774, 0.776, 0.888],
    ],
    [
        [0.294, 0.231, 0.345],
        [0.384, 0.220, 0.337],
        [0.522, 0.267, 0.267],
        [0.655, 0.439, 0.282],
        [0.820, 0.706, 0.416],
        [0.804, 0.800, 0.659],
        [0.518, 0.659, 0.522],
        [0.204, 0.376, 0.294],
        [0.176, 0.267, 0.408],
    ],
    [
        [1.000, 1.000, 1.000],
        [0.900, 0.900, 0.900],
        [0.800, 0.800, 0.800],
        [0.700, 0.700, 0.700],
        [0.500, 0.500, 0.500],
        [0.400, 0.400, 0.400],
        [0.200, 0.200, 0.200],
        [0.100, 0.100, 0.100],
        [0.000, 0.000, 0.000],
    ],
];

/// Handle angles relative to the first color. Two colors are complementary;
/// three form Arc's narrow triangle, both extras near the opposite side.
fn harmony_offsets(count: usize) -> &'static [f32] {
    match count {
        2 => &[0., 180.],
        3 => &[0., 153., 207.],
        _ => &[0.],
    }
}

/// Moves every handle with `anchor`: same distance from the center (and
/// brightness), hues kept at the harmony offsets.
pub(crate) fn harmonize(colors: &mut [ThemeColor], anchor: usize) {
    let offsets = harmony_offsets(colors.len());
    if offsets.len() != colors.len() || anchor >= colors.len() {
        return;
    }
    let (h, s, v) = colors[anchor].hsv();
    let base = h - offsets[anchor];
    for (index, (color, offset)) in colors.iter_mut().zip(offsets).enumerate() {
        if index != anchor {
            *color = ThemeColor::from_hsv((base + offset).rem_euclid(360.), s, v);
        }
    }
}

#[cfg(test)]
mod harmony_tests {
    use super::*;
    #[test]
    fn handles_stay_complementary_or_in_a_narrow_triangle() {
        let mut two = vec![ThemeColor::from_hsv(30., 0.6, 0.95); 2];
        harmonize(&mut two, 0);
        assert!((two[1].hsv().0 - 210.).abs() < 0.5 && (two[1].hsv().1 - 0.6).abs() < 0.01);
        let mut three = vec![ThemeColor::from_hsv(20., 0.8, 0.95); 3];
        harmonize(&mut three, 0);
        let hues: Vec<f32> = three.iter().map(|c| c.hsv().0).collect();
        assert!((hues[1] - 173.).abs() < 0.5 && (hues[2] - 227.).abs() < 0.5);
        // Dragging a secondary handle carries the others with it.
        three[1] = ThemeColor::from_hsv(183., 0.8, 0.95);
        harmonize(&mut three, 1);
        assert!((three[0].hsv().0 - 30.).abs() < 0.5 && (three[2].hsv().0 - 237.).abs() < 0.5);
    }
}
