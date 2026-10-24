use super::*;
use crate::session;
fn completion(name: &str) -> DownloadCompletion {
    DownloadCompletion {
        final_path: std::env::temp_dir().join(name),
        source_url: "https://fixture.test/final".into(),
        original_url: "https://fixture.test/redirect".into(),
        mime_type: "application/octet-stream".into(),
        finished_at: DownloadTime::from_cef_basetime(13_400_000_000_000_001),
    }
}
#[test]
fn independent_profile_watermarks_and_actual_metadata_survive_session_roundtrip() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let work = browser.create_user_profile("Work").unwrap();
    let first = browser
        .record_completed_download(work, completion("server (2).bin"))
        .unwrap();
    let other = browser
        .record_completed_download(default, completion("other.bin"))
        .unwrap();
    assert_eq!(first.sequence(), 1);
    assert_eq!(other.sequence(), 1);
    assert_ne!(first, other);
    let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
    let record = &restored.completed_downloads(work).unwrap()[0];
    assert_eq!(record.id(), first);
    assert_eq!(record.final_name(), "server (2).bin");
    assert_eq!(record.final_path(), completion("server (2).bin").final_path);
    assert_eq!(record.source_url(), "https://fixture.test/final");
    assert_eq!(record.original_url(), "https://fixture.test/redirect");
    assert_eq!(record.finished_at(), completion("x").finished_at);
    assert_eq!(
        restored
            .record_completed_download(work, completion("next.bin"))
            .unwrap()
            .sequence(),
        2
    );
}
#[test]
fn old_profile_without_downloads_migrates_empty_but_malformed_data_never_falls_back() {
    let browser = Browser::new();
    let mut value: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    value["profiles"]["1"]
        .as_object_mut()
        .unwrap()
        .remove("downloads");
    let restored = session::from_json(&value.to_string()).unwrap();
    assert!(
        restored
            .completed_downloads(restored.default_user_profile_id())
            .unwrap()
            .is_empty()
    );
    value["profiles"]["1"]["downloads"] =
        serde_json::json!({"last_sequence": "bad", "records": []});
    assert!(session::from_json(&value.to_string()).is_err());
    // A legacy-shaped root may not smuggle unowned download metadata into Default.
    let mut legacy = serde_json::to_value(Browser::new()).unwrap();
    for field in [
        "profiles",
        "default_user_profile_id",
        "next_user_profile_id",
    ] {
        legacy.as_object_mut().unwrap().remove(field);
    }
    for space in legacy["spaces"].as_array_mut().unwrap() {
        space.as_object_mut().unwrap().remove("user_profile_id");
    }
    legacy["downloads"] = serde_json::json!([]);
    assert!(session::from_json(&legacy.to_string()).is_err());
}
#[test]
fn corrupt_ownership_duplicate_ids_and_lowered_watermarks_are_rejected() {
    let mut browser = Browser::new();
    let id = browser.default_user_profile_id();
    browser
        .record_completed_download(id, completion("file.bin"))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    for mutation in 0..3 {
        let mut bad = value.clone();
        let registry = &mut bad["profiles"]["1"]["downloads"];
        match mutation {
            0 => registry["records"][0]["id"]["profile"] = 999.into(),
            1 => {
                let duplicate = registry["records"][0].clone();
                registry["records"].as_array_mut().unwrap().push(duplicate);
            }
            _ => registry["last_sequence"] = 0.into(),
        }
        assert!(session::from_json(&bad.to_string()).is_err());
    }
}
#[test]
fn exhaustion_is_atomic_and_a_missing_profile_does_not_record_in_default() {
    let mut browser = Browser::new();
    let default = browser.default_user_profile_id();
    let removed = browser.create_user_profile("Removed").unwrap();
    browser.delete_user_profile(removed).unwrap();
    assert_eq!(
        browser.record_completed_download(removed, completion("x.bin")),
        Err(DownloadRecordError::UnknownProfile)
    );
    browser
        .profiles
        .get_mut(&default)
        .unwrap()
        .downloads
        .last_sequence = u64::MAX - 1;
    assert_eq!(
        browser
            .record_completed_download(default, completion("last.bin"))
            .unwrap()
            .sequence(),
        u64::MAX
    );
    let before = session::to_json(&browser);
    assert_eq!(
        browser.record_completed_download(default, completion("overflow.bin")),
        Err(DownloadRecordError::IdentityExhausted)
    );
    assert_eq!(session::to_json(&browser), before);
    assert!(session::from_json(&before).is_ok());
}
#[test]
fn private_and_blank_never_receive_serializable_normal_records() {
    for mut browser in [Browser::new_private(), Browser::new_blank()] {
        let id = browser.active_user_profile_id();
        assert_eq!(
            browser.record_completed_download(id, completion("secret.bin")),
            Err(DownloadRecordError::WindowLocalModel)
        );
        let json = session::to_json(&browser);
        assert!(!json.contains("secret.bin"));
        assert!(browser.completed_downloads(id).unwrap().is_empty());
    }
}
#[test]
fn null_infinite_and_pre_windows_epoch_values_are_preserved_without_unix_conversion() {
    let mut browser = Browser::new();
    let profile = browser.default_user_profile_id();
    let times = [0, i64::MIN, i64::MAX, -1, 13_400_000_000_000_001];
    for raw in times {
        let mut item = completion("time.bin");
        item.finished_at = DownloadTime::from_cef_basetime(raw);
        browser.record_completed_download(profile, item).unwrap();
    }
    let restored = session::from_json(&session::to_json(&browser)).unwrap();
    let times: Vec<_> = restored
        .completed_downloads(profile)
        .unwrap()
        .iter()
        .map(|r| r.finished_at())
        .collect();
    assert_eq!(
        times,
        vec![
            DownloadTime::Unknown,
            DownloadTime::Invalid(i64::MIN),
            DownloadTime::Invalid(i64::MAX),
            DownloadTime::WindowsEpochMicros(-1),
            DownloadTime::WindowsEpochMicros(13_400_000_000_000_001)
        ]
    );
}
#[test]
fn invalid_paths_or_time_tags_do_not_consume_identity() {
    let mut browser = Browser::new();
    let id = browser.default_user_profile_id();
    let mut item = completion("bad.bin");
    item.final_path = "relative.bin".into();
    assert_eq!(
        browser.record_completed_download(id, item),
        Err(DownloadRecordError::InvalidFinalPath)
    );
    let mut item = completion("bad.bin");
    item.finished_at = DownloadTime::WindowsEpochMicros(0);
    assert_eq!(
        browser.record_completed_download(id, item),
        Err(DownloadRecordError::InvalidTime)
    );
    assert_eq!(
        browser
            .record_completed_download(id, completion("good.bin"))
            .unwrap()
            .sequence(),
        1
    );
}

