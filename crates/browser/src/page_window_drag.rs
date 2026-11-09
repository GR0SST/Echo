//! Settings → Advanced → "Allow window dragging from the top of webpages".
//!
//! Behavior measured in Arc 1.169 (docs/WINDOW_DRAG_FROM_PAGE.md):
//! - the draggable band is every page surface that lies within the top
//!   [`BAND_HEIGHT`] points of the window, with or without the sidebar
//!   and Toolbar (the Toolbar only pushes the page further into it);
//! - a press in the band still reaches the page, so clicks and double
//!   clicks work and double click does not zoom the window;
//! - the first pointer movement ends the page's press (it gets its mouse
//!   up) and hands the rest of the gesture to the system window move;
//! - a press on a link keeps the page's own link drag;
//! - the toggle applies live, to existing tabs.
//!
//! The policy lives here; platforms only report raw presses through
//! [`Press`] and perform the window move (`native::set_page_window_drag_regions`).

/// Height of the band, measured from the top of the window content.
pub(crate) const BAND_HEIGHT: f64 = 70.;
/// Movement (points) after which a press in the band becomes a window move.
pub(crate) const DRAG_THRESHOLD: f64 = 1.;

pub(crate) type Rect = [f64; 4];

/// Regions (window coordinates, top-left origin) where a press may move
/// the window. Empty when the option is off or the pointer is on a link.
pub(crate) fn regions(enabled: bool, link_hovered: bool, pages: &[Rect]) -> Vec<Rect> {
    if !enabled || link_hovered { return Vec::new(); }
    pages.iter().filter_map(|&[x, y, width, height]| {
        let bottom = (y + height).min(BAND_HEIGHT);
        let top = y.max(0.);
        (width > 0. && bottom > top).then_some([x, top, width, bottom - top])
    }).collect()
}

fn contains(&[x, y, width, height]: &Rect, (px, py): (f64, f64)) -> bool {
    px >= x && px < x + width && py >= y && py < y + height
}

/// What the platform does with a mouse event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Deliver the event normally.
    Pass,
    /// End the page's press (send it a mouse up), then start the system
    /// window move with this event. The event itself is not delivered.
    StartWindowDrag,
    /// The window move owns the gesture: do not deliver the event.
    Swallow,
}

/// One primary-button gesture.
#[derive(Debug, Default)]
pub(crate) struct Press {
    origin: Option<(f64, f64)>,
    moving: bool,
}

impl Press {
    /// `plain` is false for presses that open a context menu (control).
    pub(crate) fn down(&mut self, point: (f64, f64), plain: bool, regions: &[Rect]) -> Step {
        *self = Self::default();
        if plain && regions.iter().any(|region| contains(region, point)) { self.origin = Some(point); }
        Step::Pass
    }

    pub(crate) fn dragged(&mut self, point: (f64, f64)) -> Step {
        if self.moving { return Step::Swallow; }
        let Some((x, y)) = self.origin else { return Step::Pass; };
        if (point.0 - x).abs() < DRAG_THRESHOLD && (point.1 - y).abs() < DRAG_THRESHOLD { return Step::Pass; }
        self.moving = true;
        Step::StartWindowDrag
    }

    pub(crate) fn up(&mut self) -> Step {
        let moving = self.moving;
        *self = Self::default();
        if moving { Step::Swallow } else { Step::Pass }
    }
}

/// Per-window inputs; publishes regions to the platform when they change.
#[derive(Debug, Default)]
pub(crate) struct PageWindowDrag {
    pub(crate) enabled: bool,
    pub(crate) link_hovered: bool,
    pub(crate) pages: Vec<Rect>,
    published: Vec<Rect>,
}

impl PageWindowDrag {
    pub(crate) fn published(&self) -> &[Rect] { &self.published }

    pub(crate) fn update(&mut self, view: Option<*mut std::ffi::c_void>, edit: impl FnOnce(&mut Self)) {
        edit(self);
        let next = regions(self.enabled, self.link_hovered, &self.pages);
        if next == self.published { return; }
        self.published = next;
        if let Some(view) = view { crate::native::set_page_window_drag_regions(view, &self.published); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: Rect = [300., 10., 900., 780.];

    #[test]
    fn off_or_on_a_link_has_no_region() {
        assert!(regions(false, false, &[PAGE]).is_empty());
        assert!(regions(true, true, &[PAGE]).is_empty());
    }

    #[test]
    fn band_is_the_page_part_of_the_top_70_points() {
        assert_eq!(regions(true, false, &[PAGE]), vec![[300., 10., 900., 60.]]);
        // Toolbar shown: the page starts lower and keeps only its top 28 points.
        assert_eq!(regions(true, false, &[[300., 42., 900., 748.]]), vec![[300., 42., 900., 28.]]);
        // Split panes each get their own band; a page below the band gets none.
        assert_eq!(regions(true, false, &[[10., 10., 400., 700.], [420., 10., 400., 700.], [0., 80., 10., 10.]]).len(), 2);
    }

    #[test]
    fn a_click_reaches_the_page_and_movement_starts_a_window_move_once() {
        let band = regions(true, false, &[PAGE]);
        let mut press = Press::default();
        assert_eq!(press.down((500., 30.), true, &band), Step::Pass);
        assert_eq!(press.dragged((500.5, 30.)), Step::Pass);
        assert_eq!(press.up(), Step::Pass, "a click without movement belongs to the page");

        assert_eq!(press.down((500., 30.), true, &band), Step::Pass);
        assert_eq!(press.dragged((502., 30.)), Step::StartWindowDrag);
        assert_eq!(press.dragged((560., 40.)), Step::Swallow);
        assert_eq!(press.up(), Step::Swallow);
        assert_eq!(press.dragged((600., 40.)), Step::Pass);
    }

    #[test]
    fn presses_below_the_band_or_with_control_stay_with_the_page() {
        let band = regions(true, false, &[PAGE]);
        let mut press = Press::default();
        press.down((500., 70.), true, &band);
        assert_eq!(press.dragged((560., 70.)), Step::Pass);
        press.up();
        press.down((500., 30.), false, &band);
        assert_eq!(press.dragged((560., 30.)), Step::Pass);
        press.up();
        press.down((100., 30.), true, &band);
        assert_eq!(press.dragged((160., 30.)), Step::Pass, "the sidebar is not page content");
    }

    #[test]
    fn publishing_only_reports_changes() {
        let mut state = PageWindowDrag::default();
        state.update(None, |state| state.pages = vec![PAGE]);
        assert!(state.published().is_empty());
        state.update(None, |state| state.enabled = true);
        assert_eq!(state.published(), &[[300., 10., 900., 60.]]);
        state.update(None, |state| state.link_hovered = true);
        assert!(state.published().is_empty());
    }
}
