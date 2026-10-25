//! GPUI layouts are logical pixels; windowed HWND/X11 placement uses device pixels.
use crate::Bounds;

pub(crate) fn valid_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    }
}

/// Round edges rather than size independently so adjacent split panes share
/// exactly the same device-pixel boundary at fractional display scales.
pub(crate) fn physical_rect(bounds: Bounds, scale: f64) -> [i32; 4] {
    let scale = valid_scale(scale);
    let edge = |value: f64| {
        if value.is_finite() {
            (value * scale).round() as i32
        } else {
            0
        }
    };
    let x = edge(bounds.x);
    let y = edge(bounds.y);
    [
        x,
        y,
        edge(bounds.x + bounds.width.max(0.))
            .saturating_sub(x)
            .max(1),
        edge(bounds.y + bounds.height.max(0.))
            .saturating_sub(y)
            .max(1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_dpi_keeps_split_edges_and_negative_origins() {
        let a = physical_rect(
            Bounds {
                x: -4.,
                y: 8.,
                width: 101.,
                height: 80.,
            },
            1.25,
        );
        let b = physical_rect(
            Bounds {
                x: 97.,
                y: 8.,
                width: 103.,
                height: 80.,
            },
            1.25,
        );
        assert_eq!(a, [-5, 10, 126, 100]);
        assert_eq!(a[0] + a[2], b[0]);
    }
    #[test]
    fn invalid_geometry_never_produces_zero_sized_native_children() {
        assert_eq!(physical_rect(Bounds::default(), f64::NAN), [0, 0, 1, 1]);
        assert_eq!(valid_scale(-1.), 1.);
        assert_eq!(valid_scale(2.), 2.);
    }
}

/// Explicit page-focus requests never revive a hidden native child. Showing a
/// background tab does not steal focus from a GPUI popup or another split pane.
#[derive(Clone, Copy)]
pub(crate) struct FocusState {
    visible: bool,
    requested: bool,
}
impl Default for FocusState {
    fn default() -> Self {
        Self {
            visible: true,
            requested: false,
        }
    }
}
impl FocusState {
    pub(crate) fn request(&mut self) -> bool {
        self.requested = self.visible;
        self.requested
    }
    pub(crate) fn blur(&mut self) {
        self.requested = false;
    }
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.blur();
        }
    }
}

#[cfg(test)]
mod focus_tests {
    use super::FocusState;
    #[test]
    fn hiding_cancels_focus_and_showing_never_restores_it_implicitly() {
        let mut page = FocusState::default();
        assert!(page.request());
        page.set_visible(false);
        assert!(!page.requested);
        assert!(!page.request());
        page.set_visible(true);
        assert!(!page.requested);
        assert!(page.request());
        page.blur();
        assert!(!page.requested);
    }
}
