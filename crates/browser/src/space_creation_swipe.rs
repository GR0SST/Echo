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
        self.sidebar_navigation.controller.draft().is_some_and(|draft| draft.origin == origin)
                && (self.sidebar_navigation.controller.selected() == SidebarPage::CreateSpace || self.sidebar_navigation.creation_exiting)
                && self.sidebar_navigation.controller.pages(&self.browser).iter()
                    .position(|page| *page == SidebarPage::CreateSpace)
                    .is_some_and(|index| (index as f32 - self.pager.position).abs() < 1.)
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
        let default_theme = space.theme.is_none();
        let pigment: Hsla = if default_theme { gpui::rgb(0x1f2aff).into() }
            else { crate::theme_editor::color(space.theme().colors[0]).into() };
        let progress = pull.progress;
        let draw = move |bounce: f32| canvas(|_, _, _| (), move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = 15. * (1. + bounce);
            let armed = progress >= 1.;
            let mut outline = pigment;
            outline.l *= 0.38;
            let mut body = pigment;
            if armed { body.l *= if default_theme { 0.8 } else { 0.38 }; }
            let ink = if default_theme || armed { hsla(0., 0., 1., 0.95) } else { outline };
            let mut disk = gpui::PathBuilder::fill();
            let mut track = gpui::PathBuilder::stroke(px(2.));
            for i in 0..=96 {
                let angle = i as f32 / 96. * std::f32::consts::TAU;
                let point = center + point(px(radius * angle.cos()), px(radius * angle.sin()));
                if i == 0 { disk.move_to(point); track.move_to(point); }
                else { disk.line_to(point); track.line_to(point); }
            }
            disk.close();
            if let Ok(path) = disk.build() { window.paint_path(path, body); }
            if let Ok(path) = track.build() { window.paint_path(path, ink.opacity(0.25)); }
            // Progress travels along the outline clockwise from noon. The
            // interior stays solid; reversing the gesture unwinds this arc.
            let mut arc = gpui::PathBuilder::stroke(px(2.));
            let steps = (96. * progress).ceil().max(1.) as usize;
            for i in 0..=steps {
                let angle = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * progress * i as f32 / steps as f32;
                let point = center + point(px(radius * angle.cos()), px(radius * angle.sin()));
                if i == 0 { arc.move_to(point); } else { arc.line_to(point); }
            }
            if let Ok(path) = arc.build() { window.paint_path(path, ink); }
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
