//! Easels (`echo://easel/<id>`), as in Arc: a vertically scrolling canvas
//! over the Space's theme with shapes, arrows, freehand ink, text and images.
//! The toolbar sits at the bottom left; a drawn shape is selected at once and
//! offers its colors above it. Sharing is deliberately absent.
use crate::{text_input::TextInput, window::BrowserWindow};
use browser_core::easels::{
    Color, Content, Easel, EaselId, FontFamily, Frame, Object, ParagraphStyle, Point, ShapeKind, WIDTH,
};
use gpui::{
    actions, canvas, div, hsla, point, prelude::*, px, AnyElement, App, Bounds, Context, Entity, FocusHandle,
    Focusable, Hsla, KeyBinding, MouseButton, MouseDownEvent, Pixels, Window,
};
use std::{cell::Cell, collections::HashMap, rc::Rc};

const CONTEXT: &str = "Easel";
/// Shape outlines, in document points.
const STROKE: f32 = 2.5;
/// Fill opacity of rectangles and circles.
const FILL_OPACITY: f32 = 0.75;
/// A click that barely moves still makes a shape of this size.
const DEFAULT_SHAPE: f32 = 120.;
const DEFAULT_TEXT_WIDTH: f32 = 300.;
/// The title band above the first object, in pixels.
const TITLE_TOP: f32 = 56.;
/// Where objects begin, below the title.
const LAYER_TOP: f32 = TITLE_TOP + 64.;
const SELECTION: u32 = 0x6fb2dc;
/// Room to keep drawing below the lowest object, in pixels.
const TRAILING_SPACE: f32 = 400.;

