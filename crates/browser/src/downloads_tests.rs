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
        destination: DownloadDestination::Unrequested,
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
    before.destination = DownloadDestination::Requested {
        suggested_name: "callback.bin".into(),
    };
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
    request.destination = DownloadDestination::Requested {
        suggested_name: "callback.bin".into(),
    };
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
    request.destination = DownloadDestination::Requested {
        suggested_name: "callback.bin".into(),
    };
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
    request.destination = DownloadDestination::Requested {
        suggested_name: "callback.bin".into(),
    };
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

// Exercise the same lossless conversion seam called by From<DownloadEvent>,
// using fake keys because production DownloadToken has no public constructor.
fn converted_event(
    token: u64,
    owner: u64,
    destination_requested: bool,
    destination: DownloadDestination,
    suggested_info: &str,
    phase: DownloadPhase,
) -> TransferEvent<u64> {
    let mut snapshot_info = info(phase, "actual (2).bin");
    snapshot_info.suggested_file_name = suggested_info.into();
    TransferEvent::from_snapshot_parts(
        token,
        DownloadProvenance::new(owner, 83, 97, false),
        snapshot_info,
        destination,
        destination_requested,
    )
}
fn requested(token: u64, owner: u64, name: &str) -> TransferEvent<u64> {
    converted_event(
        token,
        owner,
        true,
        DownloadDestination::Requested {
            suggested_name: name.into(),
        },
        "",
        DownloadPhase::Pending,
    )
}

#[test]
fn production_conversion_seam_retains_destination_and_empty_or_different_info_name() {
    for info_name in ["", "different-item-name.bin"] {
        let converted = converted_event(
            51,
            72,
            true,
            DownloadDestination::Requested {
                suggested_name: "authoritative.bin".into(),
            },
            info_name,
            DownloadPhase::Pending,
        );
        assert_eq!(converted.token, 51);
        assert_eq!(
            converted.provenance,
            DownloadProvenance::new(72, 83, 97, false)
        );
        assert!(converted.destination_requested);
        assert_eq!(
            converted.destination,
            DownloadDestination::Requested {
                suggested_name: "authoritative.bin".into(),
            }
        );
        assert_eq!(converted.info.suggested_file_name, info_name);
    }
    for destination in [
        DownloadDestination::Unrequested,
        DownloadDestination::Requested {
            suggested_name: "retained.bin".into(),
        },
        DownloadDestination::Resolved(std::env::temp_dir().join("chosen.bin")),
        DownloadDestination::Rejected,
    ] {
        let converted = converted_event(
            51,
            72,
            false,
            destination.clone(),
            "",
            DownloadPhase::InProgress,
        );
        assert!(!converted.destination_requested);
        assert_eq!(converted.destination, destination);
    }
}

#[test]
fn request_and_update_order_freezes_authoritative_policy_request_with_empty_info() {
    for update_first in [false, true] {
        let model = Rc::new(RefCell::new(Browser::new()));
        let profile = model.borrow().default_user_profile_id();
        let mut router = DownloadRouter::default();
        let owner = register(&mut router, &model, profile);
        let request = requested(52, owner, "authoritative.bin");
        let frozen_info = request.info.clone();
        let mut update = event(52, owner, DownloadPhase::InProgress);
        update.info.suggested_file_name.clear(); // Native R3/R4: never filled later.
        update.info.received_bytes = 393_216;
        if update_first {
            assert!(
                router
                    .reduce([update.clone()], |_, _| panic!("no callback request yet"))
                    .0
                    .is_empty()
            );
        }
        let batch = if update_first {
            vec![request]
        } else {
            vec![request, update.clone()]
        };
        assert!(
            router
                .reduce(batch, |captured, request| {
                    assert_eq!(captured, profile);
                    assert_eq!(
                        request.provenance,
                        DownloadProvenance::new(owner, 83, 97, false)
                    );
                    assert_eq!(request.suggested_name, "authoritative.bin");
                    assert!(request.info.suggested_file_name.is_empty());
                    assert_eq!(request.info.phase, DownloadPhase::Pending);
                    DestinationDecision::Defer
                })
                .0
                .is_empty()
        );
        // A repeated callback name also cannot replace its original snapshot.
        let mut repeated = requested(52, owner, "authoritative.bin");
        repeated.info.url = "https://later.test/changed".into();
        repeated.info.suggested_file_name = "different-item-name.bin".into();
        let (effects, errors) = router.reduce([repeated], |_, request| {
            assert_eq!(request.info, frozen_info);
            assert_eq!(request.suggested_name, "authoritative.bin");
            DestinationDecision::Defer
        });
        assert!(effects.is_empty());
        assert!(errors.is_empty());
        // Even a nonempty later item name does not mutate the frozen request.
        update.info.suggested_file_name = "unrelated-later-item-name.bin".into();
        let (effects, errors) = router.reduce([update], |captured, request| {
            assert_eq!(captured, profile);
            assert_eq!(request.suggested_name, "authoritative.bin");
            assert!(request.info.suggested_file_name.is_empty());
            DestinationDecision::Path(std::env::temp_dir().join("explicit-parent-choice.bin"))
        });
        assert!(errors.is_empty());
        assert!(
            matches!(&effects[..], [DownloadEffect::Destination(52, path)] if path.ends_with("explicit-parent-choice.bin"))
        );
        assert!(
            router
                .reduce([], |_, _| panic!("already dispatched"))
                .0
                .is_empty()
        );
    }
}

#[test]
fn updated_requested_metadata_alone_cannot_authorize_policy_or_poison_later_request() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    let mut update = requested(53, owner, "update-only.bin");
    update.destination_requested = false;
    assert!(
        router
            .reduce([update], |_, _| panic!("Updated cannot create request"))
            .0
            .is_empty()
    );
    let (_, errors) = router.reduce([requested(53, owner, "callback.bin")], |_, request| {
        assert_eq!(request.suggested_name, "callback.bin");
        DestinationDecision::Defer
    });
    assert!(errors.is_empty());
}

