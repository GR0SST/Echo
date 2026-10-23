//! Generic service observations use fictional u64 keys. These tests do not mint
//! DownloadToken, call Engine, or claim native acceptance.
use super::*;
use browser_core::Browser;
use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
};

type Model = Rc<RefCell<Browser>>;
struct Fixture {
    _tmp: tempfile::TempDir,
    folder: PathBuf,
    path: PathBuf,
    service: Rc<RefCell<DownloadService<u64, Model>>>,
    gate: ConsumerGate,
    model: Model,
    profile: UserProfileId,
    provenance: DownloadProvenance,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().canonicalize().unwrap();
        let model = Rc::new(RefCell::new(Browser::new()));
        let profile = model.borrow().default_user_profile_id();
        let service = Rc::new(RefCell::new(DownloadService::default()));
        let owner = service
            .borrow_mut()
            .router
            .register_normal(profile, model.clone(), SourceKind::Normal)
            .unwrap();
        let provenance = DownloadProvenance::new(owner, 83, 97, false);
        let mut f = Self {
            _tmp: tmp,
            folder,
            path: PathBuf::new(),
            service,
            gate: ConsumerGate::default(),
            model,
            profile,
            provenance,
        };
        let request = f.event(DownloadPhase::Pending, true);
        assert!(f.service.borrow_mut().receive_copied([request]).is_empty());
        let stage = Plan {
            folder: f.folder.clone(),
            candidates: vec!["existing".into(), "chosen".into(), "never".into()],
        }
        .reserve(400, provenance)
        .unwrap();
        f.path = stage.stage_path().unwrap().to_owned();
        f.service.borrow_mut().retain_staging(400, stage).unwrap();
        f
    }
    fn event(&self, phase: DownloadPhase, requested: bool) -> TransferEvent<u64> {
        TransferEvent {
            token: 400,
            provenance: self.provenance,
            destination_requested: requested,
            destination: if requested {
                DownloadDestination::Requested {
                    suggested_name: "callback.bin".into(),
                }
            } else {
                DownloadDestination::Unrequested
            },
            info: DownloadInfo {
                phase,
                interrupt_reason: 0,
                current_speed: 0,
                percent_complete: 100,
                total_bytes: 10,
                received_bytes: 10,
                start_time: 0,
                end_time: 13_400_000_000_000_001,
                full_path: self.path.clone(),
                url: "https://fixture.test/final".into(),
                original_url: "https://fixture.test/redirect".into(),
                suggested_file_name: "mutable-item-name".into(),
                content_disposition: String::new(),
                mime_type: "application/octet-stream".into(),
            },
        }
    }
    fn work(&self, owner: ConsumerOwner) -> Work<u64, Model> {
        let lease = self.gate.acquire(owner).unwrap();
        let work = self.service.borrow().staged.detach(400, lease).unwrap();
        work
    }
    fn handoff(&self) {
        let mut work = self.work(ConsumerOwner::Running);
        work.handoff(|key, path| {
            assert_eq!(key, 400);
            assert_eq!(path, self.path);
            assert!(!path.exists());
            assert!(self.service.try_borrow_mut().is_ok());
            fs::write(path, b"download").unwrap();
            Ok(())
        })
        .unwrap();
    }
    fn ingest(&self, event: TransferEvent<u64>) {
        assert!(self.service.borrow_mut().receive_copied([event]).is_empty());
    }
    fn complete(&self) {
        self.ingest(self.event(DownloadPhase::Complete, false));
    }
    fn record(&self, work: &mut Work<u64, Model>) -> CompletedDownloadId {
        work.publish_and_write(|model, profile, completion, proof| {
            assert!(self.service.try_borrow_mut().is_ok());
            assert_eq!(profile, self.profile);
            assert_eq!(completion.final_path, proof.path());
            assert_ne!(completion.final_path, self.path);
            WriteOutcome::Recorded(
                model
                    .borrow_mut()
                    .record_completed_download(profile, completion)
                    .unwrap(),
            )
        })
        .unwrap()
    }
}
#[test]
fn sole_service_publishes_collision_then_records_frozen_metadata_once() {
    let f = Fixture::new();
    fs::write(f.folder.join("existing"), b"user").unwrap();
    f.handoff();
    f.complete();
    // No stage path reaches the legacy canonical writer in either ingestion mode.
    assert!(!f.service.borrow().router.has_pending_records());
    assert!(f.service.borrow().has_pending_records());
    let mut work = f.work(ConsumerOwner::Running);
    assert!(f.service.borrow().has_pending_records()); // Detached work is a barrier.
    let id = f.record(&mut work);
    assert_eq!(
        work.publish_and_write(|_, _, _, _| panic!("duplicate record")),
        Ok(id)
    );
    drop(work);
    f.complete();
    assert!(!f.service.borrow().has_pending_records());
    let model = f.model.borrow();
    let records = model.completed_downloads(f.profile).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].final_path(), f.folder.join("chosen"));
    assert_eq!(records[0].source_url(), "https://fixture.test/final");
    assert_eq!(records[0].original_url(), "https://fixture.test/redirect");
    assert_eq!(records[0].mime_type(), "application/octet-stream");
    assert_eq!(
        records[0].finished_at(),
        DownloadTime::from_cef_basetime(13_400_000_000_000_001)
    );
    assert_eq!(fs::read(f.folder.join("existing")).unwrap(), b"user");
    assert!(!f.folder.join("never").exists());
    let slot = &f.service.borrow().staged.slots[&400];
    let job = slot.job.take().unwrap();
    assert_eq!(job.request.suggested_name, "callback.bin");
    assert_eq!(job.request.info.suggested_file_name, "mutable-item-name");
    assert_eq!(job.key, (400, f.provenance));
    slot.job.set(Some(job));
}
#[test]
fn definite_writer_failure_requires_explicit_retry_and_reuses_published_path() {
    let f = Fixture::new();
    f.handoff();
    f.complete();
    let mut work = f.work(ConsumerOwner::Running);
    let mut published = PathBuf::new();
    assert_eq!(
        work.publish_and_write(|_, _, completion, _| {
            published = completion.final_path;
            WriteOutcome::NotWritten("writer unavailable".into())
        }),
        Err("writer unavailable".into())
    );
    assert_eq!(
        work.publish_and_write(|_, _, _, _| panic!("implicit retry")),
        Err("writer unavailable".into())
    );
    drop(work);
    assert!(f.service.borrow().has_pending_records());
    f.service.borrow().staged.retry(400, f.profile).unwrap();
    let mut retry = f.work(ConsumerOwner::Running);
    let id = f.record(&mut retry);
    drop(retry);
    assert_eq!(fs::read(&published).unwrap(), b"download");
    assert!(!f.folder.join("chosen").exists()); // No second publication.
    assert_eq!(
        f.model.borrow().completed_downloads(f.profile).unwrap()[0].id(),
        id
    );
    let borrow = f.service.borrow();
    let slot = &borrow.staged.slots[&400];
    let job = slot.job.take().unwrap();
    assert_eq!(job.history, ["writer unavailable"]);
    slot.job.set(Some(job));
}
#[test]
fn unknown_writer_or_unwind_blocks_retry_and_preserves_return_slot() {
    for unwind in [false, true] {
        let f = Fixture::new();
        f.handoff();
        f.complete();
        let mut work = f.work(ConsumerOwner::Running);
        if unwind {
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = work.publish_and_write(|_, _, _, _| panic!("injected writer unwind"));
                }))
                .is_err()
            );
        } else {
            assert!(
                work.publish_and_write(|_, _, _, _| WriteOutcome::Unknown("ack lost".into()))
                    .is_err()
            );
        }
        drop(work);
        assert!(f.service.borrow().staged.retry(400, f.profile).is_err());
        assert!(f.service.borrow().has_pending_records());
        let mut work = f.work(ConsumerOwner::Running);
        assert!(
            work.publish_and_write(|_, _, _, _| panic!("duplicate uncertain write"))
                .is_err()
        );
        assert_eq!(fs::read(f.folder.join("existing")).unwrap(), b"download");
        assert!(!f.folder.join("chosen").exists());
    }
}
#[test]
fn only_exact_updated_complete_publishes_and_cancellation_is_a_tombstone() {
    let f = Fixture::new();
    f.handoff();
    for phase in [
        DownloadPhase::Pending,
        DownloadPhase::InProgress,
        DownloadPhase::Paused,
        DownloadPhase::Interrupted,
    ] {
        f.ingest(f.event(phase, false));
    }
    f.ingest(f.event(DownloadPhase::Complete, true));
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| panic!("not Complete"))
            .is_err()
    );
    drop(work);
    f.ingest(f.event(DownloadPhase::Canceled, false));
    f.complete();
    assert!(!f.service.borrow().has_pending_records());
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| panic!("canceled"))
            .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), b"download");
    assert!(!f.folder.join("existing").exists());
}
#[test]
fn identity_and_path_conflicts_cannot_publish_or_fall_into_direct_writer() {
    for wrong_identity in [false, true] {
        let f = Fixture::new();
        f.handoff();
        let mut event = f.event(DownloadPhase::Complete, false);
        if wrong_identity {
            event.provenance = DownloadProvenance::new(f.provenance.owner(), 999, 97, false);
        } else {
            event.info.full_path = f.folder.join("wrong");
        }
        f.ingest(event);
        assert!(!f.service.borrow().router.has_pending_records());
        assert!(!f.service.borrow().errors.is_empty());
        let mut work = f.work(ConsumerOwner::Running);
        assert!(
            work.publish_and_write(|_, _, _, _| panic!("conflicted Complete"))
                .is_err()
        );
        assert_eq!(fs::read(&f.path).unwrap(), b"download");
        assert!(!f.folder.join("existing").exists());
    }
}
#[test]
fn reentrant_quit_sees_inflight_then_sticky_writer_failure() {
    let f = Fixture::new();
    f.handoff();
    f.complete();
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| {
            f.gate.handoff().unwrap();
            assert!(matches!(
                f.gate.acquire(ConsumerOwner::Shutdown),
                Err(ShutdownDownloadError::InFlight)
            ));
            assert!(f.service.try_borrow_mut().is_ok());
            WriteOutcome::NotWritten("in-flight write failed".into())
        })
        .is_err()
    );
    drop(work);
    assert!(
        matches!(f.gate.acquire(ConsumerOwner::Shutdown), Err(ShutdownDownloadError::Failure(e)) if e == "in-flight write failed")
    );
    assert!(f.service.borrow().has_pending_records());
}
#[test]
fn copied_complete_reentered_during_handoff_is_retained_until_lease_return() {
    let f = Fixture::new();
    let mut work = f.work(ConsumerOwner::Running);
    work.handoff(|_, path| {
        fs::write(path, b"download").unwrap();
        f.complete(); // Copied generic ingress, not a second native drain.
        f.gate.handoff().unwrap();
        assert!(matches!(
            f.gate.acquire(ConsumerOwner::Shutdown),
            Err(ShutdownDownloadError::InFlight)
        ));
        Ok(())
    })
    .unwrap();
    drop(work);
    let mut shutdown_work = f.work(ConsumerOwner::Shutdown);
    f.record(&mut shutdown_work);
    drop(shutdown_work);
    assert!(!f.service.borrow().has_pending_records());
}
#[test]
fn shutdown_ingress_uses_same_state_and_blocks_flush_until_recorded() {
    let f = Fixture::new();
    f.handoff();
    let shutdown = ShutdownDownloads::take(f.service.clone(), f.gate.clone()).unwrap();
    shutdown
        .ingest_copied_and_persist_with([f.event(DownloadPhase::Complete, false)], |_, _, _| {
            panic!("stage sent to legacy writer")
        })
        .unwrap();
    assert!(
        shutdown
            .flush_retained_with(Rc::ptr_eq, |_| panic!("premature flush"))
            .is_err()
    );
    assert!(shutdown.finish().is_err());
    let mut work = shutdown.detach_staged_work(400).unwrap();
    f.record(&mut work);
    drop(work);
    shutdown
        .flush_retained_with(Rc::ptr_eq, |_| Ok(()))
        .unwrap();
    shutdown.finish().unwrap();
}
#[test]
fn shutdown_cannot_continue_a_new_stage_and_publication_failure_retains_bytes() {
    let f = Fixture::new();
    f.gate.handoff().unwrap();
    let mut work = f.work(ConsumerOwner::Shutdown);
    assert!(
        work.handoff(|_, _| panic!("post-seal native effect"))
            .is_err()
    );
    drop(work);
    let f = Fixture::new();
    f.handoff();
    f.complete();
    for name in ["existing", "chosen", "never"] {
        fs::write(f.folder.join(name), b"user").unwrap();
    }
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| panic!("failed publication"))
            .is_err()
    );
    drop(work);
    assert!(f.service.borrow().has_pending_records());
    f.service.borrow().staged.retry(400, f.profile).unwrap();
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| panic!("exhaustion retry"))
            .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), b"download");
    for name in ["existing", "chosen", "never"] {
        assert_eq!(fs::read(f.folder.join(name)).unwrap(), b"user");
    }
}

