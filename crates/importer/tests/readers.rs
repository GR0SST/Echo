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

#[test]
fn history_readers_preserve_source_frequency_and_safari_total_beyond_retained_visits() {
    for (browser, file, schema) in [
        (BrowserKind::Chrome, "History", "CREATE TABLE urls(url TEXT,title TEXT,last_visit_time INTEGER,visit_count INTEGER); INSERT INTO urls VALUES('https://frequency.example','Recent',11644473601000000,6);"),
        (BrowserKind::Firefox, "places.sqlite", "CREATE TABLE moz_places(url TEXT,title TEXT,last_visit_date INTEGER,visit_count INTEGER); INSERT INTO moz_places VALUES('https://frequency.example','Recent',1000000,6);"),
        (BrowserKind::Safari, "History.db", "CREATE TABLE history_items(id INTEGER,url TEXT,visit_count INTEGER); CREATE TABLE history_visits(history_item INTEGER,title TEXT,visit_time REAL); INSERT INTO history_items VALUES(1,'https://frequency.example',6); INSERT INTO history_visits VALUES(1,'Older',0.2),(1,'Recent',0.5);"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(file);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(schema).unwrap();
        let before = std::fs::read(&path).unwrap();
        let report = read_profile(&source(browser, dir.path()), Selection { bookmarks: false, history: true, tabs: false }, &AtomicBool::new(false), |_| {});
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert_eq!(report.data.history.len(), 1);
        assert_eq!(report.data.history[0].visit_count, 6);
        assert_eq!(report.data.history[0].title, "Recent");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

fn export_zip(path: &Path, entries: &[(&str, &[u8])]) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn safari_export_zip_uses_content_metadata_and_preserves_reading_list_folders_and_microseconds() {
    let dir = tempfile::tempdir().unwrap();let path = dir.path().join("Safari.zip");
    let html = b"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p><DT><H3>Reading &amp; List</H3><DL><p><DT><A HREF=\"https://one.example/\">One &amp; two</A></DL><p><DT><A HREF=\"https://last.example/\">Last</A></DL>";
    let history = br#"{"metadata":{"data_type":"history","schema_version":1},"history":[{"url":"https://one.example/","title":"History","time_usec":2000001,"visits_count":6}]}"#;
    export_zip(&path, &[("Lesezeichen.html", html), ("Verlauf - Work.json", history), ("PaymentCards.json", br#"{"metadata":{"data_type":"payment_cards"},"payment_cards":[]}"#), ("Passworter.csv", b"URL,Username,Password\nhttps://one.example/,me,fixture-secret\n")]);
    let before = std::fs::read(&path).unwrap();
    let report = read_profile(&importer::export_source(&path).unwrap(), Selection::default(), &AtomicBool::new(false), |_| {});
    assert!(report.issues.is_empty(), "{:?}", report.issues);assert!(report.data.tabs.is_empty());
    assert_eq!(report.data.bookmarks.len(), 2);
    match &report.data.bookmarks[0] { browser_core::import::Bookmark::Folder {title, children} => { assert_eq!(title,"Reading & List");match &children[0] {browser_core::import::Bookmark::Page(p)=>assert_eq!(p.title,"One & two"),_=>panic!()} }, _=>panic!() }
    assert_eq!(report.data.history[0].visited_at, std::time::UNIX_EPOCH + std::time::Duration::from_micros(2_000_001));
    assert_eq!(report.data.history[0].visit_count,6);
    let passwords = importer::read_password_file(&path).unwrap();assert_eq!(passwords[0].password,"fixture-secret");
    assert_eq!(before,std::fs::read(&path).unwrap());
}
#[test]
fn archive_paths_and_unknown_history_versions_fail_without_partial_category_publication() {
    let dir = tempfile::tempdir().unwrap();let path = dir.path().join("bad.zip");
    export_zip(&path, &[("../escape.html", b"<DL></DL>")]);
    let report = read_profile(&source(BrowserKind::Safari,&path), Selection::default(), &AtomicBool::new(false), |_|{});
    assert!(!report.issues.is_empty());assert!(report.data.bookmarks.is_empty());assert!(!dir.path().join("escape.html").exists());
    export_zip(&path, &[("History.json",br#"{"metadata":{"data_type":"history","schema_version":900},"history":[]}"#)]);
    let report = read_profile(&source(BrowserKind::Safari,&path), Selection::default(), &AtomicBool::new(false), |_|{});
    assert!(report.issues.iter().any(|i| i.category==Category::History));assert!(report.data.history.is_empty());
}
#[test]
fn firefox_backup_direct_and_profile_fallback_preserve_folders_without_rewriting_sources() {
    let dir = tempfile::tempdir().unwrap();let backups = dir.path().join("bookmarkbackups");std::fs::create_dir(&backups).unwrap();
    let path = backups.join("bookmarks-2026-10-25.jsonlz4");
    let json = br#"{"type":"text/x-moz-place-container","children":[{"type":"text/x-moz-place-container","title":"Work","children":[{"type":"text/x-moz-place","title":"Saved","uri":"https://saved.example/"},{"type":"text/x-moz-place-separator"}]}]}"#;
    let mut bytes = b"mozLz40\0".to_vec();bytes.extend(lz4_flex::block::compress_prepend_size(json));std::fs::write(&path,&bytes).unwrap();
    for input in [path.as_path(), dir.path()] {
        let report = read_profile(&source(BrowserKind::Firefox,input), Selection::default(), &AtomicBool::new(false), |_|{});
        assert!(report.issues.is_empty(),"{:?}",report.issues);assert_eq!(report.data.bookmarks.len(),1);
        assert!(report.data.history.is_empty());assert_eq!(std::fs::read(&path).unwrap(),bytes);
    }
    std::fs::write(&path, b"mozLz40\0\xff\xff\xff\xff").unwrap();
    let report = read_profile(&source(BrowserKind::Firefox,&path), Selection::default(), &AtomicBool::new(false), |_|{});
    assert!(report.data.bookmarks.is_empty());assert_eq!(report.issues.len(),1);
    let report = read_profile(&source(BrowserKind::Firefox,&path), Selection::default(), &AtomicBool::new(true), |_|panic!("Canceled export read"));assert!(report.cancelled);
}
#[test]
fn export_limits_reject_deep_html_oversized_passwords_and_ambiguous_csv_without_credentials() {
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("export.zip");
    let deep=format!("{}{}", "<DL><H3>Folder</H3>".repeat(70), "</DL>".repeat(70));
    export_zip(&path,&[("Bookmarks.html",deep.as_bytes())]);
    let report=read_profile(&source(BrowserKind::Safari,&path),Selection::default(),&AtomicBool::new(false),|_|{});
    assert!(report.data.bookmarks.is_empty());assert!(report.issues.iter().any(|i|i.category==Category::Bookmarks));
    let large=vec![b' ';8*1024*1024+1];export_zip(&path,&[("Passwords.csv",&large)]);
    assert!(importer::read_password_file(&path).is_err());
    let csv=b"URL,Username,Password\nhttps://fixture.example/,user,only-a-fixture\n";
    export_zip(&path,&[("One.csv",csv),("Two.csv",csv)]);assert!(importer::read_password_file(&path).is_err());
    let backup=dir.path().join("bookmarks.json");std::fs::write(&backup,br#"{"type":"text/x-moz-place-container","children":[{"type":"text/x-moz-place-container","title":"Empty"}]}"#).unwrap();
    let report=read_profile(&source(BrowserKind::Firefox,&backup),Selection::default(),&AtomicBool::new(false),|_|{});assert!(report.issues.is_empty());assert_eq!(report.data.bookmarks.len(),1);
}
