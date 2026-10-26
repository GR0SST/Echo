//! Portable browser controls and user-selected emoji/catalog symbols.
use crate::window::{BrowserWindow, Target};
use browser_core::{SpaceId, TabId};
use gpui::{div, prelude::*, px, AnyElement, Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconTarget { Tab(TabId), Space(SpaceId), Folder(browser_core::folders::FolderId) }

pub(crate) fn glyph(icon: &str) -> AnyElement {
    if icon == "shape:circle" {
        return div().size(px(18.)).flex_none().flex().items_center().justify_center().child(gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
            let mut dot = gpui::fill(gpui::Bounds::new(bounds.origin + gpui::point(px(2.), px(2.)), gpui::size(px(14.), px(14.))), window.text_style().color);
            dot.corner_radii = px(7.).into(); window.paint_quad(dot);
        }).size(px(18.))).into_any_element();
    }
    if let Some(name) = icon.strip_prefix("ion:") {
        return ionicon(name, 18., 1.);
    }
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

/// Up/down chevrons use the same geometry as sidebar disclosures.
pub(crate) fn chevron(up: bool) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let color = window.text_style().color;
        let points = if up { [(4.5,11.25),(9.,6.75),(13.5,11.25)] }
            else { [(4.5,6.75),(9.,11.25),(13.5,6.75)] };
        let points = points.map(|(x,y)| bounds.origin + gpui::point(px(x),px(y)));
        let mut path = gpui::PathBuilder::stroke(px(1.5));
        path.move_to(points[0]); path.line_to(points[1]); path.line_to(points[2]);
        if let Ok(path) = path.build() { window.paint_path(path,color); }
    }).size(px(18.)).flex_none().into_any_element()
}

/// Escape's broken circle/arrow, on the same fixed 18 px control grid.
pub(crate) fn escape(size: f32) -> AnyElement {
    gpui::canvas(|_,_,_|(),move|bounds,_,window,_| {
        let scale=size/18.;let at=|x,y|bounds.origin+gpui::point(px(x*scale),px(y*scale));
        let mut path=gpui::PathBuilder::stroke(px(1.65*scale));
        let points=arc(9.,9.,5.5,-2.5,2.9);
        path.move_to(at(points[0].0,points[0].1));
        for (x,y) in points.into_iter().skip(1) {path.line_to(at(x,y));}
        path.move_to(at(3.5,3.5));path.line_to(at(7.5,3.5));
        path.move_to(at(3.5,3.5));path.line_to(at(3.5,7.5));
        path.move_to(at(3.5,3.5));path.line_to(at(9.,9.));
        if let Ok(path)=path.build(){window.paint_path(path,window.text_style().color);}
    }).size(px(size)).flex_none().into_any_element()
}

/// The Settings toolbar's keyboard, measured in its 22 px control slot.
pub(crate) fn keyboard(size: f32) -> AnyElement {
    div().debug_selector(|| "settings-keyboard-icon".into()).size(px(size)).flex_none()
        .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
            let scale = size / 22.;
            let at = |x, y| bounds.origin + gpui::point(px(x * scale), px(y * scale));
            let color = window.text_style().color;
            window.paint_quad(gpui::quad(gpui::Bounds::new(at(0., 5.), gpui::size(px(22. * scale), px(14. * scale))), px(2. * scale), gpui::transparent_black(), px(1.5 * scale), color, gpui::BorderStyle::Solid));
            for y in [8.25, 11.25] {
                for x in [4., 6.75, 9.5, 12.25, 15., 17.75] {
                    window.paint_quad(gpui::fill(gpui::Bounds::new(at(x, y), gpui::size(px(1.5 * scale), px(1.5 * scale))), color));
                }
            }
            for (x, width) in [(4., 1.5), (7., 10.), (18., 1.5)] {
                window.paint_quad(gpui::fill(gpui::Bounds::new(at(x, 14.), gpui::size(px(width * scale), px(1.3 * scale))), color));
            }
        }).size_full()).into_any_element()
}

