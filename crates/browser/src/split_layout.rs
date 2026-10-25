//! Shared geometry for GPUI pane chrome and the native page surfaces below it.
use browser_core::splits::Axis;
use engine::Bounds;

pub(crate) const GAP: f64 = 10.;
/// Keep pane controls below the window's draggable titlebar region.
pub(crate) const TITLE_HEIGHT: f64 = 0.;
pub(crate) const HEADER_HEIGHT: f64 = 32.;
pub(crate) const BORDER: f64 = 2.;
pub(crate) const MIN_PANE_EXTENT: f64 = 220.;

pub(crate) fn minimum_extent(count: usize) -> f64 {
    MIN_PANE_EXTENT * count as f64 + GAP * count.saturating_sub(1) as f64
}

/// Resolve proportional weights with Arc's minimum pane extent. The remaining
/// space is redistributed among panes that have not reached their minimum.
pub(crate) fn constrained_weights(extent: f64, weights: &[f32]) -> Vec<f32> {
    let count = weights.len();
    if count == 0 { return Vec::new(); }
    let available = (extent.max(minimum_extent(count)) - GAP * count.saturating_sub(1) as f64).max(0.);
    let sum: f64 = weights.iter().map(|v| f64::from(*v)).sum();
    let valid = sum.is_finite() && sum > 0. && weights.iter().all(|v| v.is_finite() && *v > 0.);
    let input: Vec<f64> = weights.iter().map(|v| if valid { f64::from(*v) } else { 1. }).collect();
    let mut fixed = vec![false; count];
    let mut sizes = vec![0.; count];
    let mut remaining = available;
    loop {
        let weight: f64 = input.iter().zip(&fixed).filter(|(_,fixed)| !**fixed).map(|(v,_)| *v).sum();
        if weight <= 0. { break; }
        let mut constrained = false;
        for i in 0..count {
            if !fixed[i] && remaining * input[i] / weight < MIN_PANE_EXTENT {
                sizes[i] = MIN_PANE_EXTENT; fixed[i] = true; constrained = true;
            }
        }
        if !constrained {
            for i in 0..count { if !fixed[i] { sizes[i] = remaining * input[i] / weight; } }
            break;
        }
        remaining = available - sizes.iter().sum::<f64>();
    }
    sizes.into_iter().map(|v| (v / available) as f32).collect()
}

/// Arc's expanded split leaves the other panes visible as 220 px strips.
/// Small windows fall back to equal shares rather than collapsing a page.
pub(crate) fn expanded_weights(extent: f64, count: usize, active: usize) -> Vec<f32> {
    if count == 0 || active >= count { return Vec::new(); }
    let available = (extent - GAP * count.saturating_sub(1) as f64).max(0.);
    if !available.is_finite() || available <= MIN_PANE_EXTENT * count as f64 {
        return vec![1. / count as f32; count];
    }
    let strip = (MIN_PANE_EXTENT / available) as f32;
    let mut weights = vec![strip; count];
    weights[active] = 1. - strip * (count - 1) as f32;
    weights
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneBounds {
    pub frame: Bounds,
    pub header: Bounds,
    pub page: Bounds,
}

pub(crate) fn layout(card: Bounds, axis: Axis, weights: &[f32], split: bool) -> Vec<PaneBounds> {
    if weights.is_empty() || ![card.x, card.y, card.width, card.height].iter().all(|value| value.is_finite()) { return Vec::new(); }
    let card = if split { let inset = TITLE_HEIGHT.min(card.height.max(0.)); Bounds { y: card.y + inset, height: (card.height - inset).max(0.), ..card } } else { card };
    let card = if split {
        let minimum = minimum_extent(weights.len());
        if axis == Axis::Horizontal { Bounds { width: card.width.max(minimum), ..card } }
        else { let height = card.height.max(minimum); Bounds { y: card.y - (height - card.height), height, ..card } }
    } else { card };
    let constrained = split.then(|| constrained_weights(if axis == Axis::Horizontal { card.width } else { card.height }, weights));
    let weights = constrained.as_deref().unwrap_or(weights);
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
        // The native surface must leave the GPUI focus contour uncovered.
        // Reserving the same border for every pane prevents a focus change
        // from resizing (and reflowing) the web document.
        let inset_x = if split { BORDER.min(frame.width / 2.) } else { 0. };
        let inset_y = if split { BORDER.min(frame.height / 2.) } else { 0. };
        let inner = Bounds { x: frame.x + inset_x, y: frame.y + inset_y,
            width: (frame.width - 2. * inset_x).max(0.), height: (frame.height - 2. * inset_y).max(0.) };
        let header_height = if split { (HEADER_HEIGHT - BORDER).min(inner.height) } else { 0. };
        PaneBounds {
            frame,
            header: Bounds { height: header_height, ..inner },
            page: Bounds { y: inner.y + header_height, height: (inner.height - header_height).max(0.), ..inner },
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
        assert_eq!(panes[0].frame.width, 233.5);
        assert_eq!(panes[1].frame.x, 491.5);
        assert_eq!(panes[0].page.y, 40.);
        assert_eq!(panes[0].page.height, 750.);
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
                    assert!(pane.frame.width >= 0. && pane.frame.height >= 0.);
                    if axis == Axis::Vertical { assert!(pane.page.y + pane.page.height <= height + 0.001); }
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
