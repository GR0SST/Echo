//! Real-filesystem tests for standalone download publication and byte preservation.
use super::*;
use std::{fs::OpenOptions, io::Write, sync::Barrier, thread};

struct Fixture {
    _root: tempfile::TempDir,
    folder: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let folder = fs::canonicalize(root.path()).unwrap().join("destination");
        fs::create_dir(&folder).unwrap();
        Self {
            _root: root,
            folder,
        }
    }
    fn stage(&self, key: u64, names: &[&str]) -> Staging<u64> {
        Staging::reserve(key, &self.folder, names.iter().map(PathBuf::from).collect()).unwrap()
    }
    fn complete(&self, mut staging: Staging<u64>, bytes: &[u8]) -> Publication {
        let path = staging.stage_path().unwrap().to_owned();
        // Emulates engine output, not an empty final-target reservation.
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
        drop(file);
        let identity = staging.registration();
        staging
            .observe(
                &identity,
                &identity.key,
                Observation::UpdatedComplete,
                &path,
            )
            .unwrap()
            .unwrap()
    }
}
#[test]
fn fresh_absent_payload_owned_directory_and_injected_names_only() {
    let f = Fixture::new();
    let staging = f.stage(10, &["foo.bin", "foo (1).bin"]);
    let path = staging.validate_handoff().unwrap().to_owned();
    assert_eq!(path.parent().unwrap().parent(), Some(f.folder.as_path()));
    assert!(matches!(fs::symlink_metadata(&path), Err(e) if e.kind() == io::ErrorKind::NotFound));
    assert!(!f.folder.join("foo.bin").exists());
    assert!(!f.folder.join("foo (1).bin").exists());
    drop(staging);
    assert!(path.parent().unwrap().is_dir()); // No automatic cleanup.
}
#[test]
fn noncomplete_never_detaches_even_with_full_bytes_on_disk() {
    let f = Fixture::new();
    let mut staging = f.stage(10, &["foo.bin"]);
    let path = staging.stage_path().unwrap().to_owned();
    fs::write(&path, b"complete bytes alone prove nothing").unwrap();
    let identity = staging.registration();
    for observation in [
        Observation::DestinationRequested,
        Observation::UpdatedPending,
        Observation::UpdatedInProgress,
        Observation::UpdatedPaused,
        Observation::UpdatedInterrupted,
        Observation::UpdatedCanceled,
    ] {
        assert!(staging
            .observe(&identity, &10, observation, &path)
            .unwrap()
            .is_none());
    }
    assert!(!f.folder.join("foo.bin").exists());
    drop(staging);
    assert_eq!(
        fs::read(path).unwrap(),
        b"complete bytes alone prove nothing"
    );
}
#[test]
fn exact_registration_key_and_path_required_then_detach_once() {
    let f = Fixture::new();
    let mut staging = f.stage(10, &["foo.bin"]);
    let another = f.stage(10, &["foo.bin"]); // Same key cannot forge nonce.
    let path = staging.stage_path().unwrap().to_owned();
    let identity = staging.registration();
    assert!(matches!(
        staging.observe(
            &another.registration(),
            &10,
            Observation::UpdatedComplete,
            &path
        ),
        Err(Error::IdentityMismatch)
    ));
    assert!(matches!(
        staging.observe(&identity, &11, Observation::UpdatedComplete, &path),
        Err(Error::IdentityMismatch)
    ));
    for wrong in [
        f.folder.join("foo.bin"),
        path.with_file_name("other"),
        path.parent().unwrap().join(".").join("payload"),
    ] {
        assert!(matches!(
            staging.observe(&identity, &10, Observation::UpdatedComplete, &wrong),
            Err(Error::PathMismatch)
        ));
    }
    let mut publication = staging
        .observe(&identity, &10, Observation::UpdatedComplete, &path)
        .unwrap()
        .unwrap();
    // Pure detach succeeds without looking at filesystem; executor detects missing bytes.
    assert!(
        matches!(publication.execute(), Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound)
    );
    assert!(matches!(
        staging.observe(&identity, &10, Observation::UpdatedComplete, &path),
        Err(Error::Detached)
    ));
    assert!(!f.folder.join("foo.bin").exists());
}
#[test]
fn rejects_escape_nested_duplicate_and_empty_candidates_before_reservation() {
    let f = Fixture::new();
    for candidates in [
        vec![],
        vec![PathBuf::from("../escape")],
        vec![PathBuf::from("nested/file")],
        vec![PathBuf::from(".")],
        vec![f.folder.parent().unwrap().join("escape")],
        vec![PathBuf::from("foo"), f.folder.join("foo")],
    ] {
        assert!(Staging::reserve(10, &f.folder, candidates).is_err());
    }
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}
#[test]
fn publication_collision_preserves_original_and_uses_next_injected_path() {
    let f = Fixture::new();
    fs::write(f.folder.join("foo.bin"), b"original").unwrap();
    let mut publication = f.complete(f.stage(10, &["foo.bin", "foo (1).bin"]), b"download");
    let proof = publication.execute().unwrap();
    assert_eq!(proof.path(), f.folder.join("foo (1).bin"));
    publication.validate_proof(&proof).unwrap();
    assert_eq!(fs::read(f.folder.join("foo.bin")).unwrap(), b"original");
    assert_eq!(fs::read(proof.path()).unwrap(), b"download");
}
#[test]
fn late_collision_between_validation_and_syscall_cannot_overwrite() {
    let f = Fixture::new();
    let mut publication = f.complete(f.stage(10, &["first", "second"]), b"download");
    let mut calls = 0;
    let proof = publication
        .execute_with(|_, target| {
            calls += 1;
            if calls == 1 {
                fs::write(target, b"late user data").unwrap();
            }
        })
        .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(proof.path(), f.folder.join("second"));
    assert_eq!(fs::read(f.folder.join("first")).unwrap(), b"late user data");
}
#[test]
fn concurrent_publishers_collision_without_overwrite() {
    let f = Fixture::new();
    let left = f.complete(f.stage(10, &["same", "left"]), b"left bytes");
    let right = f.complete(f.stage(11, &["same", "right"]), b"right bytes");
    let barrier = Arc::new(Barrier::new(2));
    let run = |mut p: Publication, barrier: Arc<Barrier>| {
        thread::spawn(move || {
            let mut first = true;
            let proof = p
                .execute_with(|_, _| {
                    if first {
                        first = false;
                        barrier.wait();
                    }
                })
                .unwrap();
            (proof.path().to_owned(), fs::read(proof.path()).unwrap())
        })
    };
    let a = run(left, barrier.clone());
    let b = run(right, barrier);
    let (ap, ab) = a.join().unwrap();
    let (bp, bb) = b.join().unwrap();
    assert_ne!(ap, bp);
    assert_eq!(ab, b"left bytes");
    assert_eq!(bb, b"right bytes");
    assert!(ap == f.folder.join("same") || bp == f.folder.join("same"));
}
#[test]
fn exhaustion_and_drop_retain_complete_stage_no_invented_suffix() {
    let f = Fixture::new();
    fs::write(f.folder.join("only"), b"user").unwrap();
    let mut p = f.complete(f.stage(10, &["only"]), b"retained");
    let stage = p.stage_path().to_owned();
    assert!(matches!(p.execute(), Err(Error::CandidatesExhausted)));
    assert!(matches!(p.execute(), Err(Error::CandidatesExhausted)));
    drop(p);
    assert_eq!(fs::read(stage).unwrap(), b"retained");
    assert_eq!(fs::read(f.folder.join("only")).unwrap(), b"user");
    assert!(!f.folder.join("only (1)").exists());
}
#[test]
fn effect_drop_before_execute_retains_complete_bytes() {
    let f = Fixture::new();
    let p = f.complete(f.stage(10, &["final"]), b"retained");
    let stage = p.stage_path().to_owned();
    drop(p);
    assert_eq!(fs::read(stage).unwrap(), b"retained");
    assert!(!f.folder.join("final").exists());
}
#[test]
fn error_and_temppath_error_drop_do_not_delete_stage() {
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["first", "never-tried"]), b"retained");
    let stage = p.stage_path().to_owned();
    // Force a deterministic non-collision OS error without permission assumptions.
    // Private test mutation only: reserve() rejects this out-of-folder candidate.
    let bad = f.folder.join("absent-parent/target");
    p.reservation.candidates[0] = bad.clone();
    assert!(matches!(p.execute(), Err(Error::Io(_))));
    assert_eq!(p.state(), &State::Attempting { candidate: bad });
    assert!(matches!(p.execute(), Err(Error::Ambiguous)));
    drop(p);
    assert_eq!(fs::read(stage).unwrap(), b"retained");
    assert!(!f.folder.join("never-tried").exists());
}
#[test]
fn repeat_publish_returns_same_identity_and_never_publishes_again() {
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["first", "never-tried"]), b"download");
    let proof = p.execute().unwrap();
    let again = p.execute().unwrap();
    assert_eq!(proof.path(), again.path());
    assert_eq!(proof.id, again.id);
    p.validate_proof(&proof).unwrap();
    assert!(!f.folder.join("never-tried").exists());
    let mut other = f.complete(f.stage(11, &["other"]), b"other");
    other.execute().unwrap();
    assert!(matches!(
        other.validate_proof(&proof),
        Err(Error::IdentityMismatch)
    ));
}
#[test]
fn replaced_target_invalidates_proof_and_repeat_does_not_retry() {
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["first", "never-tried"]), b"download");
    let proof = p.execute().unwrap();
    fs::rename(proof.path(), f.folder.join("retained-original")).unwrap();
    fs::write(proof.path(), b"replacement").unwrap();
    assert!(matches!(p.validate_proof(&proof), Err(Error::Replaced)));
    assert!(matches!(p.execute(), Err(Error::Replaced)));
    assert_eq!(fs::read(proof.path()).unwrap(), b"replacement");
    assert_eq!(
        fs::read(f.folder.join("retained-original")).unwrap(),
        b"download"
    );
    assert!(!f.folder.join("never-tried").exists());
}
#[test]
fn replaced_source_after_attempt_is_not_published() {
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["first", "second"]), b"download");
    // Emulate source identity pinned during a prior effect. Replacement during
    // the actual path-based syscall is an explicitly unsupported hostile race.
    p.source = Some(Lease::open(p.stage_path(), false).unwrap());
    let stage = p.stage_path().to_owned();
    let saved = stage.with_file_name("retained-original");
    fs::rename(&stage, &saved).unwrap();
    fs::write(&stage, b"replacement").unwrap();
    assert!(matches!(p.execute(), Err(Error::Replaced)));
    drop(p);
    assert_eq!(fs::read(saved).unwrap(), b"download");
    assert_eq!(fs::read(stage).unwrap(), b"replacement");
    assert!(!f.folder.join("first").exists());
}
#[test]
fn folder_and_staging_directory_replacement_fail_without_touching_replacements() {
    for replace_folder in [true, false] {
        let f = Fixture::new();
        let mut p = f.complete(f.stage(10, &["final"]), b"download");
        let old = if replace_folder {
            f.folder.clone()
        } else {
            p.reservation.stage_dir.clone()
        };
        let moved = old.with_file_name("moved-owned-directory");
        fs::rename(&old, &moved).unwrap();
        fs::create_dir(&old).unwrap();
        fs::write(old.join("user"), b"user").unwrap();
        assert!(matches!(p.execute(), Err(Error::Replaced)));
        drop(p);
        assert_eq!(fs::read(old.join("user")).unwrap(), b"user");
        assert!(!f.folder.join("final").exists());
        let retained = if replace_folder {
            let entries: Vec<_> = fs::read_dir(&moved)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries[0].join("payload")
        } else {
            moved.join("payload")
        };
        assert_eq!(fs::read(retained).unwrap(), b"download");
    }
}
#[test]
fn crash_window_state_blocks_retry_even_when_both_names_exist() {
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["first", "never-tried"]), b"download");
    p.source = Some(Lease::open(p.stage_path(), false).unwrap());
    let first = f.folder.join("first");
    // Emulate successful link + interruption before acknowledgment/unlink.
    p.state = State::Attempting {
        candidate: first.clone(),
    };
    fs::hard_link(p.stage_path(), &first).unwrap();
    assert!(matches!(p.execute(), Err(Error::Ambiguous)));
    assert_eq!(fs::read(p.stage_path()).unwrap(), b"download");
    assert_eq!(fs::read(first).unwrap(), b"download");
    assert!(!f.folder.join("never-tried").exists());
    // Does NOT exercise process-restart recovery; no such API exists.
}
#[test]
fn preexisting_source_hardlink_is_ambiguous_not_a_suffix_retry() {
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["first", "never-tried"]), b"download");
    fs::hard_link(p.stage_path(), f.folder.join("first")).unwrap();
    assert!(matches!(p.execute(), Err(Error::Ambiguous)));
    assert!(!f.folder.join("never-tried").exists());
    assert_eq!(fs::read(p.stage_path()).unwrap(), b"download");
}