#[test]
fn unowned_root_downloads_are_rejected_before_both_versioned_and_legacy_loading() {
    let mut value: serde_json::Value =
        serde_json::from_str(&session::to_json(&Browser::new())).unwrap();
    value["profiles"]["1"]
        .as_object_mut()
        .unwrap()
        .remove("downloads");
    let old_v2 = session::from_json(&value.to_string()).unwrap();
    assert!(
        old_v2
            .completed_downloads(old_v2.default_user_profile_id())
            .unwrap()
            .is_empty()
    );
    for root_data in [
        serde_json::json!([]),
        serde_json::json!({"records": []}),
        serde_json::Value::Null,
    ] {
        let mut v2 = value.clone();
        v2["downloads"] = root_data.clone();
        assert!(session::from_json(&v2.to_string()).is_err());
        let mut legacy = value.clone();
        for key in [
            "format_version",
            "profiles",
            "default_user_profile_id",
            "next_user_profile_id",
        ] {
            legacy.as_object_mut().unwrap().remove(key);
        }
        for space in legacy["spaces"].as_array_mut().unwrap() {
            space.as_object_mut().unwrap().remove("user_profile_id");
        }
        assert!(session::from_json(&legacy.to_string()).is_ok());
        legacy["downloads"] = root_data;
        assert!(session::from_json(&legacy.to_string()).is_err());
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_unix_path_and_filename_roundtrip_through_full_session_json() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let name = OsString::from_vec(b"actual-\xff.bin".to_vec());
    let path = std::env::temp_dir().join(&name);
    let mut browser = Browser::new();
    let profile = browser.default_user_profile_id();
    let mut item = completion("unused");
    item.final_path = path.clone();
    let id = browser.record_completed_download(profile, item).unwrap();
    let json = session::to_json(&browser); // Must never panic on a valid native path.
    assert!(json.contains("unix_bytes"));
    let restored = session::from_json(&json).unwrap();
    let record = &restored.completed_downloads(profile).unwrap()[0];
    assert_eq!(record.id(), id);
    assert_eq!(
        record.final_path().as_os_str().as_bytes(),
        path.as_os_str().as_bytes()
    );
    assert_eq!(record.final_name().as_bytes(), b"actual-\xff.bin");
    assert_eq!(session::to_json(&restored), json);
}

#[cfg(windows)]
#[test]
fn unpaired_windows_surrogate_path_and_filename_roundtrip_through_full_session_json() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let units = [
        0x0066, 0xD800, 0x0069, 0x006C, 0x0065, 0x002E, 0x0062, 0x0069, 0x006E,
    ];
    let name = OsString::from_wide(&units);
    let path = PathBuf::from(r"C:\Downloads").join(&name);
    let mut browser = Browser::new();
    let profile = browser.default_user_profile_id();
    let mut item = completion("unused");
    item.final_path = path.clone();
    let id = browser.record_completed_download(profile, item).unwrap();
    let json = session::to_json(&browser);
    assert!(json.contains("windows_wide"));
    let restored = session::from_json(&json).unwrap();
    let record = &restored.completed_downloads(profile).unwrap()[0];
    assert_eq!(record.id(), id);
    assert_eq!(
        record
            .final_path()
            .as_os_str()
            .encode_wide()
            .collect::<Vec<_>>(),
        path.as_os_str().encode_wide().collect::<Vec<_>>()
    );
    assert_eq!(record.final_name().encode_wide().collect::<Vec<_>>(), units);
    assert_eq!(session::to_json(&restored), json);
}