#[test]
fn conflicting_requested_metadata_is_rejected_without_losing_genuine_terminal_updates() {
    for phase in [DownloadPhase::Complete, DownloadPhase::Canceled] {
        let model = Rc::new(RefCell::new(Browser::new()));
        let profile = model.borrow().default_user_profile_id();
        let mut router = DownloadRouter::default();
        let owner = register(&mut router, &model, profile);
        router.reduce([requested(54, owner, "first.bin")], |_, _| {
            DestinationDecision::Defer
        });
        for is_request in [true, false] {
            let mut spoof = requested(54, owner, "spoof.bin");
            spoof.destination_requested = is_request;
            let (effects, errors) = router.reduce([spoof], |_, request| {
                assert_eq!(request.suggested_name, "first.bin");
                DestinationDecision::Defer
            });
            assert!(effects.is_empty());
            assert_eq!(errors, ["Download requested destination changed"]);
            assert_eq!(router.ledger[&54].info.phase, DownloadPhase::Pending);
        }
        let mut terminal = requested(54, owner, "conflicting-terminal-metadata.bin");
        terminal.destination_requested = false;
        terminal.info.phase = phase;
        let (effects, errors) = router.reduce([terminal.clone()], |_, _| {
            panic!("terminal must not dispatch")
        });
        assert!(effects.is_empty());
        assert_eq!(errors, ["Download requested destination changed"]);
        assert_eq!(router.ledger[&54].info.phase, phase);
        assert!(persist(&mut router).is_empty());
        let expected = usize::from(phase == DownloadPhase::Complete);
        assert_eq!(
            model.borrow().completed_downloads(profile).unwrap().len(),
            expected
        );
        assert!(
            router
                .reduce(
                    [terminal, requested(54, owner, "first.bin")],
                    |_, _| panic!("terminal replay")
                )
                .0
                .is_empty()
        );
        assert!(persist(&mut router).is_empty());
        assert_eq!(
            model.borrow().completed_downloads(profile).unwrap().len(),
            expected
        );
    }
}

#[test]
fn malformed_request_variants_do_not_bind_tokens_or_invent_policy_names() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    for destination in [
        DownloadDestination::Unrequested,
        DownloadDestination::Rejected,
        DownloadDestination::Resolved(std::env::temp_dir().join("spoof.bin")),
    ] {
        let malformed = converted_event(
            55,
            owner,
            true,
            destination,
            "item.bin",
            DownloadPhase::Complete,
        );
        let (effects, errors) = router.reduce([malformed], |_, _| panic!("malformed request"));
        assert!(effects.is_empty());
        assert_eq!(
            errors,
            ["Download request lacks requested destination metadata"]
        );
        assert!(router.ledger.is_empty());
    }
    // Empty callback names stay empty; the router never invents a name/path.
    assert!(
        router
            .reduce([requested(55, owner, "")], |_, request| {
                assert!(request.suggested_name.is_empty());
                DestinationDecision::Defer
            })
            .0
            .is_empty()
    );
}