#[test]
fn captured_profile_survives_source_drop_and_space_reassignment() {
    let f = Fixture::new();
    let source = f.model.clone();
    let other = f.model.borrow_mut().create_user_profile("Other").unwrap();
    let space = f.model.borrow_mut().add_space("Changed", 90.);
    f.model
        .borrow_mut()
        .assign_space_user_profile(space, other)
        .unwrap();
    drop(source);
    f.handoff();
    f.complete();
    let mut work = f.work(ConsumerOwner::Running);
    f.record(&mut work);
    drop(work);
    assert_eq!(
        f.model
            .borrow()
            .completed_downloads(f.profile)
            .unwrap()
            .len(),
        1
    );
    assert!(
        f.model
            .borrow()
            .completed_downloads(other)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn replaced_published_file_blocks_writer_retry_without_creating_another_link() {
    let f = Fixture::new();
    f.handoff();
    f.complete();
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| WriteOutcome::NotWritten("offline".into()))
            .is_err()
    );
    drop(work);
    fs::rename(f.folder.join("existing"), f.folder.join("saved-original")).unwrap();
    fs::write(f.folder.join("existing"), b"replacement").unwrap();
    f.service.borrow().staged.retry(400, f.profile).unwrap();
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.publish_and_write(|_, _, _, _| panic!("invalid proof reached writer"))
            .is_err()
    );
    assert_eq!(fs::read(f.folder.join("existing")).unwrap(), b"replacement");
    assert_eq!(
        fs::read(f.folder.join("saved-original")).unwrap(),
        b"download"
    );
    assert!(!f.folder.join("chosen").exists());
}
#[test]
fn native_handoff_unknown_and_queued_identity_failure_survive_quit() {
    let f = Fixture::new();
    let mut work = f.work(ConsumerOwner::Running);
    assert!(
        work.handoff(|_, _| {
            f.gate.handoff().unwrap();
            let mut bad = f.event(DownloadPhase::Complete, false);
            bad.provenance = DownloadProvenance::new(f.provenance.owner(), 999, 97, false);
            f.ingest(bad);
            Err("native acknowledgment lost".into())
        })
        .is_err()
    );
    drop(work);
    assert!(f.service.borrow().staged.retry(400, f.profile).is_err());
    assert!(
        matches!(f.gate.acquire(ConsumerOwner::Shutdown), Err(ShutdownDownloadError::Failure(e))
        if e.contains("native acknowledgment lost") && e.contains("identity changed"))
    );
    assert!(f.service.borrow().has_pending_records());
}