/// The dock menu uses compact, fixed control geometry, distinct from toolbar glyphs.
pub(crate) fn new_menu_control(icon: Icon, background: gpui::Rgba) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let color = window.text_style().color;
        let at = |x, y| bounds.origin + gpui::point(px(x), px(y));
        let paths: Vec<Vec<(f32,f32)>> = match icon {
            Icon::NewSpace => {
                let back = rounded_rect(3.,2.,11.,17.,1.5).into_iter().map(|(x,y)| {
                    let (x,y)=(x-8.5,y-10.5); let a=-0.12_f32;
                    (8.5+x*a.cos()-y*a.sin(),10.5+x*a.sin()+y*a.cos())
                }).collect();
                vec![back,rounded_rect(8.,7.,11.5,14.5,1.3),vec![(13.75,11.),(13.75,18.)],vec![(10.25,14.5),(17.25,14.5)]]
            }
            Icon::Folder => vec![vec![(2.5,7.),(2.5,6.),(3.5,5.),(8.,5.),(10.,7.),(19.,7.),(20.,8.),(20.,18.5),(19.,19.5),(3.5,19.5),(2.5,18.5),(2.5,7.)],vec![(4.,10.),(18.5,10.)]],
            Icon::Split => vec![vec![(20.,6.),(20.,5.),(19.,4.),(5.,4.),(4.,5.),(4.,18.),(5.,19.),(19.,19.),(20.,18.)],vec![(11.5,4.),(11.5,19.)],vec![(17.,8.5),(17.,14.5)],vec![(14.,11.5),(20.,11.5)]],
            Icon::NewTab => vec![rounded_rect(3.5,4.,15.5,15.,1.5),vec![(11.25,8.),(11.25,15.)],vec![(7.75,11.5),(14.75,11.5)]],
            Icon::Boost => vec![vec![(12.,3.5),(14.,2.),(21.,9.),(21.,10.5),(16.,15.5),(14.,15.5),(7.5,9.),(7.5,7.),(10.,4.5),(12.,3.5)],vec![(9.5,11.5),(3.5,17.5),(3.5,19.),(5.5,21.),(7.,21.),(13.,15.)],vec![(5.5,18.),(7.,16.5)]],
            _ => Vec::new(),
        };
        for (index,points) in paths.into_iter().enumerate() {
            if matches!(icon,Icon::NewSpace) && index==1 {
                let mut mask=gpui::PathBuilder::fill();
                for (i,(x,y)) in points.iter().enumerate(){if i==0{mask.move_to(at(*x,*y));}else{mask.line_to(at(*x,*y));}}
                mask.close();if let Ok(mask)=mask.build(){window.paint_path(mask,background);}
            }
            let mut path=gpui::PathBuilder::stroke(px(1.8));
            for (i,(x,y)) in points.iter().enumerate() { if i==0 {path.move_to(at(*x,*y));} else {path.line_to(at(*x,*y));} }
            if let Ok(path)=path.build(){window.paint_path(path,color);}
            for (x,y) in points { window.paint_quad(gpui::fill(gpui::Bounds::new(at(x-0.9,y-0.9),gpui::size(px(1.8),px(1.8))),color).corner_radii(px(0.9))); }
        }
        if matches!(icon, Icon::Boost) {
            let mut path=gpui::PathBuilder::fill();
            for (i,(x,y)) in [(5.,1.5),(6.,4.),(8.5,5.),(6.,6.),(5.,8.5),(4.,6.),(1.5,5.),(4.,4.)].into_iter().enumerate() {if i==0{path.move_to(at(x,y));}else{path.line_to(at(x,y));}}
            path.close();if let Ok(path)=path.build(){window.paint_path(path,color);}
        }
    }).size(px(24.)).flex_none().into_any_element()
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
    Search, Globe, External, History, Enter, Info, Arrow, Restore, Up, Down, Check, Mute, Command, AirTraffic, Link, Calculator,
    HapticFeedback, SoundEffects,
    SiteControls, Library, Tidy, Share, Boost, Capture, Screenshot, Lock, Unlock, NewSpace, NewTab, Incognito,
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

