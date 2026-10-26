//! Published-path availability is transient; canonical download metadata survives.
use super::*;
use browser_core::downloads::{CompletedDownloadId, DownloadCompletion, DownloadTime};
use crate::{download_files::{FileAction, MISSING_FILE}, downloads_view::DownloadsList};

fn seed_file(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext)
    -> (tempfile::TempDir, std::path::PathBuf, CompletedDownloadId)
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("availability fixture.pdf");
    std::fs::write(&path, b"synthetic completed bytes").unwrap();
    let normal = view.read_with(cx, |view, _| view.profile.handle.clone());
    let id = normal.record_completed_download(normal.default_user_profile_id(), DownloadCompletion {
        final_path: path.clone(), source_url: "https://fixture.test/file.pdf".into(),
        original_url: "https://fixture.test/file.pdf".into(), mime_type: "application/pdf".into(),
        finished_at: DownloadTime::Unknown,
    }).unwrap();
    cx.simulate_keystrokes("cmd-shift-j");
    cx.run_until_parked();
    (dir, path, id)
}

fn projected(view: &Entity<BrowserWindow>, cx: &VisualTestContext)
    -> Vec<(CompletedDownloadId, String, String)>
{
    let presenter = view.read_with(cx, |view, _| view.sidebar_navigation.downloads.host.as_ref().unwrap().view());
    presenter.read_with(cx, |view, _| match &view.projection_for_test().list {
        DownloadsList::Completed(groups) => groups.iter().flat_map(|group| group.rows.iter()
            .map(|row| (row.token, row.title.clone(), row.subtitle.clone()))).collect(),
        _ => Vec::new(),
    })
}

#[gpui::test]
fn moved_download_refreshes_on_activation_and_reentry_without_losing_its_record(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let (dir, path, id) = seed_file(&view, cx);
    assert_eq!(projected(&view, cx), [(id, "availability fixture.pdf".into(), "PDF from fixture.test".into())]);
    fake.take_calls();
    cx.deactivate_window();
    let moved = dir.path().join("moved.pdf");
    std::fs::rename(&path, &moved).unwrap();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert_eq!(projected(&view, cx), [(id, "availability fixture.pdf".into(), MISSING_FILE.into())]);
    view.read_with(cx, |view, _| {
        let inventory = view.profile.handle.download_inventory().unwrap().1;
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].id(), id);
        assert_eq!(inventory[0].final_path(), path);
    });
    click(cx, "library-nav-media");
    std::fs::rename(&moved, &path).unwrap();
    cx.simulate_keystrokes("cmd-shift-j");
    cx.run_until_parked();
    assert_eq!(projected(&view, cx)[0].2, "PDF from fixture.test");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn download_timer_refreshes_deleted_directory_and_restored_file_states(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let (_dir, path, id) = seed_file(&view, cx);
    click(cx, "downloads-search");
    cx.simulate_input("availability");
    cx.run_until_parked();
    let query = view.read_with(cx, |view, _| view.sidebar_navigation.downloads.host.as_ref().unwrap().query().to_owned());
    assert_eq!(query, "availability");
    std::fs::remove_file(&path).unwrap();
    for restored in [false, true] {
        if restored { std::fs::write(&path, b"restored bytes").unwrap(); }
        else { std::fs::create_dir(&path).unwrap(); }
        cx.executor().advance_clock(Duration::from_secs(61));
        cx.run_until_parked();
        let rows = projected(&view, cx);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, id);
        assert_eq!(rows[0].2, if restored { "PDF from fixture.test" } else { MISSING_FILE });
        assert_eq!(view.read_with(cx, |view, _| view.sidebar_navigation.downloads.host.as_ref().unwrap().query().to_owned()), query);
        if !restored { std::fs::remove_dir(&path).unwrap(); }
    }
}

#[gpui::test]
fn stale_file_actions_fail_closed_but_hide_remains_metadata_only(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let (dir, path, id) = seed_file(&view, cx);
    let owner = view.read_with(cx, |view, _| view.sidebar_navigation.downloads.host.as_ref().unwrap().owner());
    right_click(cx, "downloads-row-0-0");
    let moved = dir.path().join("kept.pdf");
    std::fs::rename(&path, &moved).unwrap();
    fake.take_calls();
    for action in [FileAction::Open, FileAction::Reveal, FileAction::Copy, FileAction::Trash] {
        cx.update(|window, cx| view.update(cx, |view, cx| view.confirm_download_menu(id, owner, action, window, cx)));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.toast.as_ref().unwrap().message, MISSING_FILE);
            let entries = view.download_menu_commands(id, owner);
            assert!(entries.iter().all(|entry| match entry {
                crate::tab_menu::Entry::Item(item) => item.enabled == matches!(item.command, Some(crate::tab_menu::Command::Download(FileAction::Hide))),
                _ => true,
            }));
            assert_eq!(view.profile.handle.download_inventory().unwrap().1.len(), 1);
        });
        assert_eq!(projected(&view, cx)[0].2, MISSING_FILE);
    }
    cx.update(|window, cx| view.update(cx, |view, cx| view.confirm_download_menu(id, owner, FileAction::Hide, window, cx)));
    cx.run_until_parked();
    assert!(projected(&view, cx).is_empty());
    assert_eq!(std::fs::read(moved).unwrap(), b"synthetic completed bytes");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}