actions!(easel, [DeleteSelection, Escape]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", DeleteSelection, Some(CONTEXT)),
        KeyBinding::new("delete", DeleteSelection, Some(CONTEXT)),
        KeyBinding::new("escape", Escape, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    Select,
    Image,
    Text,
    Circle,
    Rectangle,
    Arrow,
    Pen,
}
impl Tool {
    const ALL: [Self; 7] = [Self::Select, Self::Image, Self::Text, Self::Circle, Self::Rectangle, Self::Arrow, Self::Pen];
    fn key(self) -> &'static str {
        match self {
            Self::Select => "v",
            Self::Image => "i",
            Self::Text => "t",
            Self::Circle => "o",
            Self::Rectangle => "r",
            Self::Arrow => "a",
            Self::Pen => "p",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::Select => "easel-tool-select",
            Self::Image => "easel-tool-image",
            Self::Text => "easel-tool-text",
            Self::Circle => "easel-tool-circle",
            Self::Rectangle => "easel-tool-rectangle",
            Self::Arrow => "easel-tool-arrow",
            Self::Pen => "easel-tool-pen",
        }
    }
    fn shape(self) -> Option<ShapeKind> {
        match self {
            Self::Circle => Some(ShapeKind::Circle),
            Self::Rectangle => Some(ShapeKind::Rectangle),
            Self::Arrow => Some(ShapeKind::Arrow),
            Self::Pen => Some(ShapeKind::Freehand),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum Gesture {
    Draw { kind: ShapeKind, start: Point, current: Point, points: Vec<Point> },
    Move { id: u64, grab: Point, origin: Frame, frame: Frame },
    /// `handle` names the sides that follow the pointer: -1 the left or
    /// top, 1 the right or bottom, 0 neither. Images keep their `aspect`.
    Resize { id: u64, handle: (i8, i8), origin: Frame, aspect: Option<f32>, frame: Frame },
}

/// A text box being typed into; `id` is `None` until it has text.
#[derive(Clone, Debug)]
struct Editing {
    easel: EaselId,
    id: Option<u64>,
    frame: Frame,
    style: ParagraphStyle,
    family: FontFamily,
}

pub(crate) struct State {
    tool: Tool,
    selected: Option<u64>,
    gesture: Option<Gesture>,
    editing: Option<Editing>,
    text: Entity<TextInput>,
    /// Always editable, like plain text; every change renames the Easel.
    title: Entity<TextInput>,
    _title_changed: gpui::Subscription,
    pub(crate) focus: FocusHandle,
    /// The document layer as last laid out, scrolled with it.
    document: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: gpui::ScrollHandle,
    undo: HashMap<EaselId, Vec<Vec<Object>>>,
    redo: HashMap<EaselId, Vec<Vec<Object>>>,
    shown: Option<EaselId>,
}
impl State {
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let title = cx.new(|cx| TextInput::new(browser_core::easels::DEFAULT_TITLE, cx));
        let _title_changed = cx.subscribe(&title, |this, input, _: &crate::text_input::Changed, cx| {
            let Some(id) = this.open_easel_id() else { return; };
            let text = input.read(cx).text().trim().to_owned();
            let current = this.browser.active_user_profile().easels.get(id).map(|easel| easel.title.clone());
            if current.as_deref() == Some(if text.is_empty() { browser_core::easels::DEFAULT_TITLE } else { &text }) { return; }
            this.rename_easel(id, &text);
            cx.notify();
        });
        Self {
            tool: Tool::Select,
            selected: None,
            gesture: None,
            editing: None,
            text: cx.new(TextInput::code),
            title,
            _title_changed,
            focus: cx.focus_handle(),
            document: Rc::new(Cell::new(None)),
            scroll: gpui::ScrollHandle::new(),
            undo: HashMap::new(),
            redo: HashMap::new(),
            shown: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn tool(&self) -> Tool { self.tool }
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<u64> { self.selected }
}

/// Pigments of Arc's vibrant palette; black and white trade places on a dark page.
pub(crate) fn pigment(color: Color, dark: bool) -> Hsla {
    let rgb = match color {
        Color::Black => if dark { 0xffffff } else { 0x000000 },
        Color::White => if dark { 0x000000 } else { 0xffffff },
        Color::Gray => 0xbbbbbb,
        Color::Pink => 0xff994e,
        Color::Red => 0xf53714,
        Color::Yellow => 0xffd335,
        Color::LightGreen => 0x34e895,
        Color::DarkGreen => 0x107a00,
        Color::LightBlue => 0x00c7f3,
        Color::DarkBlue => 0x3139fb,
        Color::Purple => 0xc0009f,
    };
    gpui::rgb(rgb).into()
}

/// Point size and weight of a text style in each family.
fn text_metrics(style: ParagraphStyle, family: FontFamily) -> (f32, gpui::FontWeight, f32) {
    use gpui::FontWeight as W;
    let (size, weight) = match (family, style) {
        (FontFamily::Mono, ParagraphStyle::Ultra) => (80., W::BOLD),
        (FontFamily::Mono, ParagraphStyle::H0) => (60., W::BOLD),
        (FontFamily::Mono, ParagraphStyle::H1) => (44., W::BOLD),
        (FontFamily::Mono, ParagraphStyle::H2) => (24., W::BOLD),
        (FontFamily::Mono, ParagraphStyle::Body) => (15., W::NORMAL),
        (FontFamily::Fancy, ParagraphStyle::Ultra) => (80., W::NORMAL),
        (FontFamily::Fancy, ParagraphStyle::H0) => (60., W::NORMAL),
        (FontFamily::Fancy, ParagraphStyle::H1) => (48., W::NORMAL),
        (FontFamily::Fancy, ParagraphStyle::H2) => (28., W::NORMAL),
        (FontFamily::Fancy, ParagraphStyle::Body) => (16., W::NORMAL),
        (FontFamily::Hip, ParagraphStyle::Ultra) => (80., W::BLACK),
        (FontFamily::Hip, ParagraphStyle::H0) => (60., W::BLACK),
        (FontFamily::Hip, ParagraphStyle::H1) => (44., W::BLACK),
        (FontFamily::Hip, ParagraphStyle::H2) => (21., W::BOLD),
        (FontFamily::Hip, ParagraphStyle::Body) => (18., W::NORMAL),
        (FontFamily::Clean, ParagraphStyle::Ultra) => (80., W::EXTRA_BOLD),
        (FontFamily::Clean, ParagraphStyle::H0) => (60., W::EXTRA_BOLD),
        (FontFamily::Clean, ParagraphStyle::H1) => (44., W::EXTRA_BOLD),
        (FontFamily::Clean, ParagraphStyle::H2) => (24., W::BOLD),
        (FontFamily::Clean, ParagraphStyle::Body) => (15., W::NORMAL),
    };
    let line = match family { FontFamily::Mono => 1.48, FontFamily::Fancy => 1.32, FontFamily::Hip => 1.28, FontFamily::Clean => 1.2 };
    (size, weight, line)
}

fn font_name(family: FontFamily) -> &'static str {
    match family {
        FontFamily::Clean | FontFamily::Hip => ".SystemUIFont",
        FontFamily::Mono => if cfg!(target_os = "macos") { "Menlo" } else if cfg!(windows) { "Consolas" } else { "DejaVu Sans Mono" },
        FontFamily::Fancy => if cfg!(target_os = "linux") { "DejaVu Serif" } else { "Georgia" },
    }
}

/// The height a text box needs for its lines, in points.
fn text_height(text: &str, style: ParagraphStyle, family: FontFamily) -> f32 {
    let (size, _, line) = text_metrics(style, family);
    text.split('\n').count().max(1) as f32 * size * line + 8.
}

fn arrow_points(points: &[Point], frame: Frame) -> (Point, Point) {
    let at = |p: Option<&Point>, fallback: Point| p.map_or(fallback, |p| Point { x: frame.x + p.x * frame.width, y: frame.y + p.y * frame.height });
    (at(points.first(), Point { x: frame.x, y: frame.bottom() }), at(points.get(1), Point { x: frame.x + frame.width, y: frame.y }))
}

/// Paints one shape at document scale `scale` from the layer's `origin`.
fn paint_shape(window: &mut Window, origin: gpui::Point<Pixels>, scale: f32, frame: Frame, kind: ShapeKind, color: Hsla, points: &[Point]) {
    let at = |x: f32, y: f32| origin + point(px(x * scale), px(y * scale));
    let stroke = |window: &mut Window, points: &[(f32, f32)], closed: bool| {
        let mut path = gpui::PathBuilder::stroke(px(STROKE * scale));
        for (index, (x, y)) in points.iter().enumerate() {
            if index == 0 { path.move_to(at(*x, *y)); } else { path.line_to(at(*x, *y)); }
        }
        if closed { path.close(); }
        if let Ok(path) = path.build() { window.paint_path(path, color); }
    };
    let fill = |window: &mut Window, points: &[(f32, f32)]| {
        let mut path = gpui::PathBuilder::fill();
        for (index, (x, y)) in points.iter().enumerate() {
            if index == 0 { path.move_to(at(*x, *y)); } else { path.line_to(at(*x, *y)); }
        }
        path.close();
        if let Ok(path) = path.build() { window.paint_path(path, color.opacity(FILL_OPACITY)); }
    };
    match kind {
        ShapeKind::Rectangle => {
            let corners = [(frame.x, frame.y), (frame.x + frame.width, frame.y), (frame.x + frame.width, frame.bottom()), (frame.x, frame.bottom())];
            fill(window, &corners);
            stroke(window, &corners, true);
        }
        ShapeKind::Circle => {
            let (cx, cy, rx, ry) = (frame.x + frame.width / 2., frame.y + frame.height / 2., frame.width / 2., frame.height / 2.);
            let ring: Vec<_> = (0..72).map(|index| {
                let angle = index as f32 / 72. * std::f32::consts::TAU;
                (cx + rx * angle.cos(), cy + ry * angle.sin())
            }).collect();
            fill(window, &ring);
            stroke(window, &ring, true);
        }
        ShapeKind::Arrow => {
            let (start, end) = arrow_points(points, frame);
            let (dx, dy) = (start.x - end.x, start.y - end.y);
            let length = dx.hypot(dy).max(1.);
            let flare = 32f32.min(0.3 * length) / length;
            let (fx, fy) = (dx * flare, dy * flare);
            let turn = |angle: f32| (end.x + fx * angle.cos() - fy * angle.sin(), end.y + fx * angle.sin() + fy * angle.cos());
            stroke(window, &[(start.x, start.y), (end.x, end.y)], false);
            let quarter = std::f32::consts::FRAC_PI_4;
            stroke(window, &[turn(quarter), (end.x, end.y), turn(-quarter)], false);
        }
        ShapeKind::Freehand => {
            let line: Vec<_> = points.iter().map(|p| (frame.x + p.x * frame.width, frame.y + p.y * frame.height)).collect();
            if line.len() == 1 { stroke(window, &[line[0], (line[0].0 + 0.1, line[0].1)], false); } else { stroke(window, &line, false); }
        }
    }
}

/// The same marks the toolbar shows, drawn on an 18 px grid.
fn tool_icon(tool: Tool) -> AnyElement {
    canvas(|_, _, _| (), move |bounds, _, window, _| {
        let color = window.text_style().color;
        let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
        let line = |window: &mut Window, points: &[(f32, f32)], closed: bool| {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (index, (x, y)) in points.iter().enumerate() {
                if index == 0 { path.move_to(at(*x, *y)); } else { path.line_to(at(*x, *y)); }
            }
            if closed { path.close(); }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        };
        let ring = |cx: f32, cy: f32, r: f32| -> Vec<(f32, f32)> {
            (0..40).map(|index| { let a = index as f32 / 40. * std::f32::consts::TAU; (cx + r * a.cos(), cy + r * a.sin()) }).collect()
        };
        match tool {
            Tool::Select => line(window, &[(5., 2.5), (5., 15.), (8., 12.), (10.5, 16.5), (12.5, 15.5), (10., 11.), (14., 11.)], true),
            Tool::Image => {
                line(window, &[(2.5, 3.5), (15.5, 3.5), (15.5, 14.5), (2.5, 14.5)], true);
                line(window, &[(2.5, 12.5), (7., 8.5), (10.5, 12.), (12., 10.5), (15.5, 13.5)], false);
                line(window, &ring(11.5, 6.8, 1.2), true);
            }
            Tool::Text => {
                line(window, &[(2.5, 16.), (7.5, 2.5), (10.5, 2.5), (15.5, 16.), (12.5, 16.), (11.2, 12.), (6.8, 12.), (5.5, 16.)], true);
                line(window, &[(7.7, 9.5), (9., 5.5), (10.3, 9.5)], true);
            }
            Tool::Circle => line(window, &ring(9., 9., 6.5), true),
            Tool::Rectangle => line(window, &[(3., 3.), (15., 3.), (15., 15.), (3., 15.)], true),
            Tool::Arrow => {
                line(window, &[(3., 15.), (15., 3.)], false);
                line(window, &[(8., 3.), (15., 3.), (15., 10.)], false);
            }
            Tool::Pen => {
                line(window, &[(3., 15.), (8., 10.), (6., 14.5), (11., 9.5), (9., 14.), (14., 9.)], false);
                line(window, &[(10.5, 7.5), (14., 4.), (15.5, 5.5), (12., 9.)], true);
            }
        }
    }).size(px(18.)).flex_none().into_any_element()
}

/// Which sides of `frame` a press at `at` grabs: a corner, else an edge.
fn resize_handle(frame: Frame, at: Point, slop: f32) -> Option<(i8, i8)> {
    let side = |at: f32, low: f32, high: f32| if (at - low).abs() <= slop { Some(-1) } else if (at - high).abs() <= slop { Some(1) } else { None };
    let within = |at: f32, low: f32, high: f32| at > low - slop && at < high + slop;
    match (side(at.x, frame.x, frame.x + frame.width), side(at.y, frame.y, frame.bottom())) {
        (Some(x), Some(y)) => Some((x, y)),
        (Some(x), None) if within(at.y, frame.y, frame.bottom()) => Some((x, 0)),
        (None, Some(y)) if within(at.x, frame.x, frame.x + frame.width) => Some((0, y)),
        _ => None,
    }
}

/// `origin` with the grabbed sides moved to `at`. With an `aspect` the
/// frame scales as a whole from its fixed sides.
fn resized(origin: Frame, (hx, hy): (i8, i8), at: Point, aspect: Option<f32>) -> Frame {
    const MIN: f32 = 4.;
    let (mut left, mut top, mut right, mut bottom) = (origin.x, origin.y, origin.x + origin.width, origin.bottom());
    if hx < 0 { left = at.x } else if hx > 0 { right = at.x }
    if hy < 0 { top = at.y } else if hy > 0 { bottom = at.y }
    if let Some(ratio) = aspect {
        let (width, height) = ((right - left).abs().max(MIN), (bottom - top).abs().max(MIN));
        let (width, height) = if hy == 0 || (hx != 0 && width / ratio >= height) { (width, width / ratio) } else { (height * ratio, height) };
        let (sx, sy) = ((right - left).signum(), (bottom - top).signum());
        if hx < 0 { left = right - sx * width } else { right = left + sx * width }
        if hy < 0 { top = bottom - sy * height } else { bottom = top + sy * height }
    }
    let frame = Frame::spanning(Point { x: left, y: top }, Point { x: right, y: bottom });
    Frame { width: frame.width.max(MIN), height: frame.height.max(MIN), ..frame }
}

/// Invisible strips over the selection's edges and corners that show the
/// resize cursor; the document still receives the press.
fn resize_zones(x: f32, y: f32, w: f32, h: f32) -> Vec<gpui::Stateful<gpui::Div>> {
    use gpui::CursorStyle::*;
    const SLOP: f32 = 6.;
    let zone = |name: &'static str, left: f32, top: f32, width: f32, height: f32, cursor| div().id(name).debug_selector(move || format!("easel-resize-{name}"))
        .absolute().left(px(left)).top(px(top)).w(px(width.max(0.))).h(px(height.max(0.))).cursor(cursor);
    let (right, bottom, inner_w, inner_h) = (x + w, y + h, w - 2. * SLOP, h - 2. * SLOP);
    vec![
        zone("top", x + SLOP, y - SLOP, inner_w, 2. * SLOP, ResizeUpDown),
        zone("bottom", x + SLOP, bottom - SLOP, inner_w, 2. * SLOP, ResizeUpDown),
        zone("left", x - SLOP, y + SLOP, 2. * SLOP, inner_h, ResizeLeftRight),
        zone("right", right - SLOP, y + SLOP, 2. * SLOP, inner_h, ResizeLeftRight),
        zone("top-left", x - SLOP, y - SLOP, 2. * SLOP, 2. * SLOP, ResizeUpLeftDownRight),
        zone("bottom-right", right - SLOP, bottom - SLOP, 2. * SLOP, 2. * SLOP, ResizeUpLeftDownRight),
        zone("top-right", right - SLOP, y - SLOP, 2. * SLOP, 2. * SLOP, ResizeUpRightDownLeft),
        zone("bottom-left", x - SLOP, bottom - SLOP, 2. * SLOP, 2. * SLOP, ResizeUpRightDownLeft),
    ]
}

/// The translucent wash over the Space theme; the page and its top bar share it.
pub(crate) fn veil(dark: bool) -> Hsla {
    hsla(0., 0., if dark { 0.05 } else { 1. }, 0.35)
}

/// Easel's own mark: a page with a loose stroke across it.
pub(crate) fn favicon(size: f32) -> AnyElement {
    canvas(|_, _, _| (), move |bounds, _, window, _| {
        let scale = size / 16.;
        let at = |x: f32, y: f32| bounds.origin + point(px(x * scale), px(y * scale));
        let mut page = gpui::fill(Bounds::new(at(1.5, 1.5), gpui::size(px(13. * scale), px(13. * scale))), gpui::white());
        page.corner_radii = px(3. * scale).into();
        window.paint_quad(page);
        let mut path = gpui::PathBuilder::stroke(px(1.6 * scale));
        for (index, (x, y)) in [(4.5, 6.), (8., 4.8), (10.5, 5.5), (5., 10.), (7., 11.5), (11.5, 8.5)].into_iter().enumerate() {
            if index == 0 { path.move_to(at(x, y)); } else { path.line_to(at(x, y)); }
        }
        if let Ok(path) = path.build() { window.paint_path(path, gpui::rgb(0x1d1d22)); }
    }).size(px(size)).flex_none().into_any_element()
}

/// The New menu's Easel row: a page with a loose stroke, in the menu's line weight.
pub(crate) fn menu_icon() -> AnyElement {
    canvas(|_, _, _| (), |bounds, _, window, _| {
        let color = window.text_style().color;
        let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
        let corners = [(4.5, 4.), (19.5, 4.), (19.5, 19.), (4.5, 19.)];
        for (points, closed) in [(&corners[..], true), (&[(8., 9.5), (12.5, 8.), (15., 9.), (8.5, 15.), (10.5, 16.), (16., 12.5)][..], false)] {
            let mut path = gpui::PathBuilder::stroke(px(1.8));
            for (index, (x, y)) in points.iter().enumerate() {
                if index == 0 { path.move_to(at(*x, *y)); } else { path.line_to(at(*x, *y)); }
            }
            if closed { path.close(); }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
    }).size(px(24.)).flex_none().into_any_element()
}

/// Library's card mark: the loose stroke in Arc's warm pink.
pub(crate) fn card_mark() -> AnyElement {
    canvas(|_, _, _| (), |bounds, _, window, _| {
        let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
        let mut path = gpui::PathBuilder::stroke(px(3.));
        for (index, (x, y)) in [(3., 6.), (9., 2.5), (11.5, 4.), (3.5, 11.), (5., 13.5), (13., 7.5), (14., 9.5), (9.5, 14.)].into_iter().enumerate() {
            if index == 0 { path.move_to(at(x, y)); } else { path.line_to(at(x, y)); }
        }
        if let Ok(path) = path.build() { window.paint_path(path, gpui::rgb(0xf56d8a)); }
    }).size(px(17.)).flex_none().into_any_element()
}

impl BrowserWindow {
    /// The Easel the active tab shows, if it still exists.
    pub(crate) fn open_easel_id(&self) -> Option<EaselId> {
        if self.browser.active_split().is_some() { return None; }
        let id = EaselId::from_url(&self.browser.active_tab()?.url)?;
        self.browser.active_user_profile().easels.get(id).map(|_| id)
    }

    pub(crate) fn easel_is_open(&self) -> bool {
        self.open_easel_id().is_some()
    }

    /// Creates an Easel in the active profile and opens it.
    pub(crate) fn new_easel(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() || crate::quit::is_quitting() { return; }
        self.close_new_menu(cx);
        self.close_command_bar(cx);
        let id = self.edit_browser(|browser| browser.active_user_profile_mut().easels.create());
        self.open_easel(id, cx);
    }

    /// Activates the tab that shows `id`, opening one when there is none.
    pub(crate) fn open_easel(&mut self, id: EaselId, cx: &mut Context<Self>) {
        let Some(title) = self.browser.active_user_profile().easels.get(id).map(|easel| easel.title.clone()) else { return; };
        let url = id.url();
        let existing = self.browser.active_user_profile().favorites.iter()
            .chain(&self.browser.space().pinned)
            .chain(&self.browser.space().today)
            .find(|tab| tab.url == url)
            .map(|tab| tab.id);
        let now = std::time::SystemTime::now();
        match existing {
            Some(tab) => self.edit_browser(|browser| browser.activate_tab(tab, now)),
            None => {
                let tab = self.edit_browser(|browser| browser.new_tab(url, now));
                self.edit_browser(|browser| browser.set_title(tab, title));
            }
        }
        self.easel.tool = Tool::Select;
        self.easel.selected = None;
        // Keys go to the canvas once the page is on screen.
        let main = self.main_window;
        let owner = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| { let _ = owner.update(cx, |this, cx| if this.easel_is_open() { this.focus_easel(window, cx) }); });
        });
        cx.notify();
    }

    fn focus_easel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pages) = &self.pages { pages.borrow_mut().focus_ui(); }
        self.easel.focus.focus(window);
        cx.notify();
    }

    /// Changes one Easel's objects as a single undoable step.
    fn edit_easel(&mut self, id: EaselId, edit: impl FnOnce(&mut Easel)) {
        let Some(before) = self.browser.active_user_profile().easels.get(id).map(|easel| easel.objects.clone()) else { return; };
        let changed = self.edit_browser(|browser| {
            let Some(easel) = browser.active_user_profile_mut().easels.get_mut(id) else { return false; };
            edit(easel);
            easel.objects != before
        });
        if changed {
            let undo = self.easel.undo.entry(id).or_default();
            undo.push(before);
            if undo.len() > 100 { undo.remove(0); }
            self.easel.redo.remove(&id);
        }
    }

    fn undo_easel(&mut self, redo: bool, cx: &mut Context<Self>) {
        let Some(id) = self.open_easel_id() else { return; };
        self.commit_easel_text(cx);
        let (from, to) = if redo { (&mut self.easel.redo, &mut self.easel.undo) } else { (&mut self.easel.undo, &mut self.easel.redo) };
        let Some(objects) = from.get_mut(&id).and_then(Vec::pop) else { return; };
        let current = self.browser.active_user_profile().easels.get(id).map(|easel| easel.objects.clone()).unwrap_or_default();
        to.entry(id).or_default().push(current);
        self.edit_browser(|browser| { if let Some(easel) = browser.active_user_profile_mut().easels.get_mut(id) { easel.set_objects(objects); } });
        self.easel.selected = None;
        cx.notify();
    }

    /// Renames an Easel and every tab that shows it.
    pub(crate) fn rename_easel(&mut self, id: EaselId, title: &str) {
        let url = id.url();
        self.edit_browser(|browser| {
            if !browser.active_user_profile_mut().easels.rename(id, title) { return; }
            let title = browser.active_user_profile().easels.get(id).map(|easel| easel.title.clone()).unwrap_or_default();
            let tabs: Vec<_> = browser.all_tabs().filter(|tab| tab.url == url).map(|tab| tab.id).collect();
            for tab in tabs { browser.set_title(tab, title.clone()); }
        });
    }

    fn set_easel_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_easel_text(cx);
        if tool == Tool::Image {
            self.easel.tool = Tool::Select;
            self.pick_easel_images(cx);
        } else {
            self.easel.tool = tool;
        }
        if tool != Tool::Select { self.easel.selected = None; }
        self.focus_easel(window, cx);
    }

    fn pick_easel_images(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.open_easel_id() else { return; };
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else { return; };
            let _ = this.update(cx, |this, cx| {
                for path in paths { this.insert_easel_image(id, &path); }
                cx.notify();
            });
        }).detach();
    }

    /// Copies an image into the profile and places it in view.
    pub(crate) fn insert_easel_image(&mut self, id: EaselId, path: &std::path::Path) -> bool {
        let Ok((width, height)) = image::ImageReader::open(path).and_then(|reader| reader.with_guessed_format())
            .map_err(|_| ()).and_then(|reader| reader.into_dimensions().map_err(|_| ())) else { return false; };
        let directory = crate::profile_dir().join("Easels");
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("png").to_ascii_lowercase();
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
        let file = format!("{}-{stamp}.{extension}", id.url().rsplit('/').next().unwrap_or("easel"));
        if std::fs::create_dir_all(&directory).is_err() || std::fs::copy(path, directory.join(&file)).is_err() { return false; }
        let fit = (480. / width.max(1) as f32).min(1.).min(WIDTH * 0.8 / width.max(1) as f32);
        let (w, h) = (width as f32 * fit, height as f32 * fit);
        let top = self.easel_visible_top() + 80.;
        let frame = Frame { x: (WIDTH - w) / 2., y: top, width: w, height: h };
        let mut added = None;
        self.edit_easel(id, |easel| added = Some(easel.add(frame, Content::Image { file })));
        self.easel.selected = added;
        true
    }

    /// The document point at the top of the visible area.
    fn easel_visible_top(&self) -> f32 {
        let scale = self.easel_scale();
        (-f32::from(self.easel.scroll.offset().y) - LAYER_TOP).max(0.) / scale
    }

    fn easel_scale(&self) -> f32 {
        self.easel.document.get().map_or(1., |bounds| f32::from(bounds.size.width) / WIDTH).max(0.05)
    }

    /// Converts a window position to document points.
    fn easel_point(&self, position: gpui::Point<Pixels>) -> Option<Point> {
        let bounds = self.easel.document.get()?;
        let scale = self.easel_scale();
        Some(Point { x: f32::from(position.x - bounds.origin.x) / scale, y: f32::from(position.y - bounds.origin.y - px(LAYER_TOP)) / scale })
    }

    fn easel_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.open_easel_id() else { return; };
        let Some(at) = self.easel_point(event.position) else { return; };
        let editing_here = self.easel.editing.as_ref().is_some_and(|editing| editing.frame.contains(at, 0.));
        if editing_here { return; }
        self.commit_easel_text(cx);
        self.focus_easel(window, cx);
        let Some(easel) = self.browser.active_user_profile().easels.get(id).cloned() else { return; };
        if let Some(kind) = self.easel.tool.shape() {
            self.easel.selected = None;
            self.easel.gesture = Some(Gesture::Draw { kind, start: at, current: at, points: vec![at] });
            return;
        }
        if self.easel.tool == Tool::Text {
            self.begin_easel_text(Editing { easel: id, id: None, frame: Frame { x: at.x, y: at.y - 10., width: DEFAULT_TEXT_WIDTH, height: 0. }, style: ParagraphStyle::Body, family: FontFamily::Clean }, "", window, cx);
            self.easel.tool = Tool::Select;
            return;
        }
        let scale = self.easel_scale();
        let slop = 6. / scale;
        // A corner or edge of the selection resizes it.
        if let Some(object) = self.easel.selected.and_then(|selected| easel.object(selected)) {
            if let Some(handle) = resize_handle(object.frame, at, slop) {
                let aspect = matches!(object.content, Content::Image { .. }).then(|| object.frame.width / object.frame.height.max(1.));
                self.easel.gesture = Some(Gesture::Resize { id: object.id, handle, origin: object.frame, aspect, frame: object.frame });
                return;
            }
        }
        match easel.hit(at, slop) {
            Some(hit) => {
                let object = easel.object(hit).expect("hit object exists");
                if event.click_count >= 2 {
                    if let Content::Text { string, paragraph_style, font_family } = &object.content {
                        let editing = Editing { easel: id, id: Some(hit), frame: object.frame, style: *paragraph_style, family: *font_family };
                        let string = string.clone();
                        self.begin_easel_text(editing, &string, window, cx);
                        return;
                    }
                }
                self.easel.selected = Some(hit);
                self.easel.gesture = Some(Gesture::Move { id: hit, grab: at, origin: object.frame, frame: object.frame });
            }
            None => self.easel.selected = None,
        }
        cx.notify();
    }

    fn easel_mouse_move(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if self.easel.gesture.is_none() { return; }
        let Some(at) = self.easel_point(position) else { return; };
        match &mut self.easel.gesture {
            Some(Gesture::Draw { current, points, kind, .. }) => {
                *current = at;
                if *kind == ShapeKind::Freehand && points.last().is_none_or(|last| (last.x - at.x).hypot(last.y - at.y) > 1.) { points.push(at); }
            }
            Some(Gesture::Move { grab, origin, frame, .. }) => {
                *frame = Frame { x: origin.x + at.x - grab.x, y: (origin.y + at.y - grab.y).max(0.), ..*origin };
            }
            Some(Gesture::Resize { handle, origin, aspect, frame, .. }) => *frame = resized(*origin, *handle, at, *aspect),
            None => {}
        }
        cx.notify();
    }

    fn easel_mouse_up(&mut self, cx: &mut Context<Self>) {
        let Some(gesture) = self.easel.gesture.take() else { return; };
        let Some(id) = self.open_easel_id() else { return; };
        match gesture {
            Gesture::Draw { kind, start, current, points } => {
                let content_points;
                let mut frame = Frame::spanning(start, current);
                match kind {
                    ShapeKind::Freehand => {
                        let (min_x, max_x) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
                        let (min_y, max_y) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
                        frame = Frame { x: min_x, y: min_y, width: (max_x - min_x).max(1.), height: (max_y - min_y).max(1.) };
                        content_points = points.iter().map(|p| Point { x: (p.x - frame.x) / frame.width, y: (p.y - frame.y) / frame.height }).collect();
                    }
                    ShapeKind::Arrow => {
                        if frame.width.hypot(frame.height) < 4. { return; }
                        frame.width = frame.width.max(1.);
                        frame.height = frame.height.max(1.);
                        let fraction = |p: Point| Point { x: (p.x - frame.x) / frame.width, y: (p.y - frame.y) / frame.height };
                        content_points = vec![fraction(start), fraction(current)];
                    }
                    ShapeKind::Rectangle | ShapeKind::Circle => {
                        if frame.width < 4. && frame.height < 4. {
                            frame = Frame { x: start.x - DEFAULT_SHAPE / 2., y: start.y - DEFAULT_SHAPE / 2., width: DEFAULT_SHAPE, height: DEFAULT_SHAPE };
                        }
                        content_points = Vec::new();
                    }
                }
                frame.y = frame.y.max(0.);
                let mut added = None;
                self.edit_easel(id, |easel| added = Some(easel.add(frame, Content::Shape { kind, color: kind.default_color(), points: content_points })));
                // A drawn shape is selected for its colors; the pen keeps drawing.
                if kind != ShapeKind::Freehand {
                    self.easel.selected = added;
                    self.easel.tool = Tool::Select;
                }
            }
            Gesture::Move { id: object, frame, origin, .. } => {
                if frame != origin { self.edit_easel(id, |easel| { if let Some(o) = easel.object_mut(object) { o.frame = frame; } }); }
            }
            Gesture::Resize { id: object, frame, .. } => {
                self.edit_easel(id, |easel| { if let Some(o) = easel.object_mut(object) { o.frame = frame; } });
            }
        }
        cx.notify();
    }

    fn begin_easel_text(&mut self, editing: Editing, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.easel.selected = editing.id;
        self.easel.editing = Some(editing);
        self.easel.text.update(cx, |input, cx| { input.set_text(text.to_owned(), cx); input.select_all_text(cx); });
        // After the click, whose canvas would otherwise take focus back.
        let focus = self.easel.text.read(cx).focus_handle(cx);
        window.defer(cx, move |window, _| focus.focus(window));
        cx.notify();
    }

    /// Keeps typed text; an emptied box goes away.
    fn commit_easel_text(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.easel.editing.take() else { return; };
        let text = self.easel.text.read(cx).text().trim_end().to_owned();
        let height = text_height(&text, editing.style, editing.family);
        let frame = Frame { height, ..editing.frame };
        let content = Content::Text { string: text.clone(), paragraph_style: editing.style, font_family: editing.family };
        let mut added = None;
        self.edit_easel(editing.easel, |easel| match editing.id {
            Some(object) if text.is_empty() => { easel.remove(object); }
            Some(object) => { if let Some(o) = easel.object_mut(object) { o.frame = frame; o.content = content; } }
            None if text.is_empty() => {}
            None => added = Some(easel.add(frame, content)),
        });
        self.easel.selected = if text.is_empty() { None } else { added.or(editing.id) };
        cx.notify();
    }

    fn step_easel_text(&mut self, larger: bool, cx: &mut Context<Self>) {
        if let Some(editing) = &mut self.easel.editing {
            if let Some(style) = if larger { editing.style.larger() } else { editing.style.smaller() } { editing.style = style; }
            cx.notify();
            return;
        }
        let (Some(id), Some(selected)) = (self.open_easel_id(), self.easel.selected) else { return; };
        self.edit_easel(id, |easel| {
            if let Some(object) = easel.object_mut(selected) {
                if let Content::Text { string, paragraph_style, font_family } = &mut object.content {
                    if let Some(style) = if larger { paragraph_style.larger() } else { paragraph_style.smaller() } { *paragraph_style = style; }
                    object.frame.height = text_height(string, *paragraph_style, *font_family);
                }
            }
        });
        cx.notify();
    }

    fn cycle_easel_font(&mut self, cx: &mut Context<Self>) {
        if let Some(editing) = &mut self.easel.editing { editing.family = editing.family.next(); cx.notify(); return; }
        let (Some(id), Some(selected)) = (self.open_easel_id(), self.easel.selected) else { return; };
        self.edit_easel(id, |easel| {
            if let Some(object) = easel.object_mut(selected) {
                if let Content::Text { string, paragraph_style, font_family } = &mut object.content {
                    *font_family = font_family.next();
                    object.frame.height = text_height(string, *paragraph_style, *font_family);
                }
            }
        });
        cx.notify();
    }

    fn set_easel_color(&mut self, color: Color, cx: &mut Context<Self>) {
        let (Some(id), Some(selected)) = (self.open_easel_id(), self.easel.selected) else { return; };
        self.edit_easel(id, |easel| {
            if let Some(Object { content: Content::Shape { color: current, .. }, .. }) = easel.object_mut(selected) { *current = color; }
        });
        cx.notify();
    }

    fn easel_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.easel_title_focused(window, cx) { self.focus_easel(window, cx); return; }
        if self.easel.editing.is_some() { self.commit_easel_text(cx); self.focus_easel(window, cx); return; }
        if self.easel.selected.take().is_none() { self.easel.tool = Tool::Select; }
        cx.notify();
    }

    fn easel_title_focused(&self, window: &Window, cx: &App) -> bool {
        self.easel.title.read(cx).focus_handle(cx).is_focused(window)
    }

    fn delete_easel_selection(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(selected)) = (self.open_easel_id(), self.easel.selected.take()) else { return; };
        self.edit_easel(id, |easel| { easel.remove(selected); });
        cx.notify();
    }

    fn easel_key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if self.easel_title_focused(window, cx) {
            // Return leaves the title as Escape does; the text is already saved.
            if event.keystroke.key == "enter" { self.focus_easel(window, cx); cx.stop_propagation(); }
            return;
        }
        if self.easel.editing.is_some() || modifiers.platform || modifiers.control || modifiers.alt || modifiers.function { return; }
        if let Some(tool) = Tool::ALL.into_iter().find(|tool| tool.key() == event.keystroke.key) {
            self.set_easel_tool(tool, window, cx);
            cx.stop_propagation();
        }
    }

    /// Clears per-page state when another page takes over.
    pub(crate) fn sync_easel(&mut self, cx: &mut Context<Self>) {
        let open = self.open_easel_id();
        if self.easel.shown == open { return; }
        self.commit_easel_text(cx);
        self.easel.shown = open;
        if let Some(title) = open.and_then(|id| self.browser.active_user_profile().easels.get(id)).map(|easel| easel.title.clone()) {
            self.easel.title.update(cx, |input, cx| input.set_text_at_end(title, cx));
        }
        self.easel.selected = None;
        self.easel.gesture = None;
        self.easel.tool = Tool::Select;
        self.easel.scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(crate) fn render_easel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.open_easel_id()?;
        let easel = self.browser.active_user_profile().easels.get(id)?.clone();
        let dark = self.theme_is_dark();
        let ink = if dark { 1. } else { 0. };
        let scale = self.easel_scale();
        let state = &self.easel;
        // The object being moved or resized shows where it will land.
        let live = match &state.gesture {
            Some(Gesture::Move { id, frame, .. } | Gesture::Resize { id, frame, .. }) => Some((*id, *frame)),
            _ => None,
        };
        let frame_of = |object: &Object| live.filter(|(id, _)| *id == object.id).map_or(object.frame, |(_, frame)| frame);
        let editing_id = state.editing.as_ref().and_then(|editing| editing.id);
        let images = crate::profile_dir().join("Easels");
        let mut layer = div().absolute().left_0().right_0().top(px(LAYER_TOP)).bottom_0();
        for object in &easel.objects {
            let frame = frame_of(object);
            let place = |element: gpui::Div| element.absolute().left(px(frame.x * scale)).top(px(frame.y * scale)).w(px(frame.width * scale)).h(px(frame.height * scale));
            let element = match &object.content {
                Content::Shape { kind, color, points } => {
                    let (kind, color, points) = (*kind, pigment(*color, dark), points.clone());
                    place(div()).child(canvas(|_, _, _| (), move |bounds, _, window, _| {
                        let origin = bounds.origin - point(px(frame.x * scale), px(frame.y * scale));
                        paint_shape(window, origin, scale, frame, kind, color, &points);
                    }).size_full())
                }
                Content::Text { .. } if editing_id == Some(object.id) => continue,
                Content::Text { string, paragraph_style, font_family } => {
                    let (size, weight, line) = text_metrics(*paragraph_style, *font_family);
                    place(div()).font_family(font_name(*font_family)).text_size(px(size * scale)).line_height(px(size * line * scale))
                        .font_weight(weight).text_color(hsla(0., 0., ink, 1.)).whitespace_nowrap()
                        .when(*paragraph_style != ParagraphStyle::Body, |text| text.text_center())
                        .px(px(4. * scale))
                        .children(string.split('\n').map(|line| div().child(line.to_owned())))
                }
                // Both sides are explicit: an image's own aspect ratio would
                // otherwise size it past its frame.
                Content::Image { file } => place(div()).child(gpui::img(images.join(file)).w(px(frame.width * scale)).h(px(frame.height * scale)).object_fit(gpui::ObjectFit::Fill)),
            };
            layer = layer.child(element.debug_selector({ let id = object.id; move || format!("easel-object-{id}") }));
        }
        // The shape being drawn.
        if let Some(Gesture::Draw { kind, start, current, points }) = &state.gesture {
            let (kind, start, current, points) = (*kind, *start, *current, points.clone());
            let color = pigment(kind.default_color(), dark);
            layer = layer.child(canvas(|_, _, _| (), move |bounds, _, window, _| {
                let frame = Frame::spanning(start, current);
                match kind {
                    ShapeKind::Freehand => {
                        let frame = Frame { x: 0., y: 0., width: 1., height: 1. };
                        paint_shape(window, bounds.origin, scale, frame, kind, color, &points);
                    }
                    ShapeKind::Arrow => {
                        let frame = Frame { width: frame.width.max(0.01), height: frame.height.max(0.01), ..frame };
                        let fraction = |p: Point| Point { x: (p.x - frame.x) / frame.width, y: (p.y - frame.y) / frame.height };
                        paint_shape(window, bounds.origin, scale, frame, kind, color, &[fraction(start), fraction(current)]);
                    }
                    _ => paint_shape(window, bounds.origin, scale, frame, kind, color, &[]),
                }
            }).absolute().inset_0());
        }
        // Selection outline, handles and the controls that belong to it.
        if let Some(object) = state.selected.and_then(|id| easel.object(id)).filter(|object| Some(object.id) != editing_id) {
            let frame = frame_of(object);
            let (x, y, w, h) = (frame.x * scale, frame.y * scale, frame.width * scale, frame.height * scale);
            let blue: Hsla = gpui::rgb(SELECTION).into();
            let handle = |hx: f32, hy: f32| div().absolute().left(px(hx - 4.)).top(px(hy - 4.)).size(px(8.)).rounded_full().border(px(1.5)).border_color(blue).bg(hsla(0., 0., 1. - ink, 1.));
            layer = layer.child(div().debug_selector(|| "easel-selection".into()).absolute().left(px(x - 2.)).top(px(y - 2.)).w(px(w + 4.)).h(px(h + 4.))
                    .border(px(1.5)).border_color(blue).when(matches!(object.content, Content::Text { .. }), |outline| outline.rounded(px(6.))))
                .children([handle(x - 2., y - 2.), handle(x + w + 2., y - 2.), handle(x + w + 2., y + h + 2.), handle(x - 2., y + h + 2.)])
                .children(if state.tool == Tool::Select { resize_zones(x, y, w, h) } else { Vec::new() });
            match &object.content {
                Content::Shape { color, .. } if live.is_none() => layer = layer.child(self.render_easel_palette(*color, x + w / 2., y, dark, cx)),
                Content::Text { .. } if live.is_none() => layer = layer.child(self.render_easel_text_controls(x, y, h, cx)),
                _ => {}
            }
        }
        // The text box being typed into.
        if let Some(editing) = &state.editing {
            let text = state.text.read(cx).text().to_owned();
            let (size, weight, line) = text_metrics(editing.style, editing.family);
            let height = text_height(&text, editing.style, editing.family) * scale;
            let (x, y, w) = (editing.frame.x * scale, editing.frame.y * scale, editing.frame.width * scale);
            layer = layer.child(div().id("easel-text-editor").debug_selector(|| "easel-text-editor".into())
                    .absolute().left(px(x)).top(px(y)).min_w(px(w)).h(px(height)).px(px(4. * scale)).rounded(px(6.)).border(px(1.5)).border_color(gpui::rgb(SELECTION))
                    .font_family(font_name(editing.family)).text_size(px(size * scale)).line_height(px(size * line * scale)).font_weight(weight)
                    .text_color(hsla(0., 0., ink, 1.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(state.text.clone()))
                .child(self.render_easel_text_controls(x, y, height, cx));
        }
        let content_height = LAYER_TOP + easel.bottom() * scale + TRAILING_SPACE;
        let document = state.document.clone();
        let owner = cx.entity().downgrade();
        // Invisible text sizes the field so the title stays centered, and a
        // click places the caret, as in any text.
        let typed = state.title.read(cx).text().to_owned();
        let sizing = if typed.is_empty() { browser_core::easels::DEFAULT_TITLE.to_owned() } else { typed };
        let title = div().id("easel-title").debug_selector(|| "easel-title".into()).relative().max_w(px(600.)).overflow_hidden().cursor_text()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().whitespace_nowrap().pr(px(2.)).text_color(gpui::transparent_black()).child(sizing))
            .child(div().absolute().inset_0().child(state.title.clone()));
        let mut scroll = div().id("easel-scroll").debug_selector(|| "easel-scroll".into()).absolute().inset_0().overflow_y_scroll().track_scroll(&state.scroll);
        // A horizontal swipe belongs to the sidebar and Library.
        scroll.style().restrict_scroll_to_axis = Some(true);
        let cursor = match state.tool { Tool::Select => gpui::CursorStyle::Arrow, Tool::Text => gpui::CursorStyle::IBeam, _ => gpui::CursorStyle::Crosshair };
        let page = self.echo_page_corners(div())
            .id("easel-page")
            .debug_selector(|| "easel-page".into())
            .key_context(CONTEXT)
            .track_focus(&state.focus)
            .absolute()
            .inset_0()
            .overflow_hidden()
            .font_family(".SystemUIFont")
            .bg(veil(dark))
            .text_color(hsla(0., 0., ink, 1.))
            .on_action(cx.listener(|this, _: &DeleteSelection, _, cx| this.delete_easel_selection(cx)))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.easel_escape(window, cx)))
            .on_action(cx.listener(|this, _: &crate::text_input::Undo, _, cx| this.undo_easel(false, cx)))
            .on_action(cx.listener(|this, _: &crate::text_input::Redo, _, cx| this.undo_easel(true, cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| this.easel_key(event, window, cx)))
            .child(scroll.child(div().id("easel-document").debug_selector(|| "easel-document".into()).relative().w_full().min_h_full().h(px(content_height))
                .cursor(cursor)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| this.easel_mouse_down(event, window, cx)))
                .child(canvas(move |bounds, window, _| {
                    if document.get().map(|old| old.size.width) != Some(bounds.size.width) { window.refresh(); }
                    document.set(Some(bounds));
                }, move |_, _, window, _| {
                    let moving = owner.clone();
                    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                        if phase != gpui::DispatchPhase::Capture { return; }
                        let _ = moving.update(cx, |this, cx| this.easel_mouse_move(event.position, cx));
                    });
                    let ending = owner.clone();
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                        if phase != gpui::DispatchPhase::Capture || event.button != MouseButton::Left { return; }
                        let _ = ending.update(cx, |this, cx| this.easel_mouse_up(cx));
                    });
                }).absolute().inset_0())
                .child(div().absolute().top(px(TITLE_TOP)).left_0().right_0().flex().justify_center()
                    .text_size(px(32.)).line_height(px(40.)).font_weight(gpui::FontWeight::BOLD)
                    .child(title))
                .child(layer)))
            .child(self.render_easel_toolbar(cx));
        Some(page.into_any_element())
    }

    fn render_easel_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let tool = self.easel.tool;
        div().id("easel-toolbar").debug_selector(|| "easel-toolbar".into())
            .absolute().left(px(12.)).bottom(px(12.)).p(px(4.)).rounded(px(10.)).bg(hsla(0., 0., 0.02, 0.92))
            .flex().items_center().gap(px(2.)).text_color(gpui::white())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(Tool::ALL.into_iter().map(|item| {
                let active = item == tool;
                div().id(item.selector()).debug_selector(move || item.selector().into())
                    .relative().size(px(32.)).rounded(px(7.)).flex().items_center().justify_center()
                    .when(active, |button| button.bg(hsla(0., 0., 1., 0.2)))
                    .hover(|button| button.bg(hsla(0., 0., 1., 0.12)))
                    .on_click(cx.listener(move |this, _, window, cx| this.set_easel_tool(item, window, cx)))
                    .child(tool_icon(item))
                    // The active tool shows the key that picks it.
                    .when(active && item != Tool::Select, |button| button.child(div().absolute().right(px(2.)).bottom(px(0.)).text_size(px(8.)).font_weight(gpui::FontWeight::BOLD).child(item.key())))
            }))
            .into_any_element()
    }

    fn render_easel_palette(&self, current: Color, center_x: f32, top: f32, dark: bool, cx: &mut Context<Self>) -> AnyElement {
        const SWATCH: f32 = 16.;
        const GAP: f32 = 6.;
        let width = 6. * SWATCH + 5. * GAP + 20.;
        let rows = [&Color::ALL[..6], &Color::ALL[6..]];
        div().id("easel-palette").debug_selector(|| "easel-palette".into())
            .absolute().left(px(center_x - width / 2.)).top(px(top - 64.)).w(px(width)).p(px(10.)).rounded(px(8.)).bg(gpui::black())
            .flex().flex_col().gap(px(GAP))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(rows.into_iter().map(|row| div().flex().gap(px(GAP)).justify_center().children(row.iter().map(|color| {
                let color = *color;
                let fill = pigment(color, false);
                let name = format!("easel-color-{color:?}");
                div().id(gpui::SharedString::from(name.clone())).debug_selector(move || name.clone())
                    .size(px(SWATCH)).rounded_full().bg(fill).flex().items_center().justify_center()
                    .when(color == Color::Black, |swatch| swatch.border_1().border_color(hsla(0., 0., 1., 0.25)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_easel_color(color, cx)))
                    .when(color == current, |swatch| swatch.text_color(if matches!(color, Color::White | Color::Yellow | Color::Gray | Color::LightGreen) { gpui::black() } else { gpui::white() })
                        .child(crate::icons::control_sized(crate::icons::Icon::Check, 12.)))
            }))))
            .when(dark, |palette| palette.border_1().border_color(hsla(0., 0., 1., 0.12)))
            .into_any_element()
    }

    fn render_easel_text_controls(&self, x: f32, y: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let button = |id: &'static str| div().id(id).debug_selector(move || id.into()).w(px(22.)).h(px(18.)).flex().items_center().justify_center().rounded(px(4.))
            .hover(|button| button.bg(hsla(0., 0., 1., 0.15)));
        div().debug_selector(|| "easel-text-controls".into()).absolute().left(px(x - 36.)).top(px(y + height / 2. - 34.)).w(px(28.)).py(px(4.))
            .rounded(px(6.)).bg(gpui::black()).text_color(hsla(0., 0., 1., 0.85)).flex().flex_col().items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(button("easel-text-larger").child(crate::icons::control_sized(crate::icons::Icon::Plus, 12.))
                .on_click(cx.listener(|this, _, _, cx| this.step_easel_text(true, cx))))
            .child(button("easel-text-font").text_size(px(11.)).font_family(font_name(FontFamily::Fancy)).font_weight(gpui::FontWeight::BOLD).child("Aa")
                .on_click(cx.listener(|this, _, _, cx| this.cycle_easel_font(cx))))
            .child(button("easel-text-smaller").child(crate::icons::control_sized(crate::icons::Icon::Minus, 12.))
                .on_click(cx.listener(|this, _, _, cx| this.step_easel_text(false, cx))))
            .into_any_element()
    }
}

