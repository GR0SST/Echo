//! Host integration on GPUI's test platform. Included by sidebar_navigation.rs.
//! Right-edge coverage uses the page-selection seam, not a native swipe.
//! Cargo/native validation belongs to the integrating parent.
use crate::{
    library::LibrarySection,
    pages::{StorageSelection, WebEngine},
    sidebar_pages::SidebarPage,
    window::BrowserWindow,
};
use browser_core::{Browser, SpaceId, space_creation::SpaceCreationError};
use engine::{Bounds, Event, ViewId};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, px};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::SystemTime};

#[derive(Default)]
struct EngineState {
    next: u64,
    visible: HashMap<ViewId, bool>,
    urls: HashMap<ViewId, String>,
    focused: Option<ViewId>,
    document_changes: usize,
}

/// Same recording-engine convention as tests.rs, with only this slice's state.
#[derive(Clone, Default)]
struct RecordingEngine(Rc<RefCell<EngineState>>);

impl WebEngine for RecordingEngine {
    fn create_in_storage(
        &mut self,
        _: &StorageSelection,
        _: *mut std::ffi::c_void,
        url: &str,
        bounds: Bounds,
    ) -> Option<ViewId> {
        self.create(url, bounds)
    }
    fn create(&mut self, url: &str, _: Bounds) -> Option<ViewId> {
        let mut state = self.0.borrow_mut();
        state.next += 1;
        let id = ViewId::from_raw(state.next);
        state.visible.insert(id, false);
        state.urls.insert(id, url.into());
        state.document_changes += 1;
        Some(id)
    }
    fn set_bounds(&mut self, _: ViewId, _: Bounds) {}
    fn set_top_corners_rounded(&mut self, _: ViewId, _: bool) {}
    fn set_visible(&mut self, id: ViewId, visible: bool) {
        self.0.borrow_mut().visible.insert(id, visible);
    }
    fn focus_page(&mut self, id: ViewId) {
        self.0.borrow_mut().focused = Some(id);
    }
    fn focus_ui(&mut self) {
        self.0.borrow_mut().focused = None;
    }
    fn navigate(&mut self, _: ViewId, _: &str) {
        self.0.borrow_mut().document_changes += 1;
    }
    fn go_back(&mut self, _: ViewId) {}
    fn go_forward(&mut self, _: ViewId) {}
    fn undo(&mut self, _: ViewId) {}
    fn redo(&mut self, _: ViewId) {}
    fn reload(&mut self, _: ViewId) {
        self.0.borrow_mut().document_changes += 1;
    }
    fn force_reload(&mut self, _: ViewId) {
        self.0.borrow_mut().document_changes += 1;
    }
    fn stop_loading(&mut self, _: ViewId) {}
    fn set_audio_muted(&mut self, _: ViewId, _: bool) {}
    fn audio_muted(&self, _: ViewId) -> Option<bool> {
        Some(false)
    }
    fn zoom(&mut self, _: ViewId, _: i32) {}
    fn find(&mut self, _: ViewId, _: &str, _: bool, _: bool) {}
    fn stop_finding(&mut self, _: ViewId) {}
    fn close(&mut self, id: ViewId) {
        let mut state = self.0.borrow_mut();
        state.visible.remove(&id);
        state.document_changes += 1;
    }
    fn pump_fn(&self) -> fn() {
        || {}
    }
    fn take_events(&mut self) -> Vec<Event> {
        Vec::new()
    }
}

fn open(
    cx: &mut TestAppContext,
    browser: Browser,
) -> (
    Entity<BrowserWindow>,
    &mut VisualTestContext,
    RecordingEngine,
) {
    cx.update(crate::init);
    let engine = RecordingEngine::default();
    let (view, visual) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(engine.clone())), window, cx);
        view.focus(window);
        view
    });
    visual.simulate_resize(gpui::size(px(1920.), px(1050.)));
    visual.run_until_parked();
    (view, visual, engine)
}

fn click(visual: &mut VisualTestContext, selector: &'static str) {
    visual.run_until_parked();
    visual
        .executor()
        .advance_clock(std::time::Duration::from_secs(1));
    visual.run_until_parked();
    let bounds = visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    visual.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.run_until_parked();
}

fn session(view: &Entity<BrowserWindow>, visual: &VisualTestContext) -> String {
    view.read_with(visual, |view, _| view.session_json())
}

fn select(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext, page: SidebarPage) {
    view.update(visual, |view, cx| {
        view.select_sidebar_page(page, cx);
    });
    visual.run_until_parked();
}

fn assert_draft(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext, origin: SpaceId) {
    view.read_with(visual, |view, _| {
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::CreateSpace
        );
        let draft = view
            .sidebar_navigation
            .controller
            .draft()
            .expect("window-local draft");
        assert_eq!(draft.origin, origin);
        assert!(draft.name.is_empty());
        assert!(view.sidebar_navigation.form.is_some());
        assert!(
            view.command_bar.is_none(),
            "creation belongs in the sidebar"
        );
        assert!(!view.page_content_visible());
        let pages = view.sidebar_navigation.controller.pages(&view.browser);
        let index = pages
            .iter()
            .position(|page| *page == SidebarPage::Space(origin))
            .unwrap();
        assert_eq!(pages[index + 1], SidebarPage::CreateSpace);
    });
    assert!(visual.debug_bounds("space-creation-form").is_some());
}

fn command_bar(view: &Entity<BrowserWindow>, visual: &mut VisualTestContext) -> VisualTestContext {
    visual.run_until_parked();
    let overlay = view
        .read_with(visual, |view, _| {
            view.command_bar.as_ref().and_then(|bar| bar.overlay)
        })
        .expect("Command Bar overlay");
    VisualTestContext::from_window(overlay.into(), visual)
}

