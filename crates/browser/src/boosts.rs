//! Profile-owned Boost editor. The floating panel is GPUI; page work stays in engine.
use crate::{
    icons::{self, Icon},
    text_input::{self, TextInput},
    window::{BrowserWindow, Target},
};
use browser_core::{
    boosts::{Boost, BoostId},
    TabId, UserProfileId,
};
use gpui::{
    actions, div, point, prelude::*, px, rgb, size, AnyElement, App, Bounds, Context, Entity,
    Focusable, KeyBinding, MouseButton, Subscription, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::{cell::Cell, rc::Rc};

mod advanced;
mod picker_bar;
const HEIGHT: f32 = 590.;
const SIMPLE_WIDTH: f32 = 184.;
const CODE_WIDTH: f32 = 480.;
const FONTS: [&str; 20] = [
    ".SystemUIFont",
    "Helvetica Neue",
    "Avenir",
    "Futura",
    "DIN Alternate",
    "PT Mono",
    "Courier",
    "Times New Roman",
    "Charter",
    "Baskerville",
    "Hoefler Text",
    "Palatino",
    "Chalkboard",
    "Comic Sans MS",
    "SignPainter",
    "Snell Roundhand",
    "Papyrus",
    "Apple Chancery",
    "Luminari",
    "Wingdings",
];
actions!(boosts, [Dismiss, UndoZap]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("BoostEditor")),
        KeyBinding::new("cmd-w", Dismiss, Some("BoostEditor")),
        KeyBinding::new("cmd-z", UndoZap, Some("BoostEditor")),
    ]);
}

pub(crate) struct BoostEditor {
    browser: WeakEntity<BrowserWindow>,
    page_browser: WeakEntity<BrowserWindow>,
    pub profile: UserProfileId,
    pub id: BoostId,
    pub tab: TabId,
    pub picking: bool,
    code: bool,
    javascript: bool,
    menu: bool,
    advanced: Option<WindowHandle<advanced::Advanced>>,
    pub(crate) picker_bar: Option<WindowHandle<picker_bar::PickerBar>>,
    css: Entity<TextInput>,
    js: Entity<TextInput>,
    focus: gpui::FocusHandle,
    color_bounds: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
    /// Which color dot the current press moves.
    wheel_grip: Option<WheelGrip>,
    _subscriptions: Vec<Subscription>,
}
/// The main dot sets the page color and carries the accent; the accent dot
/// turns around it on its own and sets the text color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WheelGrip { Main, Accent }

/// Marker for GPUI's drag tracking; moves outside the pad keep steering.
#[derive(Clone)]
struct WheelDrag;
impl Render for WheelDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { div() }
}

/// Pad point (0..=1) under a window position.
fn pad_point(bounds: Bounds<gpui::Pixels>, position: gpui::Point<gpui::Pixels>) -> (f32, f32) {
    let inset = (148. - browser_core::boosts::PAD_EXTENT) / 2.;
    let local = position - bounds.origin;
    ((f32::from(local.x) - inset) / browser_core::boosts::PAD_EXTENT, (f32::from(local.y) - inset) / browser_core::boosts::PAD_EXTENT)
}

fn dot_color(point: (f32, f32)) -> gpui::Hsla {
    let (hue, strength) = browser_core::boosts::pad_color(point);
    gpui::hsla(hue / 360., 0.25 + 0.6 * strength, 0.5, 1.)
}

impl BoostEditor {
    fn grab_wheel(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.color_bounds.get() else { return; };
        let point = pad_point(bounds, position);
        let accent = self.boost(cx).and_then(|boost| boost.appearance.accent_point());
        let reach = (browser_core::boosts::ACCENT_DOT_RADIUS + 2.) / browser_core::boosts::PAD_EXTENT;
        let grip = if accent.is_some_and(|(x, y)| (x - point.0).hypot(y - point.1) <= reach) { WheelGrip::Accent } else { WheelGrip::Main };
        self.wheel_grip = Some(grip);
        self.steer_wheel(position, cx);
    }

    fn steer_wheel(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let (Some(bounds), Some(grip)) = (self.color_bounds.get(), self.wheel_grip) else { return; };
        let point = pad_point(bounds, position);
        self.edit(cx, |boost| match grip {
            WheelGrip::Main => boost.appearance.set_main_point(point),
            WheelGrip::Accent => boost.appearance.turn_accent_toward(point),
        });
    }
}

