//! Device-pixel tracking for owned top-level overlays on HWND/X11 backends.
#[derive(Clone, Copy, Debug)]
pub(super) struct Placement {
    offset: [i32; 2],
    scale: f64,
    size: [i32; 2],
    fill: [bool; 2],
}
impl Placement {
    pub(super) fn new(parent: [i32; 4], child: [i32; 4], scale: f64) -> Self {
        Self {
            scale,
            offset: [
                child[0].saturating_sub(parent[0]),
                child[1].saturating_sub(parent[1]),
            ],
            size: [child[2], child[3]],
            fill: [child[2] == parent[2], child[3] == parent[3]],
        }
    }
    pub(super) fn follow(self, parent: [i32; 4], scale: f64) -> [i32; 4] {
        let ratio = if scale.is_finite() && self.scale.is_finite() && scale > 0. && self.scale > 0.
        {
            scale / self.scale
        } else {
            1.
        };
        let scaled = |value: i32| (f64::from(value) * ratio).round() as i32;
        [
            parent[0].saturating_add(scaled(self.offset[0])),
            parent[1].saturating_add(scaled(self.offset[1])),
            if self.fill[0] {
                parent[2]
            } else {
                scaled(self.size[0])
            }
            .max(1),
            if self.fill[1] {
                parent[3]
            } else {
                scaled(self.size[1])
            }
            .max(1),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_window_popup_tracks_owner_resize_without_losing_desktop_origin() {
        let placement = Placement::new([-100, 20, 800, 600], [-100, 20, 800, 600], 1.);
        assert_eq!(
            placement.follow([40, -200, 1200, 900], 1.),
            [40, -200, 1200, 900]
        );
    }
    #[test]
    fn narrow_sidebar_keeps_width_and_follows_owner_height_and_offset() {
        let placement = Placement::new([40, 50, 900, 600], [42, 50, 240, 600], 1.);
        assert_eq!(
            placement.follow([-900, 80, 1200, 700], 1.),
            [-898, 80, 240, 700]
        );
        assert_eq!(
            placement.follow([-900, 80, 1800, 1050], 1.5),
            [-897, 80, 360, 1050]
        );
    }
}