pub(crate) fn ionicon(name: &str, size: f32, zoom: f32) -> AnyElement {
    let name = name.to_owned();
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let Some(commands) = IONICONS.get(&name) else { return; };
        let scale = f32::from(bounds.size.width) / 512. * zoom;
        let center = bounds.center();
        let at = |x, y| center + gpui::point(px((x - 256.) * scale), px((255. - y) * scale));
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

/// Inline tab audio control; its geometry stays fixed when the title elides.
pub(crate) fn speaker(muted: bool) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let offset = if muted { 3. } else { 0. };
        let mut paths = vec![vec![(1.5 + offset, 7.), (4. + offset, 7.),
            (7.5 + offset, 4.), (7.5 + offset, 14.), (4. + offset, 11.),
            (1.5 + offset, 11.), (1.5 + offset, 7.)]];
        if muted { paths.push(vec![(2.5, 2.5), (15.5, 15.5)]); }
        else { for radius in [4., 6., 8.] { paths.push(arc(7.5, 9., radius, -0.72, 0.72)); } }
        for points in paths {
            let mut path = gpui::PathBuilder::stroke(px(1.1));
            for (index, (x, y)) in points.into_iter().enumerate() {
                let point = bounds.origin + gpui::point(px(x), px(y));
                if index == 0 { path.move_to(point); } else { path.line_to(point); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, window.text_style().color); }
        }
    }).size(px(18.)).flex_none().into_any_element()
}

/// Fixed media transport geometry measured in the native 33 px strip.
pub(crate) fn media_transport(action: &str) -> AnyElement {
    let action=action.to_owned();
    gpui::canvas(|_,_,_|(),move|bounds,_,window,_|{
        let color=window.text_style().color;
        let at=|x:f32,y:f32|bounds.origin+gpui::point(px(x),px(y));
        if action=="pause" {
            for x in [5.75,11.25] {
                window.paint_quad(gpui::fill(gpui::Bounds::new(at(x,4.),gpui::size(px(3.5),px(12.))),color).corner_radii(px(1.)));
            }
        }else{
            let skip=action!="play";
            let back=action=="previous";
            let offset=if skip{if back{1.}else{-1.}}else{0.};
            let transform=|x:f32,y:f32|at((if back{20.-x}else{x})+offset,if skip{4.+(y-4.)*11./12.}else{y});
            let (left,right)=if skip{(3.5,14.)}else{(6.,16.)};
            let mut path=gpui::PathBuilder::fill();
            path.move_to(transform(left,4.8));
            path.curve_to(transform(left+1.4,4.1),transform(left,3.4));
            path.line_to(transform(right-0.4,9.2));
            path.curve_to(transform(right-0.4,10.8),transform(right+1.,10.));
            path.line_to(transform(left+1.4,15.9));
            path.curve_to(transform(left,15.2),transform(left,16.6));
            path.close();
            if let Ok(path)=path.build(){window.paint_path(path,color);}
            if skip {
                let x=if back{3.}else{14.};
                window.paint_quad(gpui::fill(gpui::Bounds::new(at(x+offset,4.),gpui::size(px(3.),px(11.))),color).corner_radii(px(1.)));
            }
        }
    }).size(px(20.)).flex_none().into_any_element()
}