#[gpui::test]
fn spaces_menu_new_space_opens_a_draft_without_allocating_model_ids(cx: &mut TestAppContext) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    let before = session(&view, visual);
    // Resolve the real menu action rather than dispatching an invented shortcut.
    let menu = crate::menus::build()
        .into_iter()
        .find(|menu| menu.name.as_ref() == "Spaces")
        .unwrap();
    let action = menu
        .items
        .into_iter()
        .find_map(|item| match item {
            gpui::MenuItem::Action { name, action, .. } if name.as_ref() == "New Space…" => {
                Some(action)
            }
            _ => None,
        })
        .expect("Spaces / New Space…");
    visual.update(|window, cx| window.dispatch_action(action, cx));
    visual.run_until_parked();
    assert_draft(&view, visual, origin);
    // Serialized bytes include allocator watermarks, tabs and profile data.
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn command_new_space_closes_the_bar_and_focuses_the_sidebar_name(cx: &mut TestAppContext) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let before = session(&view, visual);
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    visual.simulate_keystrokes("cmd-t");
    command_bar(&view, visual).simulate_input("create space");
    command_bar(&view, visual).simulate_keystrokes("enter");
    visual.run_until_parked();
    assert_draft(&view, visual, origin);
    // No click: this also catches lost focus when the Command Bar closes.
    visual.simulate_input("Research draft");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| {
            view.sidebar_navigation
                .controller
                .draft()
                .unwrap()
                .name
                .clone()
        }),
        "Research draft"
    );
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn library_spaces_plus_uses_the_library_origin_and_cancel_returns_to_it(cx: &mut TestAppContext) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    let before = session(&view, visual);
    select(&view, visual, SidebarPage::Library);
    assert_eq!(
        view.read_with(visual, |view, _| view.sidebar_navigation.library_section),
        LibrarySection::Media
    );
    assert!(view.read_with(visual, |view, _| {
        view.sidebar_navigation.controller.draft().is_none()
    }));
    click(visual, "library-nav-spaces");
    assert_eq!(
        view.read_with(visual, |view, _| view.sidebar_navigation.library_section),
        LibrarySection::Spaces
    );
    assert_eq!(session(&view, visual), before);
    click(visual, "library-new-space");
    assert_draft(&view, visual, origin);
    click(visual, "space-creation-name");
    visual.simulate_input("Discard me");
    click(visual, "space-creation-cancel");
    view.read_with(visual, |view, _| {
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::Space(origin)
        );
        assert_eq!(view.browser.space().id, origin);
        assert!(view.sidebar_navigation.controller.draft().is_none());
        assert!(view.sidebar_navigation.form.is_none());
        assert!(view.page_content_visible());
    });
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn right_edge_page_selection_opens_one_draft_and_preserves_it_on_reentry(cx: &mut TestAppContext) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let before = session(&view, visual);
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    // Native swipe physics are deliberately outside this host seam test.
    select(&view, visual, SidebarPage::CreateSpace);
    assert_draft(&view, visual, origin);
    click(visual, "space-creation-name");
    visual.simulate_input("Keep my draft");
    visual.run_until_parked();
    let form = view.read_with(visual, |view, _| {
        view.sidebar_navigation.form.clone().unwrap()
    });
    select(&view, visual, SidebarPage::Space(origin));
    select(&view, visual, SidebarPage::CreateSpace);
    view.read_with(visual, |view, _| {
        assert_eq!(view.sidebar_navigation.form.as_ref(), Some(&form));
        assert_eq!(
            view.sidebar_navigation.controller.draft().unwrap().name,
            "Keep my draft"
        );
        assert_eq!(
            view.sidebar_navigation.controller.draft().unwrap().origin,
            origin
        );
    });
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn spaces_manager_retains_document_and_creation_hides_it_without_loading_or_closing(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx, crate::demo_browser());
    let (origin, tab) = view.read_with(visual, |view, _| {
        (
            view.browser.space().id,
            view.browser.active_tab().unwrap().id,
        )
    });
    assert!(view.read_with(visual, |view, _| {
        view.pages.as_ref().unwrap().borrow().is_visible(tab)
    }));
    let documents = engine.0.borrow().document_changes;
    assert!(engine.0.borrow().visible.values().any(|visible| *visible));
    for page in [SidebarPage::Library, SidebarPage::CreateSpace] {
        select(&view, visual, page);
        if page == SidebarPage::Library { click(visual, "library-nav-spaces"); }
        assert!(!view.read_with(visual, |view, _| view.page_content_visible()));
        let retained = page == SidebarPage::Library;
        assert_eq!(view.read_with(visual, |view, _| {
            view.pages.as_ref().unwrap().borrow().is_visible(tab)
        }), retained);
        let state = engine.0.borrow();
        assert_eq!(state.visible.values().any(|visible| *visible), retained);
        assert_eq!(state.document_changes, documents);
    }
    view.update(visual, |view, cx| {
        view.cancel_space_creation(cx);
    });
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().id, origin);
        assert!(view.page_content_visible());
        assert!(view.pages.as_ref().unwrap().borrow().is_visible(tab));
    });
    assert!(engine.0.borrow().visible.values().any(|visible| *visible));
    assert_eq!(engine.0.borrow().document_changes, documents);
}

#[gpui::test]
fn returning_to_the_same_space_restores_native_page_focus(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    let document = engine
        .0
        .borrow()
        .visible
        .iter()
        .find_map(|(id, visible)| visible.then_some(*id))
        .expect("shown native page");
    select(&view, visual, SidebarPage::Library);
    assert_eq!(engine.0.borrow().focused, None);
    select(&view, visual, SidebarPage::Space(origin));
    assert_eq!(
        engine.0.borrow().focused,
        Some(document),
        "same selection still needs native responder restoration"
    );
    select(&view, visual, SidebarPage::CreateSpace);
    assert_eq!(engine.0.borrow().focused, None);
    view.update(visual, |view, cx| view.cancel_space_creation(cx));
    visual.run_until_parked();
    assert_eq!(
        engine.0.borrow().focused,
        Some(document),
        "Cancel returns focus to the existing native document"
    );
}