#[cfg(unix)]
#[test]
fn existing_symlink_and_dangling_symlink_are_collisions_not_replaced() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let user = f.folder.join("user");
    fs::write(&user, b"user").unwrap();
    let missing = f.folder.join("missing");
    symlink(&user, f.folder.join("first")).unwrap();
    symlink(&missing, f.folder.join("second")).unwrap();
    let mut p = f.complete(f.stage(10, &["first", "second", "third"]), b"download");
    assert_eq!(p.execute().unwrap().path(), f.folder.join("third"));
    assert_eq!(fs::read(&user).unwrap(), b"user");
    assert_eq!(fs::read_link(f.folder.join("first")).unwrap(), user);
    assert_eq!(fs::read_link(f.folder.join("second")).unwrap(), missing);
    assert!(!missing.exists());
}
#[cfg(unix)]
#[test]
fn symlink_folder_and_payload_rejected_without_deleting_user_path() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let alias = f.folder.with_file_name("alias");
    symlink(&f.folder, &alias).unwrap();
    assert!(Staging::reserve(10, &alias, vec!["final".into()]).is_err());
    let mut s = f.stage(10, &["final"]);
    let path = s.stage_path().unwrap().to_owned();
    let user = f.folder.join("user");
    fs::write(&user, b"user").unwrap();
    symlink(&user, &path).unwrap();
    assert!(s.validate_handoff().is_err());
    let reg = s.registration();
    let mut p = s
        .observe(&reg, &10, Observation::UpdatedComplete, &path)
        .unwrap()
        .unwrap();
    assert!(matches!(p.execute(), Err(Error::Replaced)));
    drop(p);
    assert_eq!(fs::read_link(path).unwrap(), user);
    assert_eq!(fs::read(user).unwrap(), b"user");
}
#[cfg(unix)]
#[test]
fn native_non_utf8_candidate_is_lossless() {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
    };
    let f = Fixture::new();
    let original_bytes = b"file-\xff.bin";
    let name = OsString::from_vec(original_bytes.to_vec());
    let stage = Staging::reserve(10, &f.folder, vec![PathBuf::from(&name)]).unwrap();
    let expected = f.folder.join(&name);
    // Native path preservation is unconditional, independent of what the actual
    // destination filesystem permits. No UTF-8 replacement/normalization allowed.
    assert_eq!(
        stage.reservation.as_ref().unwrap().candidates[0]
            .as_os_str()
            .as_bytes(),
        expected.as_os_str().as_bytes()
    );
    assert_eq!(
        stage.reservation.as_ref().unwrap().candidates[0]
            .file_name()
            .unwrap()
            .as_bytes(),
        original_bytes
    );
    let mut p = f.complete(stage, b"bytes");
    let staged_path = p.stage_path().to_owned();
    match p.execute() {
        Ok(proof) => {
            assert_eq!(
                proof.path().as_os_str().as_bytes(),
                expected.as_os_str().as_bytes()
            );
            assert_eq!(proof.path().file_name().unwrap().as_bytes(), original_bytes);
            p.validate_proof(&proof).unwrap();
            assert_eq!(fs::read(proof.path()).unwrap(), b"bytes");
        }
        // Apple APFS FAQ: creation requires valid UTF-8 filenames. Darwin libc
        // EILSEQ == 92 (cached libc 0.2.191 apple/mod.rs). Accept ONLY this exact
        // macOS rejection, never all errors or all Unix filesystem failures.
        Err(Error::Io(error)) if cfg!(target_os = "macos") && error.raw_os_error() == Some(92) => {
            assert_eq!(
                p.state(),
                &State::Attempting {
                    candidate: expected.clone()
                }
            );
            if let State::Attempting { candidate } = p.state() {
                assert_eq!(
                    candidate.as_os_str().as_bytes(),
                    expected.as_os_str().as_bytes()
                );
            }
            assert_eq!(fs::read(&staged_path).unwrap(), b"bytes");
            assert!(matches!(p.execute(), Err(Error::Ambiguous)));
            // Detect any silently converted/lossy final name without querying an
            // invalid path with exists(), which would hide filesystem errors.
            let entries = fs::read_dir(&f.folder)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            assert_eq!(
                entries[0].as_os_str().as_bytes(),
                staged_path.parent().unwrap().as_os_str().as_bytes()
            );
            drop(p);
            assert_eq!(fs::read(&staged_path).unwrap(), b"bytes");
        }
        Err(error) => panic!("unexpected non-UTF8 publication failure: {error:?}"),
    }
}

