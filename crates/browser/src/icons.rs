//! A focused Command Bar editor for tab/group and Space symbols.
use crate::window::{BrowserWindow, Target};
use browser_core::{SpaceId, TabId};
use gpui::{div, prelude::*, px, AnyElement, Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconTarget { Tab(TabId), Space(SpaceId), Folder(browser_core::folders::FolderId) }

pub(crate) fn glyph(icon: &str) -> AnyElement {
    div().size(px(18.)).flex_none().flex().items_center().justify_center().text_size(px(16.)).line_height(px(18.)).child(icon.to_string()).into_any_element()
}

/// Browser controls use fixed vector geometry, never font-dependent glyphs.
pub(crate) fn disclosure(collapsed: bool) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let color = window.text_style().color;
        let points = if collapsed { [(6.75, 4.5), (11.25, 9.), (6.75, 13.5)] }
            else { [(4.5, 6.75), (9., 11.25), (13.5, 6.75)] };
        let points = points.map(|(x, y)| bounds.origin + gpui::point(px(x), px(y)));
        for segment in points.windows(2) {
            let mut path = gpui::PathBuilder::stroke(px(1.8));
            path.move_to(segment[0]);
            path.line_to(segment[1]);
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
        // Round each segment's caps and the shared join at the same stroke radius.
        for point in points {
            let mut dot = gpui::fill(gpui::Bounds::new(point - gpui::point(px(0.9), px(0.9)), gpui::size(px(1.8), px(1.8))), color);
            dot.corner_radii = px(0.9).into();
            window.paint_quad(dot);
        }
    }).size(px(18.)).flex_none().into_any_element()
}

pub(crate) fn more() -> AnyElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        for x in [5., 9., 13.] {
            let mut dot = gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(px(x - 1.2), px(7.8)), gpui::size(px(2.4), px(2.4))), window.text_style().color);
            dot.corner_radii = px(1.2).into();
            window.paint_quad(dot);
        }
    }).size(px(18.)).flex_none().into_any_element()
}

/// Portable browser chrome, drawn on an 18 px grid. Custom user symbols and
/// website favicons remain content and are rendered separately.
#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Back, Forward, Reload, Sidebar, Split, Close, Minus, Plus, Folder,
    Search, External, History, Enter, Restore, Up, Down, Check, Mute, Command, Link,
    SiteControls, Library, Tidy, Share, Boost, Capture, Screenshot, Lock, Unlock, NewSpace, NewTab,
}

fn arc(x: f32, y: f32, radius: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    (0..=24).map(|index| {
        let angle = from + (to - from) * index as f32 / 24.;
        (x + radius * angle.cos(), y + radius * angle.sin())
    }).collect()
}

fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Vec<(f32, f32)> {
    use std::f32::consts::FRAC_PI_2;
    let mut points = Vec::new();
    for (cx, cy, start) in [(x + width - radius, y + radius, -FRAC_PI_2),
        (x + width - radius, y + height - radius, 0.),
        (x + radius, y + height - radius, FRAC_PI_2), (x + radius, y + radius, 2. * FRAC_PI_2)] {
        points.extend(arc(cx, cy, radius, start, start + FRAC_PI_2));
    }
    points.push(points[0]);
    points
}

type IconContours = std::collections::BTreeMap<String, Vec<Vec<f32>>>;
static IONICONS: std::sync::LazyLock<IconContours> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/ionicons/paths.json")).expect("embedded Ionicons geometry is valid")
});

fn ionicon(name: &'static str, size: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let Some(commands) = IONICONS.get(name) else { return; };
        let scale = f32::from(bounds.size.width) / 512.;
        let at = |x, y| bounds.origin + gpui::point(px(x * scale), px((511. - y) * scale));
        let mut path = gpui::PathBuilder::fill();
        for command in commands {
            match command.as_slice() {
                [0., x, y] => path.move_to(at(*x, *y)),
                [1., x, y] => path.line_to(at(*x, *y)),
                [2., cx, cy, x, y] => path.curve_to(at(*x, *y), at(*cx, *cy)),
                [3., c1x, c1y, c2x, c2y, x, y] => path.cubic_bezier_to(at(*x, *y), at(*c1x, *c1y), at(*c2x, *c2y)),
                [4.] => path.close(),
                _ => unreachable!("embedded contour command"),
            }
        }
        if let Ok(path) = path.build() { window.paint_path(path, window.text_style().color); }
    }).size(px(size)).flex_none().into_any_element()
}

pub(crate) fn control(icon: Icon) -> AnyElement { control_sized(icon, 18.) }