#[gpui::test]
fn submit_creates_one_empty_space_after_the_captured_origin(cx: &mut TestAppContext) {
    let (view, visual, engine) = open(cx, crate::demo_browser());
    let (origin, profile, old_ids, tabs) = view.read_with(visual, |view, _| {
        (
            view.browser.space().id,
            view.browser.active_user_profile_id(),
            view.browser
                .spaces
                .iter()
                .map(|space| space.id)
                .collect::<Vec<_>>(),
            view.browser
                .all_tabs()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
        )
    });
    let before = session(&view, visual);
    view.update(visual, |view, cx| {
        view.begin_space_creation(origin, cx);
    });
    visual.run_until_parked();
    click(visual, "space-creation-submit");
    assert_eq!(
        session(&view, visual),
        before,
        "empty submission cannot allocate"
    );
    click(visual, "space-creation-name");
    visual.simulate_input("  Reading 📚  ");
    visual.run_until_parked();
    assert_eq!(session(&view, visual), before, "typing cannot allocate");
    let documents = engine.0.borrow().document_changes;
    click(visual, "space-creation-submit");
    let committed = session(&view, visual);
    assert!(visual.debug_bounds("page-surface").is_some());
    assert!(
        visual.debug_bounds("page-placeholder").is_none(),
        "a committed empty Space is an empty surface, without invented copy"
    );
    view.read_with(visual, |view, _| {
        let space = view.browser.space();
        assert_eq!(space.name, "Reading 📚");
        assert!(!old_ids.contains(&space.id));
        assert_eq!(space.user_profile_id, profile);
        assert!(space.pinned.is_empty() && space.today.is_empty());
        assert!(space.active_tab.is_none());
        let index = view
            .browser
            .spaces
            .iter()
            .position(|space| space.id == origin)
            .unwrap();
        assert_eq!(view.browser.spaces[index + 1].id, space.id);
        assert_eq!(view.browser.spaces.len(), old_ids.len() + 1);
        assert_eq!(
            view.browser
                .spaces
                .iter()
                .filter(|space| space.id != view.browser.space().id)
                .map(|space| space.id)
                .collect::<Vec<_>>(),
            old_ids
        );
        assert_eq!(
            view.browser
                .all_tabs()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            tabs
        );
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::Space(space.id)
        );
        assert!(view.sidebar_navigation.controller.draft().is_none());
        assert!(view.sidebar_navigation.form.is_none());
        assert!(view.command_bar.is_none());
    });
    assert_eq!(engine.0.borrow().document_changes, documents);
    view.update(visual, |view, cx| {
        view.submit_space_creation(cx);
    });
    visual.run_until_parked();
    assert_eq!(
        session(&view, visual),
        committed,
        "a repeated submit cannot create a second Space"
    );
}

#[gpui::test]
fn invalid_commit_request_preserves_authoritative_session_and_live_draft(cx: &mut TestAppContext) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    view.update(visual, |view, cx| {
        view.begin_space_creation(origin, cx);
    });
    visual.run_until_parked();
    let before = session(&view, visual);
    let mut request = view.read_with(visual, |view, _| {
        view.sidebar_navigation.controller.request().unwrap()
    });
    request.name = "Invalid hue".into();
    request.hue = f32::NAN;
    let result = view.update(visual, |view, _| {
        view.commit_space_creation_request(&request)
    });
    assert_eq!(result, Err(SpaceCreationError::InvalidHue));
    assert_eq!(session(&view, visual), before);
    assert_draft(&view, visual, origin);
}

#[gpui::test]
fn library_cards_omit_favorites_without_changing_profile_data(cx: &mut TestAppContext) {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    let personal_profile = browser.active_user_profile_id();
    let personal = browser.new_tab("https://personal.example/favorite", SystemTime::now());
    browser.favorite_tab(personal);
    let work = browser.add_space("Work", 210.);
    let work_profile = browser.create_user_profile("Work data").unwrap();
    browser
        .assign_space_user_profile(work, work_profile)
        .unwrap();
    browser.switch_space(1);
    let work_favorite = browser.new_tab("https://work.example/favorite", SystemTime::now());
    browser.favorite_tab(work_favorite);
    browser.switch_space(0);
    let (view, visual, _) = open(cx, browser);
    let before = session(&view, visual);
    select(&view, visual, SidebarPage::Library);
    click(visual, "library-nav-spaces");
    for index in 0..2 {
        let card: &'static str = Box::leak(format!("library-space-{index}").into_boxed_str());
        let panel: &'static str = Box::leak(format!("library-space-material-{index}").into_boxed_str());
        assert_eq!(visual.debug_bounds(panel), visual.debug_bounds(card),
            "the lighter panel must cover the entire themed card, including its rounded edge");
    }
    // Check actual host-injected sidebar items, not just projection metadata.
    // Selectors come from BrowserWindow::render_favorite's existing ID contract.
    for (index, favorite) in [(0, personal), (1, work_favorite)] {
        let selector: &'static str = Box::leak(
            format!(
                "library-{:?}/favorite-{}",
                view.read_with(visual, |view, _| view.browser.spaces[index].id),
                favorite.get()
            )
            .into_boxed_str(),
        );
        assert!(visual.debug_bounds(selector).is_none(), "Favorites belong to the regular sidebar, not Library cards");

    }
    view.read_with(visual, |view, _| {
        assert_eq!(
            view.browser.space().id,
            origin,
            "rendering inactive cards cannot switch Space"
        );
        assert_eq!(view.browser.active_user_profile_id(), personal_profile);
        assert!(view.sidebar_navigation.controller.draft().is_none());
    });
    assert_eq!(session(&view, visual), before);
    click(visual, "library-space-select-1");
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().id, work);
        assert_eq!(view.browser.active_user_profile_id(), work_profile);
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::Space(work)
        );
        assert!(view.page_content_visible());
    });
}

#[gpui::test]
fn inactive_sidebars_keep_selectors_and_popup_anchors_out_of_the_active_viewport(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let sidebar = visual
        .debug_bounds("sidebar")
        .expect("fixed gesture viewport");
    let address = visual.debug_bounds("address").expect("active address only");
    assert!(sidebar.contains(&address.center()));
    let inactive = view.read_with(visual, |view, _| view.browser.spaces[1].id);
    let selector: &'static str =
        Box::leak(format!("inactive-{inactive:?}/new-tab").into_boxed_str());
    let inactive_address = visual
        .debug_bounds(selector)
        .expect("namespaced neighboring page");
    assert!(!sidebar.contains(&inactive_address.center()));
    let new_anchor = view.read_with(visual, |view, _| {
        view.new_menu_anchor.get().expect("active New menu anchor")
    });
    assert!(
        sidebar.contains(&new_anchor.center()),
        "offscreen canvases must not replace active menu anchors"
    );
    click(visual, "new-tab");
    assert!(
        view.read_with(visual, |view, _| view.command_bar.is_some()),
        "the global New Tab selector must hit the active page"
    );
}

