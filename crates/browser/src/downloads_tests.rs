use super::*;
use browser_core::{Browser, session};

fn info(phase: DownloadPhase, name: &str) -> DownloadInfo {
    DownloadInfo {
        phase,
        interrupt_reason: 0,
        current_speed: 0,
        percent_complete: 100,
        total_bytes: 10,
        received_bytes: 10,
        start_time: 0,
        end_time: 13_400_000_000_000_001,
        full_path: std::env::temp_dir().join(name),
        url: "https://fixture.test/final".into(),
        original_url: "https://fixture.test/redirect".into(),
        suggested_file_name: "suggested.bin".into(),
        content_disposition: String::new(),
        mime_type: "application/octet-stream".into(),
    }
}
fn event(token: u64, owner: u64, phase: DownloadPhase) -> TransferEvent<u64> {
    TransferEvent {
        token,
        provenance: DownloadProvenance::new(owner, 83, 97, false),
        info: info(phase, "actual (2).bin"),
        destination_requested: false,
    }
}
type Model = Rc<RefCell<Browser>>;
fn persist(router: &mut DownloadRouter<u64, Model>) -> Vec<String> {
    router.persist(|model, profile, completion| {
        model
            .borrow_mut()
            .record_completed_download(profile, completion)
            .map_err(|e| format!("{e:?}"))
    })
}
fn register(router: &mut DownloadRouter<u64, Model>, model: &Model, profile: UserProfileId) -> u64 {
    router
        .register_normal(profile, model.clone(), SourceKind::Normal)
        .unwrap()
}
#[test]
fn captured_owner_survives_source_drop_space_change_and_terminal_replay() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let first = model.borrow().default_user_profile_id();
    let second = model.borrow_mut().create_user_profile("Second").unwrap();
    let mut router = DownloadRouter::default();
    router.last_owner = 71;
    let source = model.clone();
    let owner = register(&mut router, &source, first);
    assert_eq!(owner, 72); // Opaque key is not a serialized/reconstructed profile ID.
    router.reduce([event(400, owner, DownloadPhase::InProgress)], |_, _| {
        DestinationDecision::Defer
    });
    drop(source); // No window/page/host exists in this router model.
    let space = model.borrow_mut().add_space("Changed", 90.);
    model
        .borrow_mut()
        .assign_space_user_profile(space, second)
        .unwrap();
    router.reduce([event(400, owner, DownloadPhase::Complete)], |_, _| {
        DestinationDecision::Defer
    });
    assert!(persist(&mut router).is_empty());
    let before = session::to_json(&model.borrow());
    let mut replay = event(400, owner, DownloadPhase::Complete);
    replay.info.full_path = std::env::temp_dir().join("late.bin");
    router.reduce(
        [replay, event(400, owner, DownloadPhase::InProgress)],
        |_, _| DestinationDecision::Defer,
    );
    assert!(persist(&mut router).is_empty());
    assert_eq!(session::to_json(&model.borrow()), before);
    assert!(
        model
            .borrow()
            .completed_downloads(second)
            .unwrap()
            .is_empty()
    );
    let borrow = model.borrow();
    let records = borrow.completed_downloads(first).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].final_name(), "actual (2).bin");
}
#[test]
fn full_bytes_paused_interrupted_canceled_and_destination_are_not_completion() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    for (i, phase) in [
        DownloadPhase::Pending,
        DownloadPhase::InProgress,
        DownloadPhase::Paused,
        DownloadPhase::Interrupted,
        DownloadPhase::Canceled,
    ]
    .into_iter()
    .enumerate()
    {
        router.reduce([event(i as u64, owner, phase)], |_, _| {
            DestinationDecision::Defer
        });
    }
    let mut before = event(91, owner, DownloadPhase::Complete);
    before.destination_requested = true;
    router.reduce([before], |_, _| DestinationDecision::Defer);
    assert!(persist(&mut router).is_empty());
    assert!(!router.has_pending_records());
    assert!(
        model
            .borrow()
            .completed_downloads(profile)
            .unwrap()
            .is_empty()
    );
    assert!(router.control(2, profile, DownloadControl::Resume).is_ok());
    assert!(router.control(3, profile, DownloadControl::Resume).is_ok());
    assert!(router.control(4, profile, DownloadControl::Resume).is_err());
}
#[test]
fn private_unknown_owner_and_same_token_provenance_changes_never_touch_normal_writer() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    let mut private = event(1, owner, DownloadPhase::Complete);
    private.provenance = DownloadProvenance::new(owner, 83, 97, true);
    let (_, errors) = router.reduce(
        [private, event(2, owner + 1, DownloadPhase::Complete)],
        |_, _| DestinationDecision::Defer,
    );
    assert_eq!(errors.len(), 2);
    assert!(router.ledger.is_empty());
    router.reduce([event(3, owner, DownloadPhase::InProgress)], |_, _| {
        DestinationDecision::Defer
    });
    let mut wrong = event(3, owner, DownloadPhase::Complete);
    wrong.provenance = DownloadProvenance::new(owner, 83, 98, false);
    assert_eq!(
        router
            .reduce([wrong], |_, _| DestinationDecision::Defer)
            .1
            .len(),
        1
    );
    assert!(persist(&mut router).is_empty());
    assert!(
        model
            .borrow()
            .completed_downloads(profile)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn failed_completion_commit_retries_same_token_without_default_fallback_or_duplicate() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    router.reduce([event(1, owner, DownloadPhase::Complete)], |_, _| {
        DestinationDecision::Defer
    });
    assert_eq!(
        router.persist(|_, _, _| Err("writer temporarily unavailable".into())),
        vec!["writer temporarily unavailable"]
    );
    assert!(router.has_pending_records());
    assert!(persist(&mut router).is_empty());
    assert!(!router.has_pending_records());
    assert!(persist(&mut router).is_empty());
    assert_eq!(
        model.borrow().completed_downloads(profile).unwrap().len(),
        1
    );
}
#[test]
fn effects_reenter_router_only_after_borrow_release_and_do_not_confirm_completion() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let router = Rc::new(RefCell::new(DownloadRouter::default()));
    let owner = register(&mut router.borrow_mut(), &model, profile);
    let mut request = event(1, owner, DownloadPhase::Pending);
    request.destination_requested = true;
    let effects = router
        .borrow_mut()
        .reduce([request], |captured, _| {
            assert_eq!(captured, profile);
            DestinationDecision::Path(std::env::temp_dir().join("chosen.bin"))
        })
        .0;
    let trace = RefCell::new(Vec::new());
    let failures = execute_effects(
        effects,
        |token, path| {
            let mut state = router
                .try_borrow_mut()
                .expect("native effect ran under router borrow");
            trace.borrow_mut().push((token, path));
            state.reduce([event(token, owner, DownloadPhase::InProgress)], |_, _| {
                DestinationDecision::Defer
            });
            Ok(())
        },
        |_, _| panic!("unexpected control"),
    );
    assert!(failures.is_empty());
    assert_eq!(trace.borrow().len(), 1);
    assert!(persist(&mut router.borrow_mut()).is_empty());
    assert!(
        model
            .borrow()
            .completed_downloads(profile)
            .unwrap()
            .is_empty()
    );
    let effect = router
        .borrow()
        .control(1, profile, DownloadControl::Pause)
        .unwrap();
    execute_effects(
        vec![effect],
        |_, _| panic!(),
        |token, command| {
            assert!(router.try_borrow_mut().is_ok());
            assert_eq!((token, command), (1, DownloadControl::Pause));
            Ok(())
        },
    );
}
#[test]
fn exhausted_opaque_owner_registry_never_reuses_a_key() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router: DownloadRouter<u64, Model> = DownloadRouter::default();
    router.last_owner = u64::MAX;
    assert!(
        router
            .register_normal(profile, model, SourceKind::Normal)
            .is_err()
    );
    assert!(router.owners.is_empty());
}

