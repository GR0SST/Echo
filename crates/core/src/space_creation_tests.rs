use super::*;
use crate::session;
use std::time::SystemTime;

fn request(browser: &Browser) -> SpaceCreation {
    SpaceCreation {
        origin: browser.space().id,
        name: "  Research  ".into(),
        user_profile_id: browser.active_user_profile_id(),
        custom_icon: Some("🔬".into()),
        hue: 42.0,
    }
}

#[test]
fn invalid_requests_preserve_session_and_both_allocators() {
    let browser = Browser::new();
    let valid = request(&browser);
    let cases = [
        (
            SpaceCreation {
                origin: SpaceId(999),
                ..valid.clone()
            },
            SpaceCreationError::UnknownOrigin,
        ),
        (
            SpaceCreation {
                user_profile_id: UserProfileId(999),
                ..valid.clone()
            },
            SpaceCreationError::UnknownProfile,
        ),
        (
            SpaceCreation {
                name: " \n ".into(),
                ..valid.clone()
            },
            SpaceCreationError::EmptyName,
        ),
        (
            SpaceCreation {
                custom_icon: Some(" ".into()),
                ..valid.clone()
            },
            SpaceCreationError::EmptyIcon,
        ),
        (
            SpaceCreation {
                hue: f32::NAN,
                ..valid.clone()
            },
            SpaceCreationError::InvalidHue,
        ),
        (
            SpaceCreation {
                hue: f32::INFINITY,
                ..valid.clone()
            },
            SpaceCreationError::InvalidHue,
        ),
        (
            SpaceCreation {
                hue: -1.0,
                ..valid.clone()
            },
            SpaceCreationError::InvalidHue,
        ),
        (
            SpaceCreation {
                hue: 361.0,
                ..valid
            },
            SpaceCreationError::InvalidHue,
        ),
    ];
    for (request, error) in cases {
        let mut browser = browser.clone();
        let before = session::to_json(&browser);
        let ids = (browser.next_id, browser.next_user_profile_id);
        assert_eq!(browser.create_space_after(&request), Err(error));
        assert_eq!(session::to_json(&browser), before);
        assert_eq!((browser.next_id, browser.next_user_profile_id), ids);
    }
}

#[test]
fn insertion_uses_current_origin_order_and_global_allocator_with_explicit_profile() {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let favorite = browser.new_tab("https://example.com", SystemTime::UNIX_EPOCH);
    browser.favorite_tab(favorite);
    let other = browser.add_space("Other", 10.0);
    let profile = browser.create_user_profile("Research").unwrap();
    let mut creation = request(&browser);
    creation.user_profile_id = profile;
    browser.move_space(origin, 1);
    browser.switch_space(0);
    let next = browser.next_id;
    let profiles_before = serde_json::to_string(&browser.profiles).unwrap();
    let id = browser.create_space_after(&creation).unwrap();
    assert_eq!(id, SpaceId(next));
    assert_eq!(browser.next_id, next + 1);
    assert_eq!(
        browser.spaces.iter().map(|s| s.id).collect::<Vec<_>>(),
        [other, origin, id]
    );
    assert_eq!(browser.space().id, id);
    assert_eq!(browser.space().user_profile_id, profile);
    assert_eq!(browser.space().name, "Research");
    assert_eq!(browser.space().custom_icon.as_deref(), Some("🔬"));
    assert_eq!(browser.space().hue, 42.0);
    assert!(browser.space().active_tab.is_none());
    assert!(browser.space().tabs().next().is_none());
    assert!(browser.space().folders.is_empty());
    assert!(browser.space().splits.is_empty());
    assert_eq!(
        serde_json::to_string(&browser.profiles).unwrap(),
        profiles_before
    );
    let tab = browser.new_tab("https://next.example", SystemTime::UNIX_EPOCH);
    assert_eq!(tab.get(), next + 1);
    assert_ne!(tab, favorite);
}

#[test]
fn local_windows_and_exhausted_allocator_are_unchanged_on_failure() {
    for mut browser in [Browser::new_private(), Browser::new_blank()] {
        let creation = request(&browser);
        let before = session::to_json(&browser);
        assert_eq!(
            browser.create_space_after(&creation),
            Err(SpaceCreationError::WindowLocalModel)
        );
        assert_eq!(session::to_json(&browser), before);
    }
    let mut browser = Browser::new();
    browser.next_id = u64::MAX;
    let creation = request(&browser);
    let before = session::to_json(&browser);
    assert_eq!(
        browser.create_space_after(&creation),
        Err(SpaceCreationError::IdentityExhausted)
    );
    assert_eq!(session::to_json(&browser), before);
}