#[test]
fn foreign_native_path_encoding_is_rejected_without_lossy_conversion() {
    let mut browser = Browser::new();
    let profile = browser.default_user_profile_id();
    browser
        .record_completed_download(profile, completion("encoded.bin"))
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
    #[cfg(unix)]
    {
        value["profiles"]["1"]["downloads"]["records"][0]["final_path"] =
            serde_json::json!({"encoding": "windows_wide", "units": [67, 58, 92, 55296]});
    }
    #[cfg(windows)]
    {
        value["profiles"]["1"]["downloads"]["records"][0]["final_path"] =
            serde_json::json!({"encoding": "unix_bytes", "units": [47, 255]});
    }
    assert!(session::from_json(&value.to_string()).is_err());
}

#[test]
fn hiding_downloads_preserves_other_profiles_and_persisted_identity_watermarks() {
    let mut browser = Browser::new();
    let first = browser.default_user_profile_id();
    let second = browser.create_user_profile("Second").unwrap();
    let hidden = browser.record_completed_download(first, completion("same.bin")).unwrap();
    let retained = browser.record_completed_download(second, completion("same.bin")).unwrap();
    assert!(browser.hide_completed_download(hidden));
    assert!(!browser.hide_completed_download(hidden));
    assert!(browser.completed_downloads(first).unwrap().is_empty());
    assert_eq!(browser.completed_downloads(second).unwrap()[0].id(),retained);
    let mut restored = session::from_json(&session::to_json(&browser)).unwrap();
    let next = restored.record_completed_download(first, completion("same.bin")).unwrap();
    assert_eq!(next.sequence(),hidden.sequence()+1);
    assert_eq!(restored.completed_downloads(second).unwrap()[0].id(),retained);
}