#[test]
fn policy_retry_reuses_frozen_request_and_rejects_provenance_replays() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let other = model.borrow_mut().create_user_profile("Other").unwrap();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    let other_owner = register(&mut router, &model, other);
    let request = requested(56, owner, "authoritative.bin");
    let frozen = RefCell::new(None);
    let (effects, errors) = router.reduce([request.clone()], |_, request| {
        *frozen.borrow_mut() = Some(request.clone());
        DestinationDecision::Path(std::env::temp_dir().join("explicit.bin"))
    });
    assert!(errors.is_empty());
    let failures = execute_effects(effects, |_, _| Err("failed".into()), |_, _| panic!());
    assert_eq!(failures.len(), 1);
    router.destination_failed(failures[0].0);
    for provenance in [
        DownloadProvenance::new(other_owner, 83, 97, false),
        DownloadProvenance::new(owner, 84, 97, false),
        DownloadProvenance::new(owner, 83, 98, false),
        DownloadProvenance::new(owner, 83, 97, true),
    ] {
        let mut spoof = requested(56, owner, "spoof.bin");
        spoof.provenance = provenance;
        let (effects, errors) = router.reduce([spoof], |_, _| panic!("blocked/spoofed"));
        assert!(effects.is_empty());
        assert_eq!(errors.len(), 1);
    }
    let mut update = event(56, owner, DownloadPhase::InProgress);
    update.info.suggested_file_name.clear();
    assert!(
        router
            .reduce([update, request], |_, _| panic!(
                "callback cannot authorize retry"
            ))
            .0
            .is_empty()
    );
    assert!(router.retry_destination(56, other).is_err());
    router.retry_destination(56, profile).unwrap();
    assert!(router.retry_destination(56, profile).is_err());
    let (effects, errors) = router.reduce([], |captured, request| {
        assert_eq!(captured, profile);
        assert_eq!(Some(request), frozen.borrow().as_ref());
        DestinationDecision::Reject
    });
    assert!(errors.is_empty());
    assert!(matches!(
        effects[..],
        [DownloadEffect::Control(56, DownloadControl::Cancel)]
    ));
    assert!(
        router
            .reduce([], |_, _| panic!("one retry effect only"))
            .0
            .is_empty()
    );
    let terminal = event(56, owner, DownloadPhase::Complete);
    router.reduce([terminal.clone(), terminal], |_, _| panic!("terminal"));
    assert!(persist(&mut router).is_empty());
    assert_eq!(
        model.borrow().completed_downloads(profile).unwrap().len(),
        1
    );
    assert!(
        model
            .borrow()
            .completed_downloads(other)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn differing_nonempty_item_name_does_not_override_callback_or_cross_token_requests() {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut router = DownloadRouter::default();
    let owner = register(&mut router, &model, profile);
    let mut first = requested(57, owner, "first-callback.bin");
    first.info.suggested_file_name = "different-item.bin".into();
    let second = requested(58, owner, "second-callback.bin");
    let seen = RefCell::new(Vec::new());
    let (effects, errors) = router.reduce([first, second], |captured, request| {
        assert_eq!(captured, profile);
        seen.borrow_mut().push(request.suggested_name.clone());
        match request.suggested_name.as_str() {
            "first-callback.bin" => {
                assert_eq!(request.info.suggested_file_name, "different-item.bin")
            }
            "second-callback.bin" => assert!(request.info.suggested_file_name.is_empty()),
            _ => panic!("crossed request"),
        }
        DestinationDecision::Defer
    });
    assert!(effects.is_empty());
    assert!(errors.is_empty());
    seen.borrow_mut().sort();
    assert_eq!(
        *seen.borrow(),
        ["first-callback.bin", "second-callback.bin"]
    );
    assert_eq!(
        router.ledger[&57]
            .destination_request
            .as_ref()
            .unwrap()
            .suggested_name,
        "first-callback.bin"
    );
    assert_eq!(
        router.ledger[&58]
            .destination_request
            .as_ref()
            .unwrap()
            .suggested_name,
        "second-callback.bin"
    );
}

#[test]
fn quit_barrier_uses_real_service_lease_and_persists_complete_at_final_poll_once_per_workspace() {
    use super::shutdown::{ConsumerGate, ConsumerOwner, ingest_shutdown};
    use crate::quit::{Coordinator, ShutdownDriver, FinalDownloadBarrier, download_ready,
        begin_after_downloads, advance_shutdown};
    struct Driver<'a> { complete: &'a Cell<bool>, begins: usize, polls: usize }
    impl ShutdownDriver for Driver<'_> {
        fn begin(&mut self) -> Result<engine::ShutdownStatus, String> {
            self.begins += 1;
            Ok(engine::ShutdownStatus { phase: engine::ShutdownPhase::WaitingForOwners,
                pending_owners: 0, pending_browsers: vec![], pending_cookie_flushes: 0, failures: vec![] })
        }
        fn poll(&mut self) -> Result<engine::ShutdownStatus, String> {
            self.polls += 1;
            self.complete.set(true); // Terminal copy arrives during final native poll.
            Ok(engine::ShutdownStatus { phase: engine::ShutdownPhase::Complete,
                pending_owners: 0, pending_browsers: vec![], pending_cookie_flushes: 0, failures: vec![] })
        }
    }
    let state = Rc::new(RefCell::new(Coordinator::default()));
    state.borrow_mut().request();
    assert!(state.borrow_mut().start());
    let engine_complete = Cell::new(false);
    let mut driver = Driver { complete: &engine_complete, begins: 0, polls: 0 };
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let named = model.borrow_mut().create_user_profile("Named").unwrap();
    let other = Rc::new(RefCell::new(Browser::new()));
    let other_profile = other.borrow().default_user_profile_id();
    let service = Rc::new(RefCell::new(DownloadService::<u64, Model>::default()));
    let owner = register(&mut service.borrow_mut().router, &model, profile);
    let named_owner = register(&mut service.borrow_mut().router, &model, named);
    service.borrow_mut().router.register_normal(profile, model.clone(), SourceKind::Blank).unwrap();
    let other_owner = register(&mut service.borrow_mut().router, &other, other_profile);
    let writer = |model: &Model, profile, completion| model.borrow_mut()
        .record_completed_download(profile, completion).map_err(|error| format!("{error:?}"));
    let gate = ConsumerGate::default();
    let old_batch = gate.acquire(ConsumerOwner::Running).unwrap();
    let shutdown = ShutdownDownloads::take(service.clone(), gate.clone()).unwrap();
    // Same shutdown consumer used by quit's pre-engine running drain; no drain
    // and no flush may overtake a batch already copied by the normal pump.
    assert_eq!(download_ready(shutdown.drain_copied_and_persist_with(
        || panic!("running batch owns copied queue"), writer)), Ok(false));
    assert_eq!(shutdown.flush_retained_with(Rc::ptr_eq, |_| panic!("early flush")), Err(ShutdownDownloadError::InFlight));
    let errors = ingest_shutdown(&mut service.borrow_mut().router,
        [event(100, owner, DownloadPhase::Complete)], writer);
    assert!(errors.is_empty());
    drop(old_batch);
    assert!(gate.acquire(ConsumerOwner::Running).is_err());
    assert!(download_ready(shutdown.drain_copied_and_persist_with(|| Ok(Vec::new()), writer)).unwrap());
    assert!(begin_after_downloads(&state, &mut driver,
        || shutdown.drain_copied_and_persist_with(|| Ok(Vec::new()), writer),
        || Ok(())));
    drop(model);
    drop(other); // Every source is gone; only canonical service writers remain.

    let mut barrier = FinalDownloadBarrier::default();
    let durable = RefCell::new(Vec::new());
    let order = RefCell::new(Vec::new());
    let exits = Cell::new(0);
    advance_shutdown(&state, &mut driver, &mut barrier,
        || shutdown.drain_copied_and_persist_with(|| {
            assert!(service.try_borrow_mut().is_ok());
            if engine_complete.get() {
                order.borrow_mut().push("post-Complete drain");
                Ok(vec![event(102, other_owner, DownloadPhase::Complete),
                    event(102, other_owner, DownloadPhase::Complete), event(100, owner, DownloadPhase::Complete)])
            } else {
                Ok(vec![event(101, named_owner, DownloadPhase::Complete)])
            }
        }, writer),
        || shutdown.flush_retained_with(Rc::ptr_eq, |handle| {
            assert!(service.try_borrow_mut().is_ok(), "flush under service borrow");
            order.borrow_mut().push("canonical flush");
            durable.borrow_mut().push(session::to_json(&handle.borrow()));
            Ok(())
        }),
        || { order.borrow_mut().push("consumer finish"); shutdown.finish() },
        || {
            assert_eq!(durable.borrow().len(), 2);
            exits.set(exits.get() + 1);
            order.borrow_mut().push("original exit allowed");
            Ok(())
        });
    assert_eq!((driver.begins, driver.polls, exits.get()), (1, 1, 1));
    advance_shutdown(&state, &mut driver, &mut barrier,
        || panic!("drain after exit"), || panic!("flush after exit"),
        || panic!("finish after exit"), || panic!("duplicate exit"));
    assert_eq!(*order.borrow(), ["post-Complete drain", "canonical flush", "canonical flush", "consumer finish", "original exit allowed"]);
    assert_eq!(durable.borrow().len(), 2); // Normal/Blank/named share ONE writer.
    let records: usize = durable.borrow().iter().map(|json| {
        let restored = session::from_json(json).unwrap();
        restored.user_profiles().map(|p| restored.completed_downloads(p.id()).unwrap().len()).sum::<usize>()
    }).sum();
    assert_eq!(records, 3); // Late final completion persisted, replays suppressed.
    assert!(barrier.advance(|| panic!("late drain"), || panic!("late flush"), || panic!("late finish")).unwrap());
    assert!(shutdown.drain_copied_and_persist_with(|| panic!("closed consumer"), writer).is_err());
}


