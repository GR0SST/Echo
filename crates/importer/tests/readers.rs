use importer::{
    BrowserKind, Category, HostPaths, Platform, ProfileLocator, Selection, SourceProfile,
    read_profile,
};
use std::{path::Path, sync::atomic::AtomicBool};
fn source(browser: BrowserKind, path: &Path) -> SourceProfile {
    SourceProfile {
        browser,
        name: "Fixture".into(),
        path: path.into(),
    }
}
#[test]
fn chromium_reads_live_wal_history_and_preserves_nested_bookmarks_without_writing_source() {
    let dir = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("History")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE urls(url TEXT,title TEXT,last_visit_time INTEGER); INSERT INTO urls VALUES('https://example.com','New visit',11644473601000000);").unwrap();
    std::fs::write(dir.path().join("Bookmarks"),r#"{"roots":{"bookmark_bar":{"type":"folder","name":"Bookmarks bar","children":[{"type":"folder","name":"Work","children":[{"type":"url","name":"Example","url":"https://example.com"}]}]}}}"#).unwrap();
    let original = std::fs::read(dir.path().join("History-wal")).unwrap();
    let report = read_profile(
        &source(BrowserKind::Chrome, dir.path()),
        Selection::default(),
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.data.history.len(), 1);
    assert_eq!(report.data.bookmarks.len(), 1);
    assert_eq!(
        report.data.history[0].visited_at,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)
    );
    assert_eq!(
        std::fs::read(dir.path().join("History-wal")).unwrap(),
        original
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM urls", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        1
    );
}
#[test]
fn failed_category_does_not_publish_partial_results_and_cancellation_discards_every_category() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Bookmarks"),r#"{"roots":{"bookmark_bar":{"type":"folder","name":"Good","children":[]},"other":{"type":"folder","children":[{"type":"url"}]}}}"#).unwrap();
    let report = read_profile(
        &source(BrowserKind::Brave, dir.path()),
        Selection::default(),
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(report.data.bookmarks.is_empty());
    assert_eq!(report.issues[0].category, Category::Bookmarks);
    let report = read_profile(
        &source(BrowserKind::Edge, dir.path()),
        Selection::default(),
        &AtomicBool::new(true),
        |_| panic!("Canceled import starts no category"),
    );
    assert!(report.cancelled);
    assert!(report.data.history.is_empty());
}
#[test]
fn firefox_reads_profile_ini_places_and_selected_lz4_session_entry() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("Profiles/test");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        dir.path().join("profiles.ini"),
        "[Profile0]\nName=Test\nPath=Profiles/test\nIsRelative=1\n",
    )
    .unwrap();
    let db = rusqlite::Connection::open(profile.join("places.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE moz_places(id INTEGER,url TEXT,title TEXT,last_visit_date INTEGER); CREATE TABLE moz_bookmarks(id INTEGER,parent INTEGER,type INTEGER,title TEXT,fk INTEGER,position INTEGER,guid TEXT); INSERT INTO moz_places VALUES(1,'https://firefox.example','Visit',2000000); INSERT INTO moz_bookmarks VALUES(1,0,2,'',NULL,0,'root________'),(2,1,2,'Toolbar',NULL,0,'toolbar_____'),(3,2,1,'Bookmark',1,0,'abc');").unwrap();
    let json=br#"{"windows":[{"tabs":[{"index":1,"entries":[{"url":"https://selected.example","title":"Selected"},{"url":"https://forward.example"}]}]}]}"#;
    let mut data = b"mozLz40\0".to_vec();
    data.extend(lz4_flex::block::compress_prepend_size(json));
    std::fs::write(profile.join("sessionstore.jsonlz4"), data).unwrap();
    let report = read_profile(
        &source(BrowserKind::Firefox, &profile),
        Selection::default(),
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.data.tabs[0].url, "https://selected.example");
    assert_eq!(report.data.history[0].title, "Visit");
    assert_eq!(report.data.bookmarks.len(), 1);
    let paths = HostPaths {
        platform: Platform::Linux,
        home: dir.path().to_path_buf(),
        config: dir.path().join(".config"),
        local_app_data: dir.path().into(),
        roaming_app_data: dir.path().into(),
    };
    assert_eq!(
        paths.browser_root(BrowserKind::Firefox).unwrap(),
        dir.path().join(".mozilla/firefox")
    );
}
#[test]
fn safari_binary_plist_bookmarks_and_history_epoch_and_platform_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let paths = HostPaths {
        platform: Platform::Mac,
        home: dir.path().into(),
        config: dir.path().into(),
        local_app_data: dir.path().into(),
        roaming_app_data: dir.path().into(),
    };
    let root = paths.browser_root(BrowserKind::Safari).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let value:plist::Value=plist::from_bytes(br#"<?xml version="1.0"?><plist version="1.0"><dict><key>Children</key><array><dict><key>WebBookmarkType</key><string>WebBookmarkTypeLeaf</string><key>URLString</key><string>https://safari.example</string><key>URIDictionary</key><dict><key>title</key><string>Safari bookmark</string></dict></dict></array></dict></plist>"#).unwrap();
    value.to_file_binary(root.join("Bookmarks.plist")).unwrap();
    let db = rusqlite::Connection::open(root.join("History.db")).unwrap();
    db.execute_batch("CREATE TABLE history_items(id INTEGER,url TEXT);CREATE TABLE history_visits(history_item INTEGER,title TEXT,visit_time REAL);INSERT INTO history_items VALUES(1,'https://safari.example');INSERT INTO history_visits VALUES(1,'Safari visit',0.5);").unwrap();
    let sources = paths.profiles();
    assert_eq!(sources.len(), 1);
    let report = read_profile(
        &sources[0],
        Selection::default(),
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(
        report.data.history[0].visited_at,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(978307200500)
    );
    assert_eq!(report.data.bookmarks.len(), 1);
}
