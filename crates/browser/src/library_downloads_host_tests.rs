//! GPUI host/controller contract tests. Canonical metadata fixtures are not
//! evidence of engine Complete, destination acceptance or native UI parity.
use super::*;
use crate::{downloads_view::*, window::BrowserWindow};
use browser_core::{
    downloads::{DownloadCompletion, DownloadTime},
    Browser,
};
use gpui::{
    div, prelude::*, px, Context, Focusable, Modifiers, Render, Subscription, TestAppContext,
    VisualTestContext,
};

struct Host {
    controller: LibraryDownloadsHost,
    normal: ProfileHandle,
    visible: bool,
    subscriptions: Vec<Subscription>,
    _inventory: Subscription,
    focus_log: Vec<FocusHandle>,
    native_preview: Option<Rc<std::cell::RefCell<NativePreviewFocus>>>,
}
fn colors() -> DownloadsColors {
    DownloadsColors {
        column: gpui::rgb(0xeeeeee).into(),
        preview: gpui::rgb(0xdddddd).into(),
        text: gpui::rgb(0x111111).into(),
        muted: gpui::rgb(0x666666).into(),
        control: gpui::rgb(0xcccccc).into(),
        selected: gpui::rgb(0xbbbbbb).into(),
        hover: gpui::rgb(0xaaaaaa).into(),
    }
}
fn format() -> CompletedPresentation {
    Rc::new(|records, filtered| {
        if records.is_empty() && filtered {
            return DownloadsList::FilteredEmpty;
        }
        DownloadsList::Completed(vec![DownloadsGroup {
            heading: "Injected fixture heading".into(),
            rows: records
                .into_iter()
                .map(|record| crate::downloads_view::CompletedDownload {
                    token: record.id(),
                    profile: record.id().profile(),
                    title: String::new(),
                    subtitle: "Injected fixture type".into(),
                })
                .collect(),
        }])
    })
}
impl Host {
    fn new(
        normal: ProfileHandle,
        source: Entity<BrowserWindow>,
        preview: DownloadsPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let controller =
            LibraryDownloadsHost::new(normal.clone(), colors(), format(), preview, window, cx)
                .unwrap();
        // Observe the real model-owning window's existing canonical notification
        // path. Production uses that same window's dirty flag at render entry.
        let inventory = cx.observe_in(&source, window, |this, _, window, cx| {
            assert!(this.controller.refresh(&this.normal, colors(), window, cx));
            cx.notify();
        });
        let mut this = Self {
            controller,
            normal,
            visible: false,
            subscriptions: vec![],
            _inventory: inventory,
            focus_log: vec![],
            native_preview: None,
        };
        this.activate(true, window, cx);
        this
    }
    fn activate(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.subscriptions.clear();
        self.visible = visible;
        self.controller.set_active(visible);
        if visible {
            assert!(self.controller.refresh(&self.normal, colors(), window, cx));
            let owner = self.controller.owner();
            let view = self.controller.view();
            self.subscriptions.push(cx.subscribe_in(
                &view,
                window,
                move |this, sender, event: &DownloadsIntent<CompletedDownloadId>, window, cx| {
                    if this.visible {
                        this.controller.handle_intent(
                            owner,
                            sender,
                            &this.normal,
                            event,
                            colors(),
                            window,
                            cx,
                        );
                    }
                },
            ));
            self.subscriptions.push(cx.subscribe_in(
                &view,
                window,
                move |this, sender, _: &DownloadsControlFocused, window, cx| {
                    if this.visible {
                        if let Some(focus) =
                            this.controller
                                .control_focus(owner, sender, &this.normal, window, cx)
                        {
                            // Parent-owned handoff seam: only an accepted exact
                            // control can clear retained preview intent and move
                            // the native responder back to GPUI.
                            if let Some(preview) = &this.native_preview {
                                let mut preview = preview.borrow_mut();
                                preview.intent = None;
                                preview.pages.focus_ui();
                            }
                            this.focus_log.push(focus);
                        }
                    }
                },
            ));
        }
        cx.notify();
    }
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().relative().when(self.visible, |el| {
            el.child(
                div()
                    .absolute()
                    .left(px(112.))
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .child(self.controller.view()),
            )
        })
    }
}
fn source(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    cx.update(crate::init);
    cx.update(crate::downloads_view::bind_keys);
    cx.add_window_view(|window, cx| BrowserWindow::new(Browser::new(), None, window, cx))
}
fn attach(
    source: &Entity<BrowserWindow>,
    visual: &mut VisualTestContext,
    preview: DownloadsPreview,
) -> (Entity<Host>, VisualTestContext) {
    let normal = source.read_with(visual, |source, _| source.profile.handle.clone());
    let source = source.clone();
    let handle = visual.cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| Host::new(normal, source, preview, window, cx))
        })
        .unwrap()
    });
    let root = handle.root(&mut visual.cx).unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), &visual.cx);
    visual.simulate_resize(gpui::size(px(1320.), px(800.)));
    visual.run_until_parked();
    (root, visual)
}
fn preview() -> DownloadsPreview {
    Rc::new(|_, _| {
        div()
            .size_full()
            .debug_selector(|| "retained-preview-fixture".into())
            .into_any_element()
    })
}
fn click(visual: &mut VisualTestContext, selector: &'static str) {
    let bounds = visual.debug_bounds(selector).unwrap();
    visual.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.run_until_parked();
}
fn completion(name: &str) -> DownloadCompletion {
    DownloadCompletion {
        final_path: std::env::temp_dir().join(name),
        source_url: "https://source.test/Archive".into(),
        original_url: String::new(),
        mime_type: "Archive".into(),
        finished_at: DownloadTime::Unknown,
    }
}
fn rows(host: &Host) -> Vec<CompletedDownloadId> {
    match host.controller.projection().unwrap().list {
        DownloadsList::Completed(groups) => groups
            .into_iter()
            .flat_map(|g| g.rows.into_iter().map(|r| r.token))
            .collect(),
        _ => vec![],
    }
}