#[cfg(test)]
mod resize_tests {
    use super::*;

    const FRAME: Frame = Frame { x: 100., y: 100., width: 200., height: 100. };

    #[test]
    fn corners_and_edges_grab_their_sides() {
        assert_eq!(resize_handle(FRAME, Point { x: 101., y: 99. }, 6.), Some((-1, -1)));
        assert_eq!(resize_handle(FRAME, Point { x: 300., y: 150. }, 6.), Some((1, 0)));
        assert_eq!(resize_handle(FRAME, Point { x: 200., y: 203. }, 6.), Some((0, 1)));
        assert_eq!(resize_handle(FRAME, Point { x: 200., y: 150. }, 6.), None);
    }

    #[test]
    fn edges_move_one_side_and_images_keep_their_aspect() {
        assert_eq!(resized(FRAME, (1, 0), Point { x: 400., y: 0. }, None), Frame { width: 300., ..FRAME });
        // A corner follows the dominant axis; the opposite corner stays put.
        assert_eq!(resized(FRAME, (1, 1), Point { x: 500., y: 150. }, Some(2.)), Frame { x: 100., y: 100., width: 400., height: 200. });
        assert_eq!(resized(FRAME, (-1, -1), Point { x: 200., y: 0. }, Some(2.)), Frame { x: -100., y: 0., width: 400., height: 200. });
        // An edge scales the other side with it.
        assert_eq!(resized(FRAME, (0, 1), Point { x: 0., y: 150. }, Some(2.)), Frame { x: 100., y: 100., width: 100., height: 50. });
    }
}
