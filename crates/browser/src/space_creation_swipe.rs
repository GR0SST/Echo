//! Reversible pull-to-create feedback. A full pull arms the gesture; only
//! finger release opens the creation form. No workspace identity is allocated.
use super::*;
use crate::sidebar_pages::SidebarPage;

// The source rows shift about 24 px in the supplied 240 px Arc sidebar.
const EDGE_TRAVEL: f32 = 0.10;

pub(super) struct CreationPull {
    pub origin: SpaceId,
    pub progress: f32,
    pub crossings: u64,
}
impl BrowserWindow {
    pub(crate) fn space_creation_mark_visible(&self, origin: SpaceId) -> bool {
        self.pager.creation_pull.as_ref().is_some_and(|pull| pull.origin == origin && pull.progress > 0.)
            || (self.sidebar_navigation.controller.draft().is_some_and(|draft| draft.origin == origin)
                && (self.sidebar_navigation.controller.selected() == SidebarPage::CreateSpace || self.sidebar_navigation.creation_exiting)
                && self.sidebar_navigation.controller.pages(&self.browser).iter()
                    .position(|page| *page == SidebarPage::CreateSpace)
                    .is_some_and(|index| (index as f32 - self.pager.position).abs() < 1.))
    }

    pub(crate) fn space_creation_pull_progress(&self) -> f32 {
        self.pager.creation_pull.as_ref().map_or(0., |pull| pull.progress)
    }

    pub(super) fn update_creation_pull(&mut self, origin: SpaceId, progress: f32, cx: &mut Context<Self>) {
        let progress = progress.clamp(0., 1.);
        let previous = self.pager.creation_pull.as_ref();
        let crossings = previous.map_or(0, |pull| pull.crossings);
        let crossed = previous.map_or(progress >= 1., |pull| (pull.progress >= 1.) != (progress >= 1.));
        self.pager.creation_pull = Some(CreationPull { origin, progress, crossings: crossings + u64::from(crossed) });
        if crossed && crate::feedback::enabled(self, browser_core::settings::Toggle::HapticFeedback, cx) {
            crate::native::reorder_haptic();
        }
    }

    pub(super) fn creation_pull_position(&self, start: f32, progress: f32) -> f32 {
        start + EDGE_TRAVEL * progress.clamp(0., 1.15)
    }

    pub(super) fn settle_creation_pull(&mut self, finished: bool) {
        if let Some(pull) = &mut self.pager.creation_pull {
            if finished { self.pager.creation_pull = None; return; }
            if let Some(index) = self.sidebar_navigation.controller.pages(&self.browser).iter()
                .position(|page| *page == SidebarPage::Space(pull.origin)) {
                pull.progress = ((self.pager.position - index as f32) / EDGE_TRAVEL).clamp(0., 1.);
            }
        }
    }

    pub(super) fn render_creation_pull(&self) -> Option<AnyElement> {
        let pull = self.pager.creation_pull.as_ref().filter(|pull| pull.progress > 0.)?;
        let space = self.browser.spaces.iter().find(|space| space.id == pull.origin)?;
        let pigment: Hsla = crate::theme_editor::color(space.theme().colors[0]).into();
        let progress = pull.progress;
        let draw = move |bounce: f32| canvas(|_, _, _| (), move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = 15. * (1. + bounce);
            let armed = progress >= 1.;
            let mut fill = pigment;
            let mut outline = pigment;
            outline.l *= 0.38;
            if armed { fill = outline; }
            let mut ring = gpui::PathBuilder::stroke(px(1.5));
            let mut segment = gpui::PathBuilder::fill();
            let cut = radius * (1. - 2. * progress);
            let mut count = 0;
            for i in 0..=96 {
                let angle = i as f32 / 96. * std::f32::consts::TAU;
                let x = radius * angle.cos();
                let y = radius * angle.sin();
                let point = center + point(px(x), px(y));
                if i == 0 { ring.move_to(point); } else { ring.line_to(point); }
                if y >= cut {
                    if count == 0 { segment.move_to(point); } else { segment.line_to(point); }
                    count += 1;
                }
            }
            segment.close();
            if count > 2 { if let Ok(path) = segment.build() { window.paint_path(path, fill); } }
            if let Ok(path) = ring.build() { window.paint_path(path, outline); }
            let ink = if armed { hsla(0., 0., 1., 0.95) } else { outline };
            let mut plus = gpui::PathBuilder::stroke(px(1.7));
            plus.move_to(center - point(px(6.), px(0.))); plus.line_to(center + point(px(6.), px(0.)));
            plus.move_to(center - point(px(0.), px(6.))); plus.line_to(center + point(px(0.), px(6.)));
            if let Ok(path) = plus.build() { window.paint_path(path, ink); }
        }).size_full();
        let circle = div().size_full();
        let circle = if pull.crossings > 0 {
            circle.with_animation(SharedString::from(format!("creation-pull-bounce-{}-{}", self.pager.creation_gesture, pull.crossings)),
                Animation::new(Duration::from_millis(280)), move |circle, t| {
                    circle.child(draw(0.18 * (std::f32::consts::PI * t).sin() * (1. - t)))
                }).into_any_element()
        } else { circle.child(draw(0.)).into_any_element() };
        Some(div().id("space-creation-pull").debug_selector(|| "space-creation-pull".into())
            .absolute().top(gpui::relative(0.5)).mt(px(-22.))
            .right(px(-38. + 44. * progress)).size(px(44.)).child(circle).into_any_element())
    }
}