#[gpui::test]
fn query_and_filter_intents_use_canonical_ids_and_survive_section_reopen(cx: &mut TestAppContext) {
    let (source, source_visual) = source(cx);
    let (default, work) = source.update(source_visual, |source, cx| {
        let ids = source.edit_browser(|b| {
            (
                b.default_user_profile_id(),
                b.create_user_profile("Work").unwrap(),
            )
        });
        cx.notify();
        ids
    });
    let normal = source.read_with(source_visual, |source, _| source.profile.handle.clone());
    let first = normal
        .record_completed_download(default, completion("source-tab.bin"))
        .unwrap();
    let second = normal
        .record_completed_download(work, completion("source-tab (1).bin"))
        .unwrap();
    let (host, mut visual) = attach(&source, source_visual, preview());
    click(&mut visual, "downloads-search");
    visual.simulate_input("SOURCE-TAB");
    visual.run_until_parked();
    assert_eq!(host.read_with(&visual, |h, _| rows(h)), vec![first, second]);
    click(&mut visual, "downloads-filter");
    click(&mut visual, "downloads-profile-1");
    host.read_with(&visual, |h, _| {
        assert_eq!(h.controller.selected_profile(), Some(work));
        assert_eq!(rows(h), vec![second]);
    });
    click(&mut visual, "downloads-profile-0");
    host.read_with(&visual, |h, _| {
        assert_eq!(h.controller.selected_profile(), Some(default));
        assert_eq!(rows(h), vec![first], "a different live profile replaces the previous selection");
        let projection = h.controller.projection().unwrap();
        assert!(projection.profiles.iter().all(|profile| profile.selected == (profile.id == default)));
    });
    let presenter = host.read_with(&visual, |h, _| h.controller.view());
    visual.update(|window, cx| host.update(cx, |h, cx| h.activate(false, window, cx)));
    visual.run_until_parked();
    visual.update(|window, cx| host.update(cx, |h, cx| h.activate(true, window, cx)));
    visual.run_until_parked();
    assert_eq!(
        host.read_with(&visual, |h, _| h.controller.view()),
        presenter
    );
    assert!(visual.debug_bounds("downloads-filter-inline").is_some());
    click(&mut visual, "downloads-filter-done");
    host.read_with(&visual, |h, _| {
        assert_eq!(h.controller.query(), "SOURCE-TAB")
    });
    click(&mut visual, "downloads-filter");
    click(&mut visual, "downloads-profile-1");
    assert_eq!(host.read_with(&visual, |h, _| rows(h)), vec![second]);
    click(&mut visual, "downloads-profile-1");
    assert_eq!(host.read_with(&visual, |h, _| rows(h)), vec![first, second]);
}