#[test]
fn rejected_window_creation_preserves_canonical_b_and_both_window_selections() {
    let mut browser = Browser::new();
    browser.new_tab("https://a.example", SystemTime::UNIX_EPOCH);
    let a_space = browser.space().id;
    let mut a = browser.window_selection();
    let mut creation = request(&browser);
    let b_space = browser.add_space("B", 20.);
    browser.switch_space(1);
    browser.new_tab("https://b.example", SystemTime::UNIX_EPOCH);
    let b = browser.window_selection();
    assert_eq!(browser.space().id, b_space);
    assert_eq!(browser.window_projection(&a).space().id, a_space);
    let cases = [
        (
            SpaceCreation {
                name: " ".into(),
                ..creation.clone()
            },
            SpaceCreationError::EmptyName,
        ),
        (
            SpaceCreation {
                origin: SpaceId(999),
                ..creation.clone()
            },
            SpaceCreationError::UnknownOrigin,
        ),
        (
            SpaceCreation {
                user_profile_id: UserProfileId(999),
                ..creation.clone()
            },
            SpaceCreationError::UnknownProfile,
        ),
    ];
    for (invalid, error) in cases {
        let before = session::to_json(&browser);
        let a_before = serde_json::to_string(&a).unwrap();
        let b_before = serde_json::to_string(&b).unwrap();
        let ids = (browser.next_id, browser.next_user_profile_id);
        assert_eq!(
            browser.validate_space_creation_request(&invalid),
            Err(error)
        );
        assert_eq!(
            browser.create_space_after_in_window(&mut a, &invalid),
            Err(error)
        );
        assert_eq!(session::to_json(&browser), before);
        assert_eq!(serde_json::to_string(&a).unwrap(), a_before);
        assert_eq!(serde_json::to_string(&b).unwrap(), b_before);
        assert_eq!((browser.next_id, browser.next_user_profile_id), ids);
        assert_eq!(browser.space().id, b_space);
    }
    // The same seam still applies A's selection and records the successful result.
    creation.name = "Valid".into();
    let id = browser
        .create_space_after_in_window(&mut a, &creation)
        .unwrap();
    assert_eq!(browser.window_projection(&a).space().id, id);
    assert_eq!(browser.window_projection(&b).space().id, b_space);
    assert!(browser.space().active_tab.is_none());
}

#[test]
fn last_available_identity_is_allocated_once_then_failure_is_byte_identical() {
    let mut browser = Browser::new();
    browser.next_id = u64::MAX - 1;
    let creation = request(&browser);
    let id = browser.create_space_after(&creation).unwrap();
    assert_eq!(id, SpaceId(u64::MAX - 1));
    assert_eq!(browser.next_id, u64::MAX);
    let before = session::to_json(&browser);
    let profile_watermark = browser.next_user_profile_id;
    assert_eq!(
        browser.create_space_after(&creation),
        Err(SpaceCreationError::IdentityExhausted)
    );
    assert_eq!(session::to_json(&browser), before);
    assert_eq!(browser.next_id, u64::MAX);
    assert_eq!(browser.next_user_profile_id, profile_watermark);
}

#[test]
fn session_round_trip_keeps_created_space_and_live_archived_identity_pools_disjoint() {
    fn identities(browser: &Browser) -> Vec<u64> {
        browser
            .spaces
            .iter()
            .flat_map(|space| {
                std::iter::once(space.id.0).chain(space.tabs().map(|tab| tab.id.get()))
            })
            .chain(
                browser
                    .user_profiles()
                    .flat_map(|profile| profile.favorites.iter().map(|tab| tab.id.get())),
            )
            .chain(browser.archive.iter().map(|entry| entry.tab.id.get()))
            .collect()
    }
    let mut browser = Browser::new();
    let favorite = browser.new_tab("https://favorite.example", SystemTime::UNIX_EPOCH);
    browser.favorite_tab(favorite);
    let pinned = browser.new_tab("https://pinned.example", SystemTime::UNIX_EPOCH);
    browser.toggle_pin(pinned);
    let archived = browser.new_tab("https://archived.example", SystemTime::UNIX_EPOCH);
    browser.close_tab(archived, SystemTime::UNIX_EPOCH);
    browser.new_tab("https://today.example", SystemTime::UNIX_EPOCH);
    let profile = browser.create_user_profile("Separate").unwrap();
    let mut creation = request(&browser);
    creation.user_profile_id = profile;
    let created = browser.create_space_after(&creation).unwrap();
    let before_ids = identities(&browser);
    assert_eq!(
        before_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        before_ids.len()
    );
    let watermark = browser.next_id;
    let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
    assert_eq!(identities(&restored), before_ids);
    assert_eq!(restored.next_id, watermark);
    assert_eq!(restored.next_user_profile_id, browser.next_user_profile_id);
    assert_eq!(restored.space().id, created);
    assert_eq!(restored.space().user_profile_id, profile);
    assert_eq!(restored.archive[0].tab.id, archived);
    let next_space = restored.create_space_after(&creation).unwrap();
    let next_tab = restored.new_tab("https://next.example", SystemTime::UNIX_EPOCH);
    assert_eq!(next_space, SpaceId(watermark));
    assert_eq!(next_tab.get(), watermark + 1);
    assert!(!before_ids.contains(&next_space.0));
    assert!(!before_ids.contains(&next_tab.get()));
}