#[cfg(windows)]
#[test]
fn native_unpaired_surrogate_candidate_is_lossless() {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    let f = Fixture::new();
    let name = OsString::from_wide(&[0x66, 0xD800, 0x2E, 0x62]);
    let stage = Staging::reserve(10, &f.folder, vec![PathBuf::from(&name)]).unwrap();
    let mut p = f.complete(stage, b"bytes");
    let proof = p.execute().unwrap();
    assert_eq!(proof.path().file_name(), Some(name.as_os_str()));
    assert_eq!(fs::read(proof.path()).unwrap(), b"bytes");
}

#[cfg(unix)]
#[test]
fn same_inode_and_unix_permissions_preserved() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let f = Fixture::new();
    let mut p = f.complete(f.stage(10, &["final"]), b"download");
    fs::set_permissions(p.stage_path(), fs::Permissions::from_mode(0o640)).unwrap();
    let before = fs::metadata(p.stage_path()).unwrap();
    let proof = p.execute().unwrap();
    let after = fs::metadata(proof.path()).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    assert_eq!(after.permissions().mode() & 0o777, 0o640);
}
#[cfg(target_os = "linux")]
#[test]
fn cross_filesystem_error_retains_stage_and_never_copies_or_retries() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    // Environment-dependent test: skip if no independent tmpfs is mounted.
    let Ok(other) = tempfile::tempdir_in("/dev/shm") else {
        return;
    };
    if fs::metadata(other.path()).unwrap().dev() == fs::metadata(&f.folder).unwrap().dev() {
        return;
    }
    let mut p = f.complete(f.stage(10, &["first", "never-tried"]), b"retained");
    let stage = p.stage_path().to_owned();
    let foreign = other.path().join("foreign");
    // Internal syscall test injection; public reserve rejects foreign candidates.
    p.reservation.candidates[0] = foreign.clone();
    assert!(matches!(p.execute(), Err(Error::Io(e)) if e.kind() != io::ErrorKind::AlreadyExists));
    assert!(matches!(p.execute(), Err(Error::Ambiguous)));
    assert!(!foreign.exists());
    assert!(!f.folder.join("never-tried").exists());
    drop(p);
    assert_eq!(fs::read(stage).unwrap(), b"retained");
}
#[cfg(windows)]
#[test]
fn windows_stream_and_normalization_aliases_rejected() {
    let f = Fixture::new();
    for name in ["file:stream", "file.", "file "] {
        assert!(matches!(
            Staging::reserve(10, &f.folder, vec![name.into()]),
            Err(Error::InvalidCandidate)
        ));
    }
}