#[test]
fn relinquishing_batch_failure_blocks_production_quit_orchestration() {
    use super::shutdown::{ConsumerGate, ConsumerOwner, ingest_shutdown};
    use crate::quit::{Coordinator, ShutdownDriver, FinalDownloadBarrier,
        begin_after_downloads, advance_shutdown};
    struct Driver { begins: usize, polls: usize }
    impl ShutdownDriver for Driver {
        fn begin(&mut self) -> Result<engine::ShutdownStatus, String> {
            self.begins += 1;
            panic!("engine begin after failed old batch")
        }
        fn poll(&mut self) -> Result<engine::ShutdownStatus, String> {
            self.polls += 1;
            panic!("engine poll after failed old batch")
        }
    }
    // Rejected provenance leaves no pending record; a transient writer error
    // can later succeed. Both must still block this quit attempt permanently.
    for rejected in [true, false] {
        let model = Rc::new(RefCell::new(Browser::new()));
        let profile = model.borrow().default_user_profile_id();
        let service = Rc::new(RefCell::new(DownloadService::<u64, Model>::default()));
        let owner = register(&mut service.borrow_mut().router, &model, profile);
        service.borrow_mut().errors.push("historical, unrelated".into());
        let gate = ConsumerGate::default();
        let old = gate.acquire(ConsumerOwner::Running).unwrap();
        let state = Rc::new(RefCell::new(Coordinator::default()));
        let mut driver = Driver { begins: 0, polls: 0 };
        // Inject quit reentry inside the old copied-event source, with no
        // service/coordinator borrow across the callback (as in production).
        let mut shutdown = None;
        let copied = (|| {
            state.borrow_mut().request();
            assert!(state.borrow_mut().start());
            shutdown = Some(ShutdownDownloads::take(service.clone(), gate.clone()).unwrap());
            vec![event(501, if rejected { owner + 99 } else { owner }, DownloadPhase::Complete)]
        })();
        let shutdown = shutdown.unwrap();
        let writer = |model: &Model, profile, completion| model.borrow_mut()
            .record_completed_download(profile, completion).map_err(|e| format!("{e:?}"));
        assert!(!begin_after_downloads(&state, &mut driver,
            || shutdown.drain_copied_and_persist_with(|| panic!("old lease busy"), writer),
            || -> Result<(), String> { panic!("premature detach") }));
        // Same per-batch diagnostic capture used by the normal production pump.
        old.with_service_errors(&mut service.borrow_mut(), |service| {
            let errors = ingest_shutdown(&mut service.router, copied,
                |_, _, _| Err("transient old writer failure".into()));
            assert!(!errors.is_empty());
            service.errors.extend(errors);
        });
        drop(old); // Atomically publishes genuine failures before lease release.
        if !rejected {
            assert!(persist(&mut service.borrow_mut().router).is_empty());
        }
        assert!(!service.borrow().router.has_pending_records());
        let exits = Cell::new(0);
        let mut barrier = FinalDownloadBarrier::default();
        for _ in 0..3 {
            assert!(!begin_after_downloads(&state, &mut driver,
                || shutdown.drain_copied_and_persist_with(|| panic!("sticky lease error before drain"), writer),
                || -> Result<(), String> { panic!("detach after failure") }));
            advance_shutdown(&state, &mut driver, &mut barrier,
                || shutdown.drain_copied_and_persist_with(|| panic!("no shutdown drain"), writer),
                || panic!("no final flush"), || panic!("no finish"),
                || { exits.set(exits.get() + 1); Ok(()) });
        }
        assert_eq!((driver.begins, driver.polls, exits.get()), (0, 0, 0));
        assert!(matches!(shutdown.finish(), Err(ShutdownDownloadError::Failure(_))));
    }
}

#[test]
fn historical_errors_and_intentionally_suppressed_effects_do_not_poison_handoff() {
    use super::shutdown::{ConsumerGate, ConsumerOwner};
    let service = Rc::new(RefCell::new(DownloadService::<u64, Model>::default()));
    service.borrow_mut().errors.push("historical error".into());
    let gate = ConsumerGate::default();
    let old = gate.acquire(ConsumerOwner::Running).unwrap();
    let shutdown = ShutdownDownloads::take(service.clone(), gate).unwrap();
    old.with_service_errors(&mut service.borrow_mut(), |_| {
        // Same intentional-success result as the sealed production dispatch.
        let failures = execute_effects(vec![DownloadEffect::Control(9, DownloadControl::Cancel)],
            |_, _| Ok(()), |_, _| Ok(()));
        assert!(failures.is_empty());
    });
    drop(old);
    shutdown.ingest_copied_and_persist_with([], |_, _, _| panic!("no records")).unwrap();
    shutdown.flush_retained_with(Rc::ptr_eq, |_| panic!("no handles")).unwrap();
    shutdown.finish().unwrap();
}