/// A catalog-backed control scaled to `size`; frame controls keep the 18 px grid.
pub(crate) fn control_sized(icon: Icon, size: f32) -> AnyElement {
    let name = match icon {
        Icon::Back => "arrow-back-outline", Icon::Forward => "arrow-forward-outline",
        Icon::Reload => "reload-outline", Icon::Close => "close-outline",
        Icon::Minus => "remove-outline", Icon::Plus => "add-outline",
        Icon::Folder => "folder-outline", Icon::Search => "search-outline",
        Icon::External => "open-outline", Icon::History => "time-outline",
        Icon::Enter => "return-down-back-outline", Icon::Restore => "arrow-undo-outline",
        Icon::Up => "arrow-up-outline", Icon::Down => "arrow-down-outline",
        Icon::Check => "checkmark-outline", Icon::Mute => "volume-mute-outline",
        Icon::Share => "share-outline", Icon::Boost => "color-wand-outline",
        Icon::Capture => "camera-outline", Icon::Screenshot => "scan-outline",
        Icon::NewSpace => "duplicate-outline", Icon::Lock => "lock-closed", Icon::Unlock => "lock-open",
        Icon::Sidebar | Icon::Split | Icon::Command => return chrome_frame(icon),
        Icon::Link | Icon::SiteControls | Icon::Library | Icon::Tidy | Icon::NewTab => return drawn(icon, size),
    };
    ionicon(name, size)
}

/// Reference-specific frame/key controls absent from the Ionicons catalog.
fn chrome_frame(icon: Icon) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let paths = match icon {
            Icon::Sidebar => vec![rounded_rect(1.5, 3., 15., 12., 2.), vec![(6.5, 3.), (6.5, 15.)], vec![(3.5, 6.), (4.5, 6.)], vec![(3.5, 8.5), (4.5, 8.5)]],
            Icon::Split => vec![rounded_rect(2., 3., 14., 12., 1.5), vec![(9., 3.), (9., 15.)]],
            Icon::Command => vec![rounded_rect(3., 3., 4., 4., 1.75), rounded_rect(11., 3., 4., 4., 1.75), rounded_rect(3., 11., 4., 4., 1.75), rounded_rect(11., 11., 4., 4., 1.75), rounded_rect(7., 7., 4., 4., 0.)],
            _ => unreachable!("catalog-backed control"),
        };
        let color = window.text_style().color;
        for points in paths {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (index, (x, y)) in points.iter().enumerate() {
                let point = bounds.origin + gpui::point(px(*x), px(*y));
                if index == 0 { path.move_to(point); } else { path.line_to(point); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
    }).size(px(18.)).flex_none().into_any_element()
}

/// A chain link: a capsule outline around `from`–`to`, left open for
/// `gap` radians at the start of its `to` cap where the other link passes.
fn link(from: (f32, f32), to: (f32, f32), radius: f32, gap: f32) -> Vec<(f32, f32)> {
    let angle = (to.1 - from.1).atan2(to.0 - from.0);
    let half = std::f32::consts::FRAC_PI_2;
    let mut points = arc(to.0, to.1, radius, angle - half + gap, angle + half);
    points.extend(arc(from.0, from.1, radius, angle + half, angle + 3. * half));
    points.push((to.0 + radius * (angle - half).cos(), to.1 + radius * (angle - half).sin()));
    points
}

fn circle(x: f32, y: f32, radius: f32) -> Vec<(f32, f32)> {
    arc(x, y, radius, 0., std::f32::consts::TAU)
}

