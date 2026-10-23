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
    Search, External, History, Enter, Restore, Up, Down, Check, Mute, Command,
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

fn ionicon(name: &'static str) -> AnyElement {
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
    }).size(px(18.)).flex_none().into_any_element()
}

pub(crate) fn control(icon: Icon) -> AnyElement {
    let name = match icon {
        Icon::Back => "arrow-back-outline", Icon::Forward => "arrow-forward-outline",
        Icon::Reload => "reload-outline", Icon::Close => "close-outline",
        Icon::Minus => "remove-outline", Icon::Plus => "add-outline",
        Icon::Folder => "folder-outline", Icon::Search => "search-outline",
        Icon::External => "open-outline", Icon::History => "time-outline",
        Icon::Enter => "return-down-back-outline", Icon::Restore => "arrow-undo-outline",
        Icon::Up => "arrow-up-outline", Icon::Down => "arrow-down-outline",
        Icon::Check => "checkmark-outline", Icon::Mute => "volume-mute-outline",
        Icon::Sidebar | Icon::Split | Icon::Command => return chrome_frame(icon),
    };
    ionicon(name)
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

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn bundled_ionicons_preserve_valid_curves_and_exclude_brand_glyphs() {
        assert_eq!(IONICONS.len(), 1108);
        for name in ["pizza-outline", "folder-outline", "arrow-back-outline", "close-outline", "volume-mute-outline"] {
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