/// Filled, variable-wave speaker used by the observed 33 px media strip.
pub(crate) fn media_speaker(volume: f32, muted: bool) -> AnyElement {
    gpui::canvas(|_,_,_|(),move|bounds,_,window,_|{
        let color=window.text_style().color;
        let waves=if muted||volume==0.{0}else{(volume*3.).ceil().clamp(1.,3.) as usize};
        let offset=if waves==3{-1.5}else{0.};
        let at=|x,y|bounds.origin+gpui::point(px(x+offset),px(y));
        let mut body=gpui::PathBuilder::fill();
        for (index,(x,y)) in [(0.5,7.),(4.,7.),(9.,3.5),(9.,16.5),(4.,13.),(0.5,13.)].into_iter().enumerate(){
            let p=at(x,y);
            if index==0{body.move_to(p);}else{body.line_to(p);}
        }
        body.close();if let Ok(path)=body.build(){window.paint_path(path,color);}
        for (radius,angle) in [(4.5,0.72),(8.,0.67),(11.5,0.6)].into_iter().take(waves){
            let points=arc(8.,10.,radius,-angle,angle);
            let mut path=gpui::PathBuilder::stroke(px(1.35));
            for (index,(x,y)) in points.iter().copied().enumerate(){
                let p=at(x,y);
                if index==0{path.move_to(p);}else{path.line_to(p);}
            }
            if let Ok(path)=path.build(){window.paint_path(path,color);}
            for (x,y) in [points[0],*points.last().unwrap()]{
                window.paint_quad(gpui::fill(gpui::Bounds::new(at(x-0.675,y-0.675),gpui::size(px(1.35),px(1.35))),color).corner_radii(px(0.675)));
            }
        }
        if muted||volume==0.{
            let mut slash=gpui::PathBuilder::stroke(px(1.5));slash.move_to(bounds.origin+gpui::point(px(2.),px(2.)));slash.line_to(bounds.origin+gpui::point(px(18.),px(18.)));
            if let Ok(path)=slash.build(){window.paint_path(path,color);}
        }
    }).size(px(20.)).flex_none().into_any_element()
}

/// The mini-player header's two overlapping 18 px window outlines.
pub(crate) fn media_pip() -> AnyElement {
    gpui::canvas(|_,_,_|(),move|bounds,_,window,_|{
        let at=|x,y|bounds.origin+gpui::point(px(x),px(y));
        let mut back=gpui::PathBuilder::stroke(px(1.15));
        back.move_to(at(7.,11.));back.line_to(at(3.,11.));back.curve_to(at(1.5,9.5),at(1.5,11.));
        back.line_to(at(1.5,4.));back.curve_to(at(3.,2.5),at(1.5,2.5));
        back.line_to(at(11.5,2.5));back.curve_to(at(13.,4.),at(13.,2.5));back.line_to(at(13.,7.));
        if let Ok(path)=back.build(){window.paint_path(path,window.text_style().color);}
        let mut front=gpui::PathBuilder::stroke(px(1.15));
        for(index,(x,y))in rounded_rect(7.,7.,9.5,7.5,1.5).into_iter().enumerate(){if index==0{front.move_to(at(x,y));}else{front.line_to(at(x,y));}}
        if let Ok(path)=front.build(){window.paint_path(path,window.text_style().color);}
    }).size(px(18.)).flex_none().into_any_element()
}

/// A catalog-backed control scaled to `size`; frame controls keep the 18 px grid.
pub(crate) fn control_sized(icon: Icon, size: f32) -> AnyElement { zoomed(icon, size, 1.) }

