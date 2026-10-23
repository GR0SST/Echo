//! Shared geometry for GPUI pane chrome and the native page surfaces below it.
use browser_core::splits::Axis;
use engine::Bounds;

pub(crate) const GAP: f64 = 8.;
/// Keep pane controls below the window's draggable titlebar region.
pub(crate) const TITLE_HEIGHT: f64 = 0.;
pub(crate) const HEADER_HEIGHT: f64 = 32.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneBounds {
    pub frame: Bounds,
    pub header: Bounds,
    pub page: Bounds,
}

pub(crate) fn layout(card: Bounds, axis: Axis, weights: &[f32], split: bool) -> Vec<PaneBounds> {
    if weights.is_empty() || ![card.x, card.y, card.width, card.height].iter().all(|value| value.is_finite()) { return Vec::new(); }
    let card = if split { let inset = TITLE_HEIGHT.min(card.height.max(0.)); Bounds { y: card.y + inset, height: (card.height - inset).max(0.), ..card } } else { card };
    let count = weights.len();
    let horizontal = axis == Axis::Horizontal;
    let length = if horizontal { card.width } else { card.height }.max(0.);
    let gap = if count > 1 { GAP.min(((length - count as f64).max(0.)) / (count - 1) as f64) } else { 0. };
    let available = (length - gap * (count - 1) as f64).max(0.);
    let sum: f64 = weights.iter().map(|weight| f64::from(*weight)).sum();
    let valid = sum.is_finite() && sum > 0. && weights.iter().all(|weight| weight.is_finite() && *weight > 0.);
    let mut offset = 0.;
    weights.iter().enumerate().map(|(index, weight)| {
        let fraction = if valid { f64::from(*weight) / sum } else { 1. / count as f64 };
        let length = if index + 1 == count { (length - offset).max(0.) } else { available * fraction };
        let frame = if horizontal { Bounds { x: card.x + offset, y: card.y, width: length, height: card.height.max(0.) } }
            else { Bounds { x: card.x, y: card.y + offset, width: card.width.max(0.), height: length } };
        offset += length + gap;
        let header_height = if split { HEADER_HEIGHT.min(frame.height) } else { 0. };
        PaneBounds {
            frame,
            header: Bounds { height: header_height, ..frame },
            page: Bounds { y: frame.y + header_height, height: (frame.height - header_height).max(0.), ..frame },
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_panes_reserve_their_headers_and_fill_the_card_with_exact_gaps() {
        let card = Bounds { x: 248., y: 8., width: 944., height: 784. };
        let panes = layout(card, Axis::Horizontal, &[0.25, 0.75], true);
        assert_eq!(panes[0].frame.width, 234.);
        assert_eq!(panes[1].frame.x, 490.);
        assert_eq!(panes[0].page.y, 40.);
        assert_eq!(panes[0].page.height, 752.);
        assert_eq!(panes[1].frame.x + panes[1].frame.width, card.x + card.width);
        assert_eq!(layout(card, Axis::Horizontal, &[1.], false)[0].page, card);
    }

    #[test]
    fn vertical_and_tiny_layouts_keep_nonoverlapping_finite_surfaces() {
        for (width, height) in [(944., 784.), (10., 10.), (1., 1.), (0., 0.)] {
            for axis in [Axis::Horizontal, Axis::Vertical] {
                let card = Bounds { x: 0., y: 0., width, height };
                let panes = layout(card, axis, &[1., 2., 3., 4.], true);
                for pane in &panes {
                    assert!(pane.page.width >= 0. && pane.page.height >= 0.);
                    assert!(pane.page.x + pane.page.width <= width + 0.001);
                    assert!(pane.page.y + pane.page.height <= height + 0.001);
                    assert!(pane.page.width.is_finite() && pane.page.height.is_finite());
                }
                for pair in panes.windows(2) {
                    if axis == Axis::Horizontal { assert!(pair[0].frame.x + pair[0].frame.width <= pair[1].frame.x + 0.001); }
                    else { assert!(pair[0].frame.y + pair[0].frame.height <= pair[1].frame.y + 0.001); }
                }
            }
        }
    }
}