#[gpui::test]
fn search_empty_is_router_owned_exact_raw_filename_only_query(cx: &mut TestAppContext) {
    let (source, source_visual) = source(cx);
    let normal = source.read_with(source_visual, |s, _| s.profile.handle.clone());
    let profile = normal.default_user_profile_id();
    normal
        .record_completed_download(profile, completion("source-tab.bin"))
        .unwrap();
    let (host, mut visual) = attach(&source, source_visual, preview());
    click(&mut visual, "downloads-search");
    visual.simulate_input(" SOURCE-TAB ");
    visual.run_until_parked();
    host.read_with(&visual, |h, _| {
        match h.controller.projection().unwrap().list {
            DownloadsList::SearchEmpty { query } => assert_eq!(query, " SOURCE-TAB "),
            _ => panic!("router must retain exact search miss"),
        }
    });
    assert!(visual
        .debug_bounds("downloads-search-empty-title")
        .is_some());
    // Source/type text does not participate in the existing search adapter.
    assert!(!crate::downloads::filename_matches_ascii(
        "source-tab.bin",
        "Archive"
    ));
    assert!(!crate::downloads::filename_matches_ascii(
        "source-tab.bin",
        "source.test"
    ));
}

#[gpui::test]
fn shared_canonical_completion_reprojects_through_real_window_notification(
    cx: &mut TestAppContext,
) {
    let (source, source_visual) = source(cx);
    let normal = source.read_with(source_visual, |s, _| s.profile.handle.clone());
    let (host, mut visual) = attach(&source, source_visual, preview());
    assert!(host.read_with(&visual, |h, _| rows(h)).is_empty());
    // Model fixture only: no fake engine Complete event or native acceptance.
    let id = normal
        .record_completed_download(normal.default_user_profile_id(), completion("late.bin"))
        .unwrap();
    source_visual.run_until_parked();
    visual.run_until_parked();
    assert_eq!(host.read_with(&visual, |h, _| rows(h)), vec![id]);
    host.read_with(&visual, |h, _| {
        match h.controller.projection().unwrap().list {
            DownloadsList::Completed(groups) => {
                assert_eq!(groups[0].heading, "Injected fixture heading");
                assert_eq!(groups[0].rows[0].subtitle, "Injected fixture type");
                assert_eq!(groups[0].rows[0].title, "late.bin");
            }
            _ => panic!("canonical row missing"),
        }
    });
}

#[gpui::test]
fn stale_owner_entity_generation_and_workspace_cannot_mutate_query_or_focus(
    cx: &mut TestAppContext,
) {
    let (source, source_visual) = source(cx);
    let (host, mut visual) = attach(&source, source_visual, preview());
    let old = host.read_with(&visual, |h, _| h.controller.owner());
    visual.update(|window, cx| {
        host.update(cx, |h, cx| {
            let view = h.controller.view();
            h.activate(false, window, cx);
            h.activate(true, window, cx);
            assert!(!h.controller.handle_intent(
                old,
                &view,
                &h.normal,
                &DownloadsIntent::SearchChanged("stale".into()),
                colors(),
                window,
                cx
            ));
            assert!(h
                .controller
                .control_focus(old, &view, &h.normal, window, cx)
                .is_none());
            let other = LibraryDownloadsHost::new(
                h.normal.clone(),
                colors(),
                format(),
                preview(),
                window,
                cx,
            )
            .unwrap();
            let now = h.controller.owner();
            assert!(!h.controller.handle_intent(
                now,
                &other.view(),
                &h.normal,
                &DownloadsIntent::SearchChanged("wrong entity".into()),
                colors(),
                window,
                cx
            ));
            let replacement = ProfileHandle::new(Browser::new(), cx);
            assert!(!h.controller.handle_intent(
                now,
                &view,
                &replacement,
                &DownloadsIntent::SearchChanged("wrong canonical owner".into()),
                colors(),
                window,
                cx
            ));
            assert!(!h.controller.refresh(&replacement, colors(), window, cx));
            assert_eq!(h.controller.query(), "");
        })
    });
}

