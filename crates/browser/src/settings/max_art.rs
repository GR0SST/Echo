//! Original, decorative mini-UI illustrations. No embedded reference assets.
use gpui::{prelude::*, *};

pub(super) fn feature(index: usize) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let origin = bounds.origin;
            let mut rect = |x: f32, y: f32, w: f32, h: f32, color: u32, radius: f32| {
                window.paint_quad(
                    fill(
                        Bounds::new(origin + point(px(x), px(y)), size(px(w), px(h))),
                        rgb(color),
                    )
                    .corner_radii(px(radius)),
                );
            };
            match index {
                // A sidebar organized into two small folders.
                0 => {
                    rect(12., 9., 26., 5., 0x8d86ba, 2.);
                    for (y, w) in [(20., 47.), (29., 38.), (42., 51.)] {
                        rect(12., y, 6., 6., 0x9891c1, 1.5);
                        rect(23., y + 1., w, 4., 0xf5f2ff, 1.5);
                    }
                    rect(12., 37., 23., 3., 0x8d86ba, 1.);
                }
                // A search field and a stack of matching destinations.
                1 => {
                    rect(7., 8., 74., 14., 0xf2fcff, 3.);
                    rect(12., 12., 6., 6., 0x589bb1, 3.);
                    rect(23., 13., 46., 3., 0x85aebc, 1.);
                    for (y, w) in [(29., 60.), (39., 49.), (49., 56.)] {
                        rect(9., y, 6., 5., 0xe0f3fa, 1.);
                        rect(20., y + 1., w, 3., 0xd2eaf0, 1.);
                    }
                }
                // A document preview with an original landscape illustration.
                2 => {
                    rect(20., 5., 49., 51., 0xffffff, 3.);
                    rect(24., 9., 41., 21., 0x9bcaca, 2.);
                    rect(29., 19., 18., 11., 0x639b96, 2.);
                    rect(43., 16., 18., 14., 0x417c83, 2.);
                    rect(54., 11., 6., 6., 0xf9e7a0, 3.);
                    for (y, w) in [(35., 34.), (41., 28.), (47., 31.)] {
                        rect(26., y, w, 2., 0x9a9cb3, 1.);
                    }
                }
                // Before/after title lengths within a miniature tab list.
                3 => {
                    for (y, w) in [(12., 53.), (28., 31.), (44., 40.)] {
                        rect(8., y - 3., 72., 12., 0xece6f9, 3.);
                        rect(12., y, 6., 6., 0x9684bc, 2.);
                        rect(23., y + 1., w, 3., 0x8876ac, 1.);
                    }
                }
                // Compact file cards with tidy caption lines.
                4 => {
                    for (x, y, w) in [(9., 10., 44.), (21., 34., 37.)] {
                        rect(x, y, 59., 16., 0xedfaf7, 3.);
                        rect(x + 5., y + 4., 7., 9., 0x63a69c, 1.);
                        rect(x + 17., y + 6., w - 10., 3., 0x75a39d, 1.);
                        rect(x + 17., y + 11., w - 17., 2., 0xb7d6cf, 1.);
                    }
                }
                // A command field leading into a short conversation.
                _ => {
                    rect(7., 8., 74., 15., 0xf4f7ff, 3.);
                    rect(13., 14., 53., 3., 0x8299c2, 1.);
                    rect(18., 29., 61., 11., 0xcddfed, 3.);
                    rect(10., 44., 55., 10., 0xe7eefb, 3.);
                    rect(24., 33., 42., 2., 0x7293b4, 1.);
                    rect(16., 48., 35., 2., 0x8ba0ba, 1.);
                }
            }
        },
    )
    .absolute()
    .inset_0()
}