#[gpui::test]
fn blank_uses_retained_normal_writer_and_private_handle_cannot_register(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let mut browser = Browser::new();
        let default = browser.default_user_profile_id();
        let work = browser.create_user_profile("Work").unwrap();
        let normal = ProfileHandle::new(browser, cx);
        let blank = ProfileHandle::new_with_shared_history(normal.blank_browser(work), &normal, cx);
        let private = ProfileHandle::new(Browser::new_private(), cx);
        let mut service = DownloadService::default();
        let owner = service.register_source(&blank, work).unwrap();
        assert_eq!(service.router.owners[&owner].kind, SourceKind::Blank);
        assert!(service.router.owners[&owner].handle.same_workspace(&normal));
        assert!(service.register_source(&blank, default).is_err());
        assert!(service.register_source(&private, default).is_err());
        let retained = service.router.owners[&owner].handle.clone();
        let completion = DownloadCompletion {
            final_path: std::env::temp_dir().join("blank-real.bin"),
            source_url: "https://fixture.test/file".into(),
            original_url: String::new(),
            mime_type: String::new(),
            finished_at: DownloadTime::Unknown,
        };
        drop(blank); // Source local model can disappear before completion.
        retained
            .record_completed_download(work, completion)
            .unwrap();
        let (profiles, records) = normal.download_inventory().unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id().profile(), work);
        assert!(private.download_inventory().is_none());
        let projection = project_completed(
            &normal,
            |id| id == work,
            |records| {
                assert_eq!(records.len(), 1);
                crate::downloads_view::DownloadsList::Completed(vec![
                    crate::downloads_view::DownloadsGroup {
                        heading: "Observed parent heading".into(),
                        rows: records
                            .into_iter()
                            .map(|record| crate::downloads_view::CompletedDownload {
                                token: record.id(),
                                profile: record.id().profile(),
                                title: String::new(),
                                subtitle: "Parent-formatted type".into(),
                            })
                            .collect(),
                    },
                ])
            },
        )
        .unwrap();
        assert_eq!(projection.profiles.iter().filter(|p| p.selected).count(), 1);
        match &projection.list {
            crate::downloads_view::DownloadsList::Completed(groups) => {
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].rows.len(), 1);
                assert_eq!(groups[0].rows[0].title, "blank-real.bin");
                assert_eq!(groups[0].rows[0].token, records[0].id());
                assert_eq!(groups[0].rows[0].profile, work);
            }
            _ => panic!("expected persisted completed row"),
        }
        assert!(
            project_completed(
                &normal,
                |_| false,
                |records| {
                    crate::downloads_view::DownloadsList::Completed(vec![
                        crate::downloads_view::DownloadsGroup {
                            heading: String::new(),
                            rows: vec![crate::downloads_view::CompletedDownload {
                                token: records[0].id(),
                                profile: default,
                                title: String::new(),
                                subtitle: String::new(),
                            }],
                        },
                    ])
                }
            )
            .is_none()
        );

        assert!(
            projection
                .profiles
                .iter()
                .any(|p| p.id == work && p.name == "Work")
        );
    });
}

