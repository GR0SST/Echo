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

pub(crate) fn control(icon: Icon) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        use std::f32::consts::{PI, TAU};
        let mut paths: Vec<Vec<(f32, f32)>> = match icon {
            Icon::Back => vec![vec![(13.5, 9.), (3.5, 9.)], vec![(8., 4.5), (3.5, 9.), (8., 13.5)]],
            Icon::Forward => vec![vec![(4.5, 9.), (14.5, 9.)], vec![(10., 4.5), (14.5, 9.), (10., 13.5)]],
            Icon::Reload => vec![arc(9., 9.5, 5.5, -2.5, -PI * 2.5), vec![(9., 4.), (12., 4.)], vec![(9., 1.), (12., 4.), (9., 7.)]],
            Icon::Sidebar => vec![rounded_rect(1.5, 3., 15., 12., 2.), vec![(6.5, 3.), (6.5, 15.)], vec![(3.5, 6.), (4.5, 6.)], vec![(3.5, 8.5), (4.5, 8.5)]],
            Icon::Split => vec![rounded_rect(2., 3., 14., 12., 1.5), vec![(9., 3.), (9., 15.)]],
            Icon::Close => vec![vec![(5., 5.), (13., 13.)], vec![(13., 5.), (5., 13.)]],
            Icon::Minus => vec![vec![(4.5, 9.), (13.5, 9.)]],
            Icon::Plus => vec![vec![(4., 9.), (14., 9.)], vec![(9., 4.), (9., 14.)]],
            Icon::Folder => vec![rounded_rect(1.5, 5.25, 15., 10.5, 1.75), vec![(1.5, 7.), (1.5, 4.5), (2., 3.25), (6.5, 3.25), (8.5, 5.25)]],
            Icon::Search => vec![arc(7.5, 7.5, 4.75, 0., TAU), vec![(11., 11.), (15.5, 15.5)]],
            Icon::External => vec![vec![(4., 14.), (14., 4.)], vec![(6., 4.), (14., 4.), (14., 12.)]],
            Icon::History => vec![arc(9., 9., 6.25, 0., TAU), vec![(9., 4.75), (9., 9.), (12., 10.5)]],
            Icon::Enter => vec![vec![(14., 4.), (14., 10.), (4., 10.)], vec![(7.5, 6.5), (4., 10.), (7.5, 13.5)]],
            Icon::Restore => vec![arc(9.5, 9.5, 5.5, -PI * 0.95, PI * 0.9), vec![(3.5, 3.), (3.5, 7.), (7.5, 7.)]],
            Icon::Up => vec![vec![(9., 14.), (9., 4.)], vec![(4.5, 8.5), (9., 4.), (13.5, 8.5)]],
            Icon::Down => vec![vec![(9., 4.), (9., 14.)], vec![(4.5, 9.5), (9., 14.), (13.5, 9.5)]],
            Icon::Check => vec![vec![(3.5, 9.), (7.25, 12.75), (14.5, 5.5)]],
            Icon::Mute => vec![vec![(3., 7.), (6., 7.), (10., 3.5), (10., 14.5), (6., 11.), (3., 11.), (3., 7.)], vec![(13., 7.), (16., 11.)], vec![(16., 7.), (13., 11.)]],
            Icon::Command => vec![rounded_rect(3., 3., 4., 4., 1.75), rounded_rect(11., 3., 4., 4., 1.75), rounded_rect(3., 11., 4., 4., 1.75), rounded_rect(11., 11., 4., 4., 1.75), rounded_rect(7., 7., 4., 4., 0.)],
        };
        let color = window.text_style().color;
        for points in paths.drain(..) {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (index, (x, y)) in points.iter().enumerate() {
                let point = bounds.origin + gpui::point(px(*x), px(*y));
                if index == 0 { path.move_to(point); } else { path.line_to(point); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
            // Curves are already smooth; only short polylines need round joins.
            for (index, (x, y)) in points.iter().enumerate() {
                if points.len() > 8 && index != 0 && index + 1 != points.len() { continue; }
                let mut cap = gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(px(*x - 0.75), px(*y - 0.75)), gpui::size(px(1.5), px(1.5))), color);
                cap.corner_radii = px(0.75).into();
                window.paint_quad(cap);
            }
        }
    }).size(px(18.)).flex_none().into_any_element()
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