#[gpui::test]
fn inventory_rename_add_remove_reprojects_without_stale_id_default_fallback(
    cx: &mut TestAppContext,
) {
    let (source, source_visual) = source(cx);
    let work = source.update(source_visual, |s, cx| {
        let id = s.edit_browser(|b| b.create_user_profile("Work").unwrap());
        cx.notify();
        id
    });
    let (host, mut visual) = attach(&source, source_visual, preview());
    click(&mut visual, "downloads-filter");
    click(&mut visual, "downloads-profile-1");
    source.update(source_visual, |s, cx| {
        s.edit_browser(|b| b.rename_user_profile(work, "Renamed").unwrap());
        cx.notify();
    });
    source_visual.run_until_parked();
    visual.run_until_parked();
    host.read_with(&visual, |h, _| {
        let p = h.controller.projection().unwrap();
        assert!(p
            .profiles
            .iter()
            .any(|p| p.id == work && p.name == "Renamed" && p.selected));
    });
    let replacement = source.update(source_visual, |s, cx| {
        let id = s.edit_browser(|b| {
            b.delete_user_profile(work).unwrap();
            b.create_user_profile("Replacement").unwrap()
        });
        cx.notify();
        id
    });
    source_visual.run_until_parked();
    visual.run_until_parked();
    assert_ne!(replacement, work);
    host.read_with(&visual, |h, _| {
        assert_eq!(h.controller.selected_profile(), Some(work));
        let p = h.controller.projection().unwrap();
        assert!(p.profiles.iter().all(|p| !p.selected && p.id != work));
        assert!(matches!(p.list, DownloadsList::FilteredEmpty));
    });
    visual.update(|window, cx| {
        host.update(cx, |h, cx| {
            let sender = h.controller.view();
            assert!(!h.controller.handle_intent(
                h.controller.owner(),
                &sender,
                &h.normal,
                &DownloadsIntent::ToggleProfile(work),
                colors(),
                window,
                cx
            ));
        })
    });
    click(&mut visual, "downloads-profile-1");
    assert_eq!(
        host.read_with(&visual, |h, _| h.controller.selected_profile()),
        Some(replacement)
    );
}

#[gpui::test]
fn focus_seam_captures_controls_but_passes_preview_callback_through(cx: &mut TestAppContext) {
    let (source, source_visual) = source(cx);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let (host, mut visual) = attach(
        &source,
        source_visual,
        Rc::new(move |_, _| {
            count.set(count.get() + 1);
            div()
                .size_full()
                .debug_selector(|| "actual-preview-callback".into())
                .into_any_element()
        }),
    );
    assert!(calls.get() > 0);
    let bounds = visual.debug_bounds("actual-preview-callback").unwrap();
    assert_eq!(bounds, visual.debug_bounds("downloads-preview").unwrap());
    click(&mut visual, "downloads-search");
    assert!(host.read_with(&visual, |h, _| !h.focus_log.is_empty()));
    visual.update(|window, cx| {
        let h = host.read(cx);
        assert!(h.controller.restore_control(
            h.controller.owner(),
            &h.controller.view(),
            &h.normal,
            h.focus_log.last().unwrap(),
            window,
            cx
        ));
    });
    visual.simulate_input("abc");
    visual.run_until_parked();
    assert_eq!(
        host.read_with(&visual, |h, _| h.controller.query().to_owned()),
        "abc"
    );
    click(&mut visual, "downloads-filter");
    let before = host.read_with(&visual, |h, _| h.focus_log.len());
    visual.simulate_keystrokes("tab");
    visual.run_until_parked();
    assert!(host.read_with(&visual, |h, _| h.focus_log.len()) > before);
    visual.update(|window, cx| {
        let preview_focus = cx.focus_handle();
        preview_focus.focus(window);
        host.read_with(cx, |h, cx| {
            assert!(h
                .controller
                .control_focus(
                    h.controller.owner(),
                    &h.controller.view(),
                    &h.normal,
                    window,
                    cx
                )
                .is_none())
        });
    });
}