#[test]
fn detached_queued_cancellation_overtakes_handoff_with_valid_absent_stage() {
    let f = Fixture::new();
    let mut work = f.work(ConsumerOwner::Running);
    f.ingest(f.event(DownloadPhase::Canceled, false));
    // Keep staging valid and payload absent: R1 would pass filesystem validation
    // and actually call Continue. Only the queued observation can suppress it.
    assert!(f.path.parent().unwrap().is_dir());
    assert!(!f.path.exists());
    assert_eq!(
        work.handoff(|_, _| panic!("queued cancellation authorized Continue")),
        Err("Staged transfer is canceled".into())
    );
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Running),
        Err(ShutdownDownloadError::InFlight)
    ));
    assert!(f.service.try_borrow_mut().is_ok());
    drop(work);
    assert!(!f.service.borrow().has_pending_records());
    assert!(f.path.parent().unwrap().is_dir());
    assert!(!f.path.exists());
    assert!(!f.folder.join("existing").exists());
}

#[test]
fn detached_queued_identity_failure_overtakes_handoff_and_remains_sticky_at_quit() {
    let f = Fixture::new();
    let mut work = f.work(ConsumerOwner::Running);
    let mut bad = f.event(DownloadPhase::InProgress, false);
    bad.provenance = DownloadProvenance::new(f.provenance.owner(), 999, 97, false);
    f.ingest(bad);
    // As in the cancellation regression, no filesystem failure or quit seal
    // can mask a stale authorization; R1 would invoke this closure.
    assert!(f.path.parent().unwrap().is_dir());
    assert!(!f.path.exists());
    assert_eq!(
        work.handoff(|_, _| panic!("queued identity failure authorized Continue")),
        Err("Staged download identity changed".into())
    );
    // Seal only AFTER the rejected handoff: gate suppression must not mask P2.
    f.gate.handoff().unwrap();
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Shutdown),
        Err(ShutdownDownloadError::InFlight)
    ));
    drop(work);
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Shutdown),
        Err(ShutdownDownloadError::Failure(error)) if error == "Staged download identity changed"
    ));
    assert!(f.service.borrow().has_pending_records());
    assert!(f.path.parent().unwrap().is_dir());
    assert!(!f.path.exists());
    assert!(!f.folder.join("existing").exists());
}