#[cfg(unix)]
#[test]
fn staging_directory_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let s = f.stage(10, &["final"]);
    let dir = s.stage_path().unwrap().parent().unwrap();
    assert_eq!(
        fs::metadata(dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn relative_trailing_separator_and_dot_rejected_before_reservation() {
    let f = Fixture::new();
    for input in ["report.bin/", "report.bin/."] {
        assert!(matches!(
            candidate(&f.folder, Path::new(input)),
            Err(Error::InvalidCandidate)
        ));
        assert!(matches!(
            Staging::reserve(10, &f.folder, vec![input.into()]),
            Err(Error::InvalidCandidate)
        ));
    }
    #[cfg(windows)]
    for input in [r"report.bin\", r"report.bin\."] {
        assert!(matches!(
            candidate(&f.folder, Path::new(input)),
            Err(Error::InvalidCandidate)
        ));
        assert!(matches!(
            Staging::reserve(10, &f.folder, vec![input.into()]),
            Err(Error::InvalidCandidate)
        ));
    }
    assert_eq!(
        candidate(&f.folder, Path::new("report.bin"))
            .unwrap()
            .as_os_str(),
        f.folder.join("report.bin").as_os_str()
    );
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}

#[cfg(target_os = "macos")]
#[test]
fn publication_preserves_quarantine_attribute_and_file_permissions() {
    use std::{
        ffi::{c_char, c_int, c_void, CString},
        os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    };
    // Darwin sys/xattr.h. Only disposable test files are accessed here.
    unsafe extern "C" {
        fn setxattr(
            path: *const c_char,
            name: *const c_char,
            value: *const c_void,
            size: usize,
            position: u32,
            options: c_int,
        ) -> c_int;
        fn getxattr(
            path: *const c_char,
            name: *const c_char,
            value: *mut c_void,
            size: usize,
            position: u32,
            options: c_int,
        ) -> isize;
    }
    let fixture = Fixture::new();
    let mut publication = fixture.complete(fixture.stage(10, &["report.bin"]), b"complete");
    let source = publication.stage_path().to_owned();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
    let source_path = CString::new(source.as_os_str().as_bytes()).unwrap();
    let attribute = CString::new("com.apple.quarantine").unwrap();
    let value = b"0081;00000000;Echo acceptance;00000000-0000-0000-0000-000000000000";
    assert_eq!(
        unsafe {
            setxattr(
                source_path.as_ptr(),
                attribute.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        },
        0,
        "setting owned test attribute: {}",
        io::Error::last_os_error()
    );
    let source_id = file_id(&File::open(&source).unwrap()).unwrap();

    let proof = publication.execute().unwrap();
    publication.validate_proof(&proof).unwrap();
    assert_eq!(
        file_id(&File::open(proof.path()).unwrap()).unwrap(),
        source_id
    );
    assert_eq!(
        fs::metadata(proof.path()).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let destination = CString::new(proof.path().as_os_str().as_bytes()).unwrap();
    let mut observed = vec![0u8; value.len() + 1];
    let length = unsafe {
        getxattr(
            destination.as_ptr(),
            attribute.as_ptr(),
            observed.as_mut_ptr().cast(),
            observed.len(),
            0,
            0,
        )
    };
    assert_eq!(
        length,
        value.len() as isize,
        "reading published attribute: {}",
        io::Error::last_os_error()
    );
    assert_eq!(&observed[..value.len()], value);
}

#[test]
fn absolute_trailing_separator_and_dot_rejected_before_reservation() {
    let f = Fixture::new();
    for input in ["report.bin/", "report.bin/."] {
        let absolute = f.folder.join(input);
        assert!(matches!(
            candidate(&f.folder, &absolute),
            Err(Error::InvalidCandidate)
        ));
        assert!(matches!(
            Staging::reserve(10, &f.folder, vec![absolute]),
            Err(Error::InvalidCandidate)
        ));
    }
    #[cfg(windows)]
    for input in [r"report.bin\", r"report.bin\."] {
        let absolute = f.folder.join(input);
        assert!(matches!(
            candidate(&f.folder, &absolute),
            Err(Error::InvalidCandidate)
        ));
        assert!(matches!(
            Staging::reserve(10, &f.folder, vec![absolute]),
            Err(Error::InvalidCandidate)
        ));
    }
    let accepted = f.folder.join("report.bin");
    assert_eq!(
        candidate(&f.folder, &accepted).unwrap().as_os_str(),
        accepted.as_os_str()
    );
    assert_eq!(fs::read_dir(&f.folder).unwrap().count(), 0);
}

#[test]
fn chosen_existing_file_is_untouched_until_genuine_complete_then_replaced_once() {
    let f = Fixture::new();
    let target = f.folder.join("chosen.bin");
    fs::write(&target, b"old bytes").unwrap();
    let mut staging = Staging::reserve_chosen(10, &target).unwrap();
    let path = staging.validate_handoff().unwrap().to_owned();
    let registration = staging.registration();
    fs::write(&path, b"new bytes").unwrap();
    for observation in [Observation::UpdatedInProgress, Observation::UpdatedPaused] {
        assert!(staging.observe(&registration, &10, observation, &path).unwrap().is_none());
        assert_eq!(fs::read(&target).unwrap(), b"old bytes");
    }
    let mut publication = staging.observe(&registration, &10, Observation::UpdatedComplete, &path).unwrap().unwrap();
    assert_eq!(publication.execute().unwrap().path(), target);
    assert_eq!(fs::read(&target).unwrap(), b"new bytes");
    assert_eq!(publication.execute().unwrap().path(), target);
}

#[test]
fn chosen_file_replacement_or_edit_during_transfer_preserves_both_files() {
    for replace_identity in [false, true] {
        let f = Fixture::new();
        let target = f.folder.join("chosen.bin");
        fs::write(&target, b"old bytes").unwrap();
        let staging = Staging::reserve_chosen(10, &target).unwrap();
        let mut publication = f.complete(staging, b"download bytes");
        if replace_identity {
            fs::rename(&target, f.folder.join("old.bin")).unwrap();
        }
        fs::write(&target, b"a newer user file with a different length").unwrap();
        assert!(matches!(publication.execute(), Err(Error::Replaced)));
        assert_eq!(fs::read(&target).unwrap(), b"a newer user file with a different length");
        assert_eq!(fs::read(publication.stage_path()).unwrap(), b"download bytes");
    }
}

#[test]
fn chosen_absent_path_cannot_gain_overwrite_authority_from_a_late_collision() {
    let f = Fixture::new();
    let target = f.folder.join("chosen.bin");
    let staging = Staging::reserve_chosen(10, &target).unwrap();
    let mut publication = f.complete(staging, b"download bytes");
    fs::write(&target, b"late file").unwrap();
    assert!(matches!(publication.execute(), Err(Error::CandidatesExhausted)));
    assert_eq!(fs::read(&target).unwrap(), b"late file");
    assert_eq!(fs::read(publication.stage_path()).unwrap(), b"download bytes");
}

#[test]
fn selected_replacement_rechecks_identity_immediately_before_its_syscall() {
    let f = Fixture::new();
    let target = f.folder.join("chosen.bin");
    fs::write(&target, b"old bytes").unwrap();
    let staging = Staging::reserve_chosen(10, &target).unwrap();
    let mut publication = f.complete(staging, b"download bytes");
    assert!(matches!(publication.execute_with(|_, target| {
        fs::rename(target, target.with_extension("original")).unwrap();
        fs::write(target, b"late user bytes").unwrap();
    }), Err(Error::Replaced)));
    assert_eq!(fs::read(&target).unwrap(), b"late user bytes");
    assert_eq!(fs::read(publication.stage_path()).unwrap(), b"download bytes");
}

#[cfg(unix)]
#[test]
fn native_chosen_symlink_does_not_authorize_replacing_its_target() {
    let f = Fixture::new();
    let real = f.folder.join("real.bin");
    let link = f.folder.join("chosen.bin");
    fs::write(&real, b"user bytes").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(matches!(Staging::reserve_chosen(10, &link), Err(Error::Replaced)));
    assert_eq!(fs::read(real).unwrap(), b"user bytes");
}
