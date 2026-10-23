//! Presentation only: Pages owns capture timing and each host's frozen PNG.
use browser_core::TabId;
use engine::Bounds;
use gpui::{AnyElement, RenderImage, StyledImage, div, hsla, img, prelude::*, px};
use std::{collections::HashMap, sync::Arc};

/// Bounds use this host window's logical coordinates, matching surface_layout,
/// rather than offsets within the content card or the owning window's bounds.
/// Pages keeps the PNG Arc stable until that host's frozen capture changes.
pub(crate) type Mirror = (TabId, Bounds, Arc<Vec<u8>>);

// Flat page pixels in the Arc reference lose approximately 30% brightness.
// Display color profiles prevent treating that estimate as exact calibration.
const INACTIVE_DIM: f32 = 0.30;

struct CachedImage {
    png: Arc<Vec<u8>>,
    image: Option<Arc<RenderImage>>,
}

#[derive(Default)]
pub(crate) struct SnapshotImages {
    images: HashMap<TabId, CachedImage>,
}

impl SnapshotImages {
    pub(crate) fn image(&mut self, tab: TabId, png: &Arc<Vec<u8>>) -> Option<Arc<RenderImage>> {
        if let Some(cached) = self
            .images
            .get(&tab)
            .filter(|cached| Arc::ptr_eq(&cached.png, png))
        {
            return cached.image.clone();
        }
        let image = decode(png);
        // Cache failures as well: malformed PNGs must not be decoded every frame.
        self.images.insert(
            tab,
            CachedImage {
                png: png.clone(),
                image: image.clone(),
            },
        );
        image
    }

    pub(crate) fn render(&mut self, mirrors: &[Mirror], split: bool) -> Vec<AnyElement> {
        self.images
            .retain(|tab, _| mirrors.iter().any(|(id, _, _)| id == tab));
        mirrors
            .iter()
            .filter_map(|(tab, bounds, png)| {
                if !valid_bounds(*bounds) {
                    return None;
                }
                let image = self.image(*tab, png)?;
                let key = tab.get();
                Some(
                    div()
                        .id(("window-snapshot", key))
                        .debug_selector(move || format!("window-snapshot-{key}"))
                        .absolute()
                        .left(px(bounds.x as f32))
                        .top(px(bounds.y as f32))
                        .w(px(bounds.width as f32))
                        .h(px(bounds.height as f32))
                        .overflow_hidden()
                        .rounded_b(px(crate::pages::CONTENT_CORNER_RADIUS))
                        .when(!split, |this| {
                            this.rounded_t(px(crate::pages::CONTENT_CORNER_RADIUS))
                        })
                        .bg(gpui::white())
                        .child(img(image).size_full().object_fit(gpui::ObjectFit::Fill))
                        .child(
                            div()
                                .debug_selector(move || format!("window-snapshot-dim-{key}"))
                                .absolute()
                                .inset_0()
                                .bg(hsla(0., 0., 0., INACTIVE_DIM)),
                        )
                        // No page handlers: root window activation reclaims the native view.
                        .into_any_element(),
                )
            })
            .collect()
    }
}

fn decode(png: &[u8]) -> Option<Arc<RenderImage>> {
    let mut rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    // RenderImage consumes BGRA, unlike image's RGBA decoder output.
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

fn valid_bounds(bounds: Bounds) -> bool {
    [bounds.x, bounds.y, bounds.width, bounds.height]
        .iter()
        .all(|value| value.is_finite() && value.abs() <= f32::MAX as f64)
        && bounds.width > 0.
        && bounds.height > 0.
}

/// A missing native surface alone also describes an initial load. Only a
/// frozen presentation proves that this host has relinquished the document.
pub(crate) fn frozen_address(tab: TabId, visible: bool, mirrors: &[Mirror]) -> bool {
    !visible && mirrors.iter().any(|(id, _, _)| *id == tab)
}

impl crate::window::BrowserWindow {
    pub(crate) fn render_window_snapshots(&self) -> Vec<AnyElement> {
        let mirrors = self
            .pages
            .as_ref()
            .and_then(|pages| pages.try_borrow().ok().map(|pages| pages.mirrors()))
            .unwrap_or_default();
        self.snapshot_images
            .borrow_mut()
            .render(&mirrors, self.browser.active_split().is_some())
    }

    pub(crate) fn has_frozen_active_page(&self) -> bool {
        let Some(tab) = self.browser.active_tab() else {
            return false;
        };
        let Some(pages) = self
            .pages
            .as_ref()
            .and_then(|pages| pages.try_borrow().ok())
        else {
            return false;
        };
        frozen_address(tab.id, pages.is_visible(tab.id), &pages.mirrors())
    }
}
