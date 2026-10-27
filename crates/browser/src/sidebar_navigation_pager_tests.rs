//! Register as a child test module of `window` to exercise its pager helper.
use super::rubber_band;
use crate::{sidebar_pages::SidebarPage, space_creation::SpaceCreationController};
use browser_core::Browser;

#[test]
fn spring_matches_recorded_arc_keyboard_positions_without_overshoot() {
    use super::{pager_spring, PAGER_SPRING_FREQUENCY};
    // Incoming folder center, measured in Arc at 1000x760, sidebar 228px.
    for (seconds, center) in [(0.0457, 184.), (0.1007, 95.), (0.1757, 40.), (0.2457, 30.)] {
        let (position, _) = pager_spring(0., 1., 0., PAGER_SPRING_FREQUENCY, seconds);
        assert!((26. + 228. * (1. - position) - center).abs() < 3.);
    }
    for frame in 0..60 {
        let (position, velocity) = pager_spring(0., 1., 0., PAGER_SPRING_FREQUENCY, frame as f32 / 60.);
        assert!((0. ..=1.).contains(&position));
        assert!(velocity >= 0.);
    }
}

#[test]
fn spring_preserves_position_and_velocity_when_retargeted() {
    use super::{pager_spring, PAGER_SPRING_FREQUENCY};
    let (position, velocity) = pager_spring(0., 1., 0., PAGER_SPRING_FREQUENCY, 0.08);
    assert_eq!(pager_spring(position, 0., velocity, PAGER_SPRING_FREQUENCY, 0.), (position, velocity));
    let split = pager_spring(position, 1., velocity, PAGER_SPRING_FREQUENCY, 0.08);
    let whole = pager_spring(0., 1., 0., PAGER_SPRING_FREQUENCY, 0.16);
    assert!((split.0 - whole.0).abs() < 0.00001);
    assert!((split.1 - whole.1).abs() < 0.00001);
}

#[test]
fn rubber_band_uses_virtual_page_edges_without_resisting_real_spaces() {
    let mut browser = Browser::new();
    browser.add_space("Second", 30.0);
    let controller = SpaceCreationController::new(&browser);
    let pages = controller.pages(&browser);
    assert_eq!(pages.len(), browser.spaces.len() + 2);
    for position in [0.0, 0.5, 1.0, 2.5, 3.0] {
        assert_eq!(rubber_band(position, pages.len()), position);
    }
    let left = rubber_band(-0.5, pages.len());
    let right = rubber_band(3.5, pages.len());
    assert!(left > -0.5 && left < 0.0);
    assert!(right > 3.0 && right < 3.5);
    assert!((left + right - 3.0).abs() < 0.0001);
}

#[test]
fn draft_reorder_repositions_selected_identity_and_its_virtual_neighbor() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let other = browser.add_space("Second", 30.0);
    let mut controller = SpaceCreationController::new(&browser);
    controller.begin(&browser, origin).unwrap();
    assert_eq!(controller.pages(&browser)[2], controller.selected());
    assert!(browser.move_space(origin, 1));
    controller.reconcile(&browser);
    let pages = controller.pages(&browser);
    assert_eq!(
        pages,
        [
            SidebarPage::Library,
            SidebarPage::Space(other),
            SidebarPage::Space(origin),
            SidebarPage::CreateSpace
        ]
    );
    assert_eq!(
        pages.iter().position(|page| *page == controller.selected()),
        Some(3)
    );
    assert!(controller.select(&browser, SidebarPage::Space(origin)));
    assert_eq!(
        pages.iter().position(|page| *page == controller.selected()),
        Some(2)
    );
    // Pager index 2 names the origin, while real model destination 1 names it.
    assert_eq!(pages[2].drop_destination(), Some(browser.spaces[1].id));
}

#[test]
fn library_selection_survives_reorder_without_selecting_or_allocating_a_space() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let other = browser.add_space("Second", 30.0);
    let mut controller = SpaceCreationController::new(&browser);
    assert!(controller.select(&browser, SidebarPage::Library));
    let selected_real_space = browser.space().id;
    assert!(browser.move_space(other, 0));
    controller.reconcile(&browser);
    let pages = controller.pages(&browser);
    assert_eq!(controller.selected(), SidebarPage::Library);
    assert_eq!(pages[0], controller.selected());
    assert_eq!(browser.space().id, selected_real_space);
    assert_eq!(browser.spaces.len(), 2);
    assert_eq!(
        pages
            .iter()
            .filter_map(|page| page.drop_destination())
            .collect::<Vec<_>>(),
        [other, origin]
    );
    assert!(controller.draft().is_none());
}

#[test]
fn one_swipe_reaches_only_the_neighbouring_space() {
    use super::{one_page_reach, PAGER_RUBBER_BAND};
    // Within one page of the origin the strip follows the fingers.
    for position in [1.0, 1.5, 2.0, 2.9, 3.0] {
        assert_eq!(one_page_reach(position, 2.), position);
    }
    // Beyond it, travel is damped, so dragging three pages stays near one.
    assert!((one_page_reach(5., 2.) - (3. + 2. * PAGER_RUBBER_BAND)).abs() < 1e-5);
    assert!((one_page_reach(-1., 2.) - (1. - 2. * PAGER_RUBBER_BAND)).abs() < 1e-5);
}
