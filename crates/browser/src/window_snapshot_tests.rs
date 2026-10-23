//! Frozen presentation tests; engine capture and host transfer live with Pages.
use crate::window_snapshots::{Mirror, SnapshotImages, frozen_address};
use browser_core::TabId;
use engine::Bounds;
use gpui::{Context, Modifiers, Render, TestAppContext, Window, div, prelude::*, px};
use std::{io::Cursor, sync::Arc, time::SystemTime};

fn tab() -> TabId {
    let mut browser = crate::demo_browser();
    browser.new_tab("https://snapshot.example/", SystemTime::now())
}

fn png(color: [u8; 4]) -> Arc<Vec<u8>> {
    let rgba = image::RgbaImage::from_pixel(2, 1, image::Rgba(color));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    Arc::new(bytes)
}

#[test]
fn snapshot_decode_is_bgra_and_unchanged_png_reuses_the_gpu_image() {
    let tab = tab();
    let mut cache = SnapshotImages::default();
    let original = png([17, 43, 211, 255]);
    let first = cache.image(tab, &original).unwrap();
    assert_eq!(
        first.as_bytes(0).unwrap(),
        &[211, 43, 17, 255, 211, 43, 17, 255]
    );
    assert!(Arc::ptr_eq(
        &first,
        &cache.image(tab, &original.clone()).unwrap()
    ));
    let replacement = png([9, 8, 7, 255]);
    let second = cache.image(tab, &replacement).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(second.as_bytes(0).unwrap(), &[7, 8, 9, 255, 7, 8, 9, 255]);
    // Existing source presentation remains frozen after another image is decoded.
    assert_eq!(first.as_bytes(0).unwrap()[0], 211);
    let invalid = Arc::new(vec![0, 1, 2]);
    assert!(cache.image(tab, &invalid).is_none());
    assert!(cache.image(tab, &invalid).is_none());
    assert!(cache.image(tab, &original).is_some());
}

#[test]
fn address_placeholder_requires_a_frozen_foreign_page_not_initial_loading() {
    let tab = tab();
    let mirror = (
        tab,
        Bounds {
            x: 20.,
            y: 30.,
            width: 200.,
            height: 100.,
        },
        png([255; 4]),
    );
    assert!(!frozen_address(tab, false, &[]));
    assert!(!frozen_address(tab, true, &[mirror.clone()]));
    assert!(frozen_address(tab, false, &[mirror]));
}

struct SnapshotWindow {
    cache: SnapshotImages,
    mirrors: Vec<Mirror>,
    split: bool,
    clicks: usize,
}

impl Render for SnapshotWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.clicks += 1;
                    cx.notify();
                }),
            )
            .children(self.cache.render(&self.mirrors, self.split))
    }
}

#[gpui::test]
fn frozen_page_uses_source_pane_bounds_and_leaves_clicks_with_the_host(cx: &mut TestAppContext) {
    let tab = tab();
    let key = tab.get();
    let selector: &'static str = Box::leak(format!("window-snapshot-{key}").into_boxed_str());
    let dim_selector: &'static str =
        Box::leak(format!("window-snapshot-dim-{key}").into_boxed_str());
    let bytes = png([17, 43, 211, 255]);
    let source = Bounds {
        x: 48.,
        y: 42.,
        width: 240.,
        height: 160.,
    };
    let (view, visual) = cx.add_window_view(|_, _| SnapshotWindow {
        cache: SnapshotImages::default(),
        mirrors: vec![(tab, source, bytes.clone())],
        split: false,
        clicks: 0,
    });
    visual.run_until_parked();
    let bounds = visual.debug_bounds(selector).unwrap();
    assert_eq!(bounds.origin, gpui::point(px(48.), px(42.)));
    assert_eq!(bounds.size, gpui::size(px(240.), px(160.)));
    assert_eq!(visual.debug_bounds(dim_selector), Some(bounds));
    let first = view.update(visual, |view, _| view.cache.image(tab, &bytes).unwrap());
    visual.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.run_until_parked();
    view.update(visual, |view, _| {
        assert_eq!(view.clicks, 1);
        assert!(Arc::ptr_eq(&first, &view.cache.image(tab, &bytes).unwrap()));
    });
    // Reactivation removes the image; it does not make the snapshot interactive.
    view.update(visual, |view, cx| {
        view.mirrors.clear();
        cx.notify();
    });
    visual.refresh().unwrap();
    visual.run_until_parked();
    view.update(visual, |view, _| {
        // GPUI 0.2.2 retains old debug_bounds entries across frames. Inspect
        // current presentation output instead of that historical selector map.
        assert!(view.cache.render(&view.mirrors, view.split).is_empty());
    });
}

#[gpui::test]
fn split_snapshots_preserve_page_bounds_below_headers_and_reject_bad_geometry(
    cx: &mut TestAppContext,
) {
    let mut browser = crate::demo_browser();
    let a = browser.new_tab("https://snapshot.example/a", SystemTime::now());
    let b = browser.new_tab("https://snapshot.example/b", SystemTime::now());
    let invalid = browser.new_tab("https://snapshot.example/invalid", SystemTime::now());
    let bytes = png([255; 4]);
    let selector = |tab: TabId| -> &'static str {
        Box::leak(format!("window-snapshot-{}", tab.get()).into_boxed_str())
    };
    let (a_selector, b_selector, invalid_selector) = (selector(a), selector(b), selector(invalid));
    let (_, visual) = cx.add_window_view(|_, _| SnapshotWindow {
        cache: SnapshotImages::default(),
        split: true,
        clicks: 0,
        mirrors: vec![
            (
                a,
                Bounds {
                    x: 20.,
                    y: 64.,
                    width: 180.,
                    height: 220.,
                },
                bytes.clone(),
            ),
            (
                b,
                Bounds {
                    x: 208.,
                    y: 64.,
                    width: 240.,
                    height: 220.,
                },
                bytes.clone(),
            ),
            (
                invalid,
                Bounds {
                    x: f64::NAN,
                    y: 0.,
                    width: 50.,
                    height: 50.,
                },
                bytes,
            ),
        ],
    });
    visual.run_until_parked();
    let a_bounds = visual.debug_bounds(a_selector).unwrap();
    let b_bounds = visual.debug_bounds(b_selector).unwrap();
    assert_eq!(a_bounds.origin, gpui::point(px(20.), px(64.)));
    assert_eq!(a_bounds.size, gpui::size(px(180.), px(220.)));
    assert_eq!(b_bounds.origin, gpui::point(px(208.), px(64.)));
    assert_eq!(b_bounds.size, gpui::size(px(240.), px(220.)));
    assert!(visual.debug_bounds(invalid_selector).is_none());
}