#[gpui::test]
fn private_and_blank_handles_cannot_create_a_library_downloads_host(cx: &mut TestAppContext) {
    let (source, visual) = source(cx);
    let normal = source.read_with(visual, |s, _| s.profile.handle.clone());
    visual.update(|window, cx| {
        let private = ProfileHandle::new(Browser::new_private(), cx);
        let blank = ProfileHandle::new_with_shared_history(
            normal.blank_browser(normal.default_user_profile_id()),
            &normal,
            cx,
        );
        for handle in [private, blank] {
            assert!(
                LibraryDownloadsHost::new(handle, colors(), format(), preview(), window, cx)
                    .is_none()
            );
        }
    });
}

#[gpui::test]
fn actual_weak_owner_retained_preview_callback_preserves_empty_page_and_spaces_guard(
    cx: &mut TestAppContext,
) {
    use crate::{library::LibrarySection, sidebar_pages::SidebarPage};
    let (source, source_visual) = source(cx);
    source.update(source_visual, |source, cx| {
        source.select_sidebar_page(SidebarPage::Library, cx);
        source.sidebar_navigation.library_section = LibrarySection::Downloads;
        cx.notify();
    });
    let owner = source.downgrade();
    let called = Rc::new(Cell::new(0));
    let count = called.clone();
    // Exact r5 callback body, with only a test counter at accepted invocation.
    let callback: DownloadsPreview = Rc::new(move |_, cx| {
        owner
            .update(cx, |this, cx| {
                if this.sidebar_navigation.controller.selected() != SidebarPage::Library
                    || this.sidebar_navigation.library_section != LibrarySection::Downloads
                {
                    return div().into_any_element();
                }
                count.set(count.get() + 1);
                this.render_library_page_preview(cx)
            })
            .unwrap_or_else(|_| div().into_any_element())
    });
    let (host, mut visual) = attach(&source, source_visual, callback);
    assert!(called.get() > 0);
    assert!(source.read_with(source_visual, |s, _| s.browser.active_tab().is_none()));
    assert!(visual.debug_bounds("downloads-preview").unwrap().size.width > px(0.));
    source.update(source_visual, |source, cx| {
        source.sidebar_navigation.library_section = LibrarySection::Spaces;
        cx.notify();
    });
    source_visual.run_until_parked();
    visual.run_until_parked();
    let before = called.get();
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    assert_eq!(
        called.get(),
        before,
        "Spaces must not invoke retained page renderer"
    );
    // The parent unmounts Downloads on actual section exit. No route is enabled
    // by this test harness and no native engine is attached.
    visual.update(|window, cx| host.update(cx, |h, cx| h.activate(false, window, cx)));
}

#[gpui::test]
fn completed_row_clicks_have_no_host_or_file_effect(cx: &mut TestAppContext) {
    let (source, source_visual) = source(cx);
    let normal = source.read_with(source_visual, |s, _| s.profile.handle.clone());
    let id = normal
        .record_completed_download(
            normal.default_user_profile_id(),
            completion("disabled-row.bin"),
        )
        .unwrap();
    let (host, mut visual) = attach(&source, source_visual, preview());
    click(&mut visual, "downloads-row-0-0");
    host.read_with(&visual, |h, _| {
        assert_eq!(rows(h), vec![id]);
        assert_eq!(h.controller.query(), "");
        assert_eq!(h.controller.selected_profile(), None);
    });
    visual.update(|window, cx| {
        host.update(cx, |h, cx| {
            let sender = h.controller.view();
            assert!(!h.controller.handle_intent(
                h.controller.owner(),
                &sender,
                &h.normal,
                &DownloadsIntent::RowPressed {
                    token: id,
                    profile: id.profile()
                },
                colors(),
                window,
                cx
            ));
        })
    });
}