#[test]
fn same_work_reduces_queued_complete_before_publication_without_releasing_lease() {
    let f = Fixture::new();
    let mut work = f.work(ConsumerOwner::Running);
    work.handoff(|_, path| {
        fs::write(path, b"download").unwrap();
        f.complete();
        Ok(())
    })
    .unwrap();
    // The same Work must see Complete before its NEXT effect, without Drop,
    // acquiring a new lease, or draining an independent event source.
    f.record(&mut work);
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Running),
        Err(ShutdownDownloadError::InFlight)
    ));
    drop(work);
    assert!(!f.service.borrow().has_pending_records());
    assert_eq!(
        f.model
            .borrow()
            .completed_downloads(f.profile)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(fs::read(f.folder.join("existing")).unwrap(), b"download");
}

#[test]
fn queued_identity_failure_blocks_publication_before_any_writer_or_filesystem_validation() {
    let f = Fixture::new();
    f.handoff();
    f.complete();
    let mut work = f.work(ConsumerOwner::Running);
    let mut bad = f.event(DownloadPhase::InProgress, false);
    bad.provenance = DownloadProvenance::new(f.provenance.owner(), 999, 97, false);
    f.ingest(bad);
    let retained = f.folder.join("retained-stage");
    fs::rename(f.path.parent().unwrap(), &retained).unwrap();
    assert_eq!(
        work.publish_and_write(|_, _, _, _| panic!("queued failure reached canonical writer")),
        Err("Staged download identity changed".into())
    );
    drop(work);
    assert!(f.service.borrow().has_pending_records());
    assert_eq!(fs::read(retained.join("payload")).unwrap(), b"download");
    assert!(!f.folder.join("existing").exists());
    assert!(
        f.model
            .borrow()
            .completed_downloads(f.profile)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn queued_cancel_during_handoff_blocks_next_effect_on_same_work() {
    let f = Fixture::new();
    let mut work = f.work(ConsumerOwner::Running);
    work.handoff(|_, path| {
        fs::write(path, b"partial retained").unwrap();
        f.ingest(f.event(DownloadPhase::Canceled, false));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        work.publish_and_write(|_, _, _, _| panic!("canceled handoff reached writer")),
        Err("Staged transfer is canceled".into())
    );
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Running),
        Err(ShutdownDownloadError::InFlight)
    ));
    drop(work);
    assert!(!f.service.borrow().has_pending_records());
    assert_eq!(fs::read(&f.path).unwrap(), b"partial retained");
    assert!(!f.folder.join("existing").exists());
}