#[gpui::test]
fn creation_return_keeps_the_draft_and_two_escapes_cancel_without_allocating(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    let before = session(&view, visual);
    select(&view, visual, SidebarPage::CreateSpace);
    click(visual, "space-creation-name");
    visual.simulate_input("Keyboard draft");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    assert_eq!(session(&view, visual), before);
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .request()
            .unwrap()
            .name),
        "Keyboard draft"
    );
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .selected()),
        SidebarPage::CreateSpace
    );
    assert_eq!(session(&view, visual), before);
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .selected()),
        SidebarPage::Space(origin)
    );
    assert!(view.read_with(visual, |view, _| view.sidebar_navigation.form.is_none()));
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn space_shortcut_leaves_creation_and_restores_the_same_loaded_document_focus(
    cx: &mut TestAppContext,
) {
    let (view, visual, engine) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    let document = engine
        .0
        .borrow()
        .visible
        .iter()
        .find_map(|(id, visible)| visible.then_some(*id))
        .unwrap();
    select(&view, visual, SidebarPage::CreateSpace);
    click(visual, "space-creation-name");
    assert_eq!(engine.0.borrow().focused, None);
    visual.simulate_keystrokes("ctrl-1");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .selected()),
        SidebarPage::Space(origin)
    );
    assert_eq!(engine.0.borrow().focused, Some(document));
    assert!(engine.0.borrow().visible[&document]);
}

#[gpui::test]
fn library_split_segment_uses_its_card_profile_and_opens_the_clicked_pane(cx: &mut TestAppContext) {
    let mut browser = Browser::new();
    browser.new_tab("https://source.example/", SystemTime::now());
    let destination = browser.add_space("Destination", 210.);
    let destination_profile = browser.create_user_profile("Destination data").unwrap();
    browser
        .assign_space_user_profile(destination, destination_profile)
        .unwrap();
    browser.switch_space(1);
    let left = browser.new_tab("https://destination.example/left", SystemTime::now());
    let right = browser.new_tab("https://destination.example/right", SystemTime::now());
    browser
        .add_split(
            left,
            right,
            browser_core::splits::Placement::Right,
            SystemTime::now(),
        )
        .unwrap();
    browser.activate_tab(right, SystemTime::now());
    browser.switch_space(0);
    let (view, visual, engine) = open(cx, browser);
    select(&view, visual, SidebarPage::Library);
    click(visual, "library-nav-spaces");
    let selector: &'static str =
        Box::leak(format!("library-{destination:?}/sidebar-pane-{}", left.get()).into_boxed_str());
    click(visual, selector);
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().id, destination);
        assert_eq!(view.browser.active_user_profile_id(), destination_profile);
        assert_eq!(view.browser.space().active_tab, Some(left));
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::Space(destination)
        );
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(pages.is_visible(left));
        assert!(pages.is_visible(right));
    });
    let state = engine.0.borrow();
    assert_eq!(
        state
            .focused
            .and_then(|view| state.urls.get(&view).map(String::as_str)),
        Some("https://destination.example/left")
    );
}

#[gpui::test]
fn external_reorder_repositions_a_retained_draft_when_underlying_space_index_is_unchanged(
    cx: &mut TestAppContext,
) {
    let mut browser = Browser::new();
    let origin = browser.space().id;
    browser.add_space("X", 90.);
    let underlying = browser.add_space("B", 180.);
    let (view, visual, _) = open(cx, browser);
    select(&view, visual, SidebarPage::CreateSpace);
    click(visual, "space-creation-name");
    visual.simulate_input("Retained A draft");
    select(&view, visual, SidebarPage::Space(underlying));
    select(&view, visual, SidebarPage::CreateSpace);
    click(visual, "space-creation-name");
    let form = view.read_with(visual, |view, _| {
        assert_eq!(view.browser.active_space, 2);
        assert_eq!(view.sidebar_page_index(), 2);
        assert_eq!(view.pager.position, 2.);
        view.sidebar_navigation.form.clone().unwrap()
    });
    let profile = view.read_with(visual, |view, _| view.profile.handle.clone());
    let peer = visual.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| BrowserWindow::with_profile(profile, None, None, window, cx))
        })
        .unwrap()
    });
    peer.update(&mut visual.cx, |peer, _, cx| {
        assert!(peer.edit_browser(|browser| browser.move_space(origin, 1)));
        cx.notify();
    })
    .unwrap();
    visual.run_until_parked();
    view.update(visual, |view, _| view.refresh_profile_projection());
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().id, underlying);
        assert_eq!(view.browser.active_space, 2);
        assert_eq!(
            view.sidebar_navigation.controller.selected(),
            SidebarPage::CreateSpace
        );
        assert_eq!(view.sidebar_page_index(), 3);
        assert_eq!(view.pager.position, 3.);
        assert_eq!(view.sidebar_navigation.form.as_ref(), Some(&form));
        let draft = view.sidebar_navigation.controller.request().unwrap();
        assert_eq!(draft.origin, origin);
        assert_eq!(draft.name, "Retained A draft");
    });
}

#[gpui::test]
fn composite_same_space_navigation_syncs_only_the_final_loaded_selection(cx: &mut TestAppContext) {
    let mut browser = Browser::new();
    let loaded = browser.new_tab("https://loaded.example/", SystemTime::now());
    let intermediate = browser.new_tab("https://intermediate.example/", SystemTime::now());
    browser.activate_tab(loaded, SystemTime::now());
    let (view, visual, engine) = open(cx, browser);
    let documents = engine.0.borrow().document_changes;
    let document = engine.0.borrow().focused.expect("loaded native document");
    view.update(visual, |view, cx| {
        // Core moves may select a neighboring tab before their caller restores
        // the captured drag/folder pane. Space selection must compose with it.
        view.edit_browser(|browser| browser.activate_tab(intermediate, SystemTime::now()));
        view.switch_space(0, cx);
        view.edit_browser(|browser| browser.activate_tab(loaded, SystemTime::now()));
        cx.notify();
    });
    visual.run_until_parked();
    assert_eq!(
        engine.0.borrow().document_changes,
        documents,
        "an intermediate selected neighbor must never create a document"
    );
    assert_eq!(engine.0.borrow().focused, Some(document));
    view.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().active_tab, Some(loaded));
        let pages = view.pages.as_ref().unwrap().borrow();
        assert!(pages.is_visible(loaded));
        assert!(!pages.is_visible(intermediate));
    });
}

