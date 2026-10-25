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

use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone)]
pub(super) struct Attachment {
    pub(super) parent: usize,
    pub(super) placement: Placement,
    lease: Rc<()>,
}
impl Attachment {
    pub(super) fn new(parent: usize, placement: Placement) -> Self {
        Self {
            parent,
            placement,
            lease: Rc::new(()),
        }
    }
}
pub(super) type Registry = RefCell<HashMap<usize, Attachment>>;

/// HWND movement synchronously dispatches WM_SIZE/WM_MOVE. Snapshot and release
/// the registry before entering native code, which can re-enter this function or
/// release/replace an overlay. A failed old request must not remove a new lease.
pub(super) fn synchronize(
    registry: &Registry,
    parent: usize,
    mut update: impl FnMut(usize, Placement) -> bool,
) {
    let snapshot = registry
        .borrow()
        .iter()
        .filter(|(_, attachment)| attachment.parent == parent)
        .map(|(handle, attachment)| (*handle, attachment.clone()))
        .collect::<Vec<_>>();
    for (handle, attachment) in snapshot {
        // An earlier synchronous callback may have retired another snapshot
        // entry before its turn. Never move that entry's replacement.
        let live = registry
            .borrow()
            .get(&handle)
            .is_some_and(|current| Rc::ptr_eq(&current.lease, &attachment.lease));
        if !live {
            continue;
        }
        if update(handle, attachment.placement) {
            continue;
        }
        let mut registry = registry.borrow_mut();
        if registry
            .get(&handle)
            .is_some_and(|current| Rc::ptr_eq(&current.lease, &attachment.lease))
        {
            registry.remove(&handle);
        }
    }
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    #[test]
    fn native_callbacks_can_reenter_and_replace_an_overlay_without_losing_its_new_lease() {
        let registry = Registry::default();
        let placement = Placement::new([0, 0, 800, 600], [0, 0, 800, 600], 1.);
        registry
            .borrow_mut()
            .insert(1, Attachment::new(10, placement));
        synchronize(&registry, 10, |handle, _| {
            assert!(registry.try_borrow_mut().is_ok());
            synchronize(&registry, 10, |_, _| true);
            registry.borrow_mut().remove(&handle);
            registry
                .borrow_mut()
                .insert(handle, Attachment::new(10, placement));
            registry
                .borrow_mut()
                .insert(2, Attachment::new(10, placement));
            false // Failure belongs only to the snapshotted, released lease.
        });
        assert_eq!(registry.borrow().len(), 2);
        synchronize(&registry, 10, |_, _| false);
        assert!(registry.borrow().is_empty());

        registry
            .borrow_mut()
            .insert(1, Attachment::new(10, placement));
        registry
            .borrow_mut()
            .insert(2, Attachment::new(10, placement));
        let mut calls = 0;
        synchronize(&registry, 10, |handle, _| {
            calls += 1;
            registry
                .borrow_mut()
                .remove(&if handle == 1 { 2 } else { 1 });
            true
        });
        assert_eq!(
            calls, 1,
            "a retired snapshot entry must never enter native code"
        );
    }
}