// Controlled service fixture tests use fictional u64 keys, never native tokens.
fn acceptance_service() -> (DownloadService<u64, Model>, Rc<RefCell<crate::download_acceptance::FixtureAudit<u64>>>, Model) {
    let model = Rc::new(RefCell::new(Browser::new()));
    let profile = model.borrow().default_user_profile_id();
    let mut service = DownloadService::default();
    let owner = service.router.register_normal(profile, model.clone(), SourceKind::Normal).unwrap();
    let arm = crate::download_acceptance::PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap();
    let audit = Rc::new(RefCell::new(crate::download_acceptance::FixtureAudit::new(arm, profile, owner)));
    service.attach_fixture(audit.clone()).unwrap();
    (service, audit, model)
}
fn acceptance_event(audit: &crate::download_acceptance::FixtureAudit<u64>, requested: bool, phase: DownloadPhase) -> TransferEvent<u64> {
    let mut e = event(100, audit.owner, phase);
    e.info.url = audit.config.fixture_url();
    e.info.original_url = audit.config.fixture_url();
    e.info.full_path = audit.config.output.clone();
    e.info.suggested_file_name = "different-item-name.bin".into();
    e.destination_requested = requested;
    e.destination = if requested { DownloadDestination::Requested { suggested_name: audit.config.filename.clone() } } else { DownloadDestination::Resolved(audit.config.output.clone()) };
    e
}
#[test]
fn acceptance_real_router_ack_shutdown_ingress_and_replay_use_one_canonical_writer() {
    use super::shutdown::ConsumerGate;
    let (mut service, audit, model) = acceptance_service();
    let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
    assert!(matches!(service.receive_copied([request]).as_slice(), [DownloadEffect::Destination(100, _)]));
    let terminal = acceptance_event(&audit.borrow(), false, DownloadPhase::Complete);
    let profile = audit.borrow().profile;
    let service = Rc::new(RefCell::new(service));
    let shutdown = ShutdownDownloads::take(service.clone(), ConsumerGate::default()).unwrap();
    for _ in 0..2 {
        shutdown.ingest_copied_and_persist_with([terminal.clone()], |model, id, completion| {
            model.borrow_mut().record_completed_download(id, completion).map_err(|e| format!("{e:?}"))
        }).unwrap();
    }
    let id = service.borrow().router.ledger[&100].committed.unwrap();
    assert_eq!(id.profile(), profile);
    assert!(audit.borrow().terminal_updated_seen);
    assert_eq!(audit.borrow().phase, Some(DownloadPhase::Complete));
    assert_eq!(model.borrow().completed_downloads(profile).unwrap().len(), 1);
    shutdown.flush_retained_with(Rc::ptr_eq, |_| Ok(())).unwrap();
    shutdown.finish().unwrap();
    assert!(shutdown.ingest_copied_and_persist_with([terminal], |_, _, _| panic!("after finish")).is_err());
}
#[test]
fn acceptance_late_conflicting_terminal_persists_before_returning_audit_failure() {
    use super::shutdown::ConsumerGate;
    let (mut service, audit, model) = acceptance_service();
    let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
    service.receive_copied([request]);
    let mut terminal = acceptance_event(&audit.borrow(), false, DownloadPhase::Complete);
    terminal.destination = DownloadDestination::Requested { suggested_name: "conflict.bin".into() };
    let service = Rc::new(RefCell::new(service));
    let shutdown = ShutdownDownloads::take(service.clone(), ConsumerGate::default()).unwrap();
    assert!(shutdown.ingest_copied_and_persist_with([terminal], |m, id, c| m.borrow_mut().record_completed_download(id, c).map_err(|e| format!("{e:?}"))).is_err());
    assert!(service.borrow().router.ledger[&100].committed.is_some());
    assert_eq!(model.borrow().completed_downloads(audit.borrow().profile).unwrap().len(), 1);
    assert!(audit.borrow().failure.is_some());
}
#[test]
fn acceptance_quarantine_in_relinquishing_batch_is_sticky_without_pending_record() {
    use super::shutdown::{ConsumerGate, ConsumerOwner};
    let (service, audit, _) = acceptance_service();
    let service = Rc::new(RefCell::new(service));
    let gate = ConsumerGate::default();
    let lease = gate.acquire(ConsumerOwner::Running).unwrap();
    let shutdown = ShutdownDownloads::take(service.clone(), gate).unwrap();
    let unbound_terminal = acceptance_event(&audit.borrow(), false, DownloadPhase::Complete);
    lease.with_service_errors(&mut service.borrow_mut(), |s| { assert!(s.receive_copied([unbound_terminal]).is_empty()); });
    assert!(service.borrow().router.ledger.is_empty());
    drop(lease);
    assert!(matches!(shutdown.flush_retained_with(Rc::ptr_eq, |_| Ok(())), Err(ShutdownDownloadError::Failure(_))));
}
#[test]
fn acceptance_only_genuine_requested_name_authorizes_and_second_token_cannot_share_target() {
    for bad_kind in 0..4 {
        let (mut s, audit, _) = acceptance_service();
        let mut e = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
        match bad_kind {
            0 => e.destination_requested = false,
            1 => e.destination = DownloadDestination::Requested { suggested_name: "wrong.bin".into() },
            2 => e.provenance = DownloadProvenance::new(audit.borrow().owner, 83, 97, true),
            _ => e.info.phase = DownloadPhase::Complete,
        }
        assert!(s.receive_copied([e]).is_empty());
        assert!(s.router.ledger.is_empty());
        assert!(audit.borrow().token.is_none());
    }
    let (mut s, audit, _) = acceptance_service();
    let mut early = acceptance_event(&audit.borrow(), false, DownloadPhase::InProgress);
    early.destination = DownloadDestination::Unrequested;
    assert!(s.receive_copied([early]).is_empty());
    assert_eq!(audit.borrow().token, Some(100));
    let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
    assert!(matches!(s.receive_copied([request.clone()]).as_slice(), [DownloadEffect::Destination(100, _)]));
    let mut other = request;
    other.token = 101;
    assert!(s.receive_copied([other]).is_empty());
    assert_eq!(s.router.ledger.len(), 1);
    assert!(audit.borrow().failure.is_some());
}
#[test]
fn acceptance_writer_failure_never_acknowledges_and_attachment_cannot_replace_policy() {
    use super::shutdown::ConsumerGate;
    let (mut s, audit, _) = acceptance_service();
    assert!(s.attach_fixture(audit.clone()).is_err());
    let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
    s.receive_copied([request]);
    let terminal = acceptance_event(&audit.borrow(), false, DownloadPhase::Complete);
    let s = Rc::new(RefCell::new(s));
    let shutdown = ShutdownDownloads::take(s.clone(), ConsumerGate::default()).unwrap();
    assert!(shutdown.ingest_copied_and_persist_with([terminal], |_, _, _| Err("writer unavailable".into())).is_err());
    assert!(s.borrow().router.ledger[&100].committed.is_none());
    assert!(s.borrow().router.has_pending_records());
}
#[test]
fn acceptance_detached_output_check_rejects_collision_marker_change_and_symlink() {
    use crate::download_acceptance::FixtureOutput;
    let output = FixtureOutput::reserve_in_tmp().unwrap();
    let target = output.directory().join("fixture.bin");
    assert!(output.validate_target_absent(&target).is_ok());
    std::fs::write(&target, b"existing").unwrap();
    assert!(output.validate_target_absent(&target).is_err());
    // Disposable fixture paths only; retain them for inspection (no cleanup).
    let output = FixtureOutput::reserve_in_tmp().unwrap();
    std::fs::write(output.directory().join(".echo-owner"), b"changed").unwrap();
    assert!(output.validate_target_absent(&output.directory().join("fixture.bin")).is_err());
    #[cfg(unix)] {
        let output = FixtureOutput::reserve_in_tmp().unwrap();
        let target = output.directory().join("fixture.bin");
        std::os::unix::fs::symlink(output.directory().join("missing"), &target).unwrap();
        assert!(output.validate_target_absent(&target).is_err());
    }
}
#[test]
fn acceptance_arm_grammar_has_no_external_output_or_nonloopback_mode() {
    use crate::download_acceptance::PreparedArm;
    for arg in [
        "http://127.0.0.1:18473 /tmp continue-normal",
        "http://127.0.0.1:18473 baseline",
        "http://localhost:18473 continue-normal",
        "http://127.0.0.1:0 continue-normal",
        "http://127.0.0.1:18473/path continue-normal",
        "http://user@127.0.0.1:18473 continue-normal",
        "http://[::1]:18473?x continue-normal",
    ] { assert!(PreparedArm::parse(arg).is_err()); }
}
#[test]
fn acceptance_quarantine_payloads_are_bounded_and_never_enter_router() {
    let (mut service, audit, _) = acceptance_service();
    let rejected = acceptance_event(&audit.borrow(), false, DownloadPhase::Complete);
    for _ in 0..100 {
        assert!(service.receive_copied([rejected.clone()]).is_empty());
    }
    assert!(service.router.ledger.is_empty());
    assert_eq!(audit.borrow().sequence, 100);
    assert!(service.errors.iter().any(|e| e.contains("retention stops")));
    assert!(audit.borrow().token.is_none());
}