/// `icon` in a `size` slot, drawn at `zoom` around the slot's center. The
/// slot keeps its layout size so animated zoom stays smooth and sub-pixel.
fn zoomed(icon: Icon, size: f32, zoom: f32) -> AnyElement {
    let name = match icon {
        Icon::Globe => return command_globe(size, zoom),
        Icon::HapticFeedback | Icon::SoundEffects => return feedback_glyph(matches!(icon,Icon::HapticFeedback),size,zoom),
        Icon::Back => "arrow-back-outline", Icon::Forward => "arrow-forward-outline",
        Icon::Reload => "reload-outline", Icon::Close => "close-outline",
        Icon::Minus => "remove-outline", Icon::Plus => "add-outline",
        Icon::Folder => "folder-outline", Icon::Search => "search-outline",
        Icon::Info => "information-circle-outline", Icon::Arrow => "arrow-forward-outline",
        Icon::Calculator => "calculator-outline",
        Icon::External => "open-outline", Icon::History => "time-outline",
        Icon::Enter => "return-down-back-outline", Icon::Restore => "arrow-undo-outline",
        Icon::Up => "arrow-up-outline", Icon::Down => "arrow-down-outline",
        Icon::Check => "checkmark-outline", Icon::Mute => "volume-mute-outline",
        Icon::Share => "share-outline", Icon::Boost => "color-wand-outline",
        Icon::Capture => "camera-outline", Icon::Screenshot => "scan-outline",
        Icon::NewSpace => "duplicate-outline", Icon::Lock => "lock-closed", Icon::Unlock => "lock-open",
        Icon::Incognito => "eye-off-outline",
        Icon::Sidebar | Icon::Split | Icon::Command => return chrome_frame(icon),
        Icon::AirTraffic | Icon::Link | Icon::SiteControls | Icon::Library | Icon::Tidy | Icon::NewTab => return drawn(icon, size, zoom),
    };
    ionicon(name, size, zoom)
}

/// The Command Bar's neutral page symbol has one equator and one meridian.
fn command_globe(size: f32, zoom: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let scale = size / 16. * zoom;
        let mut meridian = arc(8., 8., 5.75, 0., std::f32::consts::TAU);
        for (x, _) in &mut meridian { *x = 8. + (*x - 8.) * 0.43; }
        for points in [arc(8., 8., 5.75, 0., std::f32::consts::TAU), meridian, vec![(2.25, 8.), (13.75, 8.)]] {
            let mut path = gpui::PathBuilder::stroke(px(1.25 * scale));
            for (index, (x, y)) in points.into_iter().enumerate() {
                let point = bounds.center() + gpui::point(px((x - 8.) * scale), px((y - 8.) * scale));
                if index == 0 { path.move_to(point); } else { path.line_to(point); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, window.text_style().color); }
        }
    }).size(px(size)).flex_none().into_any_element()
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
fn drawn(icon: Icon, size: f32, zoom: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let (strokes, fills): (Vec<Vec<(f32, f32)>>, Vec<Vec<Vec<(f32, f32)>>>) = match icon {
            // Horizontal airplane used by the Air Traffic Control command.
            Icon::AirTraffic => (vec![], vec![vec![vec![(17.,9.),(15.,7.8),(10.,7.8),(6.,1.),(4.5,1.),(6.7,7.8),(4.2,7.8),(2.,5.3),(1.,5.3),(2.,9.),(1.,12.7),(2.,12.7),(4.2,10.2),(6.7,10.2),(4.5,17.),(6.,17.),(10.,10.2),(15.,10.2)]]]),
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
        let scale = size / 18. * zoom;
        let center = bounds.center();
        let at = |(x, y): (f32, f32)| center + gpui::point(px((x - 9.) * scale), px((y - 9.) * scale));
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
    zoomed(icon, 18., scale.max(0.01))
}

/// The plus as two strokes turned by `angle` radians: open menus show it as a cross.
pub(crate) fn plus(angle: f32, scale: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let center = bounds.center();
        let (sin, cos) = angle.sin_cos();
        let color = window.text_style().color;
        let arm = 5.5 * scale;
        for (dx, dy) in [(arm, 0.), (0., arm)] {
            let (x, y) = (dx * cos - dy * sin, dx * sin + dy * cos);
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            path.move_to(center - gpui::point(px(x), px(y)));
            path.line_to(center + gpui::point(px(x), px(y)));
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        }
    }).size(px(18.)).flex_none().into_any_element()
}

