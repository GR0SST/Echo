//! Native failure shape: OnBeforeDownload has an authoritative name but ALL
//! copied item info names are empty. Fake driver keys do not reconstruct tokens.
use super::*;
fn fixture() -> (Config, engine::DownloadInfo, DownloadProvenance) {
    let config = Config::parse(&format!(
        "http://127.0.0.1:18473 {} baseline",
        std::env::temp_dir().display()
    ))
    .unwrap();
    let info = engine::DownloadInfo {
        phase: engine::DownloadPhase::InProgress,
        interrupt_reason: 0,
        current_speed: 0,
        percent_complete: 0,
        total_bytes: 33_554_432,
        received_bytes: 0,
        start_time: 0,
        end_time: 0,
        full_path: PathBuf::new(),
        url: config.fixture_url(),
        original_url: config.fixture_url(),
        suggested_file_name: String::new(),
        content_disposition: String::new(),
        mime_type: "application/octet-stream".into(),
    };
    (config, info, DownloadProvenance::new(901, 20, 30, false))
}
#[test]
fn requested_authoritative_name_with_empty_info_resolves_then_empty_progress_is_accepted() {
    let (config, mut info, provenance) = fixture();
    let requested = DownloadDestination::Requested {
        suggested_name: config.filename.clone(),
    };
    let mut bound = None;
    let token = 7u64; // Generic driver key, never a constructed DownloadToken.
    let mut known = None;
    let mut resolved = Vec::new();
    assert!(!config.accepts(&info, &DownloadDestination::Unrequested, provenance, false));
    if config.accepts(
        &info,
        &requested,
        provenance,
        is_bound_item(bound, known.as_ref(), provenance, &token),
    ) {
        bind_provenance(901, &mut bound, provenance).unwrap();
        known = Some(token);
        resolved.push(config.output.clone());
    }
    assert_eq!(resolved, [config.output.clone()]);
    assert_eq!(bound, Some(provenance));
    info.received_bytes = 393_216; // First nonzero callback seen in failed baseline.
    info.full_path = config.output.with_extension("bin.crdownload");
    assert!(info.suggested_file_name.is_empty());
    let bound_item = is_bound_item(bound, known.as_ref(), provenance, &token);
    assert!(config.accepts(
        &info,
        &DownloadDestination::Resolved(config.output.clone()),
        provenance,
        bound_item
    ));
    info.phase = engine::DownloadPhase::Complete;
    info.received_bytes = 33_554_432;
    info.full_path = config.output.clone();
    assert!(config.accepts(
        &info,
        &DownloadDestination::Resolved(config.output.clone()),
        provenance,
        bound_item
    ));
}
#[test]
fn unknown_name_wrong_requested_name_and_unbound_resolved_target_are_denied() {
    let (config, mut info, provenance) = fixture();
    for name in [
        String::new(),
        "another.bin".into(),
        format!("../{}", config.filename),
    ] {
        assert!(!config.accepts(
            &info,
            &DownloadDestination::Requested {
                suggested_name: name
            },
            provenance,
            false
        ));
    }
    assert!(!config.accepts(
        &info,
        &DownloadDestination::Resolved(config.output.clone()),
        provenance,
        false
    ));
    info.suggested_file_name = config.filename.clone();
    assert!(!config.accepts(&info, &DownloadDestination::Unrequested, provenance, false));
    info.suggested_file_name = "different.bin".into();
    assert!(!config.accepts(
        &info,
        &DownloadDestination::Requested {
            suggested_name: config.filename.clone()
        },
        provenance,
        false
    ));
    assert!(!config.accepts(
        &info,
        &DownloadDestination::Resolved(config.output.clone()),
        provenance,
        true
    ));
}
#[test]
fn exact_resolved_path_original_urls_privacy_provenance_and_token_stay_required() {
    let (config, mut info, provenance) = fixture();
    let resolved = DownloadDestination::Resolved(config.output.clone());
    assert!(!config.accepts(
        &info,
        &DownloadDestination::Resolved(config.output.with_file_name("other.bin")),
        provenance,
        true
    ));
    for changed in [
        DownloadProvenance::new(902, 20, 30, false),
        DownloadProvenance::new(901, 21, 30, false),
        DownloadProvenance::new(901, 20, 31, false),
        DownloadProvenance::new(901, 20, 30, true),
    ] {
        let known = is_bound_item(Some(provenance), Some(&7u64), changed, &7);
        assert!(!config.accepts(&info, &resolved, changed, known));
    }
    assert!(!config.accepts(
        &info,
        &resolved,
        provenance,
        is_bound_item(Some(provenance), Some(&7u64), provenance, &8)
    ));
    info.url.push_str("?different");
    assert!(!config.accepts(&info, &resolved, provenance, true));
    info.url = config.fixture_url();
    info.original_url = "https://example.test/unrelated".into();
    assert!(!config.accepts(&info, &resolved, provenance, true));
}
