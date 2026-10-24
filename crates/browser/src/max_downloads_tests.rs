use super::max_feature_tests::ProviderRequests;
use super::*;
use browser_core::downloads::{CompletedDownloadId, DownloadCompletion, DownloadTime};
use std::sync::Arc;
fn record(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    path: std::path::PathBuf,
) -> CompletedDownloadId {
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::TidyDownloads, true);
            b.settings.max.provider = echo_ai::ProviderConfig {
                provider: echo_ai::Provider::OpenAiCompatible,
                base_url: "http://127.0.0.1:8129/v1".into(),
                model: "local-model".into(),
                api_key_id: "opaque-reference".into(),
                ..Default::default()
            };
        });
        w.profile
            .handle
            .record_completed_download(
                w.browser.active_user_profile_id(),
                DownloadCompletion {
                    final_path: path,
                    source_url: "https://example.test/file?private=secret".into(),
                    original_url: "https://example.test/file?private=secret".into(),
                    mime_type: "application/pdf".into(),
                    finished_at: DownloadTime::Unknown,
                },
            )
            .unwrap()
    })
}
#[gpui::test]
fn max_download_renames_completed_file_and_toast_undo_is_local(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let requests = Arc::new(ProviderRequests::default());
    cx.update(|_, cx| cx.set_global(crate::max::InferenceService(requests.clone())));
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("123.pdf");
    std::fs::write(&original, b"download contents").unwrap();
    let id = record(&view, cx, original.clone());
    cx.run_until_parked();
    let reply = {
        let pending = requests.0.lock().unwrap();
        assert_eq!(pending.len(), 1);
        let input = &pending[0].0[0];
        assert_eq!(input["filename"], "123.pdf");
        assert!(!input.to_string().contains("secret"));
        assert!(!input
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string()));
        pending[0].1.clone()
    };
    reply.complete(Ok("{\"stem\":\"Quarterly Report\"}".into()));
    cx.run_until_parked();
    let renamed = dir.path().join("Quarterly Report.pdf");
    assert!(renamed.exists());
    assert!(!original.exists());
    view.read_with(cx, |w, _| {
        assert_eq!(
            w.profile.handle.read_settings(|b| b
                .completed_download(id)
                .unwrap()
                .final_path()
                .to_owned()),
            renamed
        );
        assert_eq!(w.toast.as_ref().unwrap().message, "Download renamed!");
    });
    view.update(cx, |w, _| {
        w.edit_profile(|b| {
            b.settings
                .max
                .set_enabled(echo_ai::MaxFeature::TidyDownloads, false);
            b.settings.max.provider.authentication = echo_ai::Authentication::Disconnected;
        })
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    // Exercise the real overlay's accessory hit target.
    let overlay = view.read_with(cx, |w, _| w.toast.as_ref().unwrap().overlay.unwrap());
    let mut toast_cx = VisualTestContext::from_window(overlay.into(), &cx.cx);
    click(&mut toast_cx, "toast-download-undo");
    cx.run_until_parked();
    assert_eq!(std::fs::read(&original).unwrap(), b"download contents");
    assert!(!renamed.exists());
    view.read_with(cx, |w, _| {
        assert_eq!(w.toast.as_ref().unwrap().message, "Download name reverted!")
    });
    assert_eq!(requests.0.lock().unwrap().len(), 1);
}
#[gpui::test]
fn max_download_late_hidden_record_and_disabled_feature_do_not_move_files(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let requests = Arc::new(ProviderRequests::default());
    cx.update(|_, cx| cx.set_global(crate::max::InferenceService(requests.clone())));
    let dir = tempfile::tempdir().unwrap();
    for hidden in [false, true] {
        let path = dir
            .path()
            .join(if hidden { "hidden.pdf" } else { "disabled.pdf" });
        std::fs::write(&path, b"original").unwrap();
        let id = record(&view, cx, path.clone());
        cx.run_until_parked();
        view.update(cx, |w, _| {
            if hidden {
                w.profile.handle.hide_completed_download(id);
            } else {
                w.edit_profile(|b| {
                    b.settings
                        .max
                        .set_enabled(echo_ai::MaxFeature::TidyDownloads, false)
                });
            }
        });
        let reply = requests.0.lock().unwrap().last().unwrap().1.clone();
        reply.complete(Ok("{\"stem\":\"Do Not Move\"}".into()));
        cx.run_until_parked();
        assert!(path.exists());
        assert!(!dir.path().join("Do Not Move.pdf").exists());
    }
}