fn quiet_acceptance_turn() -> RunningTurn {
    RunningTurn { drained_empty: true, effects_empty: true, ..Default::default() }
}
#[test]
fn acceptance_running_driver_survives_pending_and_delayed_first_callback_then_terminal() {
    use super::shutdown::{ConsumerGate, ConsumerOwner};
    let (mut service, audit, model) = acceptance_service();
    let gate = ConsumerGate::default();
    // The SAME production continuation seam is driven with no engine-work hint,
    // no effects, no records and an empty drain, before any callback exists.
    // Each continued turn corresponds to the existing 8ms awaited task timer.
    for _ in 0..12 {
        let lease = gate.acquire(ConsumerOwner::Running).unwrap();
        assert!(service.receive_copied([]).is_empty());
        drop(lease);
        assert!(continue_running(&gate, false, &service, quiet_acceptance_turn()));
    }
    assert!(service.fixture_unfinished()); // Also drives pumping when the engine hint is false.
    let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
    assert!(matches!(service.receive_copied([request]).as_slice(), [DownloadEffect::Destination(100, _)]));
    for _ in 0..12 { assert!(continue_running(&gate, false, &service, quiet_acceptance_turn())); }
    let terminal = acceptance_event(&audit.borrow(), false, DownloadPhase::Complete);
    service.receive_copied([terminal]);
    assert!(continue_running(&gate, false, &service, RunningTurn { records_pending: true, ..quiet_acceptance_turn() }));
    let errors = service.router.persist(|model, profile, completion| model.borrow_mut().record_completed_download(profile, completion).map_err(|e| format!("{e:?}")));
    assert!(errors.is_empty());
    assert_eq!(model.borrow().completed_downloads(audit.borrow().profile).unwrap().len(), 1);
    assert!(!continue_running(&gate, false, &service, quiet_acceptance_turn()));
}
#[test]
fn acceptance_running_driver_stops_failure_quit_shutdown_and_finished() {
    use super::shutdown::{ConsumerGate, ConsumerOwner};
    let (service, audit, _) = acceptance_service();
    let gate = ConsumerGate::default();
    assert!(continue_running(&gate, false, &service, quiet_acceptance_turn()));
    assert!(!continue_running(&gate, true, &service, quiet_acceptance_turn()));
    audit.borrow_mut().fail("creation failed");
    assert!(!continue_running(&gate, false, &service, RunningTurn { engine_pending: true, ..quiet_acceptance_turn() }));
    let (mut service, _, _) = acceptance_service();
    service.errors.push("native dispatch failed".into());
    assert!(!continue_running(&gate, false, &service, quiet_acceptance_turn()));
    let (service, _, _) = acceptance_service();
    let service = Rc::new(RefCell::new(service));
    let shutdown = ShutdownDownloads::take(service.clone(), gate.clone()).unwrap();
    assert_eq!(gate.snapshot(), (ConsumerOwner::Shutdown, false));
    assert!(!continue_running(&gate, false, &service.borrow(), quiet_acceptance_turn()));
    shutdown.flush_retained_with(Rc::ptr_eq, |_| Ok(())).unwrap();
    shutdown.finish().unwrap();
    assert_eq!(gate.snapshot(), (ConsumerOwner::Finished, false));
    assert!(!continue_running(&gate, false, &service.borrow(), quiet_acceptance_turn()));
}
#[gpui::test]
fn acceptance_consumer_status_reads_actual_gate_and_task_without_acquiring(cx: &mut gpui::TestAppContext) {
    use super::shutdown::ConsumerOwner;
    cx.update(|cx| {
        let app = AppDownloads::default();
        cx.set_global(app.clone());
        let before = app.consumer_status(cx);
        assert_eq!(before.owner, ConsumerOwner::Running);
        assert!(!before.busy && !before.scheduled && !before.running);
        assert!(before.legacy_consumer_excluded);
        assert_eq!(before, app.consumer_status(cx));
        // Scheduling is established by the actual wake call, not by status.
        app.wake(cx);
        app.wake(cx); // Duplicate wake retains the same task reservation.
        let scheduled = app.consumer_status(cx);
        assert!(scheduled.scheduled && !scheduled.running);
        assert_eq!(scheduled, app.consumer_status(cx));
        let lease = app.consumer.acquire(ConsumerOwner::Running).unwrap();
        assert!(app.consumer_status(cx).busy);
        let shutdown = app.begin_shutdown().unwrap();
        let handing_off = app.consumer_status(cx);
        assert_eq!(handing_off.owner, ConsumerOwner::Shutdown);
        assert!(handing_off.busy);
        assert_eq!(handing_off, app.consumer_status(cx));
        drop(lease);
        assert!(!app.consumer_status(cx).busy);
        shutdown.flush_retained_with(ProfileHandle::same_workspace, |_| Ok(())).unwrap();
        shutdown.finish().unwrap();
        assert_eq!(app.consumer_status(cx).owner, ConsumerOwner::Finished);
    });
}