#[test]
fn running_driver_holds_one_lease_through_handoff_complete_publication_and_record() {
    let f = Fixture::new();
    let lease = f.gate.acquire(ConsumerOwner::Running).unwrap();
    let calls = Cell::new(0);
    let (lease, result) = drive(
        &f.service,
        lease,
        |token, path| {
            assert_eq!(token, 400);
            assert!(f.service.try_borrow_mut().is_ok());
            assert!(matches!(
                f.gate.acquire(ConsumerOwner::Running),
                Err(ShutdownDownloadError::InFlight)
            ));
            fs::write(path, b"download").unwrap();
            f.complete();
            Ok(())
        },
        |model, profile, completion, proof| {
            assert!(f.service.try_borrow_mut().is_ok());
            assert!(matches!(
                f.gate.acquire(ConsumerOwner::Running),
                Err(ShutdownDownloadError::InFlight)
            ));
            assert_eq!(completion.final_path, proof.path());
            calls.set(calls.get() + 1);
            WriteOutcome::Recorded(
                model
                    .borrow_mut()
                    .record_completed_download(profile, completion)
                    .unwrap(),
            )
        },
    );
    assert!(result.is_ok());
    assert_eq!(calls.get(), 1);
    let (lease, result) = drive(
        &f.service,
        lease,
        |_, _| panic!("second Continue"),
        |_, _, _, _| panic!("second record"),
    );
    assert!(result.is_ok());
    assert!(f.gate.snapshot().1);
    assert!(!f.service.borrow().has_pending_records());
    drop(lease);
    assert!(!f.gate.snapshot().1);
    assert_eq!(
        f.model
            .borrow()
            .completed_downloads(f.profile)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn driver_cancellation_during_reentrant_handoff_does_not_fail_quit() {
    let f = Fixture::new();
    let lease = f.gate.acquire(ConsumerOwner::Running).unwrap();
    let (lease, result) = drive(
        &f.service,
        lease,
        |_, path| {
            fs::write(path, b"partial").unwrap();
            f.gate.handoff().unwrap();
            f.ingest(f.event(DownloadPhase::Canceled, false));
            Ok(())
        },
        |_, _, _, _| panic!("canceled transfer recorded"),
    );
    assert!(result.is_ok());
    assert!(f.service.borrow().errors.is_empty());
    assert!(!f.service.borrow().has_pending_records());
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Shutdown),
        Err(ShutdownDownloadError::InFlight)
    ));
    drop(lease);
    drop(f.gate.acquire(ConsumerOwner::Shutdown).unwrap());
    assert_eq!(fs::read(&f.path).unwrap(), b"partial");
}