fn click_without_waiting_for_settle(visual: &mut VisualTestContext, selector: &'static str) {
    visual.run_until_parked();
    let bounds = visual.debug_bounds(selector).expect("visible control");
    visual.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.run_until_parked();
}

#[gpui::test]
fn fixed_dock_and_favorites_accept_clicks_during_space_paging(cx: &mut TestAppContext) {
    let mut browser = crate::demo_browser();
    let favorite = browser.space().active_tab.unwrap();
    browser.favorite_tab(favorite);
    let (view, visual, _) = open(cx, browser);
    let dock = visual.debug_bounds("space-2").unwrap();
    let address = visual.debug_bounds("address").unwrap();
    visual.simulate_keystrokes("ctrl-2");
    assert_eq!(visual.debug_bounds("space-2"), Some(dock));
    assert_eq!(visual.debug_bounds("address"), Some(address));
    let selector: &'static str = Box::leak(format!("favorite-{}", favorite.get()).into_boxed_str());
    click_without_waiting_for_settle(visual, selector);
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.space().active_tab),
        Some(favorite)
    );
    click_without_waiting_for_settle(visual, "space-2");
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.active_space),
        2
    );
}

#[gpui::test]
fn creation_keeps_the_bottom_dock_and_its_draft_marker_with_contained_controls(
    cx: &mut TestAppContext,
) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    let origin = view.read_with(visual, |view, _| view.browser.space().id);
    select(&view, visual, SidebarPage::CreateSpace);
    visual.run_until_parked();
    let sidebar = visual.debug_bounds("sidebar").unwrap();
    let form = visual.debug_bounds("space-creation-form").unwrap();
    assert_eq!(form.size.width, sidebar.size.width);
    for selector in [
        "space-creation-name",
        "space-creation-profile-row",
        "space-creation-theme",
        "space-creation-submit",
        "space-creation-cancel",
    ] {
        let control = visual.debug_bounds(selector).unwrap();
        assert_eq!(control.left() - form.left(), px(14.));
        assert_eq!(form.right() - control.right(), px(13.));
    }
    assert_eq!(
        visual
            .debug_bounds("space-creation-art")
            .unwrap()
            .center()
            .x,
        form.center().x
    );
    for selector in ["space-creation-title", "space-creation-subtitle"] {
        assert_eq!(
            visual.debug_bounds(selector).unwrap().size.width,
            form.size.width
        );
    }
    for selector in [
        "space-creation-name",
        "space-creation-profile-row",
        "space-creation-theme",
        "space-creation-submit",
        "space-creation-cancel",
        "library",
        "space-0",
        "space-creation-mark",
        "new-space",
    ] {
        let control = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing {selector}"));
        assert!(
            control.left() >= sidebar.left() && control.right() <= sidebar.right(),
            "{selector} must fit the host viewport: {control:?} vs {sidebar:?}"
        );
        assert!(control.top() >= sidebar.top() && control.bottom() <= sidebar.bottom());
    }
    let draft_mark = visual.debug_bounds("space-creation-mark").unwrap();
    assert!(draft_mark.left() > visual.debug_bounds("space-0").unwrap().right());
    assert!(draft_mark.right() < visual.debug_bounds("space-1").unwrap().left());
    click_without_waiting_for_settle(visual, "space-0");
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .selected()),
        SidebarPage::Space(origin)
    );
    assert!(view.read_with(visual, |view, _| {
        view.sidebar_navigation.controller.draft().is_some()
    }));
}

#[gpui::test]
fn library_row_drags_choose_the_upper_and_lower_insertion_edges(cx: &mut TestAppContext) {
    for after in [false, true] {
        let mut browser = Browser::new();
        let space = browser.space().id;
        let first = browser.new_tab("https://first.example/", SystemTime::now());
        let target = browser.new_tab("https://target.example/", SystemTime::now());
        let source = browser.new_tab("https://source.example/", SystemTime::now());
        // New tabs are inserted at the front. Establish the intended order
        // explicitly so both pointer-edge assertions exercise that order.
        assert!(browser.move_tab_relative(first, target, false));
        assert!(browser.move_tab_relative(source, target, true));
        assert_eq!(
            browser
                .space()
                .today
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            vec![first, target, source]
        );
        let (view, visual, _) = open(cx, browser);
        select(&view, visual, SidebarPage::Library);
        click(visual, "library-nav-spaces");
        let source_selector: &'static str =
            Box::leak(format!("library-{space:?}/tab-{}", source.get()).into_boxed_str());
        let target_selector: &'static str =
            Box::leak(format!("library-{space:?}/tab-{}", target.get()).into_boxed_str());
        let start = visual.debug_bounds(source_selector).unwrap().center();
        visual.simulate_mouse_move(start, None, Modifiers::none());
        visual.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
        visual.simulate_mouse_move(
            start + gpui::point(px(5.), px(0.)),
            gpui::MouseButton::Left,
            Modifiers::none(),
        );
        visual.run_until_parked();
        let bounds = visual.debug_bounds(target_selector).unwrap();
        let end = gpui::point(
            bounds.center().x,
            if after {
                bounds.bottom() - px(4.)
            } else {
                bounds.top() + px(4.)
            },
        );
        visual.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        visual.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::none());
        visual.run_until_parked();
        let actual = view.read_with(visual, |view, _| {
            view.browser
                .space()
                .today
                .iter()
                .map(|tab| tab.id)
                .collect::<Vec<_>>()
        });
        let expected = if after {
            vec![first, target, source]
        } else {
            vec![first, source, target]
        };
        assert_eq!(
            actual, expected,
            "Library card insertion edge after={after}"
        );
    }
}

