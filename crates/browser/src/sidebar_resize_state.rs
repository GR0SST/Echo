//! Pure sidebar resize calculations. The host owns visibility and retained open width.

pub(crate) const SIDEBAR_DEFAULT_WIDTH: f32 = 240.0;
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 164.0;
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 500.0;
pub(crate) const SIDEBAR_RESET_WIDTH: f32 = 228.0;
const COLLAPSE_THRESHOLD: f32 = 82.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SidebarResizeOutcome {
    /// Live width during updates; retained open width after finish or cancel.
    pub(crate) width: f32,
    pub(crate) visible: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SidebarResizeState {
    begin_width: f32,
    begin_visible: bool,
    begin_pointer_x: f32,
}

impl SidebarResizeState {
    /// `width` is the retained open width, even when the sidebar is hidden.
    /// The host maps its pinned-open mode to the observed mode-zero branch.
    /// Arc's mode-tag meaning and floating end behavior remain native gates.
    pub(crate) fn begin(width: f32, visible: bool, pointer_x: f32) -> Self {
        Self {
            begin_width: width,
            begin_visible: visible,
            begin_pointer_x: pointer_x,
        }
    }

    pub(crate) fn update(&self, pointer_x: f32) -> SidebarResizeOutcome {
        let raw = self.raw_width(pointer_x);
        let visible = raw >= COLLAPSE_THRESHOLD;
        SidebarResizeOutcome {
            width: if visible { clamp_width(raw) } else { 0.0 },
            visible,
        }
    }

    /// Consumes the gesture. A collapse retains the beginning's open width,
    /// never the transient zero width returned by `update`.
    pub(crate) fn finish(self, pointer_x: f32) -> SidebarResizeOutcome {
        let raw = self.raw_width(pointer_x);
        if self.begin_visible && raw <= COLLAPSE_THRESHOLD {
            SidebarResizeOutcome {
                width: clamp_width(self.begin_width),
                visible: false,
            }
        } else {
            SidebarResizeOutcome {
                width: clamp_width(raw),
                visible: true,
            }
        }
    }

    /// Used for both cancelled and failed gestures; restores the exact input.
    pub(crate) fn cancel(self) -> SidebarResizeOutcome {
        SidebarResizeOutcome {
            width: self.begin_width,
            visible: self.begin_visible,
        }
    }

    /// Double-click resets the open width without changing visibility.
    pub(crate) fn reset(visible: bool) -> SidebarResizeOutcome {
        SidebarResizeOutcome {
            width: SIDEBAR_RESET_WIDTH,
            visible,
        }
    }

    fn raw_width(&self, pointer_x: f32) -> f32 {
        // Round only after adding translation to the beginning's open width.
        // Rust's round uses nearest integer with halfway cases away from zero.
        (self.begin_width + (pointer_x - self.begin_pointer_x)).round()
    }
}

fn clamp_width(width: f32) -> f32 {
    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(width: f32, visible: bool) -> SidebarResizeOutcome {
        SidebarResizeOutcome { width, visible }
    }

    fn gesture(visible: bool) -> SidebarResizeState {
        // This makes pointer_x equal the unrounded raw width in boundary tests.
        SidebarResizeState::begin(SIDEBAR_DEFAULT_WIDTH, visible, SIDEBAR_DEFAULT_WIDTH)
    }

    #[test]
    fn initial_default_is_distinct_from_double_click_reset() {
        assert_eq!(SIDEBAR_DEFAULT_WIDTH, 240.0);
        assert_eq!(SidebarResizeState::reset(true), outcome(228.0, true));
        assert_eq!(SidebarResizeState::reset(false), outcome(228.0, false));
    }

    #[test]
    fn changed_threshold_uses_rounded_raw_width() {
        for (raw, expected) in [
            (81.0, outcome(0.0, false)),
            (81.49, outcome(0.0, false)),
            (81.5, outcome(164.0, true)),
            (82.0, outcome(164.0, true)),
            (82.49, outcome(164.0, true)),
            (82.5, outcome(164.0, true)),
        ] {
            assert_eq!(gesture(true).update(raw), expected, "raw={raw}");
        }
    }

    #[test]
    fn end_threshold_is_inclusive_only_for_an_open_beginning() {
        for raw in [81.0, 81.5, 82.0, 82.49] {
            assert_eq!(gesture(true).finish(raw), outcome(240.0, false));
            assert_eq!(gesture(false).finish(raw), outcome(164.0, true));
        }
        for visible in [false, true] {
            assert_eq!(gesture(visible).finish(82.5), outcome(164.0, true));
        }
    }

    #[test]
    fn exact_threshold_is_visible_during_drag_but_collapses_on_end() {
        let state = gesture(true);
        assert_eq!(state.update(82.0), outcome(164.0, true));
        assert_eq!(state.finish(82.0), outcome(240.0, false));
    }

    #[test]
    fn negative_raw_widths_round_away_from_zero_and_collapse() {
        let state = gesture(true);
        assert_eq!(state.raw_width(-0.5), -1.0);
        assert_eq!(state.raw_width(-81.5), -82.0);
        assert_eq!(state.update(-1000.0), outcome(0.0, false));
        assert_eq!(state.finish(-1000.0), outcome(240.0, false));
        assert_eq!(gesture(false).finish(-1000.0), outcome(164.0, true));
    }

    #[test]
    fn normal_widths_round_and_clamp_to_minimum_and_maximum() {
        for (raw, width) in [
            (83.0, 164.0),
            (163.5, 164.0),
            (164.0, 164.0),
            (227.49, 227.0),
            (227.5, 228.0),
            (499.5, 500.0),
            (500.0, 500.0),
            (1000.0, 500.0),
        ] {
            assert_eq!(gesture(true).update(raw), outcome(width, true));
            assert_eq!(gesture(true).finish(raw), outcome(width, true));
        }
    }

    #[test]
    fn translation_is_always_relative_to_beginning() {
        let state = SidebarResizeState::begin(240.25, true, 100.0);
        assert_eq!(state.update(110.25), outcome(251.0, true));
        assert_eq!(state.update(120.0), outcome(260.0, true));
        assert_eq!(state.update(100.0), outcome(240.0, true));
        assert_eq!(state.finish(90.0), outcome(230.0, true));
    }

    #[test]
    fn finish_uses_final_pointer_even_without_an_update() {
        let state = gesture(true);
        assert_eq!(state.update(0.0), outcome(0.0, false));
        assert_eq!(state.finish(300.0), outcome(300.0, true));
        assert_eq!(gesture(true).finish(300.0), outcome(300.0, true));
    }

    #[test]
    fn cancelled_or_failed_drag_restores_exact_beginning() {
        for width in [100.0, 240.25, 600.0] {
            for visible in [false, true] {
                let state = SidebarResizeState::begin(width, visible, 100.0);
                let _ = state.update(-1000.0);
                let _ = state.update(1000.0);
                assert_eq!(state.cancel(), outcome(width, visible));
            }
        }
    }

    #[test]
    fn collapse_retains_clamped_begin_width_for_reopening() {
        for (begin_width, retained_width) in [(100.0, 164.0), (317.25, 317.25), (600.0, 500.0)] {
            let state = SidebarResizeState::begin(begin_width, true, begin_width);
            assert_eq!(state.update(0.0), outcome(0.0, false));
            let collapsed = state.finish(0.0);
            assert_eq!(collapsed, outcome(retained_width, false));
            // Reopening uses the host's retained width, rather than live zero.
            let reopened = SidebarResizeState::begin(collapsed.width, true, 100.0);
            assert_eq!(reopened.cancel(), outcome(retained_width, true));
        }
    }
}