/// A checkmark at `scale`, erased from its end back to its start as `trim` goes 0 → 1.
pub(crate) fn check(trim: f32, scale: f32) -> AnyElement {
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let points = [(4.5f32, 9.4f32), (7.6, 12.5), (13.5, 5.6)];
        let lengths: Vec<f32> = points.windows(2).map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1)).collect();
        let total: f32 = lengths.iter().sum();
        let mut keep = total * (1. - trim.clamp(0., 1.));
        if keep <= 0. { return; }
        let center = bounds.center();
        let at = |(x, y): (f32, f32)| center + gpui::point(px((x - 9.) * scale), px((y - 9.) * scale));
        let mut path = gpui::PathBuilder::stroke(px(1.6));
        path.move_to(at(points[0]));
        for (pair, length) in points.windows(2).zip(&lengths) {
            if keep >= *length { path.line_to(at(pair[1])); keep -= length; continue; }
            let t = keep / length;
            path.line_to(at((pair[0].0 + (pair[1].0 - pair[0].0) * t, pair[0].1 + (pair[1].1 - pair[0].1) * t)));
            break;
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
        for name in ["pizza-outline", "link-outline", "folder-outline", "arrow-back-outline", "close-outline", "volume-mute-outline", "eye-off-outline"] {
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

pub(crate) fn folder(custom: Option<&str>) -> AnyElement { folder_state(custom, false) }

pub(crate) fn folder_state(custom: Option<&str>, open: bool) -> AnyElement {
    let name = if open && custom.is_none() { "folder-open" } else { "folder" };
    let folder = div().size(px(18.)).flex_none().relative()
        .child(div().absolute().inset_0().text_color(gpui::white()).child(ionicon(name, 18., 1.)))
        .child(div().absolute().top(px(1.)).left(px(1.)).text_color(gpui::rgb(0x343bfa)).child(ionicon(name, 16., 1.)));
    folder.when_some(custom, |this, icon| this.child(div().absolute().inset_0().pt(px(2.)).flex().items_center().justify_center().text_color(gpui::white())
        .child(if icon == "shape:circle" { div().size(px(7.)).rounded_full().bg(gpui::white()).into_any_element() }
            else if let Some(name) = icon.strip_prefix("ion:") { ionicon(name, 10., 1.) }
            else { div().text_size(px(11.)).line_height(px(12.)).child(icon.to_string()).into_any_element() })))
        .into_any_element()
}

impl BrowserWindow {
    pub(crate) fn edit_icon(&mut self, target: IconTarget, cx: &mut Context<Self>) {
        if self.browser.is_blank() && !matches!(target, IconTarget::Tab(_)) { return; }
        let anchor = self.icon_anchors.borrow().get(&target).copied();
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::Icon(target);
            bar.icon_anchor = anchor;
            bar.icon_picker = crate::icon_catalog::Picker { catalog: if matches!(target, IconTarget::Folder(_)) { crate::icon_catalog::Catalog::Symbols } else { self.last_icon_catalog }, folder: matches!(target, IconTarget::Folder(_)), ..Default::default() };
            bar.selected = 0;
            bar.input.update(cx, |input, cx| { input.set_placeholder("Search", cx); input.set_text("", cx); });
        }
        cx.notify();
    }
}

/// Library artwork traced as our own geometry from the installed reference.
/// The 24 px slot includes a 3 px optical baseline correction. Selected artwork
/// gains an accent fill and white contours; the tile surface remains theme-owned.
pub(crate) fn library_section(section: crate::library::LibrarySection, selected: bool) -> AnyElement {
    use crate::library::LibrarySection as S;
    gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
        let ink = if selected { gpui::rgb(0xf4f4ff).into() } else { window.text_style().color };
        let accent = gpui::rgb(0x262ff5).into();
        let at = |x, y| bounds.origin + gpui::point(px(x), px(y + 3.));
        // Filled contours deliberately precede their matching outlines.
        let mut contours: Vec<(Vec<(f32, f32)>, bool, bool)> = Vec::new();
        let mut outline = |points| contours.push((points, false, false));
        match section {
            S::Media => {
                outline(vec![(2.,4.),(17.,2.),(18.,7.)]);
                outline(vec![(2.,4.),(3.5,17.),(7.,17.)]);
                outline(rounded_rect(7.,7.,17.,14.,2.));
            }
            S::Downloads => {
                if selected { contours.push((arc(12.,12.,9.5,0.,std::f32::consts::TAU),true,true)); }
                contours.push((arc(12.,12.,9.5,0.,std::f32::consts::TAU),false,false));
                contours.push((vec![(12.,7.),(12.,17.),(8.,13.),(12.,17.),(16.,13.)],false,false));
            }
            S::Easels => {
                outline(rounded_rect(3.,4.,18.,16.,2.));
                outline(vec![(3.,7.),(21.,7.)]);
                outline(vec![(15.,10.),(11.,9.),(10.,10.5),(15.,15.),(13.5,17.),(8.,12.5),(7.,14.),(11.5,18.)]);
                contours.push((rounded_rect(3.,3.,18.,4.,1.5),true,false));
            }
            S::Spaces => {
                outline(vec![(8.,20.),(5.,19.),(1.5,5.),(3.,3.),(12.,0.5),(14.,2.),(15.,5.)]);
                outline(rounded_rect(9.,5.,13.,19.,2.5));
            }
            S::Boosts => {
                outline(vec![(3.,10.),(1.5,5.),(3.,2.),(17.,0.),(20.,2.),(23.,16.),(21.,19.),(14.,21.)]);
                outline(vec![(7.,9.),(10.,5.),(18.,12.),(14.,17.),(7.,9.)]);
                outline(vec![(10.,14.),(7.,17.),(5.,16.),(1.,20.),(1.,22.),(3.,23.),(7.,19.),(7.,17.)]);
                outline(vec![(17.,4.),(17.,8.)]);
                outline(vec![(15.,6.),(19.,6.)]);
            }
            S::ArchivedTabs => {
                outline(rounded_rect(3.,4.,18.,4.,1.));
                outline(rounded_rect(5.,8.,14.,13.,1.5));
                outline(vec![(9.,12.),(15.,12.)]);
            }
        }
        if selected && section != S::Downloads {
            // Each backing panel has its own solid accent rather than tinting
            // the label or painting a blue tile behind the whole control.
            let panels = match section {
                S::Media => vec![vec![(2.,4.),(17.,2.),(18.,7.),(7.,7.),(7.,17.),(3.5,17.)], rounded_rect(7.,7.,17.,14.,2.)],
                S::Easels => vec![rounded_rect(3.,4.,18.,16.,2.)],
                S::Spaces => vec![vec![(1.5,5.),(12.,0.5),(15.,5.),(9.,5.),(9.,24.),(5.,19.)],rounded_rect(9.,5.,13.,19.,2.5)],
                S::Boosts => vec![vec![(3.,10.),(1.5,5.),(3.,2.),(17.,0.),(20.,2.),(23.,16.),(21.,19.),(14.,21.)]],
                S::ArchivedTabs => vec![rounded_rect(5.,8.,14.,13.,1.5)],
                _ => vec![],
            };
            for panel in panels.into_iter().rev() { contours.insert(0,(panel,true,true)); }
        }
        if section == S::Media {
            contours.push((vec![(7.,17.),(11.,13.),(14.,16.),(18.,12.),(24.,17.),(24.,20.),(22.,21.),(9.,21.),(7.,20.)],true,false));
            contours.push((arc(12.,11.,1.5,0.,std::f32::consts::TAU),true,false));
        }
        for (points, filled, blue) in contours {
            let mut path = if filled { gpui::PathBuilder::fill() } else { gpui::PathBuilder::stroke(px(1.5)) };
            for (index, (x,y)) in points.into_iter().enumerate() {
                if index == 0 { path.move_to(at(x,y)); } else { path.line_to(at(x,y)); }
            }
            if filled { path.close(); }
            if let Ok(path) = path.build() { window.paint_path(path, if blue { accent } else { ink }); }
        }
    }).size(px(24.)).flex_none().into_any_element()
}