#[gpui::test]
fn virtual_pages_keep_root_window_controls_when_the_sidebar_was_hidden(cx: &mut TestAppContext) {
    let (view, visual, _) = open(cx, crate::demo_browser());
    visual.simulate_keystrokes("cmd-s");
    visual
        .executor()
        .advance_clock(std::time::Duration::from_secs(1));
    visual.run_until_parked();
    assert!(!view.read_with(visual, |view, _| view.sidebar_visible));
    select(&view, visual, SidebarPage::Library);
    let controls = visual
        .debug_bounds("traffic-lights")
        .expect("Library root window controls");
    assert_eq!(controls.left(), px(0.));
    assert_eq!(controls.top(), px(0.));
    assert_eq!(controls.size.width, px(74.));
    assert_eq!(
        view.read_with(visual, |view, _| view.window_controls_offset()),
        0.
    );
    select(&view, visual, SidebarPage::CreateSpace);
    assert_eq!(visual.debug_bounds("traffic-lights"), Some(controls));
    assert_eq!(
        view.read_with(visual, |view, _| view.window_controls_offset()),
        0.
    );
}

fn open_library_tab_for_rename(
    cx: &mut TestAppContext,
) -> (
    Entity<BrowserWindow>,
    &mut VisualTestContext,
    SpaceId,
    browser_core::TabId,
) {
    let mut browser = Browser::new();
    browser.new_tab("https://source.example/", SystemTime::now());
    let space = browser.add_space("Library card", 210.);
    let profile = browser.create_user_profile("Card data").unwrap();
    browser.assign_space_user_profile(space, profile).unwrap();
    browser.switch_space(1);
    let tab = browser.new_tab("https://card.example/", SystemTime::now());
    browser.set_title(tab, "Original card title");
    browser.switch_space(0);
    let (view, visual, _) = open(cx, browser);
    select(&view, visual, SidebarPage::Library);
    click(visual, "library-nav-spaces");
    (view, visual, space, tab)
}

fn rename_library_card_tab(
    view: &Entity<BrowserWindow>,
    visual: &mut VisualTestContext,
    space: SpaceId,
    tab: browser_core::TabId,
) {
    let selector: &'static str =
        Box::leak(format!("library-{space:?}/tab-{}", tab.get()).into_boxed_str());
    let at = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_move(at, None, Modifiers::none());
    visual.simulate_mouse_down(at, gpui::MouseButton::Right, Modifiers::none());
    visual.simulate_mouse_up(at, gpui::MouseButton::Right, Modifiers::none());
    visual.run_until_parked();
    let overlay = view.read_with(visual, |view, _| {
        view.tab_menu.as_ref().unwrap().overlay.unwrap()
    });
    click(
        &mut VisualTestContext::from_window(overlay.into(), visual),
        "tab-menu-rename",
    );
    visual.run_until_parked();
}

#[gpui::test]
fn library_context_rename_renders_only_its_captured_card_and_can_cancel_or_commit_once(
    cx: &mut TestAppContext,
) {
    use gpui::Focusable;
    let (view, visual, space, tab) = open_library_tab_for_rename(cx);
    rename_library_card_tab(&view, visual, space, tab);
    let selector: &'static str =
        Box::leak(format!("library-{space:?}/inline-tab-rename").into_boxed_str());
    let editor = visual
        .debug_bounds(selector)
        .expect("captured card editor is rendered");
    let card = visual.debug_bounds("library-space-body-1").unwrap();
    assert!(card.contains(&editor.origin) && card.contains(&editor.bottom_right()));
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .selected()),
        SidebarPage::Library
    );
    assert!(visual.update(|window, cx| {
        view.read(cx)
            .tab_rename
            .as_ref()
            .unwrap()
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    visual.simulate_input("Discarded name");
    visual.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.item_title(tab)),
        "Original card title"
    );
    rename_library_card_tab(&view, visual, space, tab);
    let before = session(&view, visual);
    visual.simulate_input("Committed card name");
    assert_eq!(
        session(&view, visual),
        before,
        "typing is local until commit"
    );
    visual.simulate_keystrokes("enter");
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.item_title(tab)),
        "Committed card name"
    );
    assert!(view.read_with(visual, |view, _| view.tab_rename.is_none()));
    let committed = session(&view, visual);
    visual.simulate_keystrokes("enter");
    assert_eq!(
        session(&view, visual),
        committed,
        "a disposed editor cannot commit twice"
    );
    assert_eq!(
        view.read_with(visual, |view, _| view
            .sidebar_navigation
            .controller
            .selected()),
        SidebarPage::Library
    );
}

#[gpui::test]
fn library_inline_rename_rejects_a_reassigned_card_owner(cx: &mut TestAppContext) {
    let (view, visual, space, tab) = open_library_tab_for_rename(cx);
    rename_library_card_tab(&view, visual, space, tab);
    visual.simulate_input("Stale owner name");
    view.update(visual, |view, cx| {
        let profile = view.browser.default_user_profile_id();
        view.edit_profile(|browser| browser.assign_space_user_profile(space, profile).unwrap());
        cx.notify();
    });
    visual.run_until_parked();
    assert!(view.read_with(visual, |view, _| view.tab_rename.is_none()));
    let reassigned = session(&view, visual);
    visual.simulate_keystrokes("enter");
    assert_eq!(session(&view, visual), reassigned);
    assert_eq!(
        view.read_with(visual, |view, _| view.browser.item_title(tab)),
        "Original card title"
    );
}

