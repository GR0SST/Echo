use super::*;
fn fixture() -> (super::super::Config, Source) {
    let config = super::super::Config::parse(&format!(
        "http://127.0.0.1:18473 {} continue-normal",
        std::env::temp_dir().display()
    ))
    .unwrap();
    let mut browser = browser_core::Browser::new();
    let tab = browser.new_tab(config.fixture_url(), std::time::SystemTime::UNIX_EPOCH);
    let profile = browser.active_user_profile_id();
    let source = Source {
        identity: PageIdentity {
            tab,
            profile,
            generation: 41,
        },
        space: browser.space().id,
        storage: StorageSelection::for_profile(profile),
        parent: 17,
    };
    (config, source)
}
#[test]
fn context_pending_stays_eligible_then_claim_is_one_use_and_reentrant_safe() {
    let (config, source) = fixture();
    let shared = install(config.clone(), source.clone(), 9001);
    for _ in 0..3 {
        assert!(matches!(
            inspect(
                source.identity,
                source.space,
                &source.storage,
                source.parent,
                &config.fixture_url()
            ),
            Decision::Candidate(_)
        ));
        // This is the context Pending boundary: neither begin nor CEF executes.
        assert_eq!(shared.borrow().phase, Phase::Waiting);
        assert_eq!(shared.borrow().generation, None);
    }
    assert!(begin(&shared).is_some());
    assert_eq!(shared.borrow().phase, Phase::Creating);
    assert!(matches!(
        inspect(
            source.identity,
            source.space,
            &source.storage,
            source.parent,
            &config.fixture_url()
        ),
        Decision::Rejected
    ));
    assert!(begin(&shared).is_none());
    finish(&shared, true);
    assert_eq!(shared.borrow().phase, Phase::Claimed);
    assert_eq!(shared.borrow().generation, Some(source.identity.generation));
    assert!(matches!(
        inspect(
            source.identity,
            source.space,
            &source.storage,
            source.parent,
            &config.fixture_url()
        ),
        Decision::Rejected
    ));
    reset();
}
#[test]
fn mismatch_is_permanently_denied_and_cannot_revive_a_stale_source() {
    let (config, source) = fixture();
    let mut browser = browser_core::Browser::new();
    let other_profile = browser.create_user_profile("Admission mismatch").unwrap();
    let other_space = browser.add_space("Other source", 20.);
    let mut generation = source.identity;
    generation.generation += 1;
    let mut profile = source.identity;
    profile.profile = other_profile;
    let cases = vec![
        (
            generation,
            source.space,
            source.storage.clone(),
            config.fixture_url(),
        ),
        (
            profile,
            source.space,
            source.storage.clone(),
            config.fixture_url(),
        ),
        (
            source.identity,
            other_space,
            source.storage.clone(),
            config.fixture_url(),
        ),
        (
            source.identity,
            source.space,
            StorageSelection::Current,
            config.fixture_url(),
        ),
        (
            source.identity,
            source.space,
            StorageSelection::Named("other".into()),
            config.fixture_url(),
        ),
        (
            source.identity,
            source.space,
            source.storage.clone(),
            "https://example.test/remote".into(),
        ),
    ];
    for (identity, space, storage, url) in cases {
        let shared = install(config.clone(), source.clone(), 9002);
        assert!(matches!(
            inspect(identity, space, &storage, source.parent, &url),
            Decision::Rejected
        ));
        assert_eq!(shared.borrow().phase, Phase::Failed);
        assert_eq!(shared.borrow().generation, None);
        assert!(matches!(
            inspect(
                source.identity,
                source.space,
                &source.storage,
                source.parent,
                &config.fixture_url()
            ),
            Decision::Rejected
        ));
    }
    // Another workspace can reuse core IDs: ordinary denied creation must keep
    // working in its own parent instead of inheriting this source's capability.
    let shared = install(config.clone(), source.clone(), 9002);
    assert!(matches!(
        inspect(
            source.identity,
            source.space,
            &source.storage,
            source.parent + 1,
            &config.fixture_url()
        ),
        Decision::Ordinary
    ));
    assert_eq!(shared.borrow().phase, Phase::Waiting);
    reset();
}
#[test]
fn unrelated_tabs_stay_on_denied_legacy_path_and_native_failure_is_not_pending() {
    let (config, source) = fixture();
    let shared = install(config.clone(), source.clone(), 9003);
    let mut other = browser_core::Browser::new();
    other.new_tab(
        "https://example.test/first",
        std::time::SystemTime::UNIX_EPOCH,
    );
    let other_tab = other.new_tab(
        "https://example.test/unrelated",
        std::time::SystemTime::UNIX_EPOCH,
    );
    let identity = PageIdentity {
        tab: other_tab,
        profile: other.active_user_profile_id(),
        generation: 41,
    };
    assert!(matches!(
        inspect(
            identity,
            source.space,
            &source.storage,
            source.parent,
            &config.fixture_url()
        ),
        Decision::Ordinary
    ));
    assert!(begin(&shared).is_some());
    finish(&shared, false);
    assert_eq!(shared.borrow().phase, Phase::Failed);
    assert!(begin(&shared).is_none());
    assert!(matches!(
        inspect(
            source.identity,
            source.space,
            &source.storage,
            source.parent,
            &config.fixture_url()
        ),
        Decision::Rejected
    ));
    reset();
}