#[test]
fn acceptance_canceled_stops_waiting_without_claiming_complete_or_writing_record() {
    use super::shutdown::ConsumerGate;
    let (mut service, audit, model) = acceptance_service();
    let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
    service.receive_copied([request]);
    let canceled = acceptance_event(&audit.borrow(), false, DownloadPhase::Canceled);
    service.receive_copied([canceled]);
    assert!(!audit.borrow().terminal_updated_seen);
    assert!(!service.fixture_unfinished());
    assert!(!continue_running(&ConsumerGate::default(), false, &service, quiet_acceptance_turn()));
    assert!(!service.router.has_pending_records());
    assert!(model.borrow().completed_downloads(audit.borrow().profile).unwrap().is_empty());
}
#[test]
fn acceptance_descriptor_pending_claimed_failed_drive_actual_continuation_seam() {
    use crate::{pages::{PageIdentity, StorageSelection}, window::download_probe::admission};
    use super::shutdown::ConsumerGate;
    let (service, audit, model) = acceptance_service();
    let tab = model.borrow_mut().new_tab(audit.borrow().config.fixture_url(), std::time::SystemTime::UNIX_EPOCH);
    let descriptor = Rc::new(RefCell::new(admission::Descriptor {
        source: admission::Source {
            identity: PageIdentity { tab, profile: audit.borrow().profile, generation: 1 },
            space: model.borrow().space().id,
            storage: StorageSelection::for_profile(audit.borrow().profile), parent: 1,
        },
        owner: audit.borrow().owner, config: audit.borrow().config.clone(),
        phase: admission::Phase::Waiting, generation: None,
    }));
    audit.borrow_mut().admission = Some(descriptor.clone());
    let gate = ConsumerGate::default();
    for phase in [admission::Phase::Waiting, admission::Phase::Creating, admission::Phase::Claimed] {
        descriptor.borrow_mut().phase = phase;
        assert!(service.fixture_unfinished());
        assert!(continue_running(&gate, false, &service, quiet_acceptance_turn()));
    }
    descriptor.borrow_mut().phase = admission::Phase::Failed;
    assert!(!service.fixture_unfinished());
    assert!(!continue_running(&gate, false, &service, RunningTurn { engine_pending: true, ..quiet_acceptance_turn() }));
}

/// Models the quiet interval before CEF delivers its first callback. No native
/// token, admission transition, destination acceptance or completion is invented.
#[derive(Default)]
struct QuietFirstCallbackDriver {
    calls: RefCell<Vec<&'static str>>,
}
impl RunningEngineDriver for QuietFirstCallbackDriver {
    fn download_work_pending(&self) -> bool {
        self.calls.borrow_mut().push("hint");
        false
    }
    fn pump(&self) {
        self.calls.borrow_mut().push("pump");
    }
    fn take_download_events(&self) -> Result<Vec<DownloadEvent>, String> {
        self.calls.borrow_mut().push("drain");
        Ok(Vec::new())
    }
    fn prepare_download_retirements(&self) -> Result<engine::DownloadRetirementPreparation, String> {
        self.calls.borrow_mut().push("retire");
        Ok(engine::DownloadRetirementPreparation::Ready)
    }
}

#[gpui::test]
fn acceptance_actual_task_pumps_and_drains_while_first_callback_is_delayed_without_another_wake(
    cx: &mut gpui::TestAppContext,
) {
    let driver = Rc::new(QuietFirstCallbackDriver::default());
    let app = AppDownloads {
        test_driver: Some(driver.clone()),
        ..AppDownloads::default()
    };
    let audit = cx.update(|cx| {
        crate::quit::reset_for_test(cx);
        let browser = Browser::new();
        let profile = browser.default_user_profile_id();
        let normal = ProfileHandle::new(browser, cx);
        let owner = app.service.borrow_mut().register_source(&normal, profile).unwrap();
        let arm = crate::download_acceptance::PreparedArm::parse(
            "http://127.0.0.1:18473 continue-normal",
        ).unwrap();
        let audit = Rc::new(RefCell::new(crate::download_acceptance::FixtureAudit::new(
            arm, profile, owner,
        )));
        app.service.borrow_mut().attach_fixture(audit.clone()).unwrap();
        cx.set_global(app.clone());
        app.wake(cx); // The only wake in this regression.
        let status = app.consumer_status(cx);
        assert!(status.scheduled && !status.running);
        audit
    });

    cx.run_until_parked(); // Start the actual detached task and park at its timer.
    for turn in 1..=4 {
        // The actual pump predicate must call pump despite a false engine hint.
        // Reverting it to hint-only leaves "pump" absent on the very first turn.
        let expected: Vec<_> = (0..turn)
            .flat_map(|_| ["hint", "pump", "drain", "retire", "hint"])
            .collect();
        assert_eq!(*driver.calls.borrow(), expected);
        cx.update(|cx| {
            let status = app.consumer_status(cx);
            assert_eq!(status.owner, ConsumerOwner::Running);
            assert!(status.running && !status.scheduled && !status.busy);
            assert!(status.legacy_consumer_excluded);
            assert_eq!(status, app.consumer_status(cx));
            assert!(Rc::ptr_eq(&app.service, &cx.global::<AppDownloads>().service));
            let service = app.service.borrow();
            assert!(service.errors.is_empty());
            assert!(service.router.ledger.is_empty());
            assert!(!service.router.has_pending_records());
        });
        let audit = audit.borrow();
        assert_eq!(audit.sequence, 0);
        assert!(audit.token.is_none() && audit.phase.is_none());
        assert!(audit.failure.is_none());
        drop(audit);

        if turn < 4 {
            // No wake, Pages retry or service mutation: only the existing 8ms
            // timer can start another pump/drain turn. Also catch a busy loop.
            cx.executor().advance_clock(Duration::from_millis(7));
            cx.run_until_parked();
            assert_eq!(*driver.calls.borrow(), expected);
            cx.executor().advance_clock(Duration::from_millis(1));
            cx.run_until_parked();
        }
    }

    // Exercise the task guard's real exit after handoff, without a native drain.
    let shutdown = app.begin_shutdown().unwrap();
    cx.executor().advance_clock(Duration::from_millis(8));
    cx.run_until_parked();
    assert_eq!(driver.calls.borrow().len(), 20);
    cx.update(|cx| {
        let status = app.consumer_status(cx);
        assert_eq!(status.owner, ConsumerOwner::Shutdown);
        assert!(!status.running && !status.scheduled && !status.busy);
    });
    shutdown.flush_retained_with(ProfileHandle::same_workspace, |_| Ok(())).unwrap();
    shutdown.finish().unwrap();
}

