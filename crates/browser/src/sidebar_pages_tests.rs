use super::*;

#[test]
fn virtual_pages_preserve_real_order_and_have_no_drop_destination() {
    let mut browser = Browser::new();
    let a = browser.space().id;
    let b = browser.add_space("B", 20.0);
    let c = browser.add_space("C", 30.0);
    assert_eq!(
        sidebar_pages(&browser, None),
        [
            SidebarPage::Library,
            SidebarPage::Space(a),
            SidebarPage::Space(b),
            SidebarPage::Space(c),
            SidebarPage::CreateSpace
        ]
    );
    assert_eq!(
        sidebar_pages(&browser, Some(b)),
        [
            SidebarPage::Library,
            SidebarPage::Space(a),
            SidebarPage::Space(b),
            SidebarPage::CreateSpace,
            SidebarPage::Space(c)
        ]
    );
    browser.move_space(b, 0);
    let pages = sidebar_pages(&browser, Some(b));
    assert_eq!(
        pages,
        [
            SidebarPage::Library,
            SidebarPage::Space(b),
            SidebarPage::CreateSpace,
            SidebarPage::Space(a),
            SidebarPage::Space(c)
        ]
    );
    assert_eq!(
        pages
            .iter()
            .filter_map(|page| page.drop_destination())
            .collect::<Vec<_>>(),
        [b, a, c]
    );
    assert_eq!(SidebarPage::Library.drop_destination(), None);
    assert_eq!(SidebarPage::CreateSpace.drop_destination(), None);
}