#[test]
fn driver_returns_sticky_identity_failure_before_releasing_quit_lease() {
    let f = Fixture::new();
    let lease = f.gate.acquire(ConsumerOwner::Running).unwrap();
    let (lease, result) = drive(
        &f.service,
        lease,
        |_, _| {
            let mut bad = f.event(DownloadPhase::InProgress, false);
            bad.provenance = DownloadProvenance::new(f.provenance.owner(), 999, 97, false);
            f.ingest(bad);
            f.gate.handoff().unwrap();
            Ok(())
        },
        |_, _, _, _| panic!("identity failure reached writer"),
    );
    assert_eq!(result, Err("Staged download identity changed".into()));
    assert_eq!(
        f.service.borrow().errors,
        ["Staged download identity changed"]
    );
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Shutdown),
        Err(ShutdownDownloadError::InFlight)
    ));
    drop(lease);
    assert!(matches!(
        f.gate.acquire(ConsumerOwner::Shutdown),
        Err(ShutdownDownloadError::Failure(_))
    ));
}

#[test]
fn shutdown_driver_publishes_final_copied_complete_before_flush_and_finish() {
    let f = Fixture::new();
    f.handoff();
    fs::write(&f.path, b"download").unwrap();
    let shutdown = shutdown::ShutdownDownloads::take(f.service.clone(), f.gate.clone()).unwrap();
    shutdown
        .ingest_copied_and_persist_with([f.event(DownloadPhase::Complete, false)], |_, _, _| {
            panic!("stage path entered legacy writer")
        })
        .unwrap();
    assert!(shutdown
        .flush_retained_with(Rc::ptr_eq, |_| Ok(()))
        .is_err());
    shutdown
        .publish_staged_with(|model, profile, completion, proof| {
            assert_eq!(completion.final_path, proof.path());
            assert!(f.service.try_borrow_mut().is_ok());
            assert!(matches!(
                f.gate.acquire(ConsumerOwner::Shutdown),
                Err(ShutdownDownloadError::InFlight)
            ));
            WriteOutcome::Recorded(
                model
                    .borrow_mut()
                    .record_completed_download(profile, completion)
                    .unwrap(),
            )
        })
        .unwrap();
    shutdown
        .flush_retained_with(Rc::ptr_eq, |_| Ok(()))
        .unwrap();
    shutdown.finish().unwrap();
    assert_eq!(f.gate.snapshot(), (ConsumerOwner::Finished, false));
    assert_eq!(
        f.model
            .borrow()
            .completed_downloads(f.profile)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn shutdown_driver_never_continues_an_unresolved_stage() {
    let f = Fixture::new();
    let shutdown = shutdown::ShutdownDownloads::take(f.service.clone(), f.gate.clone()).unwrap();
    shutdown
        .publish_staged_with(|_, _, _, _| panic!("uncompleted transfer recorded"))
        .unwrap();
    assert!(!f.path.exists());
    assert!(shutdown
        .flush_retained_with(Rc::ptr_eq, |_| Ok(()))
        .is_err());
    shutdown
        .ingest_copied_and_persist_with([f.event(DownloadPhase::Canceled, false)], |_, _, _| {
            panic!("canceled transfer recorded")
        })
        .unwrap();
    shutdown
        .publish_staged_with(|_, _, _, _| panic!("canceled transfer recorded"))
        .unwrap();
    shutdown
        .flush_retained_with(Rc::ptr_eq, |_| Ok(()))
        .unwrap();
    shutdown.finish().unwrap();
}

#[gpui::test]
fn production_profile_adapter_records_only_final_proof_and_keeps_definite_errors_unmutated(
    cx: &mut gpui::TestAppContext,
) {
    let f = Fixture::new();
    let (handle, profile) = cx.update(|cx| {
        let browser = Browser::new();
        let profile = browser.default_user_profile_id();
        (ProfileHandle::new(browser, cx), profile)
    });
    let mut stage = Plan {
        folder: f.folder.clone(),
        candidates: vec!["real-profile.bin".into()],
    }
    .reserve(401u64, f.provenance)
    .unwrap();
    let registration = stage.registration();
    let path = stage.validate_handoff().unwrap().to_owned();
    fs::write(&path, b"actual owned bytes").unwrap();
    let mut publication = stage
        .observe(
            &registration,
            &(401, f.provenance),
            Observation::UpdatedComplete,
            &path,
        )
        .unwrap()
        .unwrap();
    let proof = publication.execute().unwrap();
    publication.validate_proof(&proof).unwrap();
    let mut completion = DownloadCompletion {
        final_path: proof.path().to_owned(),
        source_url: "https://fixture.test/final".into(),
        original_url: "https://fixture.test/original".into(),
        mime_type: "application/octet-stream".into(),
        finished_at: DownloadTime::WindowsEpochMicros(0),
    };
    let before = handle.download_inventory().unwrap();
    assert!(matches!(
        record_profile(&handle, profile, completion.clone(), &proof),
        WriteOutcome::NotWritten(_)
    ));
    assert_eq!(handle.download_inventory().unwrap(), before);
    completion.finished_at = DownloadTime::Unknown;
    let mut alias = completion.clone();
    alias.final_path = proof
        .path()
        .parent()
        .unwrap()
        .join(".")
        .join(proof.path().file_name().unwrap());
    assert_ne!(alias.final_path.as_os_str(), proof.path().as_os_str());
    assert!(matches!(
        record_profile(&handle, profile, alias, &proof),
        WriteOutcome::NotWritten(_)
    ));
    assert_eq!(handle.download_inventory().unwrap(), before);
    let id = match record_profile(&handle, profile, completion, &proof) {
        WriteOutcome::Recorded(id) => id,
        _ => panic!("valid proof record rejected"),
    };
    assert_eq!(id.sequence(), 1);
    assert_eq!(id.profile(), profile);
    let (_, records) = handle.download_inventory().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].final_path(), proof.path());
    assert_ne!(records[0].final_path(), path);
    assert_eq!(
        fs::read(records[0].final_path()).unwrap(),
        b"actual owned bytes"
    );
}