#[test]
fn acceptance_shutdown_canceled_empty_path_never_writes_or_adds_failure_and_preserves_real_errors() {
    use super::shutdown::{ConsumerGate, ConsumerOwner};
    // Cover a shutdown-owned batch, an old batch finishing after handoff, and
    // that same old batch carrying an independent genuine failure.
    for (in_flight, genuine_error) in [(false, false), (true, false), (true, true)] {
        let (mut service, audit, model) = acceptance_service();
        let request = acceptance_event(&audit.borrow(), true, DownloadPhase::InProgress);
        assert!(matches!(service.receive_copied([request]).as_slice(), [DownloadEffect::Destination(100, _)]));
        let mut active = acceptance_event(&audit.borrow(), false, DownloadPhase::InProgress);
        active.info.received_bytes = 5;
        assert!(service.receive_copied([active]).is_empty());
        assert!(audit.borrow().resolved_path_match);
        let mut canceled = acceptance_event(&audit.borrow(), false, DownloadPhase::Canceled);
        canceled.info.full_path = PathBuf::new();
        canceled.info.received_bytes = canceled.info.total_bytes; // Bytes are not completion.
        if genuine_error { service.errors.push("historical diagnostic".into()); }
        let service = Rc::new(RefCell::new(service));
        let gate = ConsumerGate::default();
        let old = in_flight.then(|| gate.acquire(ConsumerOwner::Running).unwrap());
        let shutdown = ShutdownDownloads::take(service.clone(), gate).unwrap();
        if let Some(old) = old {
            old.with_service_errors(&mut service.borrow_mut(), |service| {
                if genuine_error { service.errors.push("genuine preceding batch failure".into()); }
                assert!(service.receive_copied([canceled.clone()]).is_empty());
            });
            drop(old);
        }
        if genuine_error {
            assert_eq!(service.borrow().errors, ["historical diagnostic", "genuine preceding batch failure"]);
            assert!(matches!(shutdown.drain_copied_and_persist_with(
                || panic!("sticky genuine failure must block before drain"),
                |_, _, _| panic!("cancellation cannot create a record"),
            ), Err(ShutdownDownloadError::Failure(error)) if error.contains("genuine preceding batch failure")));
            assert!(matches!(shutdown.finish(), Err(ShutdownDownloadError::Failure(_))));
        } else {
            // Exact production shutdown ingress; replay also must stay harmless.
            shutdown.drain_copied_and_persist_with(
                || Ok(vec![canceled.clone()]),
                |_, _, _| panic!("cancellation cannot create a record"),
            ).unwrap();
            shutdown.ingest_copied_and_persist_with([canceled],
                |_, _, _| panic!("cancellation replay cannot create a record"),
            ).unwrap();
            assert!(service.borrow().errors.is_empty());
            let flushes = Cell::new(0);
            shutdown.flush_retained_with(Rc::ptr_eq, |_| { flushes.set(flushes.get() + 1); Ok(()) }).unwrap();
            assert_eq!(flushes.get(), 1);
            shutdown.finish().unwrap();
        }
        let audit = audit.borrow();
        assert_eq!(audit.phase, Some(DownloadPhase::Canceled));
        assert!(audit.failure.is_none());
        assert!(!audit.resolved_path_match && !audit.terminal_updated_seen);
        assert!(service.borrow().router.ledger[&100].completion.is_none());
        assert!(service.borrow().router.ledger[&100].committed.is_none());
        assert!(!service.borrow().router.has_pending_records());
        assert!(model.borrow().completed_downloads(audit.profile).unwrap().is_empty());
    }
}

#[test]
fn chooser_cancellation_is_profile_bound_deduplicated_and_waits_for_native_terminal() {
    let model=Rc::new(RefCell::new(Browser::new()));
    let profile=model.borrow().default_user_profile_id();
    let other=model.borrow_mut().create_user_profile("Other").unwrap();
    let mut service=DownloadService::<u64,Model>::default();
    let owner=register(&mut service.router,&model,profile);
    let mut request=event(81,owner,DownloadPhase::InProgress);
    request.destination_requested=true;
    request.destination=DownloadDestination::Requested { suggested_name:"authoritative.dat".into() };
    assert!(service.receive_copied([request]).is_empty());
    assert_eq!(service.pending_destinations()[0].request.suggested_name,"authoritative.dat");
    assert!(service.queue_cancel(81,other).is_err());
    assert_eq!(service.pending_destinations().len(),1);
    service.queue_cancel(81,profile).unwrap();service.queue_cancel(81,profile).unwrap();
    assert!(service.pending_destinations().is_empty());
    let dir=tempfile::tempdir().unwrap();
    let stage=crate::downloads_publication::Staging::reserve((81,service.router.ledger[&81].provenance),&dir.path().canonicalize().unwrap(),vec![std::path::PathBuf::from("late-choice.dat")]).unwrap();
    assert!(service.retain_chosen_staging(81,profile,stage).is_err());
    assert!(matches!(service.receive_copied([]).as_slice(),[DownloadEffect::Control(81,DownloadControl::Cancel)]));
    assert_eq!(service.router.ledger[&81].info.phase,DownloadPhase::InProgress);
    assert!(service.receive_copied([event(81,owner,DownloadPhase::Canceled)]).is_empty());
    assert!(service.queue_cancel(81,profile).is_err());
    assert!(model.borrow().completed_downloads(profile).unwrap().is_empty());
}

#[test]
fn transfer_ui_key_resolves_original_token_and_profile_after_identical_names_and_new_updates() {
    let model=Rc::new(RefCell::new(Browser::new()));let first=model.borrow().default_user_profile_id();
    let second=model.borrow_mut().create_user_profile("Second").unwrap();
    let mut service=DownloadService::<u64,Model>::default();
    let owner_a=register(&mut service.router,&model,first);let owner_b=register(&mut service.router,&model,second);
    let request=|token,owner|{let mut e=event(token,owner,DownloadPhase::InProgress);e.destination_requested=true;e.destination=DownloadDestination::Requested{suggested_name:"same.bin".into()};e};
    service.receive_copied([request(701,owner_a),request(702,owner_b)]);
    let a=service.transfer_presentations().into_iter().find(|r|r.token==701).unwrap();
    let b=service.transfer_presentations().into_iter().find(|r|r.token==702).unwrap();assert_ne!(a.id,b.id);
    service.receive_copied([event(702,owner_b,DownloadPhase::InProgress)]);
    assert_eq!(service.transfer_for_id(a.id).unwrap().profile,first);
    assert_eq!(service.transfer_for_id(b.id).unwrap().token,702);
    assert!(service.request_open(701,second).is_err());assert!(service.request_hide(701,second).is_err());
    service.request_hide(701,first).unwrap();assert!(service.transfer_for_id(a.id).is_none());
    assert_eq!(service.transfer_for_id(b.id).unwrap().profile,second);
}
