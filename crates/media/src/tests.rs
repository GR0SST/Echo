use super::*;
use std::time::Instant;
fn grant(path: &Path) -> ApprovedFolders {
    ApprovedFolders::from_system_selection(vec![path.into()]).unwrap()
}
#[test]
fn denied_or_cancelled_selection_does_not_create_scan_capability() {
    assert!(ApprovedFolders::from_system_selection(vec![]).is_none());
    assert!(ApprovedFolders::from_system_selection(vec!["relative".into()]).is_none());
}
#[test]
fn fixture_scan_is_bounded_deduplicated_and_excludes_hidden_documents_and_links() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::create_dir(root.path().join("hidden.app")).unwrap();
    for name in [
        "a.png",
        "b.MOV",
        ".secret.jpg",
        "report.pdf",
        "nested/c.jpg",
        "hidden.app/icon.png",
    ] {
        fs::write(root.path().join(name), b"fixture").unwrap();
    }
    fs::write(outside.path().join("outside.png"), b"not authorized").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
    }
    let approved = ApprovedFolders::from_system_selection(vec![
        root.path().into(),
        root.path().join("nested"),
    ])
    .unwrap();
    let snapshot = scan(&approved, ScanLimits::default(), &AtomicBool::new(false));
    assert_eq!(snapshot.items.len(), 3);
    assert!(snapshot.items.iter().all(|i| i
        .stamp
        .path
        .starts_with(root.path().canonicalize().unwrap())));
    assert!(!snapshot.truncated);
    assert_eq!(snapshot.denied.len(), 0);
    let limited = scan(
        &approved,
        ScanLimits {
            entries: 1,
            depth: 0,
        },
        &AtomicBool::new(false),
    );
    assert!(limited.truncated);
    let cancelled = scan(&approved, ScanLimits::default(), &AtomicBool::new(true));
    assert!(cancelled.items.is_empty());
}
#[test]
fn action_stamp_rejects_replacement_and_parent_link_redirection() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("a.png");
    fs::write(&path, b"first").unwrap();
    let before = stamp(&path).unwrap();
    assert!(validate(&before).is_ok());
    fs::rename(&path, root.path().join("old.png")).unwrap();
    fs::write(&path, b"first").unwrap();
    assert!(validate(&before).is_err());
}
#[test]
fn watching_fixture_updates_create_rename_and_remove_without_polling_user_folders() {
    let root = tempfile::tempdir().unwrap();
    let watcher = Discovery::start(grant(root.path()));
    let mut generation = 0;
    let mut wait = |predicate: &dyn Fn(&Snapshot) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((next, snapshot)) = watcher.snapshot_since(generation) {
                generation = next;
                if predicate(&snapshot) {
                    return snapshot;
                }
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not publish fixture update"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&|s| s.items.is_empty());
    let first = root.path().join("a.png");
    fs::write(&first, b"fixture").unwrap();
    wait(&|s| s.items.len() == 1);
    fs::rename(&first, root.path().join("b.png")).unwrap();
    wait(&|s| s.items.first().is_some_and(|i| i.name == "b.png"));
    fs::remove_file(root.path().join("b.png")).unwrap();
    wait(&|s| s.items.is_empty());
}

#[test]
fn captures_read_only_named_files_in_the_captured_profile_without_external_access() {
    let root = tempfile::tempdir().unwrap();
    let mut browser = browser_core::Browser::new();
    let profile = browser.active_user_profile_id();
    let captures = &mut browser.user_profile_mut(profile).unwrap().captures;
    let id = captures
        .add(
            "Echo fixture".into(),
            "Fixture".into(),
            "https://example.org".into(),
            "own.png".into(),
            20,
            10,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap();
    fs::write(root.path().join("own.png"), b"own capture").unwrap();
    fs::write(root.path().join("not-in-model.png"), b"unrelated").unwrap();
    let items = capture_items(root.path(), profile, captures);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source, MediaSource::Capture { profile, id });
    fs::remove_file(root.path().join("own.png")).unwrap();
    assert!(capture_items(root.path(), profile, captures).is_empty());
}