#[gpui::test]
fn queued_old_presenter_event_cannot_cross_section_generation(cx: &mut TestAppContext) {
    let (source, source_visual) = source(cx);
    let (host, mut visual) = attach(&source, source_visual, preview());
    visual.update(|window, cx| {
        host.update(cx, |h, cx| {
            h.controller.view().update(cx, |_, cx| {
                cx.emit(DownloadsIntent::<CompletedDownloadId>::SearchChanged(
                    "queued old route".into(),
                ))
            });
            h.activate(false, window, cx);
            h.activate(true, window, cx);
        })
    });
    visual.run_until_parked();
    assert_eq!(
        host.read_with(&visual, |h, _| h.controller.query().to_owned()),
        ""
    );
    click(&mut visual, "downloads-search");
    visual.simulate_input("new route");
    visual.run_until_parked();
    assert_eq!(
        host.read_with(&visual, |h, _| h.controller.query().to_owned()),
        "new route"
    );
}

// A recording engine exercises real Pages responder intent without CEF/native
// input. The optional harness bridge mirrors the documented parent handoff;
// production BrowserWindow wiring remains a parent acceptance gate.
#[derive(Clone, Default)]
struct FocusEngine(Rc<Cell<usize>>);
impl crate::pages::WebEngine for FocusEngine {
    fn create(&mut self, _: &str, _: engine::Bounds) -> Option<engine::ViewId> {
        Some(engine::ViewId::from_raw(1))
    }
    fn set_bounds(&mut self, _: engine::ViewId, _: engine::Bounds) {}
    fn set_top_corners_rounded(&mut self, _: engine::ViewId, _: bool) {}
    fn set_visible(&mut self, _: engine::ViewId, _: bool) {}
    fn focus_page(&mut self, _: engine::ViewId) {}
    fn focus_ui(&mut self) {
        self.0.set(self.0.get() + 1);
    }
    fn navigate(&mut self, _: engine::ViewId, _: &str) {}
    fn go_back(&mut self, _: engine::ViewId) {}
    fn go_forward(&mut self, _: engine::ViewId) {}
    fn undo(&mut self, _: engine::ViewId) {}
    fn redo(&mut self, _: engine::ViewId) {}
    fn reload(&mut self, _: engine::ViewId) {}
    fn force_reload(&mut self, _: engine::ViewId) {}
    fn stop_loading(&mut self, _: engine::ViewId) {}
    fn set_audio_muted(&mut self, _: engine::ViewId, _: bool) {}
    fn audio_muted(&self, _: engine::ViewId) -> Option<bool> {
        Some(false)
    }
    fn zoom(&mut self, _: engine::ViewId, _: i32) {}
    fn find(&mut self, _: engine::ViewId, _: &str, _: bool, _: bool) {}
    fn stop_finding(&mut self, _: engine::ViewId) {}
    fn close(&mut self, _: engine::ViewId) {}
    fn pump_fn(&self) -> fn() {
        || {}
    }
    fn take_events(&mut self) -> Vec<engine::Event> {
        vec![]
    }
}
struct NativePreviewFocus {
    pages: crate::pages::Pages,
    intent: Option<crate::pages::ResizePageFocus>,
}