impl BrowserWindow {
    pub(crate) fn new_boost(&mut self, cx: &mut Context<Self>) {
        let Some((tab, url)) = self
            .browser
            .active_tab()
            .map(|tab| (tab.id, tab.url.clone()))
            .filter(|(_, url)| url.starts_with("http://") || url.starts_with("https://"))
        else {
            self.boost_creation_pending = true;
            let owner = cx.entity().downgrade();
            let main = self.main_window;
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.open_command_bar(Target::NewTab, "", window, cx)
                    });
                });
            });
            return;
        };
        self.close_command_bar(cx);
        let profile = self.browser.active_user_profile_id();
        let Some(id) =
            self.edit_browser(|browser| browser.user_profile_mut(profile)?.boosts.create(&url))
        else {
            return;
        };
        self.open_boost_editor(profile, id, tab, cx);
    }
    pub(crate) fn customize_page(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.browser.active_tab() else {
            self.new_boost(cx);
            return;
        };
        let found = self
            .browser
            .active_user_profile()
            .boosts
            .iter()
            .find(|boost| {
                boost.domain()
                    == url::Url::parse(&tab.url)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_owned))
                        .as_deref()
                        .unwrap_or("")
            })
            .map(|boost| boost.id);
        if let Some(id) = found {
            self.open_boost_editor(self.browser.active_user_profile_id(), id, tab.id, cx);
        } else {
            self.new_boost(cx);
        }
    }
    pub(crate) fn sync_boosts(&mut self, cx: &mut Context<Self>) {
        if let Some(pages) = &self.pages {
            pages
                .borrow_mut()
                .sync(&self.browser, self.page_content_visible());
        }
        cx.notify();
    }
    pub(crate) fn open_boost_editor(
        &mut self,
        profile: UserProfileId,
        id: BoostId,
        tab: TabId,
        cx: &mut Context<Self>,
    ) {
        self.open_boost_editor_on(profile, id, tab, cx.weak_entity(), cx);
    }
    pub(crate) fn open_boost_editor_on(
        &mut self,
        profile: UserProfileId,
        id: BoostId,
        tab: TabId,
        page_browser: WeakEntity<BrowserWindow>,
        cx: &mut Context<Self>,
    ) {
        self.close_site_controls(cx);
        self.close_new_menu(cx);
        self.close_command_bar(cx);
        if let Some(old) = self.boost_editor.take() {
            if let Ok((tab, page)) = old.update(cx, |editor, window, cx| {
                editor.close_advanced(cx);
                editor.picking = false;
                window.remove_window();
                (editor.tab, editor.page_browser.clone())
            }) {
                if page.entity_id() == cx.entity().entity_id() {
                    if let Some(pages) = &self.pages {
                        pages.borrow_mut().set_boost_picker(tab, false);
                    }
                } else {
                    let _ = page.update(cx, |page, _| {
                        if let Some(pages) = &page.pages {
                            pages.borrow_mut().set_boost_picker(tab, false);
                        }
                    });
                }
            }
        }
        self.sync_boosts(cx);
        let owner = cx.entity().downgrade();
        let main = self.main_window;
        let parent = if page_browser.entity_id() == cx.entity().entity_id() {
            self.native_view
        } else {
            page_browser
                .read_with(cx, |page, _| page.native_view)
                .ok()
                .flatten()
        };
        cx.defer(move |cx| {
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else {
                return;
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    bounds.origin
                        + point(
                            (bounds.size.width - px(SIMPLE_WIDTH + 24.)).max(px(0.)),
                            px(80.),
                        ),
                    size(px(SIMPLE_WIDTH), px(HEIGHT)),
                ))),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let child_view = Rc::new(Cell::new(None));
            let opened_view = child_view.clone();
            if let Ok(handle) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    opened_view.set(Some((parent, child)));
                }
                cx.new(|cx| {
                    BoostEditor::new(
                        owner.clone(),
                        page_browser.clone(),
                        profile,
                        id,
                        tab,
                        window,
                        cx,
                    )
                })
            }) {
                let _ = page_browser.update(cx, |page, _| page.boost_editor = Some(handle));
                let _ = owner.update(cx, |this, cx| {
                    this.boost_editor = Some(handle);
                    this.boost_editor_native = child_view.get().map(|(parent, child)| (parent as usize, child as usize, true));
                    this.boost_editor_shown = true;
                    cx.notify();
                });
            }
        });
    }
    /// Arc hides the open editor while the app is in the background or
    /// another page is selected, and brings it back on return until closed.
    pub(crate) fn sync_boost_editor_visibility(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.boost_editor.and_then(|handle| handle.read(cx).ok()) else { return; };
        let (tab, page) = (editor.tab, editor.page_browser.entity_id());
        // A Peek or other host owns its own selection; only this window's is checked.
        let on_page = page != cx.entity().entity_id()
            || self.browser.space().active_tab.is_some_and(|active| self.browser.related_tabs(active).contains(&tab));
        let wanted = cx.active_window().is_some() && on_page && self.page_content_visible();
        if wanted == self.boost_editor_shown { return; }
        self.boost_editor_shown = wanted;
        if let Some((parent, child, _)) = self.boost_editor_native {
            let (parent, child) = (parent as *mut std::ffi::c_void, child as *mut std::ffi::c_void);
            // Ordering a child out detaches it from its parent; attach it again first.
            if wanted { crate::native::attach_overlay(parent, child, false); }
            crate::native::set_overlay_visible(parent, child, wanted);
        }
    }

    pub(crate) fn site_boosts(&self) -> Vec<Boost> {
        let Some(tab) = self.browser.active_tab() else {
            return Vec::new();
        };
        let domain = url::Url::parse(&tab.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned));
        self.browser
            .active_user_profile()
            .boosts
            .iter()
            .filter(|boost| domain.as_deref() == Some(boost.domain()))
            .cloned()
            .collect()
    }
}
impl BoostEditor {
    fn new(
        browser: WeakEntity<BrowserWindow>,
        page_browser: WeakEntity<BrowserWindow>,
        profile: UserProfileId,
        id: BoostId,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let boost = browser
            .read_with(cx, |browser, _| {
                browser
                    .browser
                    .user_profile(profile)?
                    .boosts
                    .iter()
                    .find(|boost| boost.id == id)
                    .cloned()
            })
            .ok()
            .flatten();
        let css = cx.new(TextInput::code);
        let js = cx.new(TextInput::code);
        if let Some(boost) = boost {
            css.update(cx, |input, cx| input.set_text(boost.css, cx));
            js.update(cx, |input, cx| input.set_text(boost.javascript, cx));
        }
        let focus = cx.focus_handle();
        focus.focus(window);
        let mut subscriptions = vec![
            cx.subscribe(&css, |this, input, _: &text_input::Changed, cx| {
                let value = input.read(cx).text().to_string();
                this.edit(cx, |boost| boost.css = value);
            }),
            cx.subscribe(&js, |this, input, _: &text_input::Changed, cx| {
                let value = input.read(cx).text().to_string();
                this.edit(cx, |boost| boost.javascript = value);
            }),
        ];
        if let Some(owner) = browser.upgrade() {
            subscriptions.push(cx.observe(&owner, |_, _, cx| cx.notify()));
        }
        // Leaving the app from the editor itself must hide it too.
        subscriptions.push(cx.observe_window_activation(window, |editor: &mut BoostEditor, _, cx| {
            let owner = editor.browser.clone();
            cx.defer(move |cx| { let _ = owner.update(cx, |owner, cx| owner.sync_boost_editor_visibility(cx)); });
        }));
        subscriptions.push(cx.on_release(|editor, cx| {
            if let Some(handle) = editor.picker_bar.take() {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            if let Some(window) = editor.advanced.take() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        }));
        Self {
            browser,
            page_browser,
            profile,
            id,
            tab,
            picking: false,
            code: false,
            javascript: false,
            menu: false,
            advanced: None,
            picker_bar: None,
            css,
            js,
            focus,
            color_bounds: Rc::new(Cell::new(None)),
            wheel_grip: None,
            _subscriptions: subscriptions,
        }
    }
    fn boost(&self, cx: &App) -> Option<Boost> {
        self.browser
            .read_with(cx, |browser, _| {
                browser
                    .browser
                    .user_profile(self.profile)?
                    .boosts
                    .iter()
                    .find(|boost| boost.id == self.id)
                    .cloned()
            })
            .ok()
            .flatten()
    }
    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Boost)) {
        let profile = self.profile;
        let id = self.id;
        let registry = self
            .browser
            .update(cx, |browser, cx| {
                browser.edit_browser(|model| {
                    if let Some(boost) = model
                        .user_profile_mut(profile)
                        .and_then(|profile| profile.boosts.get_mut(id))
                    {
                        edit(boost);
                    }
                });
                browser.sync_boosts(cx);
                browser
                    .browser
                    .user_profile(profile)
                    .map(|profile| profile.boosts.clone())
            })
            .ok()
            .flatten();
        if self.page_browser.entity_id() != self.browser.entity_id() {
            if let Some(registry) = registry {
                let _ = self.page_browser.update(cx, |page, cx| {
                    page.edit_browser(|browser| {
                        if let Some(profile) = browser.user_profile_mut(profile) {
                            profile.boosts = registry;
                        }
                    });
                    page.sync_boosts(cx);
                });
            }
        }
        cx.notify();
    }
    pub(crate) fn stop_picker(&mut self, cx: &mut Context<Self>) {
        self.picking = false;
        self.picker_engine(false, cx);
    }
    fn picker_engine(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.picker_bar(enabled, cx);
        let _ = self.page_browser.update(cx, |browser, _| {
            if let Some(pages) = &browser.pages {
                pages.borrow_mut().set_boost_picker(self.tab, enabled);
            }
        });
        if enabled {
            let owner = self.page_browser.clone();
            let main = owner.read_with(cx, |browser, _| browser.main_window).ok();
            cx.defer(move |cx| {
                if let Some(main) = main {
                    let _ = main.update(cx, |_, window, _| window.activate_window());
                }
                let _ = owner.update(cx, |browser, _| {
                    if let Some(pages) = &browser.pages {
                        pages.borrow_mut().focus_shown();
                    }
                });
            });
        }
    }
    pub(crate) fn receive_zap(
        &mut self,
        source: gpui::WindowId,
        profile: UserProfileId,
        tab: TabId,
        selector: String,
        cx: &mut Context<Self>,
    ) {
        if !self.picking
            || profile != self.profile
            || tab != self.tab
            || self
                .page_browser
                .read_with(cx, |page, _| page.main_window.window_id())
                .ok()
                != Some(source)
        {
            return;
        }
        if selector == "\u{1b}" {
            self.stop_picker(cx);
            cx.notify();
        } else if selector.is_empty() {
            self.undo_zap(cx);
        } else {
            self.edit(cx, |boost| {
                if !boost.zap_selectors.contains(&selector) {
                    boost.zap_selectors.push(selector);
                }
            });
        }
    }
    pub(crate) fn undo_last_zap(&mut self, cx: &mut Context<Self>) {
        self.undo_zap(cx);
    }
    fn undo_zap(&mut self, cx: &mut Context<Self>) {
        self.edit(cx, |boost| {
            boost.zap_selectors.pop();
        });
    }
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_picker(cx);
        self.close_advanced(cx);
        let _ = self.browser.update(cx, |browser, cx| {
            browser.boost_editor = None;
            cx.notify();
        });
        if self.page_browser.entity_id() != self.browser.entity_id() {
            let _ = self
                .page_browser
                .update(cx, |page, _| page.boost_editor = None);
        }
        window.remove_window();
    }
    fn code_mode(&mut self, code: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_picker(cx);
        self.close_advanced(cx);
        self.code = code;
        let bounds = window.bounds();
        let width = px(if code { CODE_WIDTH } else { SIMPLE_WIDTH });
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(bounds.right() - width, bounds.top()),
                size(width, px(HEIGHT)),
            ))),
            titlebar: None,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let editor = cx.entity();
        let owner = self.browser.clone();
        let page_browser = self.page_browser.clone();
        let parent = page_browser
            .read_with(cx, |browser, _| browser.native_view)
            .ok()
            .flatten();
        let old = window.window_handle();
        let focus = if code {
            if self.javascript {
                self.js.read(cx).focus_handle(cx)
            } else {
                self.css.read(cx).focus_handle(cx)
            }
        } else {
            self.focus.clone()
        };
        // Opening a root that reuses this entity must happen after its handler
        // releases the entity borrow. Keep the right edge fixed, as Arc does.
        cx.defer(move |cx| {
            let current = owner
                .read_with(cx, |browser, _| {
                    browser
                        .boost_editor
                        .map(Into::<gpui::AnyWindowHandle>::into)
                        == Some(old)
                })
                .unwrap_or(false);
            if !current {
                return;
            }
            if let Ok(handle) = cx.open_window(options, move |window, _| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                focus.focus(window);
                editor
            }) {
                let _ = page_browser.update(cx, |page, _| page.boost_editor = Some(handle));
                let _ = owner.update(cx, |browser, cx| {
                    browser.boost_editor = Some(handle);
                    cx.notify();
                });
                let _ = old.update(cx, |_, window, _| window.remove_window());
            }
        });
        cx.notify();
    }
    fn control(id: &'static str) -> gpui::Stateful<gpui::Div> {
        Self::control_hover(id, 0xdedee1)
    }
    fn control_hover(id: &'static str, hover: u32) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .rounded(px(8.))
            .text_size(px(13.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .bg(rgb(0xebebed))
            .hover(move |style| style.bg(rgb(hover)))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
    }
    fn braces() -> AnyElement {
        gpui::canvas(
            |_, _, _| (),
            |bounds, _, window, _| {
                for flip in [false, true] {
                    let at = |x: f32, y: f32| {
                        bounds.origin + point(px(if flip { 18. - x } else { x }), px(y))
                    };
                    let mut path = gpui::PathBuilder::stroke(px(1.4));
                    path.move_to(at(7., 2.));
                    for (x, y) in [
                        (5., 2.),
                        (4., 3.),
                        (4., 6.),
                        (2.5, 8.),
                        (4., 10.),
                        (4., 13.),
                        (5., 14.),
                        (7., 14.),
                    ] {
                        path.line_to(at(x, y));
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, window.text_style().color);
                    }
                }
            },
        )
        .size(px(18.))
        .into_any_element()
    }
    fn prohibition() -> AnyElement {
        gpui::canvas(
            |_, _, _| (),
            |bounds, _, window, _| {
                let mut path = gpui::PathBuilder::stroke(px(1.3));
                for index in 0..=32 {
                    let angle = index as f32 * std::f32::consts::TAU / 32.;
                    let at =
                        bounds.origin + point(px(9. + 6. * angle.cos()), px(9. + 6. * angle.sin()));
                    if index == 0 {
                        path.move_to(at);
                    } else {
                        path.line_to(at);
                    }
                }
                path.move_to(bounds.origin + point(px(4.8), px(4.8)));
                path.line_to(bounds.origin + point(px(13.2), px(13.2)));
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(0xb4b4b8));
                }
            },
        )
        .size(px(18.))
        .into_any_element()
    }
    fn draw_wheel(
        appearance: &browser_core::boosts::BoostAppearance,
        measured: Rc<Cell<Option<Bounds<gpui::Pixels>>>>,
    ) -> AnyElement {
        let tint = appearance.tint;
        let inset = (148. - browser_core::boosts::PAD_EXTENT) / 2.;
        // Untinted Boosts show the neutral dots in Arc's resting spots.
        let dots = match (appearance.main_point(), appearance.accent_point()) {
            (Some(main), Some(accent)) => [
                (inset + main.0 * browser_core::boosts::PAD_EXTENT, inset + main.1 * browser_core::boosts::PAD_EXTENT, browser_core::boosts::MAIN_DOT_RADIUS, dot_color(main)),
                (inset + accent.0 * browser_core::boosts::PAD_EXTENT, inset + accent.1 * browser_core::boosts::PAD_EXTENT, browser_core::boosts::ACCENT_DOT_RADIUS, dot_color(accent)),
            ],
            _ => [(113., 73., 17., rgb(0x6f6f6f).into()), (101., 102., 11., rgb(0x343434).into())],
        };
        gpui::canvas(
            move |bounds, _, _| {
                measured.set(Some(bounds));
            },
            move |bounds, _, window, _| {
                window.paint_shadows(
                    bounds,
                    px(17.).into(),
                    &[gpui::BoxShadow {
                        color: gpui::hsla(0., 0., 0., 0.18),
                        offset: point(px(0.), px(3.)),
                        blur_radius: px(6.),
                        spread_radius: px(0.),
                    }],
                );
                let mut outer = gpui::fill(
                    bounds,
                    gpui::linear_gradient(
                        145.,
                        gpui::linear_color_stop(rgb(0x303030), 0.),
                        gpui::linear_color_stop(rgb(0xe5e5e5), 1.),
                    ),
                );
                outer.corner_radii = px(17.).into();
                window.paint_quad(outer);
                // The neutral frame brightens down the left edge and remains
                // darker on the right; a single diagonal gradient loses that shape.
                for row in 1..147 {
                    let y = row as f32 + 0.5;
                    let corner = if y < 17. {
                        17. - y
                    } else if y > 131. {
                        y - 131.
                    } else {
                        0.
                    };
                    let inset = (17. - (17_f32.powi(2) - corner.powi(2)).max(0.).sqrt())
                        .ceil()
                        .max(1.);
                    let shade = |x: f32| {
                        let t = y / 148.;
                        let left = 40. + 190. * (t * 1.6).min(1.);
                        let right = 92. + 60. * t + 20. * (-y / 12.).exp();
                        let mix = (x / 148.).powf(1. + t);
                        let value = (left
                            + (right - left) * mix
                            + 20. * (1. - (2. * x / 148. - 1.).powi(2)) * t.powi(4))
                        .clamp(0., 255.) as u32;
                        rgb(value * 0x010101)
                    };
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            bounds.origin + point(px(inset), px(row as f32)),
                            size(px(148. - 2. * inset), px(1.)),
                        ),
                        gpui::linear_gradient(
                            90.,
                            gpui::linear_color_stop(shade(inset), 0.),
                            gpui::linear_color_stop(shade(148. - inset), 1.),
                        ),
                    ));
                }
                let mut inner = gpui::fill(
                    Bounds::new(
                        bounds.origin + point(px(7.), px(7.)),
                        bounds.size - size(px(14.), px(14.)),
                    ),
                    tint.map(|hue| {
                        gpui::solid_background(gpui::hsla(hue as f32 / 360., 0.3, 0.96, 1.))
                    })
                    .unwrap_or_else(|| {
                        gpui::linear_gradient(
                            180.,
                            gpui::linear_color_stop(rgb(0xf5f5f5), 0.),
                            gpui::linear_color_stop(rgb(0xffffff), 1.),
                        )
                    }),
                );
                inner.corner_radii = px(12.).into();
                window.paint_quad(inner);
                for y in (11..138).step_by(4) {
                    for x in (11..138).step_by(4) {
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                bounds.origin + point(px(x as f32), px(y as f32)),
                                size(px(0.8), px(0.8)),
                            ),
                            gpui::hsla(0., 0., 0., 0.12),
                        ));
                    }
                }
                // The accent dot is drawn last, above the main dot it orbits.
                for (x, y, r, color) in dots {
                    let at = bounds.origin + point(px(x - r), px(y - r));
                    let circle = Bounds::new(at, size(px(r * 2.), px(r * 2.)));
                    window.paint_shadows(
                        circle,
                        px(r).into(),
                        &[gpui::BoxShadow {
                            color: gpui::hsla(0., 0., 0., 0.35),
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(3.),
                            spread_radius: px(0.),
                        }],
                    );
                    let mut edge = gpui::fill(circle, rgb(0xffffff));
                    edge.corner_radii = px(r).into();
                    window.paint_quad(edge);
                    let mut dot = gpui::fill(
                        Bounds::new(
                            at + point(px(2.), px(2.)),
                            size(px(r * 2. - 4.), px(r * 2. - 4.)),
                        ),
                        color,
                    );
                    dot.corner_radii = px(r - 2.).into();
                    window.paint_quad(dot);
                }
            },
        )
        .size(px(148.))
        .into_any_element()
    }
}
impl Render for BoostEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let boost = self.boost(cx);
        let appearance = boost
            .as_ref()
            .map(|boost| boost.appearance.clone())
            .unwrap_or_default();
        let mut root = div()
            .id("boost-editor")
            .debug_selector(|| "boost-editor".into())
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(0xa7a7a7))
            .overflow_hidden()
            .bg(rgb(0xfdfdff))
            .text_color(rgb(0x333333))
            .text_size(px(12.))
            .font_family(".SystemUIFont")
            .key_context("BoostEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &UndoZap, _, cx| this.undo_zap(cx)));
        if self.code {
            let tabs = div()
                .flex()
                .w(px(100.))
                .h(px(24.))
                .rounded(px(5.))
                .bg(rgb(0xe4e4e4))
                .children(
                    [(false, "CSS"), (true, "JS")]
                        .into_iter()
                        .map(|(js, label)| {
                            Self::control_hover(
                                if js { "boost-tab-js" } else { "boost-tab-css" },
                                if self.javascript == js {
                                    0xc9c9c9
                                } else {
                                    0xdedee1
                                },
                            )
                            .w(px(50.))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .h_full()
                            .rounded(px(5.))
                            .when(self.javascript == js, |el| el.bg(rgb(0xc9c9c9)))
                            .child(label)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.javascript = js;
                                    let input = if js { &this.js } else { &this.css };
                                    input.read(cx).focus_handle(cx).focus(window);
                                    cx.notify();
                                },
                            ))
                        }),
                );
            root = root
                .child(
                    div()
                        .h(px(40.))
                        .bg(rgb(0xf7f7f8))
                        .flex_none()
                        .relative()
                        .flex()
                        .items_center()
                        .px(px(12.))
                        .border_b_1()
                        .border_color(rgb(0xe9e9e9))
                        .child(
                            div()
                                .id("boost-code-back")
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .debug_selector(|| "boost-code-back".into())
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .cursor_pointer()
                                .child(icons::control_sized(Icon::Back, 12.))
                                .child("Back")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.code_mode(false, window, cx)
                                })),
                        )
                        .child(div().absolute().left(px(190.)).child(tabs)),
                )
                .child(
                    div()
                        .id("boost-code-scroll")
                        .bg(rgb(0xf8f9fa))
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(px(12.))
                        .font_family("Menlo")
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .child(if self.javascript {
                            self.js.clone()
                        } else {
                            self.css.clone()
                        }),
                )
                .child(
                    div()
                        .h(px(68.))
                        .bg(rgb(0xffffff))
                        .flex_none()
                        .px(px(14.))
                        .border_t_1()
                        .border_color(rgb(0xebebeb))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(
                                    Self::control("boost-code-picker")
                                        .size(px(38.))
                                        .child(icons::ionicon("eyedrop-outline", 18., 1.))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.picking = !this.picking;
                                            this.picker_engine(this.picking, cx);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Self::control("boost-code-devtools")
                                        .size(px(38.))
                                        .child(icons::ionicon("hammer-outline", 18., 1.))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let _ = this.page_browser.update(cx, |page, cx| {
                                                page.run_devtools(
                                                    crate::devtools::Command::Toggle,
                                                    cx,
                                                )
                                            });
                                        })),
                                ),
                        )
                        .when(self.javascript, |el| {
                            el.child(
                                Self::control("boost-refresh")
                                    .px(px(10.))
                                    .h(px(30.))
                                    .child("Refresh to Run")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let _ = this.page_browser.update(cx, |browser, _| {
                                            if let Some(pages) = &browser.pages {
                                                pages.borrow_mut().reload(this.tab);
                                            }
                                        });
                                    })),
                            )
                        }),
                );
        } else {
            root = root
                .child(
                    div()
                        .h(px(40.))
                        .bg(rgb(0xf6f6f8))
                        .flex_none()
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(rgb(0xe9e9e9))
                        .window_control_area(gpui::WindowControlArea::Drag)
                        .child(
                            div()
                                .id("boost-close")
                                .debug_selector(|| "boost-close".into())
                                .cursor_pointer()
                                .text_color(rgb(0xaaaaaa))
                                .child(icons::ionicon("close-circle", 18., 1.))
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.dismiss(window, cx)),
                                ),
                        )
                        .child(
                            div()
                                .id("boost-name")
                                .debug_selector(|| "boost-name".into())
                                .flex()
                                .gap(px(4.))
                                .items_center()
                                .cursor_pointer()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_size(px(13.))
                                .child(
                                    boost
                                        .as_ref()
                                        .map(|boost| boost.name.clone())
                                        .unwrap_or_default(),
                                )
                                .child(icons::chevron(false))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.menu = !this.menu;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("boost-reset")
                                .cursor_pointer()
                                .text_color(rgb(0xaaaaaa))
                                .child(icons::ionicon("reload-outline", 16., 1.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.edit(cx, |boost| boost.appearance = Default::default())
                                })),
                        ),
                )
                .child(
                    div()
                        .p(px(17.))
                        .pl(px(18.))
                        .pr(px(16.))
                        .pt(px(18.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id("boost-color")
                                .debug_selector(|| "boost-color".into())
                                .size(px(148.))
                                .cursor_pointer()
                                .child(Self::draw_wheel(&appearance, self.color_bounds.clone()))
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| this.grab_wheel(event.position, cx)))
                                .on_drag(WheelDrag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                                .on_drag_move::<WheelDrag>(cx.listener(|this, event: &gpui::DragMoveEvent<WheelDrag>, _, cx| this.steer_wheel(event.event.position, cx)))
                                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.wheel_grip = None)),
                        )
                        .child(
                            div().mt(px(14.)).flex().gap(px(8.)).children(
                                [
                                    (0, "boost-invert"),
                                    (1, "boost-advanced"),
                                    (2, "boost-color-reset"),
                                ]
                                .into_iter()
                                .map(|(index, id)| {
                                    Self::control_hover(
                                        id,
                                        if index == 1 && self.advanced.is_some() {
                                            0x333333
                                        } else {
                                            0xdedee1
                                        },
                                    )
                                    .w(px(44.))
                                    .h(px(38.))
                                    .when(index == 1 && self.advanced.is_some(), |el| {
                                        el.bg(rgb(0x333333)).text_color(rgb(0xffffff))
                                    })
                                    .child(if index == 2 {
                                        Self::prohibition()
                                    } else {
                                        icons::ionicon(
                                            if index == 0 {
                                                "bulb-outline"
                                            } else {
                                                "options-outline"
                                            },
                                            18.,
                                            1.,
                                        )
                                    })
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            if index == 1 {
                                                this.toggle_advanced(window, cx);
                                                return;
                                            }
                                            this.edit(cx, |boost| {
                                                if index == 0 {
                                                    boost.appearance.inverted =
                                                        !boost.appearance.inverted;
                                                } else if index == 2 {
                                                    boost.appearance.clear_color();
                                                }
                                            })
                                        },
                                    ))
                                }),
                            ),
                        ),
                );
        }
        if !self.code {
            // Body positions are absolute so the small panel retains Arc's measured pitch.
            let mut grid = div()
                .absolute()
                .left(px(18.))
                .top(px(273.))
                .w(px(148.))
                .h(px(122.))
                .p(px(6.))
                .rounded(px(12.))
                .bg(rgb(0xffffff))
                .shadow_sm()
                .flex()
                .flex_col();
            for row in 0..4 {
                let mut line = div().flex().flex_1();
                for column in 0..5 {
                    let index = row * 5 + column;
                    line = line.child(
                        div()
                            .id(gpui::SharedString::from(format!("boost-font-{index}")))
                            .debug_selector(move || format!("boost-font-{index}"))
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.))
                            .text_color(rgb(0x999999))
                            .font_family(FONTS[index])
                            .cursor_pointer()
                            .when(
                                appearance.font.as_deref()
                                    == Some(if index == 0 {
                                        "system-ui"
                                    } else {
                                        FONTS[index]
                                    }),
                                |el| {
                                    el.rounded(px(4.))
                                        .bg(rgb(0xffa029))
                                        .text_color(rgb(0xffffff))
                                },
                            )
                            .child("Aa")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit(cx, |boost| {
                                    boost.appearance.font = Some(
                                        if index == 0 {
                                            "system-ui"
                                        } else {
                                            FONTS[index]
                                        }
                                        .into(),
                                    )
                                })
                            })),
                    );
                }
                grid = grid.child(line);
            }
            root = root
                .child(grid)
                .child(
                    div()
                        .absolute()
                        .left(px(18.))
                        .top(px(408.))
                        .flex()
                        .gap(px(8.))
                        .child(
                            Self::control_hover(
                                "boost-size",
                                if appearance.size != 0 {
                                    0xffa029
                                } else {
                                    0xdedee1
                                },
                            )
                            .when(appearance.size != 0, |el| {
                                el.bg(rgb(0xffa029)).text_color(rgb(0xffffff))
                            })
                            .w(px(70.))
                            .h(px(38.))
                            .child(if appearance.size == 0 {
                                "Size".into()
                            } else {
                                format!("{}%", 100 + i16::from(appearance.size) * 10)
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit(cx, |boost| {
                                    boost.appearance.size = if boost.appearance.size >= 5 {
                                        -2
                                    } else {
                                        boost.appearance.size + 1
                                    }
                                })
                            })),
                        )
                        .child(
                            Self::control_hover(
                                "boost-case",
                                if appearance.case != 0 {
                                    0xffa029
                                } else {
                                    0xdedee1
                                },
                            )
                            .when(appearance.case != 0, |el| {
                                el.bg(rgb(0xffa029)).text_color(rgb(0xffffff))
                            })
                            .w(px(70.))
                            .h(px(38.))
                            .child("Case")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit(cx, |boost| {
                                    boost.appearance.case = (boost.appearance.case + 1) % 4
                                })
                            })),
                        ),
                )
                .child(
                    Self::control_hover(
                        "boost-zap",
                        if self.picking { 0x333333 } else { 0xdedee1 },
                    )
                    .absolute()
                    .left(px(18.))
                    .top(px(461.))
                    .w(px(148.))
                    .h(px(38.))
                    .px(px(12.))
                    .justify_between()
                    .when(self.picking, |el| {
                        el.bg(rgb(0x333333)).text_color(rgb(0xffffff))
                    })
                    .child("Zap")
                    .child(icons::ionicon("flash", 18., 1.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.picking = !this.picking;
                        this.picker_engine(this.picking, cx);
                        cx.notify();
                    })),
                )
                .child(
                    Self::control("boost-code")
                        .absolute()
                        .left(px(18.))
                        .top(px(514.))
                        .w(px(148.))
                        .h(px(38.))
                        .px(px(12.))
                        .justify_between()
                        .child("Code")
                        .child(Self::braces())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.code_mode(true, window, cx)),
                        ),
                );
        }
        if self.menu {
            root = root.child(
                div()
                    .absolute()
                    .top(px(38.))
                    .left(px(25.))
                    .w(px(140.))
                    .p(px(5.))
                    .rounded(px(8.))
                    .bg(rgb(0xffffff))
                    .shadow_md()
                    .child(
                        Self::control("boost-enabled")
                            .w_full()
                            .h(px(30.))
                            .child(if boost.as_ref().is_some_and(|boost| boost.enabled) {
                                "Turn Off"
                            } else {
                                "Turn On"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit(cx, |boost| boost.enabled = !boost.enabled);
                                this.menu = false;
                            })),
                    ),
            );
        }
        root
    }
}