#[gpui::test]
fn library_inline_rename_enter_revalidates_the_authoritative_owner_before_notification_drain(
    cx: &mut TestAppContext,
) {
    let (view, visual, space, tab) = open_library_tab_for_rename(cx);
    let profile = view.read_with(visual, |view, _| view.profile.handle.clone());
    let peer = visual.cx.update(|cx| {
        use gpui::AppContext;
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| BrowserWindow::with_profile(profile, None, None, window, cx))
        })
        .unwrap()
    });
    rename_library_card_tab(&view, visual, space, tab);
    visual.simulate_input("Unnotified stale rename");
    visual.update(|window, cx| {
        let captured_profile = view.read(cx).tab_rename.as_ref().unwrap().profile;
        let reassigned = peer
            .update(cx, |peer, _, _| {
                let destination = peer.browser.default_user_profile_id();
                assert_ne!(captured_profile, destination);
                peer.edit_profile(|browser| {
                    browser
                        .assign_space_user_profile(space, destination)
                        .unwrap()
                });
                peer.profile.session_json(&peer.split_draft)
            })
            .unwrap();
        // No foreground/executor drain between the peer's authoritative edit
        // and Enter: A still paints the old profile and holds its old editor.
        assert_eq!(
            view.read(cx)
                .browser
                .spaces
                .iter()
                .find(|entry| entry.id == space)
                .unwrap()
                .user_profile_id,
            captured_profile
        );
        assert_eq!(
            view.read(cx).tab_rename.as_ref().unwrap().profile,
            captured_profile
        );
        window.dispatch_keystroke(
            gpui::Keystroke {
                modifiers: Modifiers::none(),
                key: "enter".into(),
                key_char: None,
            },
            cx,
        );
        assert!(
            view.read(cx).tab_rename.is_none(),
            "Enter must reach the existing inline editor handler"
        );
        assert_eq!(view.read(cx).browser.item_title(tab), "Original card title");
        let after = peer
            .update(cx, |peer, _, _| {
                peer.profile.session_json(&peer.split_draft)
            })
            .unwrap();
        assert_eq!(
            after, reassigned,
            "rejection must not rename or change canonical window selection"
        );
    });
}

#[gpui::test]
fn library_real_tab_rows_fill_the_card_and_short_titles_fit_without_clipping(
    cx: &mut TestAppContext,
) {
    let mut browser = Browser::new();
    let space = browser.space().id;
    let mut rows = Vec::new();
    for (index, title) in [
        "GitHub",
        "Gmail",
        "Home",
        "GPUI",
        "Arc",
        "A deliberately long title that requires ellipsis in this fixed Library card",
    ]
    .into_iter()
    .enumerate()
    {
        let tab = browser.new_tab(format!("https://row-{index}.example/"), SystemTime::now());
        browser.set_title(tab, title);
        if index < 2 {
            browser.toggle_pin(tab);
        }
        rows.push((tab, title));
    }
    let (view, visual, _) = open(cx, browser);
    select(&view, visual, SidebarPage::Library);
    click(visual, "library-nav-spaces");
    let body = visual.debug_bounds("library-space-body-0").unwrap();
    assert_eq!(body.size.width, px(224.));
    for pair in rows[2..].windows(2) {
        let selectors = pair.iter().map(|(tab, _)| Box::leak(format!("library-{space:?}/tab-{}", tab.get()).into_boxed_str()) as &'static str).collect::<Vec<_>>();
        let first = visual.debug_bounds(selectors[0]).unwrap();
        let second = visual.debug_bounds(selectors[1]).unwrap();
        assert_eq!((second.top() - first.top()).abs(), px(41.));
    }
    crate::sidebar_title::take_painted_titles();
    for (tab, title) in rows {
        let row_selector: &'static str =
            Box::leak(format!("library-{space:?}/tab-{}", tab.get()).into_boxed_str());
        let title_selector: &'static str = Box::leak(
            format!("library-{space:?}/sidebar-item-title-{}", tab.get()).into_boxed_str(),
        );
        let row = visual.debug_bounds(row_selector).unwrap();
        assert_eq!(
            row.size.width, body.size.width,
            "real Library row must fill the card: {title}"
        );
        visual.simulate_mouse_move(row.center(), None, Modifiers::none());
        visual.run_until_parked();
        visual.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let title_bounds = visual.debug_bounds(title_selector).unwrap();
        // Inspect the actual element's paint, not a separate shape_line estimate.
        let records = crate::sidebar_title::take_painted_titles();
        let painted = records.iter().rev().find(|record| record.original == title)
            .expect("real Library title must reach paint");
        assert_eq!(painted.width, title_bounds.size.width);
        assert_eq!(painted.len, painted.text.len());
        assert!(painted.shaped_width <= painted.width);
        if title.starts_with("A deliberately") {
            assert!(painted.text.ends_with('…'));
            assert_ne!(painted.text, title);
        } else {
            assert_eq!(painted.text, title);
            // ASCII fixtures have one glyph per character. This catches the
            // native shortened-run failure even when text/len report full input.
            assert_eq!(painted.glyph_indices, (0..title.len()).collect::<Vec<_>>());
        }

        let full_width = visual.update(|window, _| {
            let mut style = window.text_style().clone();
            style.font_weight = gpui::FontWeight::MEDIUM;
            window
                .text_system()
                .shape_line(title.into(), px(14.), &[style.to_run(title.len())], None)
                .width
        });
        if title.starts_with("A deliberately") {
            assert!(
                full_width > title_bounds.size.width,
                "long text keeps its full value and uses the existing ellipsis renderer"
            );
        } else {
            assert!(
                full_width <= title_bounds.size.width,
                "painted short title {title} needs {full_width:?}, got {title_bounds:?}"
            );
        }
        assert!(title_bounds.left() >= row.left() && title_bounds.right() <= row.right());
        assert_eq!(
            view.read_with(visual, |view, _| view.browser.item_title(tab)),
            title
        );
    }
}

