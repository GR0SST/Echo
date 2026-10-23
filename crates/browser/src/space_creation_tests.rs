use super::*;
use browser_core::session;

#[test]
fn cancellation_is_byte_identical_including_allocator_watermarks() {
    let browser = Browser::new();
    let before = session::to_json(&browser);
    let origin = browser.space().id;
    let mut controller = SpaceCreationController::new(&browser);
    controller.begin(&browser, origin).unwrap();
    controller.edit_draft(|draft| {
        draft.name = "Unsubmitted".into();
        draft.hue = 15.0;
    });
    assert_eq!(controller.selected(), SidebarPage::CreateSpace);
    assert_eq!(controller.cancel(&browser), SidebarPage::Space(origin));
    assert!(controller.request().is_none());
    assert_eq!(session::to_json(&browser), before);
}

#[test]
fn drafts_are_window_local_and_inherit_the_origin_profile() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let profile = browser.create_user_profile("Work").unwrap();
    browser.assign_space_user_profile(origin, profile).unwrap();
    let mut a = SpaceCreationController::new(&browser);
    let mut b = SpaceCreationController::new(&browser);
    a.begin(&browser, origin).unwrap();
    b.begin(&browser, origin).unwrap();
    a.edit_draft(|draft| draft.name = "A".into());
    assert_eq!(a.draft().unwrap().user_profile_id, profile);
    assert!(b.draft().unwrap().name.is_empty());
    a.begin(&browser, origin).unwrap();
    assert_eq!(a.draft().unwrap().name, "A");
    let id = browser.create_space_after(&a.request().unwrap()).unwrap();
    a.committed(id);
    b.reconcile(&browser);
    assert_eq!(a.selected(), SidebarPage::Space(id));
    assert!(a.draft().is_none());
    assert_eq!(b.selected(), SidebarPage::CreateSpace);
    assert_eq!(b.draft().unwrap().origin, origin);
    assert_eq!(
        b.pages(&browser)
            .iter()
            .filter(|page| **page == SidebarPage::CreateSpace)
            .count(),
        1
    );
    assert!(browser.space().active_tab.is_none());
}

#[test]
fn reconciliation_follows_identity_and_removed_origin_cancels_without_editing_model() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let other = browser.add_space("Other", 0.0);
    let mut controller = SpaceCreationController::new(&browser);
    controller.begin(&browser, origin).unwrap();
    controller.edit_draft(|draft| draft.origin = other);
    assert_eq!(controller.draft().unwrap().origin, origin);
    browser.move_space(origin, 1);
    controller.reconcile(&browser);
    assert_eq!(
        controller.pages(&browser),
        [
            SidebarPage::Library,
            SidebarPage::Space(other),
            SidebarPage::Space(origin),
            SidebarPage::CreateSpace
        ]
    );
    // In-memory fixture only; never delete a Space in a live session.
    browser.spaces.retain(|space| space.id != origin);
    browser.switch_space(0);
    let before = session::to_json(&browser);
    controller.reconcile(&browser);
    assert!(controller.draft().is_none());
    assert_eq!(controller.selected(), SidebarPage::Space(other));
    assert_eq!(session::to_json(&browser), before);
}

#[test]
fn invalid_submission_keeps_draft_and_profile_disappearance_never_falls_back() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let profile = browser.create_user_profile("Temporary").unwrap();
    let mut controller = SpaceCreationController::new(&browser);
    controller.begin(&browser, origin).unwrap();
    let before = session::to_json(&browser);
    assert_eq!(
        browser.create_space_after(&controller.request().unwrap()),
        Err(SpaceCreationError::EmptyName)
    );
    assert_eq!(session::to_json(&browser), before);
    assert!(controller.draft().is_some());
    controller.edit_draft(|draft| {
        draft.name = "Ready".into();
        draft.user_profile_id = profile;
    });
    browser.delete_user_profile(profile).unwrap();
    controller.reconcile(&browser);
    assert_eq!(controller.draft().unwrap().user_profile_id, profile);
    let before = session::to_json(&browser);
    assert_eq!(
        browser.create_space_after(&controller.request().unwrap()),
        Err(SpaceCreationError::UnknownProfile)
    );
    assert_eq!(session::to_json(&browser), before);
    assert!(controller.select(&browser, SidebarPage::Library));
    assert_eq!(controller.selected(), SidebarPage::Library);
    assert_eq!(controller.cancel(&browser), SidebarPage::Space(origin));
}

#[test]
fn commit_updates_only_originating_window_selection_and_other_drafts_follow_reorder() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let other = browser.add_space("Other", 20.0);
    let mut a_selection = browser.window_selection();
    let b_selection = browser.new_window_selection();
    let mut a = SpaceCreationController::new(&browser.window_projection(&a_selection));
    let mut b = SpaceCreationController::new(&browser.window_projection(&b_selection));
    a.begin(&browser.window_projection(&a_selection), origin)
        .unwrap();
    b.begin(&browser.window_projection(&b_selection), origin)
        .unwrap();
    a.edit_draft(|draft| draft.name = "Created in A".into());
    let request = a.request().unwrap();
    let id = browser
        .create_space_after_in_window(&mut a_selection, &request)
        .unwrap();
    a.committed(id);
    assert_eq!(browser.window_projection(&a_selection).space().id, id);
    assert_eq!(browser.window_projection(&b_selection).space().id, origin);
    browser.move_space(origin, 2);
    let projection = browser.window_projection(&b_selection);
    b.reconcile(&projection);
    assert_eq!(
        b.pages(&projection),
        [
            SidebarPage::Library,
            SidebarPage::Space(id),
            SidebarPage::Space(other),
            SidebarPage::Space(origin),
            SidebarPage::CreateSpace
        ]
    );
    assert_eq!(b.draft().unwrap().origin, origin);
    assert_eq!(b.selected(), SidebarPage::CreateSpace);
}

#[test]
fn begin_and_create_page_selection_reject_local_models_without_changing_controller() {
    let mut normal = Browser::new();
    normal.add_space("Draft origin absent from local models", 45.);
    normal.switch_space(1);
    for local in [Browser::new_private(), Browser::new_blank()] {
        for existing_draft in [false, true] {
            let mut controller = SpaceCreationController::new(&normal);
            if existing_draft {
                controller.begin(&normal, normal.space().id).unwrap();
                controller.edit_draft(|draft| draft.name = "Retained draft".into());
            } else {
                controller.select(&normal, SidebarPage::Library);
            }
            let selected_before = controller.selected();
            let draft_before = controller.request();
            let local_before = session::to_json(&local);
            assert_eq!(
                controller.begin(&local, local.space().id),
                Err(SpaceCreationError::WindowLocalModel)
            );
            assert_eq!(controller.selected(), selected_before);
            assert_eq!(controller.request(), draft_before);
            assert!(!controller.select(&local, SidebarPage::CreateSpace));
            assert_eq!(controller.selected(), selected_before);
            assert_eq!(controller.request(), draft_before);
            assert_eq!(session::to_json(&local), local_before);
        }
    }
}