#[test]
fn deferred_destination_and_failed_effect_retry_keep_original_owner_without_new_callback() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    let mut request = event(5, owner, DownloadPhase::Pending);
    request.destination_requested = true;
    assert!(
        router
            .reduce([request], |_, _| DestinationDecision::Defer)
            .0
            .is_empty()
    );
    let effects = router
        .reduce([], |captured, _| {
            assert_eq!(captured, profile);
            DestinationDecision::Path(std::env::temp_dir().join("actual.bin"))
        })
        .0;
    assert_eq!(effects.len(), 1);
    assert_eq!(
        execute_effects(effects, |_, _| Err("target failed".into()), |_, _| panic!()).len(),
        1
    );
    router.destination_failed(5);
    assert!(
        router
            .reduce([], |_, _| panic!("failure must stay blocked"))
            .0
            .is_empty()
    );
    router.retry_destination(5, profile).unwrap();
    let retry = router
        .reduce([], |captured, _| {
            assert_eq!(captured, profile);
            DestinationDecision::Reject
        })
        .0;
    assert_eq!(retry.len(), 1);
    assert!(matches!(
        retry[0],
        DownloadEffect::Control(5, DownloadControl::Cancel)
    ));
    assert!(
        router
            .reduce([], |_, _| panic!("already dispatched"))
            .0
            .is_empty()
    );
    assert!(persist(&mut router).is_empty());
    assert!(
        model
            .borrow()
            .completed_downloads(profile)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn deleted_captured_profile_does_not_fall_back_to_default_on_completion() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let default = model.borrow().default_user_profile_id();
    let removed = model.borrow_mut().create_user_profile("Removed").unwrap();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, removed);
    model.borrow_mut().delete_user_profile(removed).unwrap();
    router.reduce([event(6, owner, DownloadPhase::Complete)], |_, _| {
        DestinationDecision::Defer
    });
    assert_eq!(persist(&mut router), vec!["UnknownProfile"]);
    assert!(router.has_pending_records());
    assert!(
        model
            .borrow()
            .completed_downloads(default)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn filename_search_matches_native_ascii_fixtures_literally() {
    let names = [
        "echo-source-tab-reference-20261023.bin",
        "echo-source-tab-reference-20261023 (1).bin",
        "echo-source-window-reference-20261023.bin",
    ];
    for (query, count) in [
        ("echo-source", 3),
        ("SOURCE-TAB", 2),
        ("(1)", 1),
        ("127.0.0.1", 0),
        ("65520", 0),
        ("Archive", 0),
        (" SOURCE-TAB ", 0),
        ("source-tab 20261023", 0),
        ("", 3),
        (" ", 1),
    ] {
        assert_eq!(
            names
                .iter()
                .filter(|name| filename_matches_ascii(name, query))
                .count(),
            count,
            "query={query:?}"
        );
    }
    // This policy deliberately leaves Unicode case and normalization unchanged.
    assert!(filename_matches_ascii("Résumé.bin", "Résumé"));
    assert!(!filename_matches_ascii("Résumé.bin", "RÉSUMÉ"));
}

#[gpui::test]
fn persisted_filename_search_projects_real_rows_or_distinct_owned_search_empty(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let browser = Browser::new();
        let profile = browser.default_user_profile_id();
        let normal = ProfileHandle::new(browser, cx);
        for name in [
            "echo-source-tab-reference-20261023.bin",
            "echo-source-tab-reference-20261023 (1).bin",
            "echo-source-window-reference-20261023.bin",
        ] {
            normal
                .record_completed_download(
                    profile,
                    DownloadCompletion {
                        final_path: std::env::temp_dir().join(name),
                        source_url: "http://127.0.0.1:65520/file".into(),
                        original_url: "http://127.0.0.1:65520/redirect".into(),
                        mime_type: "Archive".into(),
                        finished_at: DownloadTime::Unknown,
                    },
                )
                .unwrap();
        }
        let build = |records: Vec<CompletedDownload>| {
            crate::downloads_view::DownloadsList::Completed(vec![
                crate::downloads_view::DownloadsGroup {
                    heading: "Parent-provided heading".into(),
                    rows: records
                        .into_iter()
                        .map(|record| crate::downloads_view::CompletedDownload {
                            token: record.id(),
                            profile: record.id().profile(),
                            title: String::new(),
                            subtitle: "Archive".into(),
                        })
                        .collect(),
                },
            ])
        };
        for (query, count) in [("SOURCE-TAB", 2), ("(1)", 1), ("", 3)] {
            let result =
                project_completed_search(&normal, query, |_| false, |_| true, build).unwrap();
            match result.list {
                crate::downloads_view::DownloadsList::Completed(groups) => {
                    assert_eq!(groups[0].rows.len(), count);
                    assert!(
                        groups[0]
                            .rows
                            .iter()
                            .all(|row| filename_matches_ascii(&row.title, query))
                    );
                }
                _ => panic!("expected actual filename matches"),
            }
        }
        for query in [
            "127.0.0.1",
            "65520",
            "Archive",
            " SOURCE-TAB ",
            "source-tab 20261023",
            "echo-download-query-no-match-20261024",
        ] {
            let result = project_completed_search(
                &normal,
                query,
                |_| false,
                |_| true,
                |_| panic!("search miss must not use ordinary/filter-empty builder"),
            )
            .unwrap();
            match result.list {
                crate::downloads_view::DownloadsList::SearchEmpty { query: owned } => {
                    assert_eq!(owned, query)
                }
                _ => panic!("search-empty differs from profile-filtered empty"),
            }
            assert_eq!(result.profiles.len(), 1);
        }
        // Empty search leaves the parent's empty-database/profile-filter decision intact.
        let result = project_completed_search(
            &normal,
            "",
            |_| false,
            |_| false,
            |records| {
                assert!(records.is_empty());
                crate::downloads_view::DownloadsList::FilteredEmpty
            },
        )
        .unwrap();
        assert!(matches!(
            result.list,
            crate::downloads_view::DownloadsList::FilteredEmpty
        ));
        assert_eq!(normal.download_inventory().unwrap().1.len(), 3);
    });
}