#[gpui::test]
fn disabled_rows_and_background_accept_control_focus_after_native_preview_intent(
    cx: &mut TestAppContext,
) {
    let (source, source_visual) = source(cx);
    let normal = source.read_with(source_visual, |s, _| s.profile.handle.clone());
    let profile = normal.default_user_profile_id();
    let id = normal
        .record_completed_download(profile, completion("disabled-row.bin"))
        .unwrap();
    let engine = FocusEngine::default();
    let mut browser = Browser::new();
    browser.new_tab("https://preview.test/", std::time::SystemTime::now());
    let mut pages = crate::pages::Pages::new(Box::new(engine.clone()));
    pages.set_bounds(engine::Bounds {
        x: 0.,
        y: 0.,
        width: 640.,
        height: 480.,
    });
    pages.sync(&browser, true);
    let bridge = Rc::new(std::cell::RefCell::new(NativePreviewFocus {
        pages,
        intent: None,
    }));
    let preview_bridge = bridge.clone();
    let preview_focus = source_visual.update(|_, cx| cx.focus_handle());
    let fixture_focus = preview_focus.clone();
    let (host, mut visual) = attach(
        &source,
        source_visual,
        Rc::new(move |_, _| {
            let bridge = preview_bridge.clone();
            div()
                .size_full()
                .debug_selector(|| "native-preview-focus-fixture".into())
                .track_focus(&fixture_focus)
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, _| {
                    window.prevent_default();
                    let mut preview = bridge.borrow_mut();
                    // Like CEF, this changes Pages intent while retaining GPUI's
                    // previous focus handle. No blanket preview interception.
                    preview.pages.focus_shown();
                    let intent = preview.pages.capture_resize_focus();
                    assert!(intent.has_native_focus());
                    preview.intent = Some(intent);
                })
                .into_any_element()
        }),
    );
    host.update(&mut visual, |h, _| h.native_preview = Some(bridge.clone()));
    click(&mut visual, "downloads-search");
    visual.simulate_input("disabled");
    visual.run_until_parked();

    for filtered in [false, true] {
        if filtered {
            click(&mut visual, "downloads-filter");
            click(&mut visual, "downloads-profile-0");
        }
        for background in [false, true] {
            if background {
                // Cover a retained GPUI preview handle as well as the previous
                // exact Search/Profile control retained on native page focus.
                visual.update(|window, _| preview_focus.focus(window));
            }
            let before_preview = host.read_with(&visual, |h, _| h.focus_log.len());
            let before_preview_ui_calls = engine.0.get();
            click(&mut visual, "native-preview-focus-fixture");
            assert_eq!(
                host.read_with(&visual, |h, _| h.focus_log.len()),
                before_preview
            );
            assert_eq!(engine.0.get(), before_preview_ui_calls);
            assert!(bridge.borrow().intent.as_ref().unwrap().has_native_focus());
            assert!(bridge
                .borrow()
                .pages
                .capture_resize_focus()
                .has_native_focus());
            let expected = visual.update(|window, cx| {
                let h = host.read(cx);
                let view = h.controller.view();
                view.read(cx)
                    .focused_control(window, cx)
                    .unwrap_or_else(|| view.read(cx).focus_handle(cx))
            });
            let before = host.read_with(&visual, |h, _| h.focus_log.len());
            let ui_calls = engine.0.get();
            if background {
                // Empty space below the sole disabled row, not a child control.
                let bounds = visual.debug_bounds("downloads-list-body").unwrap();
                let point = gpui::point(bounds.center().x, bounds.bottom() - px(8.));
                visual.simulate_mouse_move(point, None, Modifiers::none());
                visual.run_until_parked();
                visual.simulate_click(point, Modifiers::none());
                visual.run_until_parked();
            } else {
                click(&mut visual, "downloads-row-0-0");
            }
            assert!(host.read_with(&visual, |h, _| h.focus_log.len()) > before);
            assert!(
                engine.0.get() > ui_calls,
                "accepted focus must call Pages::focus_ui"
            );
            assert!(bridge.borrow().intent.is_none());
            assert!(!bridge
                .borrow()
                .pages
                .capture_resize_focus()
                .has_native_focus());
            visual.update(|window, cx| {
                let h = host.read(cx);
                let view = h.controller.view();
                let accepted = h.focus_log.last().unwrap();
                assert_eq!(window.focused(cx).as_ref(), Some(accepted));
                assert_eq!(accepted, &expected);
                assert!(view.read(cx).is_control_available(accepted, cx));
                assert!(h.controller.restore_control(
                    h.controller.owner(),
                    &view,
                    &h.normal,
                    accepted,
                    window,
                    cx
                ));
                assert_eq!(h.controller.query(), "disabled");
                assert_eq!(h.controller.selected_profile(), filtered.then_some(profile));
                assert_eq!(rows(h), vec![id]);
            });
            assert_eq!(
                visual.debug_bounds("downloads-filter-inline").is_some(),
                filtered
            );
        }
    }
}