/// Controls drawn on an 18 px grid to match the reference's symbols:
/// `strokes` are 1.5 px outlines, `fills` are solid contours (a reversed
/// inner contour cuts a hole).
fn drawn(icon: Icon, size: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let (strokes, fills): (Vec<Vec<(f32, f32)>>, Vec<Vec<Vec<(f32, f32)>>>) = match icon {
            // Two interlocking links on the rising diagonal.
            Icon::Link => (vec![link((6., 12.), (7.8, 10.2), 2.9, 1.5), link((12., 6.), (10.2, 7.8), 2.9, 1.5)], vec![]),
            // Two sliders inside a rounded square: knob right on top, left below.
            Icon::SiteControls => (vec![
                rounded_rect(2.5, 2.5, 13., 13., 3.8),
                vec![(5.6, 7.2), (9., 7.2)], vec![(11.6, 7.2), (12.4, 7.2)], circle(10.3, 7.2, 1.3),
                vec![(5.6, 10.8), (6.4, 10.8)], vec![(9., 10.8), (12.4, 10.8)], circle(7.7, 10.8, 1.3),
            ], vec![]),
            // A plus inside a rounded square.
            Icon::NewTab => (vec![rounded_rect(2.5, 2.5, 13., 13., 3.), vec![(9., 6.), (9., 12.)], vec![(6., 9.), (12., 9.)]], vec![]),
            // A filled storage box with a handle slot and items peeking out.
            Icon::Library => (vec![], vec![
                {
                    let mut slot = rounded_rect(7., 10.2, 4., 1.8, 0.9);
                    slot.reverse();
                    vec![rounded_rect(2.8, 7.4, 12.4, 8.8, 2.2), slot]
                },
                vec![vec![(3.8, 6.8), (6.2, 3.2), (8.8, 4.8), (9.8, 6.8)]],
                vec![vec![(9.2, 6.8), (12.6, 4.4), (14.4, 6.8)]],
                vec![circle(13., 2.2, 1.2)],
            ]),
            // A broom: handle on the falling diagonal and a fanned head.
            Icon::Tidy => (vec![vec![(13.5, 2.5), (9.8, 8.6)]], vec![
                vec![vec![(7.6, 8.), (11.6, 10.4), (10.2, 15.5), (8.6, 14.2), (7.4, 15.4), (6.4, 13.8), (4.6, 14.2)]],
            ]),
            _ => unreachable!("drawn control"),
        };
        let scale = size / 18.;
        let at = |(x, y): (f32, f32)| bounds.origin + gpui::point(px(x * scale), px(y * scale));
        let color = window.text_style().color;
        for points in strokes {
            let width = if matches!(icon, Icon::SiteControls) { 1.25 } else { 1.5 };
            let mut path = gpui::PathBuilder::stroke(px(width * scale));
            for (index, point) in points.into_iter().enumerate() {
                if index == 0 { path.move_to(at(point)); } else { path.line_to(at(point)); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
        for contours in fills {
            let mut path = gpui::PathBuilder::fill();
            for contour in contours {
                for (index, point) in contour.into_iter().enumerate() {
                    if index == 0 { path.move_to(at(point)); } else { path.line_to(at(point)); }
                }
                path.close();
            }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
    }).size(px(size)).flex_none().into_any_element()
}

/// `icon` drawn at `scale` around the center of its 18 px slot, for
/// press and appear animations.
pub(crate) fn scaled(icon: Icon, scale: f32) -> AnyElement {
    div().size(px(18.)).flex_none().flex().items_center().justify_center()
        .child(control_sized(icon, 18. * scale.max(0.01))).into_any_element()
}

/// The plus as two strokes turned by `angle` radians: open menus show it as a cross.
pub(crate) fn plus(angle: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let center = bounds.center();
        let (sin, cos) = angle.sin_cos();
        let color = window.text_style().color;
        for (dx, dy) in [(5.5f32, 0.), (0., 5.5f32)] {
            let (x, y) = (dx * cos - dy * sin, dx * sin + dy * cos);
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            path.move_to(center - gpui::point(px(x), px(y)));
            path.line_to(center + gpui::point(px(x), px(y)));
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
    }).size(px(18.)).flex_none().into_any_element()
}

/// A checkmark at `scale`, erased from its start as `trim` goes 0 → 1.
pub(crate) fn check(trim: f32, scale: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let points = [(4.5f32, 9.4f32), (7.6, 12.5), (13.5, 5.6)];
        let lengths: Vec<f32> = points.windows(2).map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1)).collect();
        let total: f32 = lengths.iter().sum();
        let mut skip = total * trim.clamp(0., 1.);
        if skip >= total { return; }
        let center = bounds.center();
        let at = |(x, y): (f32, f32)| center + gpui::point(px((x - 9.) * scale), px((y - 9.) * scale));
        let mut path = gpui::PathBuilder::stroke(px(1.6));
        let mut started = false;
        for (pair, length) in points.windows(2).zip(&lengths) {
            if skip >= *length { skip -= length; continue; }
            let t = skip / length;
            skip = 0.;
            let from = (pair[0].0 + (pair[1].0 - pair[0].0) * t, pair[0].1 + (pair[1].1 - pair[0].1) * t);
            if !started { path.move_to(at(from)); started = true; }
            path.line_to(at(pair[1]));
        }
        if let Ok(path) = path.build() { window.paint_path(path, window.text_style().color); }
    }).size(px(18.)).flex_none().into_any_element()
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn bundled_ionicons_preserve_valid_curves_and_exclude_brand_glyphs() {
        assert_eq!(IONICONS.len(), 1108);
        for name in ["pizza-outline", "link-outline", "folder-outline", "arrow-back-outline", "close-outline", "volume-mute-outline"] {
            assert!(IONICONS.contains_key(name), "{name}");
        }
        for (name, commands) in IONICONS.iter() {
            assert!(!name.starts_with("logo-"));
            assert!(!commands.is_empty(), "{name}");
            for command in commands {
                let expected = match command[0] { 0. | 1. => 3, 2. => 5, 3. => 7, 4. => 1, _ => panic!("unknown command in {name}") };
                assert_eq!(command.len(), expected, "{name}");
                assert!(command.iter().all(|value| value.is_finite()), "{name}");
            }
        }
    }
}

pub(crate) fn folder(custom: Option<&str>) -> AnyElement {
    custom.map(glyph).unwrap_or_else(|| control(Icon::Folder))
}

impl BrowserWindow {
    pub(crate) fn edit_icon(&mut self, target: IconTarget, cx: &mut Context<Self>) {
        let icon = match target {
            IconTarget::Folder(id) => self.browser.folder(id).and_then(|folder| folder.custom_icon.as_deref()),
            IconTarget::Tab(id) => self.browser.item_icon(id),
            IconTarget::Space(id) => self.browser.spaces.iter().find(|space| space.id == id).and_then(|space| space.custom_icon.as_deref()),
        }.unwrap_or_default().to_string();
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Icon(target);
            bar.selected = 0;
            bar.input.update(cx, |input, cx| { input.set_placeholder("Enter one symbol or emoji…", cx); input.set_text(icon, cx); });
        }
        cx.notify();
    }
}