#[test]
fn destination_failure_requires_one_explicit_authorization_for_each_attempt() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let other = model.borrow_mut().create_user_profile("Other").unwrap();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    let mut request = event(88, owner, DownloadPhase::Pending);
    request.destination_requested = true;
    let mut effects = router
        .reduce([request.clone()], |_, _| {
            DestinationDecision::Path(std::env::temp_dir().join("one.bin"))
        })
        .0;
    let attempts = Cell::new(0);
    let mut failures = Vec::new();
    for attempt in 0..3 {
        assert_eq!(effects.len(), 1);
        let errors = execute_effects(
            std::mem::take(&mut effects),
            |_, _| {
                attempts.set(attempts.get() + 1);
                Err("destination unavailable".into())
            },
            |_, _| panic!(),
        );
        for (token, error) in errors {
            router.destination_failed(token);
            failures.push(error);
        }
        for _ in 0..100 {
            assert!(
                router
                    .reduce([], |_, _| panic!("unauthorized timer retry"))
                    .0
                    .is_empty()
            );
            assert!(
                router
                    .reduce([request.clone()], |_, _| panic!(
                        "callback must not reset failure"
                    ))
                    .0
                    .is_empty()
            );
        }
        assert_eq!(attempts.get(), attempt + 1);
        assert_eq!(failures.len(), attempt + 1);
        assert!(router.retry_destination(88, other).is_err());
        if attempt < 2 {
            router.retry_destination(88, profile).unwrap();
            assert!(router.retry_destination(88, profile).is_err());
            effects = router
                .reduce([], |_, _| {
                    DestinationDecision::Path(std::env::temp_dir().join("one.bin"))
                })
                .0;
        }
    }
    assert!(
        model
            .borrow()
            .completed_downloads(profile)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn shutdown_handoff_keeps_preseal_batch_and_late_complete_with_one_consumer_and_writer() {
    use super::shutdown::{ConsumerGate, ConsumerOwner, ingest_shutdown};
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let named = model.borrow_mut().create_user_profile("Named").unwrap();
    let mut router = DownloadRouter::default();
    let normal = register(&mut router, &model, profile);
    let blank = router
        .register_normal(named, model.clone(), SourceKind::Blank)
        .unwrap();
    let gate = ConsumerGate::default();
    let batch = gate.acquire(ConsumerOwner::Running).unwrap();
    router.reduce([event(90, normal, DownloadPhase::Complete)], |_, _| {
        DestinationDecision::Defer
    });
    // Native quit may seal/hand off inside an engine call while its batch is owned.
    gate.handoff().unwrap();
    assert!(!gate.is_running());
    assert!(gate.handoff().is_err());
    assert!(gate.acquire(ConsumerOwner::Running).is_err());
    assert!(gate.acquire(ConsumerOwner::Shutdown).is_err());
    assert!(persist(&mut router).is_empty()); // Already-copied batch must not be lost.
    drop(batch);
    let lease = gate.acquire(ConsumerOwner::Shutdown).unwrap();
    let mut late = event(91, blank, DownloadPhase::Complete);
    late.provenance = DownloadProvenance::new(blank, 83, 98, false);
    let errors = ingest_shutdown(
        &mut router,
        [
            late.clone(),
            event(90, normal, DownloadPhase::Complete),
            late,
        ],
        |model, profile, item| {
            model
                .borrow_mut()
                .record_completed_download(profile, item)
                .map_err(|e| format!("{e:?}"))
        },
    );
    assert!(errors.is_empty());
    assert!(gate.acquire(ConsumerOwner::Shutdown).is_err()); // No second consumer.
    assert!(ingest_shutdown(&mut router, [], |_, _, _| panic!("already committed")).is_empty());
    drop(lease);
    let handles = router.unique_handles_by(Rc::ptr_eq);
    assert_eq!(handles.len(), 1); // Normal + Blank + named Profile share one session.
    drop(model); // Only app registry/writer ownership remains; no source is needed.
    let _lease = gate.acquire(ConsumerOwner::Shutdown).unwrap();
    let durable = session::to_json(&handles[0].borrow()); // Fake final session effect.
    let restored = session::from_json(&durable).unwrap();
    assert_eq!(restored.completed_downloads(profile).unwrap().len(), 1);
    assert_eq!(restored.completed_downloads(named).unwrap().len(), 1);
}

#[gpui::test]
fn shutdown_api_retains_unique_canonical_handles_flushes_after_late_ingestion_and_finishes(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let browser = Browser::new();
        let profile = browser.default_user_profile_id();
        let normal = ProfileHandle::new(browser, cx);
        let path = std::env::temp_dir().join(format!(
            "echo-download-shutdown-test-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        normal.save_session_to(path.clone(), cx);
        let blank =
            ProfileHandle::new_with_shared_history(normal.blank_browser(profile), &normal, cx);
        let app = AppDownloads::default();
        app.service
            .borrow_mut()
            .register_source(&normal, profile)
            .unwrap();
        app.service
            .borrow_mut()
            .register_source(&blank, profile)
            .unwrap();
        let item = || DownloadCompletion {
            final_path: std::env::temp_dir().join("shutdown-real.bin"),
            source_url: "https://fixture.test/file".into(),
            original_url: String::new(),
            mime_type: String::new(),
            finished_at: DownloadTime::Unknown,
        };
        normal.record_completed_download(profile, item()).unwrap(); // Complete before seal.
        let shutdown = app.begin_shutdown().unwrap();
        assert!(app.begin_shutdown().is_err());
        drop(blank);
        drop(normal);
        let writers = shutdown.retained_normal_handles().unwrap();
        assert_eq!(writers.len(), 1);
        assert!(shutdown.finish().is_err());
        shutdown
            .drain_and_persist_with(|| {
                assert!(
                    app.service.try_borrow_mut().is_ok(),
                    "drain must run outside service borrow"
                );
                assert!(
                    shutdown.retained_normal_handles().is_err(),
                    "batch must exclude final flush"
                );
                Ok(Vec::new())
            })
            .unwrap();
        shutdown.flush_retained(cx).unwrap();
        assert_eq!(
            session::load(&path)
                .unwrap()
                .unwrap()
                .completed_downloads(profile)
                .unwrap()
                .len(),
            1
        );
        // A late batch reopens the flush barrier, including after an earlier flush.
        writers[0]
            .record_completed_download(profile, item())
            .unwrap();
        shutdown.ingest_and_persist(Vec::new()).unwrap();
        assert!(shutdown.finish().is_err());
        shutdown.flush_retained(cx).unwrap();
        assert_eq!(
            session::load(&path)
                .unwrap()
                .unwrap()
                .completed_downloads(profile)
                .unwrap()
                .len(),
            2
        );
        shutdown.finish().unwrap();
        assert!(shutdown.ingest_and_persist(Vec::new()).is_err());
        assert!(
            shutdown
                .drain_and_persist_with(|| panic!("finished consumer cannot drain"))
                .is_err()
        );
        std::fs::remove_file(&path).unwrap();
    });
}

#[test]
fn supplied_copied_completions_use_shutdown_service_api_and_final_flush_without_engine_access() {
    use super::shutdown::{ConsumerGate, ConsumerOwner};
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let service = Rc::new(RefCell::new(DownloadService::<u64, Model>::default()));
    let owner = register(&mut service.borrow_mut().router, &model, profile);
    service
        .borrow_mut()
        .router
        .register_normal(profile, model.clone(), SourceKind::Blank)
        .unwrap();
    let gate = ConsumerGate::default();
    let older_batch = gate.acquire(ConsumerOwner::Running).unwrap();
    service
        .borrow_mut()
        .router
        .reduce([event(100, owner, DownloadPhase::Complete)], |_, _| {
            DestinationDecision::Defer
        });
    let shutdown = ShutdownDownloads::take(service.clone(), gate.clone()).unwrap();
    let writer = |model: &Model, profile, item| {
        model
            .borrow_mut()
            .record_completed_download(profile, item)
            .map_err(|e| format!("{e:?}"))
    };
    assert!(matches!(
        shutdown.drain_copied_and_persist_with(|| panic!("older batch still owns drain"), writer),
        Err(ShutdownDownloadError::InFlight)
    ));
    assert!(persist(&mut service.borrow_mut().router).is_empty());
    drop(older_batch);
    drop(model); // Only the app-owned registry retains the writer.
    let late = event(101, owner, DownloadPhase::Complete);
    shutdown
        .ingest_copied_and_persist_with([late.clone(), late], writer)
        .unwrap();
    assert_eq!(shutdown.retained_handles_by(Rc::ptr_eq).unwrap().len(), 1);
    let durable = RefCell::new(Vec::new());
    shutdown
        .flush_retained_with(Rc::ptr_eq, |handle| {
            assert!(
                service.try_borrow_mut().is_ok(),
                "flush under service borrow"
            );
            durable
                .borrow_mut()
                .push(session::to_json(&handle.borrow()));
            Ok(())
        })
        .unwrap();
    assert_eq!(durable.borrow().len(), 1);
    assert_eq!(
        session::from_json(&durable.borrow()[0])
            .unwrap()
            .completed_downloads(profile)
            .unwrap()
            .len(),
        2
    );
    let drains = Cell::new(0);
    shutdown
        .drain_copied_and_persist_with(
            || {
                drains.set(drains.get() + 1);
                assert!(
                    service.try_borrow_mut().is_ok(),
                    "source drain under model/service borrow"
                );
                assert!(matches!(
                    gate.acquire(ConsumerOwner::Running),
                    Err(ShutdownDownloadError::Inactive)
                ));
                assert!(matches!(
                    shutdown.ingest_copied_and_persist_with([], writer),
                    Err(ShutdownDownloadError::InFlight)
                ));
                Ok(vec![event(102, owner, DownloadPhase::Complete)]) // Supplied copied event, no unavailable engine API.
            },
            writer,
        )
        .unwrap();
    assert_eq!(drains.get(), 1);
    assert!(shutdown.finish().is_err()); // Completion after flush must be flushed again.
    shutdown
        .flush_retained_with(Rc::ptr_eq, |handle| {
            durable
                .borrow_mut()
                .push(session::to_json(&handle.borrow()));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        session::from_json(&durable.borrow()[1])
            .unwrap()
            .completed_downloads(profile)
            .unwrap()
            .len(),
        3
    );
    assert!(matches!(
        shutdown.flush_retained_with(Rc::ptr_eq, |_| Err("session write failed".into())),
        Err(ShutdownDownloadError::Failure(_))
    ));
    assert!(shutdown.finish().is_err());
    shutdown
        .flush_retained_with(Rc::ptr_eq, |handle| {
            durable
                .borrow_mut()
                .push(session::to_json(&handle.borrow()));
            Ok(())
        })
        .unwrap();
    shutdown.finish().unwrap();
    assert!(matches!(
        shutdown.drain_copied_and_persist_with(|| panic!("finished"), writer),
        Err(ShutdownDownloadError::Inactive)
    ));
}