#[gpui::test]
fn library_archive_route_search_and_filters_preserve_inventory_and_profile_scope(cx: &mut TestAppContext) {
    let mut browser = Browser::new();
    let profile = browser.active_user_profile_id();
    let first = browser.new_tab("https://example.org/needle", SystemTime::now());
    browser.set_title(first, "Archive needle");
    browser.close_tab(first, SystemTime::now());
    let second = browser.new_tab("https://example.org/other", SystemTime::now());
    browser.close_tab(second, SystemTime::now());
    browser.archive[1].reason = browser_core::ArchiveReason::Automatic;
    let other = browser.create_user_profile("Other").unwrap();
    let foreign_space = browser.add_space("Other Space", 120.);
    browser.assign_space_user_profile(foreign_space, other).unwrap();
    browser.switch_space(1);
    let foreign = browser.new_tab("https://private.example/needle", SystemTime::now());
    browser.close_tab(foreign, SystemTime::now());
    browser.switch_space(0);
    let (view, visual, _) = open(cx, browser);
    visual.simulate_resize(gpui::size(px(1000.), px(760.)));
    let before = session(&view, visual);
    select(&view, visual, SidebarPage::Library);
    click(visual, "library-nav-archive");
    assert!(visual.debug_bounds("library-archive").is_some());
    let search = visual.debug_bounds("library-archive-search").unwrap();
    assert_eq!(search.origin, gpui::point(px(120.), px(10.)));
    assert_eq!(search.size, gpui::size(px(213.), px(36.)));
    let first_row: &'static str = Box::leak(format!("library-archive-row-{}", first.get()).into_boxed_str());
    assert_eq!(visual.debug_bounds(first_row).unwrap().size.height, px(56.));
    let foreign_row: &'static str = Box::leak(format!("library-archive-row-{}", foreign.get()).into_boxed_str());
    assert!(visual.debug_bounds(foreign_row).is_none());
    click(visual, "library-archive-search");
    visual.simulate_input("needle");
    visual.run_until_parked();
    let visible_titles = |visual: &mut VisualTestContext| {
        crate::sidebar_title::take_painted_titles();
        visual.update(|window, cx| { let _ = window.draw(cx); });
        crate::sidebar_title::take_painted_titles().into_iter().map(|title| title.original).collect::<Vec<_>>()
    };
    let painted = visible_titles(visual);
    assert!(painted.iter().any(|title| title == "Archive needle"));
    assert!(!painted.iter().any(|title| title.contains("example.org/other")));
    click(visual, "library-archive-filter");
    click(visual, "library-archive-reason-0");
    assert!(!visible_titles(visual).iter().any(|title| title == "Archive needle"));
    click(visual, "library-archive-reason-0");
    click(visual, "archive-filter-done");
    assert!(visual.debug_bounds(first_row).is_some());
    click(visual, "library-nav-media");
    visual.update(|window, cx| view.update(cx, |view, cx| view.open_library_archived_tab(profile, first, "https://example.org/needle", window, cx)));
    assert!(view.read_with(visual, |view, _| view.peek_child.is_none()), "retired Archive rows cannot open a Peek");
    click(visual, "library-nav-archive");
    assert!(visual.debug_bounds(first_row).is_some());
    assert!(!visible_titles(visual).iter().any(|title| title.contains("example.org/other")), "retained search survives Library navigation");
    view.read_with(visual, |view, _| assert_eq!(view.browser.active_user_profile_id(), profile));
    assert_eq!(session(&view, visual), before);
}

#[gpui::test]
fn library_archive_hover_actions_preserve_selection_and_reject_stale_rows(cx: &mut TestAppContext) {
    use crate::library_archive::ArchiveAction;
    let mut browser = Browser::new();
    let now = SystemTime::now();
    let profile = browser.active_user_profile_id();
    let first = browser.new_tab("https://example.org/first", now);
    browser.set_title(first, "Fixture One");
    browser.close_tab(first, now);
    let second = browser.new_tab("https://example.org/second", now);
    browser.set_title(second, "Fixture Two");
    browser.close_tab(second, now);
    let other = browser.add_space("Other", 120.);
    browser.switch_space(1);
    let selected = browser.new_tab("https://selected.example/", now);
    let foreign = browser.create_user_profile("Foreign").unwrap();
    let (view, visual, _) = open(cx, browser);
    select(&view, visual, SidebarPage::Library);
    click(visual, "library-nav-archive");
    click(visual, "library-archive-search");
    visual.simulate_input("Fixture");
    visual.run_until_parked();
    let selector = |action: &str, id: browser_core::TabId| -> &'static str {
        Box::leak(format!("library-archive-{action}-{}", id.get()).into_boxed_str())
    };
    let hover_row = |visual: &mut VisualTestContext, id| {
        let row = visual.debug_bounds(selector("row", id)).unwrap();
        visual.simulate_mouse_move(row.center(), None, Modifiers::none());
        visual.run_until_parked();
    };
    hover_row(visual, second);
    let text = visual.debug_bounds(selector("text", second)).unwrap();
    let action = visual.debug_bounds(selector("delete", second)).unwrap();
    assert!(text.right() <= action.left(), "hover text cannot paint beneath Archive controls");
    click(visual, selector("delete", second));
    view.read_with(visual, |view, _| {
        assert!(!view.browser.archive.iter().any(|entry| entry.tab.id == second));
        assert!(view.peek_child.is_none(), "action clicks cannot bubble into row Peek");
        assert_eq!(view.browser.space().active_tab, Some(selected));
    });
    hover_row(visual, first);
    click(visual, selector("restore", first));
    visual.run_until_parked();
    view.read_with(visual, |view, _| {
        assert!(view.browser.archive.is_empty());
        assert_eq!(view.browser.space().id, other);
        assert_eq!(view.browser.space().active_tab, Some(selected));
        assert!(view.browser.spaces[0].today.iter().any(|tab| tab.id == first));
        assert_eq!(view.browser.spaces[0].active_tab, None);
        assert!(view.peek_child.is_none());
        assert_eq!(view.sidebar_navigation.controller.selected(), SidebarPage::Library);
    });
    assert!(visual.debug_bounds("library-archive-empty").is_some());
    let later = now + std::time::Duration::from_secs(1);
    view.update(visual, |view, cx| {
        view.edit_browser(|browser| { browser.switch_space(0); browser.close_tab(first, later); browser.switch_space(1); });
        view.apply_library_archive_action(profile, first, "https://example.org/first", now, ArchiveAction::Delete, cx);
        view.apply_library_archive_action(foreign, first, "https://example.org/first", later, ArchiveAction::Delete, cx);
    });
    view.read_with(visual, |view, _| assert_eq!(view.browser.archive.len(), 1));
    click(visual, "library-nav-media");
    view.update(visual, |view, cx| view.apply_library_archive_action(profile, first, "https://example.org/first", later, ArchiveAction::Delete, cx));
    view.read_with(visual, |view, _| assert_eq!(view.browser.archive.len(), 1));
}