/// Portable feedback controls drawn from the observed 16 px Settings geometry.
fn feedback_glyph(haptic: bool, size: f32, zoom: f32) -> AnyElement {
    gpui::canvas(|_,_,_|(),move|bounds,_,window,_|{
        let scale=size/16.*zoom;
        let inset=(size-16.*scale)/2.;
        let at=|x:f32,y:f32|bounds.origin+gpui::point(px(inset+x*scale),px(inset+y*scale));
        let color=window.text_style().color;
        if haptic {
            let mut hand=gpui::PathBuilder::fill();
            hand.move_to(at(8.5,14.5));
            hand.curve_to(at(5.5,12.5),at(6.7,14.3));
            hand.line_to(at(4.1,10.7));hand.curve_to(at(4.5,9.7),at(3.7,9.9));
            hand.curve_to(at(5.8,10.5),at(5.1,9.5));hand.line_to(at(6.6,11.3));
            hand.line_to(at(5.8,5.7));hand.curve_to(at(6.1,5.1),at(5.6,5.1));
            hand.curve_to(at(6.8,5.6),at(6.7,4.9));hand.line_to(at(8.2,9.2));
            hand.line_to(at(8.1,7.6));hand.curve_to(at(8.6,7.),at(8.,7.));
            hand.curve_to(at(9.2,7.5),at(9.1,6.8));hand.line_to(at(9.5,8.5));
            hand.line_to(at(9.8,7.2));hand.curve_to(at(10.5,7.),at(10.1,6.7));
            hand.curve_to(at(11.1,7.7),at(11.,7.1));hand.line_to(at(11.3,8.4));
            hand.line_to(at(11.7,7.8));hand.curve_to(at(12.4,8.3),at(12.2,7.5));
            hand.line_to(at(12.7,10.2));hand.curve_to(at(11.8,13.8),at(13.,12.2));
            hand.curve_to(at(8.5,14.5),at(10.8,15.4));hand.close();
            if let Ok(path)=hand.build(){window.paint_path(path,color);}
            let mut tap=gpui::PathBuilder::stroke(px(1.2*scale));
            tap.move_to(at(3.5,7.5));tap.curve_to(at(3.3,4.8),at(2.8,6.));
            tap.curve_to(at(5.3,3.),at(3.5,3.2));tap.curve_to(at(8.3,4.4),at(7.3,2.9));
            tap.curve_to(at(8.8,6.),at(8.8,5.));
            if let Ok(path)=tap.build(){window.paint_path(path,color.opacity(0.8));}
        } else {
            let mut body=gpui::PathBuilder::fill();
            for(index,(x,y))in[(1.5,6.),(4.,6.),(8.5,3.),(8.5,13.),(4.,10.),(1.5,10.)].into_iter().enumerate(){if index==0{body.move_to(at(x,y));}else{body.line_to(at(x,y));}}
            body.close();if let Ok(path)=body.build(){window.paint_path(path,color);}
            for(radius,angle)in[(3.2,0.7),(6.7,0.8)]{
                let points=arc(if radius<4.{7.4}else{6.8},8.,radius,-angle,angle);
                let mut path=gpui::PathBuilder::stroke(px(1.05*scale));
                for(index,(x,y))in points.into_iter().enumerate(){if index==0{path.move_to(at(x,y));}else{path.line_to(at(x,y));}}
                if let Ok(path)=path.build(){window.paint_path(path,color.opacity(0.87));}
            }
        }
    }).size(px(size)).flex_none().into_any_element()
}
