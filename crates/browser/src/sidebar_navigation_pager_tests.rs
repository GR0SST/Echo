//! Register as a child test module of `window` to exercise its pager helper.
use super::rubber_band;
use crate::{sidebar_pages::SidebarPage, space_creation::SpaceCreationController};
use browser_core::Browser;

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
