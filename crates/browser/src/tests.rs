//! Drives the real window through gpui's test platform: keystrokes and
//! clicks go through the same key bindings, focus and hit-testing as the app.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use engine::{Bounds, Event, Shortcut, ViewId};
use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};

use crate::pages::WebEngine;
use crate::window::{BrowserWindow, Suggestion};
use crate::{demo_browser, init};

fn open(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    open_with(cx, None)
}

fn open_with(
    cx: &mut TestAppContext,
    engine: Option<Box<dyn WebEngine>>,
) -> (Entity<BrowserWindow>, &mut VisualTestContext) {
    cx.update(init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(demo_browser(), engine, window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx)
}

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Create(u64, String),
    Visible(u64, bool),
    TopCorners(u64, bool),
    FocusPage(u64),
    FocusUi,
    Navigate(u64, String),
    Back(u64),
    Forward(u64),
    ForceReload(u64),
    StopLoading(u64),
    Mute(u64, bool),
    Zoom(u64, i32),
    Find(u64, String, bool, bool),
    StopFinding(u64),
    Reload(u64),
    Close(u64),
}

/// Records what the window asks of the engine; `events` are returned by
/// the next pump, as if the page reported them.
#[derive(Default)]
struct FakeState {
    calls: Vec<Call>,
    /// Latest bounds given to each view.
    bounds: std::collections::HashMap<u64, Bounds>,
    rounded_top: std::collections::HashMap<u64, bool>,
    next_view: u64,
    events: Vec<Event>,
    muted: std::collections::HashSet<u64>,
}

#[derive(Clone, Default)]
struct FakeEngine(Rc<RefCell<FakeState>>);

impl FakeEngine {
    fn take_calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.0.borrow_mut().calls)
    }

    fn send(&self, cx: &mut VisualTestContext, event: Event) {
        self.0.borrow_mut().events.push(event);
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
    }

    fn call(&self, call: Call) {
        self.0.borrow_mut().calls.push(call);
    }
}

impl WebEngine for FakeEngine {
    fn create(&mut self, url: &str, bounds: Bounds) -> Option<ViewId> {
        let mut state = self.0.borrow_mut();
        state.next_view += 1;
        let id = state.next_view;
        state.bounds.insert(id, bounds);
        state.rounded_top.insert(id, true);
        state.calls.push(Call::Create(id, url.to_string()));
        Some(ViewId::from_raw(id))
    }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds) {
        self.0.borrow_mut().bounds.insert(raw(view), bounds);
    }
    fn set_top_corners_rounded(&mut self, view: ViewId, rounded: bool) {
        let mut state = self.0.borrow_mut();
        if state.rounded_top.insert(raw(view), rounded) != Some(rounded) {
            state.calls.push(Call::TopCorners(raw(view), rounded));
        }
    }
    fn set_visible(&mut self, view: ViewId, visible: bool) {
        self.call(Call::Visible(raw(view), visible));
    }
    fn focus_page(&mut self, view: ViewId) {
        self.call(Call::FocusPage(raw(view)));
    }
    fn focus_ui(&mut self) {
        self.call(Call::FocusUi);
    }
    fn navigate(&mut self, view: ViewId, url: &str) {
        self.call(Call::Navigate(raw(view), url.to_string()));
    }
    fn go_back(&mut self, view: ViewId) {
        self.call(Call::Back(raw(view)));
    }
    fn go_forward(&mut self, view: ViewId) { self.call(Call::Forward(raw(view))); }
    fn reload(&mut self, view: ViewId) {
        self.call(Call::Reload(raw(view)));
    }
    fn force_reload(&mut self, view: ViewId) { self.call(Call::ForceReload(raw(view))); }
    fn stop_loading(&mut self, view: ViewId) { self.call(Call::StopLoading(raw(view))); }
    fn set_audio_muted(&mut self, view: ViewId, muted: bool) {
        if muted { self.0.borrow_mut().muted.insert(raw(view)); } else { self.0.borrow_mut().muted.remove(&raw(view)); }
        self.call(Call::Mute(raw(view), muted));
    }
    fn audio_muted(&self, view: ViewId) -> Option<bool> { Some(self.0.borrow().muted.contains(&raw(view))) }
    fn zoom(&mut self, view: ViewId, direction: i32) { self.call(Call::Zoom(raw(view), direction)); }
    fn find(&mut self, view: ViewId, text: &str, forward: bool, next: bool) { self.call(Call::Find(raw(view), text.into(), forward, next)); }
    fn stop_finding(&mut self, view: ViewId) { self.call(Call::StopFinding(raw(view))); }
    fn close(&mut self, view: ViewId) {
        self.call(Call::Close(raw(view)));
    }
    fn pump_fn(&self) -> fn() {
        || {}
    }
    fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.0.borrow_mut().events)
    }
}

fn raw(view: ViewId) -> u64 {
    format!("{view:?}")
        .trim_start_matches("ViewId(")
        .trim_end_matches(')')
        .parse()
        .unwrap()
}

fn page(id: u64) -> ViewId {
    ViewId::from_raw(id)
}

// GPUI's test platform requires static selectors, including generated tab ids.
fn test_selector(selector: String) -> &'static str { Box::leak(selector.into_boxed_str()) }

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("`{selector}` is not on screen"));
    // Hover first, like a real pointer: some controls only appear on hover.
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// The Space bar plus opens the New menu; New Space opens the name editor.
fn open_new_space_from_menu(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    click(cx, "new-space");
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.new_menu.as_ref().and_then(|menu| menu.overlay)).expect("new menu is open");
    click(&mut VisualTestContext::from_window(overlay.into(), cx), "new-menu-space");
}

fn today_urls(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |w, _| {
        w.browser.space().today.iter().map(|t| t.url.clone()).collect()
    })
}

fn active_url(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |w, _| w.browser.active_tab().map(|t| t.url.clone()))
}

fn bar_text(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |w, cx| {
        w.command_bar
            .as_ref()
            .map(|bar| bar.input.read(cx).text().to_string())
    })
}

/// The command bar's overlay window, for typing and clicking into the bar.
fn bar(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view
        .read_with(cx, |w, _| w.command_bar.as_ref().and_then(|bar| bar.overlay))
        .expect("command bar overlay is not open");
    VisualTestContext::from_window(overlay.into(), cx)
}

fn bar_input_focused(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> bool {
    bar(view, cx).update(|window, cx| {
        let w = view.read(cx);
        let bar = w.command_bar.as_ref().expect("command bar is closed");
        bar.input.read(cx).focus_handle(cx).is_focused(window)
    })
}

#[gpui::test]
fn cmd_t_opens_command_bar_instead_of_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);

    cx.simulate_keystrokes("cmd-t");

    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn typing_an_address_and_enter_opens_it_in_a_new_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let count = today_urls(&view, cx).len();

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("github.com/zed-industries/zed");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("github.com/zed-industries/zed"));
    bar(&view, cx).simulate_keystrokes("enter");

    assert_eq!(bar_text(&view, cx), None);
    let urls = today_urls(&view, cx);
    assert_eq!(urls.len(), count + 1);
    assert_eq!(urls[0], "https://github.com/zed-industries/zed");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com/zed-industries/zed"));
}

#[gpui::test]
fn typing_words_searches_google(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("как дела");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://www.google.com/search?q=%D0%BA%D0%B0%D0%BA+%D0%B4%D0%B5%D0%BB%D0%B0")
    );
    let title = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone());
    assert_eq!(title, "как дела");
}

#[gpui::test]
fn backspace_and_arrows_edit_the_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("helo");
    bar(&view, cx).simulate_keystrokes("left");
    bar(&view, cx).simulate_input("l");
    bar(&view, cx).simulate_keystrokes("end backspace backspace");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("hel"));
}

#[gpui::test]
fn escape_closes_without_opening_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("something");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(today_urls(&view, cx), before);

    // Window shortcuts work again once the bar is gone.
    cx.simulate_keystrokes("cmd-w");
    assert_eq!(today_urls(&view, cx).len(), before.len() - 1);
}

#[gpui::test]
fn arrow_down_and_enter_switch_to_a_matching_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("gpui");

    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(suggestions[0], Suggestion::Load(_)));
    assert!(matches!(&suggestions[1], Suggestion::SwitchToTab { url, .. } if url == "https://www.gpui.rs"));

    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn cmd_l_edits_the_current_tab_url(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx).len();
    let current = active_url(&view, cx).unwrap();

    cx.simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), Some(current));
    // The URL is selected, so typing replaces it.
    bar(&view, cx).simulate_input("example.com");
    bar(&view, cx).simulate_keystrokes("enter");

    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com"));
    assert_eq!(today_urls(&view, cx).len(), before);
}

#[gpui::test]
fn clicking_new_tab_opens_command_bar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    click(cx, "new-tab");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("rust");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some("https://www.google.com/search?q=rust")
    );
}

#[gpui::test]
fn clicking_a_suggestion_runs_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("chromium");
    click(&mut bar(&view, cx), "suggestion-1");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn clicking_outside_dismisses_but_inside_does_not(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    click(&mut bar(&view, cx), "command-bar-input");
    assert!(bar_text(&view, cx).is_some());
    assert!(bar_input_focused(&view, cx));

    let mut overlay = bar(&view, cx);
    let backdrop = overlay.debug_bounds("command-bar-backdrop").unwrap();
    overlay.simulate_click(
        backdrop.bottom_right() - gpui::point(gpui::px(20.), gpui::px(20.)),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn clicking_tabs_activates_and_closes_them(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (first, last) = view.read_with(cx, |w, _| {
        let today = &w.browser.space().today;
        (today[0].id.get(), today.last().unwrap().id.get())
    });
    let tab: &'static str = Box::leak(format!("tab-{last}").into_boxed_str());
    click(cx, tab);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), last);

    let before = today_urls(&view, cx).len();
    let close: &'static str = Box::leak(format!("close-{first}").into_boxed_str());
    click(cx, close);
    assert_eq!(today_urls(&view, cx).len(), before - 1);
}

#[gpui::test]
fn space_shortcuts_and_dots_switch_spaces(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 1);
    click(cx, "space-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 2);
    cx.simulate_keystrokes("cmd-alt-right");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    cx.simulate_keystrokes("cmd-alt-left");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 2);
}

#[gpui::test]
fn cmd_alt_arrows_and_cmd_digits_pick_tabs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    // Sidebar: GitHub, Gmail | Chromium, GPUI, Arc is shutting down.
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
    cx.simulate_keystrokes("cmd-alt-down");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://mail.google.com"));
    cx.simulate_keystrokes("cmd-alt-up cmd-alt-up");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://thebrowser.company"));
    cx.simulate_keystrokes("cmd-3");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    cx.simulate_keystrokes("cmd-9");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://thebrowser.company"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
}

#[gpui::test]
fn cmd_w_archives_and_cmd_shift_t_restores(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    let closed = active_url(&view, cx).unwrap();
    cx.simulate_keystrokes("cmd-w");
    assert!(!today_urls(&view, cx).contains(&closed));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);

    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(active_url(&view, cx), Some(closed));
    assert_eq!(today_urls(&view, cx).len(), before.len());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 0);
}

#[gpui::test]
fn cmd_shift_k_clears_today(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let count = today_urls(&view, cx).len();
    cx.simulate_keystrokes("cmd-shift-k");
    assert!(today_urls(&view, cx).is_empty());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), count);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
}

#[gpui::test]
fn closing_a_pinned_tab_keeps_it_pinned(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let pinned = view.read_with(cx, |w, _| w.browser.space().pinned[0].id.get());
    let close: &'static str = Box::leak(format!("close-{pinned}").into_boxed_str());
    click(cx, close);
    let urls: Vec<String> = view.read_with(cx, |w, _| {
        w.browser.space().pinned.iter().map(|t| t.url.clone()).collect()
    });
    assert_eq!(urls, vec!["https://github.com", "https://mail.google.com"]);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
}

#[gpui::test]
fn the_session_is_saved_and_restored(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("browser-session-test-{}", std::process::id()));
    let path = dir.join("session.json");
    let _ = std::fs::remove_dir_all(&dir);

    let (view, cx) = open(cx);
    view.update(cx, |w, cx| w.save_session_to(path.clone(), cx));
    cx.simulate_keystrokes("ctrl-2");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();

    let restored = browser_core::session::load(&path).unwrap().expect("session saved");
    assert_eq!(restored.active_space, 1);
    assert_eq!(restored.spaces.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn cmd_s_hides_the_sidebar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("sidebar-clip").unwrap().size.width, gpui::px(0.));
}

// Web pages, through the fake engine.

fn open_pages(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    let fake = FakeEngine::default();
    let (view, cx) = open_with(cx, Some(Box::new(fake.clone())));
    (view, cx, fake)
}

#[gpui::test]
fn the_active_tab_gets_a_visible_focused_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let url = active_url(&view, cx).unwrap();
    assert_eq!(
        fake.take_calls(),
        vec![Call::Create(1, url), Call::Visible(1, true), Call::FocusPage(1)]
    );
}

fn active_page_status(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |w, _| {
        w.pages.as_ref().unwrap().borrow().state(w.browser.active_tab().unwrap().id).status
    })
}

#[gpui::test]
fn hovered_link_events_preserve_the_exact_target_and_do_not_change_page_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    let target = "https://example.com/a?utm_source=hover&token=a%2Fb#section";
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    assert_eq!(active_page_status(&view, cx), target);
    assert!(fake.take_calls().is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: String::new() });
    assert!(active_page_status(&view, cx).is_empty());
    assert!(fake.take_calls().is_empty());
}

#[gpui::test]
fn hovered_links_are_cleared_on_navigation_and_cannot_leak_from_hidden_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let target = "https://example.com/target";
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    fake.send(cx, Event::LoadingChanged { view: page(1), loading: true, can_go_back: false, can_go_forward: false });
    assert!(active_page_status(&view, cx).is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/new".into() });
    assert!(active_page_status(&view, cx).is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    click(cx, "tab-7");
    assert!(active_page_status(&view, cx).is_empty());
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.into() });
    fake.send(cx, Event::StatusChanged { view: page(999), text: target.into() });
    assert!(active_page_status(&view, cx).is_empty());
    click(cx, "tab-8");
    assert!(active_page_status(&view, cx).is_empty(), "returning to a tab needs a fresh pointer event");
}

#[gpui::test]
fn link_status_expands_after_a_dwell_and_restarts_the_timer_for_a_new_target(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    let target = format!("https://example.com/{}", "long-path/".repeat(50));
    fake.send(cx, Event::StatusChanged { view: page(1), text: target.clone() });
    let overlay = view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().overlay.unwrap());
    let frame = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_bounds()[0]);
    let mut status_window = VisualTestContext::from_window(overlay.into(), cx);
    let preview = status_window.debug_bounds("link-status").unwrap();
    assert!(preview.size.width <= gpui::px(360.));
    assert!(preview.origin.x >= gpui::px(frame.x as f32) && preview.bottom_right().y <= gpui::px((frame.y + frame.height) as f32), "{preview:?}, page {frame:?}");
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    fake.send(cx, Event::StatusChanged { view: page(1), text: format!("{target}next") });
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(!view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().expanded));
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().expanded));
    let expanded = status_window.debug_bounds("link-status").unwrap();
    assert!(expanded.size.width > preview.size.width);
    assert!(expanded.bottom_right().x <= gpui::px((frame.x + frame.width) as f32));
    assert!(fake.take_calls().is_empty(), "passive status must not affect the page or its focus");
    cx.simulate_keystrokes("cmd-t");
    assert!(view.read_with(cx, |w, _| w.link_status.is_none()));
    assert!(bar_input_focused(&view, cx));
    fake.send(cx, Event::StatusChanged { view: page(1), text: target });
    assert!(view.read_with(cx, |w, _| w.link_status.is_none()));
}

#[gpui::test]
fn link_status_uses_the_hovered_split_pane_without_activating_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let active = active_url(&view, cx);
    fake.take_calls();
    fake.send(cx, Event::StatusChanged { view: page(1), text: "https://example.com/left".into() });
    assert_eq!(active_url(&view, cx), active);
    let overlay = view.read_with(cx, |w, _| w.link_status.as_ref().unwrap().overlay.unwrap());
    let mut status_window = VisualTestContext::from_window(overlay.into(), cx);
    let bounds = status_window.debug_bounds("link-status").unwrap();
    let first = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_bounds()[0]);
    assert!(bounds.origin.x >= gpui::px(first.x as f32) && bounds.bottom_right().x <= gpui::px((first.x + first.width) as f32));
    fake.send(cx, Event::StatusChanged { view: page(2), text: String::new() });
    assert!(view.read_with(cx, |w, _| w.link_status.is_some()), "another pane's empty status cannot clear this target");
    assert!(fake.take_calls().is_empty());
    cx.simulate_mouse_move(gpui::point(gpui::px(30.), gpui::px(200.)), None, Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.link_status.is_none()));
}

#[gpui::test]
fn the_page_stays_visible_under_the_command_bar(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    cx.simulate_keystrokes("cmd-t");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(fake.take_calls(), vec![], "the page is neither hidden nor unfocused");

    bar(&view, cx).simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(fake.take_calls(), vec![Call::FocusPage(1)]);
}

#[gpui::test]
fn opening_a_url_creates_a_page_for_the_new_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("zed.dev");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();

    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Create(2, "https://zed.dev".into())), "{calls:?}");
    assert_eq!(calls.last(), Some(&Call::FocusPage(2)));
}

#[gpui::test]
fn cmd_l_loads_the_new_url_in_the_same_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("example.com");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();

    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Navigate(1, "https://example.com".into())), "{calls:?}");
    assert!(!calls.iter().any(|c| matches!(c, Call::Create(..))), "{calls:?}");
}

#[gpui::test]
fn switching_tabs_swaps_pages_and_closing_destroys_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    let (first, last, last_url) = view.read_with(cx, |w, _| {
        let today = &w.browser.space().today;
        let last = today.last().unwrap();
        (today[0].id.get(), last.id.get(), last.url.clone())
    });

    let tab: &'static str = Box::leak(format!("tab-{last}").into_boxed_str());
    click(cx, tab);
    assert_eq!(
        fake.take_calls(),
        vec![
            Call::Visible(1, false),
            Call::FocusUi,
            Call::Create(2, last_url),
            Call::Visible(2, true),
            Call::FocusPage(2),
        ]
    );

    let close: &'static str = Box::leak(format!("close-{first}").into_boxed_str());
    click(cx, close);
    assert_eq!(fake.take_calls(), vec![Call::Close(1)]);
}

#[gpui::test]
fn closing_a_pinned_tab_unloads_its_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();
    fake.take_calls();

    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(2)), "{calls:?}");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 2);

    // Focus moved to the next pinned tab (page 3); reopening GitHub loads
    // its pinned page again.
    cx.simulate_keystrokes("cmd-1");
    cx.run_until_parked();
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Create(4, "https://github.com".into())), "{calls:?}");
}

#[gpui::test]
fn page_reports_update_the_sidebar_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();

    fake.send(cx, Event::TitleChanged { view: page(1), title: "Chromium Home".into() });
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://www.chromium.org/home".into() });

    let tab = view.read_with(cx, |w, _| w.browser.active_tab().cloned().unwrap());
    assert_eq!(tab.title, "Chromium Home");
    assert_eq!(tab.url, "https://www.chromium.org/home");
    assert_eq!(fake.take_calls(), vec![], "redirects must not trigger a reload");
}

#[gpui::test]
fn popups_open_as_new_tabs(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    fake.send(cx, Event::OpenInNewTab { opener: page(1), url: "https://docs.rs".into() });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://docs.rs"));
    assert!(fake.take_calls().contains(&Call::Create(2, "https://docs.rs".into())));
}

#[gpui::test]
fn shortcuts_pressed_inside_a_page_reach_the_browser(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let shortcut = |key: &str| Shortcut { cmd: true, key: key.into(), ..Default::default() };

    fake.send(cx, Event::Shortcut(shortcut("t")));
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_keystrokes("escape");
    fake.take_calls();

    fake.send(cx, Event::Shortcut(shortcut("[")));
    fake.send(cx, Event::Shortcut(shortcut("r")));
    assert_eq!(fake.take_calls(), vec![Call::Back(1), Call::Reload(1)]);
}

#[test]
fn every_window_shortcut_is_known_to_the_engine() {
    let shortcuts = crate::window::page_shortcuts();
    assert_eq!(shortcuts.len(), crate::window::WINDOW_SHORTCUTS.len());
    assert!(shortcuts.contains(&Shortcut { cmd: true, key: "t".into(), ..Default::default() }));
    assert!(shortcuts.contains(&Shortcut { cmd: true, alt: true, key: "down".into(), ..Default::default() }));
    assert!(shortcuts.contains(&Shortcut { ctrl: true, key: "1".into(), ..Default::default() }));
    assert!(shortcuts.contains(&Shortcut { ctrl: true, shift: true, key: "tab".into(), ..Default::default() }));
}

#[gpui::test]
fn long_titles_do_not_widen_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let title = "A very long page title that is much wider than the window ".repeat(20);
    fake.send(cx, Event::TitleChanged { view: page(1), title });
    cx.run_until_parked();

    let window_width = f64::from(f32::from(cx.update(|window, _| window.viewport_size().width)));
    let bounds = fake.0.borrow().bounds[&1];
    let sidebar = view.read_with(cx, |w, _| w.sidebar_visible);
    assert!(sidebar);
    assert_eq!(
        (bounds.x, bounds.y, bounds.width),
        (240.0, 8.0, window_width - 240.0 - 8.0),
        "page must exactly cover the content card"
    );
}

#[gpui::test]
fn ctrl_tab_cycles_through_tabs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let order: Vec<String> = view.read_with(cx, |w, _| {
        let space = w.browser.space();
        space.pinned.iter().chain(&space.today).map(|t| t.url.clone()).collect()
    });
    let start = order.iter().position(|u| Some(u) == active_url(&view, cx).as_ref()).unwrap();

    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_url(&view, cx), Some(order[(start + 1) % order.len()].clone()));
    cx.simulate_keystrokes("ctrl-shift-tab ctrl-shift-tab");
    assert_eq!(active_url(&view, cx), Some(order[(start + order.len() - 1) % order.len()].clone()));
}

fn swipe(cx: &mut VisualTestContext, over: &'static str, dx: f32, steps: usize) {
    use gpui::{point, px, ScrollDelta, ScrollWheelEvent, TouchPhase};
    cx.run_until_parked();
    let position = cx.debug_bounds(over).unwrap().center();
    for i in 0..=steps {
        let touch_phase = match i {
            0 => TouchPhase::Started,
            i if i == steps => TouchPhase::Ended,
            _ => TouchPhase::Moved,
        };
        let delta = if i == 0 { 0.0 } else { dx / steps as f32 };
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(delta), px(0.5))),
            modifiers: Modifiers::none(),
            touch_phase,
        });
    }
    // Momentum after the fingers lift must not switch again.
    for _ in 0..5 {
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(dx / steps as f32), px(0.))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
        });
    }
    cx.run_until_parked();
}

#[gpui::test]
fn swiping_the_sidebar_switches_one_space_per_gesture(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let space = |cx: &mut VisualTestContext| view.read_with(cx, |w, _| w.browser.active_space);

    swipe(cx, "sidebar", -200.0, 10);
    assert_eq!(space(cx), 1);
    // Also over the tab list, which scrolls vertically.
    swipe(cx, "new-tab", -200.0, 10);
    assert_eq!(space(cx), 2);
    swipe(cx, "sidebar", -200.0, 10);
    assert_eq!(space(cx), 2, "no wrap past the last space");
    swipe(cx, "sidebar", 200.0, 10);
    assert_eq!(space(cx), 1);
    // A short or mostly vertical gesture does nothing.
    swipe(cx, "sidebar", -30.0, 3);
    assert_eq!(space(cx), 1);
}

fn scroll(cx: &mut VisualTestContext, over: &'static str, phase: gpui::TouchPhase, dx: f32, dy: f32) {
    use gpui::{point, px, ScrollDelta, ScrollWheelEvent};
    let position = cx.debug_bounds(over).unwrap().center();
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        modifiers: Modifiers::none(),
        touch_phase: phase,
    });
}

#[gpui::test]
fn the_sidebar_follows_the_fingers_and_settles_on_release(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    let state = |cx: &mut VisualTestContext| view.read_with(cx, |w, _| (w.pager.position, w.browser.active_space));

    scroll(cx, "sidebar", Started, 0.0, 0.0);
    for _ in 0..6 {
        scroll(cx, "sidebar", Moved, -20.0, 0.0);
    }
    // Halfway through 120 of 240 px: between spaces, nothing switched yet.
    let (position, active) = state(cx);
    assert!((position - 0.5).abs() < 0.01, "{position}");
    assert_eq!(active, 0);

    scroll(cx, "sidebar", Ended, 0.0, 0.0);
    assert_eq!(state(cx).1, 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(state(cx), (1.0, 1));
}

#[gpui::test]
fn a_small_drag_springs_back(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    scroll(cx, "sidebar", Started, 0.0, 0.0);
    scroll(cx, "sidebar", Moved, -30.0, 0.0);
    scroll(cx, "sidebar", Ended, 0.0, 0.0);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| (w.pager.position, w.browser.active_space)), (0.0, 0));
}

#[gpui::test]
fn a_fast_short_sidebar_flick_commits_by_velocity(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    scroll(cx, "sidebar", Started, 0., 0.);
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    // 30px is below the distance threshold, but 667px/s is a flick.
    scroll(cx, "sidebar", Ended, 0., 0.);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 1);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.pager.position), 1.);
}

#[gpui::test]
fn pausing_before_releasing_a_short_sidebar_drag_cancels_its_flick_velocity(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    scroll(cx, "sidebar", Started, 0., 0.);
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(15));
    scroll(cx, "sidebar", Moved, -10., 0.);
    cx.executor().advance_clock(Duration::from_millis(60));
    scroll(cx, "sidebar", Ended, 0., 0.);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.pager.position), 0.);
}

#[gpui::test]
fn dragging_past_the_ends_resists(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    scroll(cx, "sidebar", Started, 0.0, 0.0);
    scroll(cx, "sidebar", Moved, 120.0, 0.0); // towards a space before the first
    let position = view.read_with(cx, |w, _| w.pager.position);
    assert!((position + 0.15).abs() < 0.01, "{position}");
}

#[gpui::test]
fn vertical_scrolling_does_not_page(cx: &mut TestAppContext) {
    use gpui::TouchPhase::*;
    let (view, cx) = open(cx);
    cx.run_until_parked();
    scroll(cx, "sidebar", Started, 0.0, 0.0);
    scroll(cx, "sidebar", Moved, -2.0, 10.0);
    scroll(cx, "sidebar", Moved, -200.0, 0.0);
    scroll(cx, "sidebar", Ended, 0.0, 0.0);
    assert_eq!(view.read_with(cx, |w, _| (w.pager.position, w.browser.active_space)), (0.0, 0));
}

#[gpui::test]
fn keyboard_space_switches_animate_the_pager(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-3");
    let mid = view.read_with(cx, |w, _| w.pager.position);
    assert!(mid < 2.0, "starts animating from the old space");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.pager.position), 2.0);
}

#[gpui::test]
fn long_input_scrolls_inside_the_command_bar(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    let mut overlay = bar(&view, cx);
    overlay.simulate_input(&"sdfg".repeat(60));
    overlay.run_until_parked();

    let panel = overlay.debug_bounds("command-bar").unwrap();
    let field = overlay.debug_bounds("command-bar-input").unwrap();
    assert!(field.right() <= panel.right(), "field {field:?} sticks out of {panel:?}");
    let scroll = view.read_with(cx, |w, cx| w.command_bar.as_ref().unwrap().input.read(cx).scroll_x());
    assert!(scroll > gpui::px(0.), "the cursor at the end is scrolled into view");

    overlay.simulate_keystrokes("home");
    overlay.run_until_parked();
    let scroll = view.read_with(cx, |w, cx| w.command_bar.as_ref().unwrap().input.read(cx).scroll_x());
    assert_eq!(scroll, gpui::px(0.));
}

fn floating(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<VisualTestContext> {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.floating_sidebar.as_ref().and_then(|f| f.overlay))?;
    Some(VisualTestContext::from_window(overlay.into(), cx))
}

fn move_mouse(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_move(gpui::point(gpui::px(x), gpui::px(y)), None, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_hidden_sidebar_floats_in_from_the_left_edge(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 300.0, 300.0);
    assert!(floating(&view, cx).is_none(), "only the edge reveals it");

    move_mouse(cx, 2.0, 300.0);
    let mut overlay = floating(&view, cx).expect("floating sidebar opens");
    let panel = overlay.debug_bounds("floating-sidebar").unwrap();

    // Over the panel, then away: it slides out and its window goes.
    move_mouse(&mut overlay, 100.0, 300.0);
    // Clicking a tab in it works like in the sidebar.
    assert!(overlay.debug_bounds("new-tab").is_some());
    move_mouse(cx, f32::from(panel.right()) + 360.0, 300.0);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.floating_sidebar.is_none()));
}

#[gpui::test]
fn a_revealed_sidebar_hides_if_never_entered(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 2.0, 300.0);
    assert!(floating(&view, cx).is_some());
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.floating_sidebar.is_none()));
}

#[gpui::test]
fn new_tab_from_the_floating_sidebar_uses_the_browser_window_bounds(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 2.0, 300.0);
    let mut sidebar = floating(&view, cx).unwrap();
    // GPUI's element Animation uses std::time::Instant, not the test clock.
    std::thread::sleep(Duration::from_millis(200));
    sidebar.update(|_, cx| cx.refresh_windows());
    sidebar.run_until_parked();
    move_mouse(&mut sidebar, 100.0, 300.0);
    click(&mut sidebar, "new-tab");
    let mut popup = bar(&view, cx);
    let panel = popup.debug_bounds("command-bar").unwrap();
    assert!(panel.size.width > gpui::px(500.), "popup should fit the full browser window: {panel:?}");
    assert!(panel.origin.x >= gpui::px(0.));
    popup.simulate_input("example.com/from-floating-sidebar");
    popup.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/from-floating-sidebar"));
}

#[gpui::test]
fn showing_the_sidebar_again_removes_the_floating_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    move_mouse(cx, 2.0, 300.0);
    assert!(floating(&view, cx).is_some());
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_visible && w.floating_sidebar.is_none()));
}

#[gpui::test]
fn command_bar_runs_actions_and_preserves_search(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx).len();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin tab");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&suggestions[0], Suggestion::Action { label, .. } if label == "Pin Tab"));
    assert!(suggestions.iter().any(|s| matches!(s, Suggestion::Load(_))));
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().pinned_url.is_some()));
    assert_eq!(today_urls(&view, cx).len(), before - 1);
    assert_eq!(bar_text(&view, cx), None);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unpin tab");
    click(&mut bar(&view, cx), "suggestion-0");
    assert_eq!(today_urls(&view, cx).len(), before);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("hide sidebar");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
    cx.simulate_keystrokes("cmd-s");
    assert!(view.read_with(cx, |w, _| w.sidebar_visible));
}

#[gpui::test]
fn command_bar_switches_spaces_by_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("switch to research");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 2);
    assert!(today_urls(&view, cx).is_empty());
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn command_bar_finds_and_focuses_tabs_in_other_spaces(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-2 cmd-t");
    bar(&view, cx).simulate_input("gpui");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&suggestions[1], Suggestion::SwitchToTab { space, .. } if space == "Personal"));
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(today_urls(&view, cx).len(), 3);
}

#[gpui::test]
fn unavailable_command_is_not_offered_and_escape_does_not_execute(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("restore last closed tab");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::Action { .. }))));
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("clear today");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(today_urls(&view, cx).len(), 3);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("clear today");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(today_urls(&view, cx).is_empty());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reopen closed tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(today_urls(&view, cx).len(), 1);
}


#[gpui::test]
fn page_controls_work_from_ui_and_from_page_focus(cx: &mut TestAppContext) {
    let (_, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-shift-r cmd-. cmd-= cmd-- cmd-0 cmd-left cmd-right");
    assert_eq!(fake.take_calls(), vec![Call::ForceReload(1), Call::StopLoading(1), Call::Zoom(1, 1), Call::Zoom(1, -1), Call::Zoom(1, 0), Call::Back(1), Call::Forward(1)]);
    for (key, shift) in [("r", true), (".", false), ("=", false), ("-", false), ("0", false)] {
        fake.send(cx, Event::Shortcut(Shortcut { cmd: true, shift, key: key.into(), ..Default::default() }));
    }
    assert_eq!(fake.take_calls(), vec![Call::ForceReload(1), Call::StopLoading(1), Call::Zoom(1, 1), Call::Zoom(1, -1), Call::Zoom(1, 0)]);
}

#[gpui::test]
fn page_controls_are_available_by_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    for (query, expected) in [("hard reload", Call::ForceReload(1)), ("stop loading", Call::StopLoading(1)), ("zoom in", Call::Zoom(1, 1)), ("zoom out", Call::Zoom(1, -1)), ("reset zoom", Call::Zoom(1, 0))] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        assert!(fake.take_calls().contains(&expected), "{query}");
    }
}


#[gpui::test]
fn clear_button_archives_today_without_touching_pinned_or_other_spaces(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    // Tidy and Clear appear only while the pointer is over the sidebar.
    move_mouse(cx, 600., 400.);
    assert!(!view.read_with(cx, |w, _| w.sidebar_hovered));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    move_mouse(cx, f32::from(sidebar.center().x), f32::from(sidebar.center().y));
    assert!(view.read_with(cx, |w, _| w.sidebar_hovered));
    assert!(cx.debug_bounds("tidy-today").is_some());
    click(cx, "clear-today");
    assert!(today_urls(&view, cx).is_empty());
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 2);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 3);

    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(today_urls(&view, cx).len(), 1);
    cx.run_until_parked();
    assert!(cx.debug_bounds("clear-today").is_some());
}

#[gpui::test]
fn pinned_section_collapse_is_local_persistent_and_available_by_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let expanded_y = cx.debug_bounds("new-tab").unwrap().origin.y;
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    // GPUI retains old debug bounds, so check the actual layout movement.
    cx.update(|window, cx| { let _ = window.draw(cx); });
    assert!(cx.debug_bounds("new-tab").unwrap().origin.y < expanded_y);
    let saved = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    let restored = browser_core::session::from_json(&saved).unwrap();
    assert!(restored.space().pinned_collapsed);
    assert!(!restored.spaces[1].pinned_collapsed);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("expand pinned section");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, expanded_y);
}

#[gpui::test]
fn dock_creates_named_space_and_rejects_blank_names(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    open_new_space_from_menu(&view, cx);
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("   ");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 3);
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("  Reading 📚  ");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Reading 📚");
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 4);
    assert!(today_urls(&view, cx).is_empty());
    assert_eq!(bar_text(&view, cx), None);
    let restored = view.read_with(cx, |w, _| browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap());
    assert_eq!(restored.space().name, "Reading 📚");
}

#[gpui::test]
fn create_and_rename_space_actions_keep_focus_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("create space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Reading");
    let id = view.read_with(cx, |w, _| w.browser.space().id);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Reading"));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("Research Archive");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Research Archive");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().id), id);

    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename space");
    bar(&view, cx).simulate_keystrokes("enter");
    bar(&view, cx).simulate_input("Cancelled");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Research Archive");
    open_new_space_from_menu(&view, cx);
    bar(&view, cx).simulate_input("Cancelled Space");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces.len()), 4);
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces[0].pinned.len()), 2);
}

fn find_overlay(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.find_bar.as_ref().and_then(|bar| bar.overlay)).expect("find overlay is not open");
    VisualTestContext::from_window(overlay.into(), cx)
}

#[gpui::test]
fn find_in_page_searches_as_you_type_and_navigates_matches(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-f");
    let mut find = find_overlay(&view, cx);
    let focused = find.update(|window, cx| view.read(cx).find_bar.as_ref().unwrap().input.read(cx).focus_handle(cx).is_focused(window));
    assert!(focused);
    find.simulate_input("needle");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), true, false)));
    find.simulate_keystrokes("enter shift-enter cmd-g cmd-shift-g");
    let calls = fake.take_calls();
    assert_eq!(calls, vec![Call::Find(1, "needle".into(), true, true), Call::Find(1, "needle".into(), false, true), Call::Find(1, "needle".into(), true, true), Call::Find(1, "needle".into(), false, true)]);
    click(&mut find, "find-next");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), true, true)));
    click(&mut find, "find-previous");
    assert!(fake.take_calls().contains(&Call::Find(1, "needle".into(), false, true)));
    fake.send(cx, Event::FindResult { view: page(1), count: 4, active: 2, final_update: true });
    assert_eq!(view.read_with(cx, |w, _| w.find_bar.as_ref().map(|bar| (bar.count, bar.active, bar.searching))), Some((Some(4), 2, false)));
    find.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::StopFinding(1)));
    assert!(calls.contains(&Call::FocusPage(1)));
}

#[gpui::test]
fn find_handles_no_matches_empty_input_and_refocus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-f");
    let mut find = find_overlay(&view, cx);
    find.simulate_input("missing");
    fake.send(cx, Event::FindResult { view: page(1), count: 0, active: 0, final_update: true });
    assert_eq!(view.read_with(cx, |w, _| w.find_bar.as_ref().unwrap().count), Some(0));
    find.simulate_keystrokes("cmd-f");
    find.simulate_input("replacement");
    assert_eq!(view.read_with(cx, |w, cx| w.find_bar.as_ref().unwrap().input.read(cx).text().to_string()), "replacement");
    fake.take_calls();
    find.simulate_keystrokes("cmd-a backspace");
    assert!(fake.take_calls().contains(&Call::StopFinding(1)));
    // A late engine response after clearing the input cannot restore the old count.
    fake.send(cx, Event::FindResult { view: page(1), count: 5, active: 1, final_update: true });
    assert_eq!(view.read_with(cx, |w, _| w.find_bar.as_ref().unwrap().count), None);
    click(&mut find, "find-close");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
}

#[gpui::test]
fn find_shortcut_from_page_focus_and_tab_switch_clear_find(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::Shortcut(Shortcut { cmd: true, key: "f".into(), ..Default::default() }));
    find_overlay(&view, cx).simulate_input("page text");
    fake.take_calls();
    // Clicking another sidebar item must not keep searching the old page.
    click(cx, "tab-4");
    cx.update(|window, cx| { let _ = window.draw(cx); });
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    assert!(fake.take_calls().contains(&Call::StopFinding(1)));
}

#[gpui::test]
fn find_command_opens_a_small_panel_with_input_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("find in page");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.command_bar.is_none()));
    let mut find = find_overlay(&view, cx);
    let panel = find.update(|window, _| window.bounds());
    let main = cx.update(|window, _| window.bounds());
    assert!(panel.size.height < main.size.height / 4.0);
    assert!(panel.size.width < main.size.width);
    assert!(find.update(|window, cx| view.read(cx).find_bar.as_ref().unwrap().input.read(cx).focus_handle(cx).is_focused(window)));
    find.simulate_input("text");
    assert!(fake.take_calls().contains(&Call::Find(1, "text".into(), true, false)));
}

#[gpui::test]
fn auto_archive_picker_offers_intervals_and_persists_never(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("archive tabs after");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(bar_input_focused(&view, cx));
    let options = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert_eq!(options.len(), 7);
    assert!(matches!(&options[0], Suggestion::ArchiveAfter { seconds: Some(43200), .. }));
    bar(&view, cx).simulate_input("Never");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive_after), None);
    assert_eq!(bar_text(&view, cx), None);
    let restored = view.read_with(cx, |w, _| browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap());
    assert_eq!(restored.archive_after, None);
}

#[gpui::test]
fn auto_archive_picker_changes_interval_and_escape_keeps_setting(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("auto archive settings");
    bar(&view, cx).simulate_keystrokes("enter");
    bar(&view, cx).simulate_input("7 days");
    click(&mut bar(&view, cx), "suggestion-0");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive_after), Some(Duration::from_secs(7 * 86400)));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("archive tabs after");
    bar(&view, cx).simulate_keystrokes("enter");
    bar(&view, cx).simulate_input("1 hour");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive_after), Some(Duration::from_secs(7 * 86400)));
}

#[gpui::test]
fn auto_archive_picker_preserves_a_custom_saved_interval_on_enter(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, _| w.browser.archive_after = Some(Duration::from_secs(12345)));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("archive tabs after");
    bar(&view, cx).simulate_keystrokes("enter enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive_after), Some(Duration::from_secs(12345)));
}

#[gpui::test]
fn copy_url_cleans_trackers_and_keeps_page_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/article?utm_source=mail&id=4#part".into() });
    fake.take_calls();
    fake.send(cx, Event::Shortcut(Shortcut { cmd: true, shift: true, key: "c".into(), ..Default::default() }));
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard.as_deref(), Some("https://example.com/article?id=4#part"));
    assert_eq!(view.read_with(cx, |w, _| w.toast.as_ref().map(|toast| toast.message.clone())).as_deref(), Some("Copied a clean link without trackers"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusUi)));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("cmd-v");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("https://example.com/article?id=4#part"));
}

#[gpui::test]
fn markdown_copy_and_command_actions_write_links(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::TitleChanged { view: page(1), title: "A [page]".into() });
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/a(b)?fbclid=track&lang=en".into() });
    cx.simulate_keystrokes("cmd-alt-shift-c");
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard.as_deref(), Some("[A \\[page\\]](https://example.com/a%28b%29?lang=en)"));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("copy url as markdown");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())), clipboard);
}

#[gpui::test]
fn repeated_toasts_replace_each_other_and_expire_without_stealing_focus(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-shift-c");
    assert!(view.read_with(cx, |w, _| w.toast.is_some()));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-alt-shift-c");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.toast.as_ref().map(|toast| toast.message.clone())).as_deref(), Some("Copied URL as Markdown"));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.toast.is_none()));
}

fn open_archive(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input("view archive");
    bar(view, cx).simulate_keystrokes("enter");
}

#[gpui::test]
fn archive_search_restores_a_selected_tab_to_its_original_space(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w ctrl-2");
    open_archive(&view, cx);
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert_eq!(suggestions.len(), 2);
    assert!(matches!(&suggestions[0], Suggestion::ArchivedTab { title, detail, .. } if title == "GPUI" && detail.contains("Manually Closed") && detail.contains("Personal")));
    bar(&view, cx).simulate_input("chromium");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 0);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(view.read_with(cx, |w, _| w.toast.as_ref().map(|toast| toast.message.clone())).as_deref(), Some("Tab restored to Personal"));
}

#[gpui::test]
fn archive_filters_spaces_and_deletes_without_restoring(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w ctrl-2 cmd-t");
    bar(&view, cx).simulate_input("https://work.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-w ctrl-1");
    open_archive(&view, cx);
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 3);
    click(&mut bar(&view, cx), "archive-this-space");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 2);
    bar(&view, cx).simulate_input("gpui");
    click(&mut bar(&view, cx), "delete-archived-7");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    assert_eq!(bar_text(&view, cx).as_deref(), Some("gpui"));
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://thebrowser.company"));
    bar(&view, cx).simulate_keystrokes("cmd-a backspace");
    click(&mut bar(&view, cx), "archive-all-spaces");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 2);
}

#[gpui::test]
fn clearing_the_archive_requires_confirmation_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w");
    open_archive(&view, cx);
    click(&mut bar(&view, cx), "clear-archive");
    bar(&view, cx).simulate_input("Clear Archive");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    open_archive(&view, cx);
    click(&mut bar(&view, cx), "clear-archive");
    // Enter without the confirmation phrase returns to the archive.
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 2);
    click(&mut bar(&view, cx), "clear-archive");
    bar(&view, cx).simulate_input("Clear Archive");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
    assert_eq!(bar_text(&view, cx), None);
    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(today_urls(&view, cx).len(), 1);
}

#[gpui::test]
fn cmd_t_and_cmd_l_restart_the_command_overlay_from_archive_mode(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w");
    open_archive(&view, cx);
    bar(&view, cx).simulate_input("no matches here");
    bar(&view, cx).simulate_keystrokes("cmd-t");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), active_url(&view, cx));
    bar(&view, cx).simulate_input("example.com");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

#[gpui::test]
fn reset_pinned_tab_navigates_back_to_the_original_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d cmd-l");
    bar(&view, cx).simulate_input("https://example.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reset pinned tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert!(fake.take_calls().contains(&Call::Navigate(1, "https://www.chromium.org".into())));
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 3);
}

#[gpui::test]
fn replace_pinned_url_changes_the_page_used_after_closing(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d cmd-l");
    bar(&view, cx).simulate_input("https://example.com/replacement");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("replace pin with current page");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().pinned_url.clone()).as_deref(), Some("https://example.com/replacement"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Navigate(_, _))));
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("https://example.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-w");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.url == "https://example.com/replacement" && tab.pinned_url.as_deref() == Some("https://example.com/replacement"))));
}

#[gpui::test]
fn reset_space_discards_pinned_pages_and_reopens_the_active_original(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original_id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-d cmd-l");
    bar(&view, cx).simulate_input("https://example.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reset space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), original_id);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)));
    assert!(calls.contains(&Call::Create(2, "https://www.chromium.org".into())));
    assert_eq!(today_urls(&view, cx).len(), 2);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
}

#[gpui::test]
fn exact_command_names_outrank_longer_actions_with_shared_words(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    for (query, expected) in [("pin tab", "Pin Tab"), ("archive tab", "Archive Tab"), ("archive tabs after", "Archive Tabs After…"), ("archive", "View Archive"), ("copy url", "Copy URL"), ("copy url as markdown", "Copy URL as Markdown"), ("reset space", "Reset Space")] {
        bar(&view, cx).simulate_keystrokes("cmd-a");
        bar(&view, cx).simulate_input(query);
        let label = view.read_with(cx, |w, cx| match w.suggestions(cx).first() { Some(Suggestion::Action { label, .. }) => label.clone(), _ => String::new() });
        assert_eq!(label, expected, "query: {query}");
    }
}

fn double_click(cx: &mut VisualTestContext, selector: &'static str) {
    click(cx, selector);
    let position = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent { position, button: gpui::MouseButton::Left, modifiers: Modifiers::none(), click_count: 2, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { position, button: gpui::MouseButton::Left, modifiers: Modifiers::none(), click_count: 2 });
    cx.run_until_parked();
}

fn inline_name(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |w, cx| w.tab_rename.as_ref().map(|editor| editor.input.read(cx).text().to_string()))
}

fn inline_name_focused(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| view.read(cx).tab_rename.as_ref().is_some_and(|editor| editor.input.read(cx).focus_handle(cx).is_focused(window)))
}

#[gpui::test]
fn double_click_renames_a_tab_and_engine_titles_do_not_overwrite_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    double_click(cx, "tab-8");
    assert_eq!(inline_name(&view, cx).as_deref(), Some("Chromium"));
    assert!(inline_name_focused(&view, cx));
    assert!(bar_text(&view, cx).is_none());
    let row = cx.debug_bounds("tab-8").unwrap();
    let editor = cx.debug_bounds("inline-tab-rename").unwrap();
    assert!(row.contains(&editor.origin) && row.contains(&editor.bottom_right()));
    cx.simulate_input("Engine research");
    cx.simulate_keystrokes("enter");
    fake.send(cx, Event::TitleChanged { view: page(1), title: "Updated by website".into() });
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Engine research");
    let saved = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    assert_eq!(browser_core::session::from_json(&saved).unwrap().active_tab().unwrap().title, "Engine research");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("reset tab name");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Updated by website");
}

#[gpui::test]
fn rename_tab_command_keeps_focus_and_escape_does_not_change_the_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename current tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(inline_name(&view, cx).as_deref(), Some("Chromium"));
    assert!(inline_name_focused(&view, cx));
    cx.simulate_input("Cancelled name");
    cx.simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Chromium");
    double_click(cx, "tab-8");
    cx.simulate_input("  ");
    cx.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Chromium");
    assert!(inline_name(&view, cx).is_none());
}

#[gpui::test]
fn clearing_an_inline_name_restores_the_latest_page_title(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Custom name");
    cx.simulate_keystrokes("enter");
    double_click(cx, "tab-8");
    fake.send(cx, Event::TitleChanged { view: page(1), title: "Latest website title".into() });
    assert_eq!(inline_name(&view, cx).as_deref(), Some("Custom name"));
    cx.simulate_keystrokes("backspace enter");
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Latest website title");
    assert!(!view.read_with(cx, |w, _| w.browser.item_has_custom_name(w.browser.active_tab().unwrap().id)));
}

#[gpui::test]
fn leaving_an_inline_name_by_clicking_another_tab_commits_without_reloading_the_original(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Saved on click");
    fake.take_calls();
    click(cx, "tab-7");
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().find(|tab| tab.id.get() == 8).unwrap().title.clone()), "Saved on click");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(1) | Call::Navigate(1, _) | Call::Reload(1))));
}

#[gpui::test]
fn returning_native_focus_to_the_page_commits_its_inline_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    double_click(cx, "tab-8");
    assert!(fake.take_calls().contains(&Call::FocusUi));
    cx.simulate_input("Saved on native focus");
    fake.send(cx, Event::Focused { view: page(1) });
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Saved on native focus");
    assert!(fake.take_calls().is_empty());
}

#[gpui::test]
fn opening_the_command_bar_commits_the_inline_name_and_restores_normal_input(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Saved before search");
    cx.simulate_keystrokes("cmd-t");
    assert!(inline_name(&view, cx).is_none());
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("independent query");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("independent query"));
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Saved before search");
}

#[gpui::test]
fn hiding_the_sidebar_finishes_inline_editing_instead_of_leaving_an_invisible_input(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Saved before hiding");
    cx.simulate_keystrokes("cmd-s");
    assert!(inline_name(&view, cx).is_none());
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Saved before hiding");
}

#[gpui::test]
fn closing_a_tab_during_inline_editing_preserves_the_draft_in_the_archive(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    double_click(cx, "tab-8");
    cx.simulate_input("Archived custom name");
    cx.simulate_keystrokes("cmd-w");
    assert!(inline_name(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.last().unwrap().tab.title.clone()), "Archived custom name");
    cx.simulate_keystrokes("cmd-shift-t");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Archived custom name");
}

#[gpui::test]
fn renaming_a_hidden_pinned_tab_reveals_its_sidebar_row_without_recreating_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-d");
    run_named_command(&view, cx, "collapse pinned section");
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    fake.take_calls();
    run_named_command(&view, cx, "rename current tab");
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_visible && !w.browser.space().pinned_collapsed));
    assert!(inline_name_focused(&view, cx));
    assert!(cx.debug_bounds("inline-tab-rename").is_some());
    cx.simulate_input("Pinned name");
    cx.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Pinned name");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn move_commands_change_space_and_section_without_recreating_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    fake.take_calls();
    for (query, space, pinned) in [("move to pinned in work", 1, true), ("move to today in research", 2, false), ("move to work", 1, false)] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), space);
        assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), id);
        assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().pinned_url.is_some()), pinned);
        assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
        assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, _) | Call::Close(_) | Call::Navigate(_, _))));
    assert_eq!(view.read_with(cx, |w, _| w.browser.spaces[0].today.len()), 2);
}

#[gpui::test]
fn pin_to_space_command_preserves_a_custom_tab_name(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename tab");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_input("Engine notes");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin to work");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Engine notes");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned.len()), 1);
}

fn menu_action(menu: &str, label: &str) -> Box<dyn gpui::Action> {
    fn find(items: Vec<gpui::MenuItem>, label: &str) -> Option<Box<dyn gpui::Action>> {
        items.into_iter().find_map(|item| match item {
            gpui::MenuItem::Action { name, action, .. } if name.as_ref() == label => Some(action),
            gpui::MenuItem::Submenu(menu) => find(menu.items, label),
            _ => None,
        })
    }
    find(crate::menus::build().into_iter().find(|item| item.name.as_ref() == menu).unwrap().items, label)
        .unwrap_or_else(|| panic!("menu item missing: {menu} / {label}"))
}

#[gpui::test]
fn view_source_from_menu_shortcut_and_command_bar_opens_a_new_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    let count = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    for route in 0..3 {
        fake.take_calls();
        match route {
            0 => dispatch_menu(cx, "View", "View Source"),
            1 => { cx.simulate_keystrokes("cmd-alt-u"); cx.run_until_parked(); }
            _ => run_named_command(&view, cx, "view source"),
        }
        let source = format!("view-source:{original}");
        assert_eq!(active_url(&view, cx).as_deref(), Some(source.as_str()));
        assert!(bar_text(&view, cx).is_none());
        assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), count + 1);
        let calls = fake.take_calls();
        assert!(calls.iter().any(|call| matches!(call, Call::Create(_, url) if url == &source)));
        assert!(!calls.iter().any(|call| matches!(call, Call::Close(_) | Call::Navigate(..) | Call::Reload(_))));
        assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::ViewSource, cx)));
        cx.simulate_keystrokes("cmd-alt-u");
        assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), count + 1);
        cx.simulate_keystrokes("cmd-w");
        cx.run_until_parked();
        assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    }
}

#[gpui::test]
fn view_source_from_native_split_focus_preserves_the_original_group(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let original = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.send(cx, Event::Shortcut(Shortcut { cmd: true, alt: true, key: "u".into(), ..Default::default() }));
    assert_eq!(active_url(&view, cx).as_deref(), Some("view-source:https://pane1.example"));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.id, original.id);
        assert_eq!(split.panes, original.panes);
        assert_eq!(split.active, original.active);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn internal_pages_do_not_offer_view_source(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-y");
    let original = active_url(&view, cx);
    assert!(!cx.update(|window, cx| window.is_action_available(&crate::window::ViewSource, cx)));
    cx.simulate_keystrokes("cmd-alt-u");
    assert_eq!(active_url(&view, cx), original);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("view source");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::ViewSource, .. }))));
}

fn dispatch_menu(cx: &mut VisualTestContext, menu: &str, label: &str) {
    let action = menu_action(menu, label);
    cx.update(|window, cx| window.dispatch_action(action, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn application_menu_actions_are_reachable_from_the_browser_window(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    for menu in crate::menus::build() {
        for item in menu.items {
            if let gpui::MenuItem::Action { name, action, os_action: None } = item {
                // TestPlatform does not automatically activate newly opened windows.
                let available = cx.update(|window, cx| window.is_action_available(action.as_ref(), cx))
                    || cx.cx.update(|cx| cx.is_action_available(action.as_ref()));
                assert!(available, "unreachable menu action: {} / {name}", menu.name);
            }
        }
    }
}

#[gpui::test]
fn menu_actions_open_editors_and_edit_the_command_input(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    dispatch_menu(cx, "Spaces", "New Space…");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("Menu Space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Menu Space");
    dispatch_menu(cx, "Spaces", "Rename Space…");
    bar(&view, cx).simulate_input("Renamed Space");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Renamed Space");
    dispatch_menu(cx, "File", "New Tab…");
    let mut input = bar(&view, cx);
    input.simulate_input("clipboard text");
    dispatch_menu(&mut input, "Edit", "Select All");
    dispatch_menu(&mut input, "Edit", "Copy");
    dispatch_menu(&mut input, "Edit", "Cut");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    dispatch_menu(&mut input, "Edit", "Paste");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("clipboard text"));
}

#[gpui::test]
fn fullscreen_targets_the_main_window_from_pages_and_overlays(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("ctrl-cmd-f");
    assert!(cx.update(|window, _| window.is_fullscreen()));
    fake.send(cx, Event::Shortcut(Shortcut { ctrl: true, cmd: true, key: "f".into(), ..Default::default() }));
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("ctrl-cmd-f");
    assert_eq!(bar_text(&view, cx), None);
    assert!(cx.update(|window, _| window.is_fullscreen()));
    cx.simulate_keystrokes("cmd-f");
    find_overlay(&view, cx).simulate_keystrokes("ctrl-cmd-f");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    for (query, expected) in [("enter full screen", true), ("enter full screen", true), ("exit full screen", false)] {
        cx.simulate_keystrokes("cmd-t");
        bar(&view, cx).simulate_input(query);
        bar(&view, cx).simulate_keystrokes("enter");
        assert_eq!(cx.update(|window, _| window.is_fullscreen()), expected);
    }
}

fn record_history_fixture(fake: &FakeEngine, cx: &mut VisualTestContext) {
    for (url, title) in [("https://example.com/one", "First page"), ("https://example.com/two", "Second page")] {
        fake.send(cx, Event::UrlChanged { view: page(1), url: url.into() });
        fake.send(cx, Event::TitleChanged { view: page(1), title: title.into() });
    }
}

#[gpui::test]
fn history_opens_a_gpui_today_tab_without_a_chromium_view(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    fake.take_calls();
    let before = today_urls(&view, cx).len();
    cx.simulate_keystrokes("cmd-y");
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    assert_eq!(today_urls(&view, cx).len(), before + 1);
    assert!(cx.debug_bounds("history-page").is_some());
    assert!(cx.update(|window, cx| view.read(cx).history_page.input.read(cx).focus_handle(cx).is_focused(window)));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Visible(1, false)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(_, _))));
    cx.simulate_keystrokes("cmd-y");
    assert_eq!(today_urls(&view, cx).len(), before + 1);
    cx.simulate_input("first page");
    cx.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/one"));
    assert!(fake.take_calls().iter().any(|call| matches!(call, Call::Create(_, url) if url == "https://example.com/one")));
}

#[gpui::test]
fn history_deletion_and_confirmation_preserve_tabs_and_can_be_cancelled(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-y");
    let tabs = today_urls(&view, cx);
    click(cx, "history-remove-0");
    assert_eq!(view.read_with(cx, |w, _| w.browser.history.len()), 1);
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    click(cx, "clear-history");
    click(cx, "history-clear-cancel");
    assert_eq!(view.read_with(cx, |w, _| w.browser.history.len()), 1);
    click(cx, "clear-history");
    cx.simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.history.len()), 1);
    click(cx, "clear-history");
    click(cx, "history-clear-confirm");
    assert!(view.read_with(cx, |w, _| w.browser.history.is_empty()));
    assert_eq!(today_urls(&view, cx), tabs);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
}

#[gpui::test]
fn internal_navigation_closes_the_old_engine_view_and_returns_focus_to_gpui(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("browser://history");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)));
    assert!(calls.contains(&Call::FocusUi));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(_, url) if url.starts_with("browser://"))));
    cx.simulate_input("search text");
    cx.simulate_keystrokes("cmd-f");
    assert!(view.read_with(cx, |w, _| w.find_bar.is_none()));
    cx.simulate_input("replacement query");
    assert_eq!(view.read_with(cx, |w, cx| w.history_page.input.read(cx).text().to_string()), "replacement query");
}

#[gpui::test]
fn history_actions_work_from_the_command_bar_and_menu(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("view history");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(active_url(&view, cx).as_deref(), Some(crate::history::HISTORY_URL));
    let count = today_urls(&view, cx).len();
    dispatch_menu(cx, "Archive", "View History");
    assert_eq!(today_urls(&view, cx).len(), count);
}

#[gpui::test]
fn native_menu_new_tab_toggles_an_existing_overlay_without_leaking_windows(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("old query");
    for _ in 0..3 {
        // macOS menu dispatch may target the main window while a child is key.
        dispatch_menu(cx, "File", "New Tab…");
        assert_eq!(bar_text(&view, cx), None);
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
        dispatch_menu(cx, "File", "New Tab…");
        assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
        assert!(bar_input_focused(&view, cx));
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 2);
    }
}

#[gpui::test]
fn repeating_command_bar_shortcuts_dismisses_and_different_shortcuts_switch_mode(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unfinished query");
    bar(&view, cx).simulate_keystrokes("cmd-t");
    assert_eq!(bar_text(&view, cx), None);
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), None);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_keystrokes("cmd-l");
    assert_eq!(bar_text(&view, cx), active_url(&view, cx));
    bar(&view, cx).simulate_keystrokes("cmd-t");
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(today_urls(&view, cx), before);
}

#[gpui::test]
fn confirming_the_current_url_reloads_without_replacing_the_title(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::TitleChanged { view: page(1), title: "Page title".into() });
    fake.take_calls();
    let before = today_urls(&view, cx);
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_keystrokes("enter");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Reload(1)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Navigate(..) | Call::Create(..))));
    assert_eq!(today_urls(&view, cx), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Page title");
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn command_bar_finds_closed_pages_in_history_and_opens_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-w cmd-t");
    bar(&view, cx).simulate_input("first page");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(matches!(&suggestions[1], Suggestion::History { title, url } if title == "First page" && url == "https://example.com/one"));
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/one"));
    assert_eq!(bar_text(&view, cx), None);
}

#[gpui::test]
fn history_completion_in_cmd_l_navigates_the_current_tab_and_keeps_its_custom_name(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-w cmd-t");
    bar(&view, cx).simulate_input("rename tab");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_input("Custom label");
    cx.simulate_keystrokes("enter");
    let before = today_urls(&view, cx).len();
    let id = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("first page");
    click(&mut bar(&view, cx), "suggestion-1");
    assert_eq!(today_urls(&view, cx).len(), before);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), id);
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://example.com/one"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().title.clone()), "Custom label");
}

#[gpui::test]
fn open_tabs_match_their_page_titles_without_duplicate_history_rows(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("rename tab");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_input("Renamed tab");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("second page");
    let suggestions = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert!(suggestions.iter().any(|suggestion| matches!(suggestion, Suggestion::SwitchToTab { title, .. } if title == "Renamed tab")));
    assert!(!suggestions.iter().any(|suggestion| matches!(suggestion, Suggestion::History { url, .. } if url == "https://example.com/two")));
}

#[gpui::test]
fn deleting_a_history_suggestion_keeps_the_bar_and_does_not_open_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    record_history_fixture(&fake, cx);
    cx.simulate_keystrokes("cmd-w cmd-t");
    let before = today_urls(&view, cx);
    bar(&view, cx).simulate_input("first page");
    bar(&view, cx).simulate_keystrokes("down");
    click(&mut bar(&view, cx), "delete-suggestion-1");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("first page"));
    assert!(bar_input_focused(&view, cx));
    assert_eq!(today_urls(&view, cx), before);
    view.read_with(cx, |w, cx| {
        assert_eq!(w.command_bar.as_ref().unwrap().selected, 0);
        assert!(!w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::History { .. })));
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(restored.history.len(), 1);
        assert_eq!(restored.history[0].url, "https://example.com/two");
        assert_eq!(restored.archive.len(), w.browser.archive.len());
    });
    bar(&view, cx).simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("first page");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 1);
}

#[gpui::test]
fn favorites_are_shared_and_keep_the_loaded_page_across_spaces(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("favorite tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.favorites.len()), 1);
    assert!(cx.debug_bounds("favorite-8").is_some());
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-2");
    assert!(active_url(&view, cx).is_none());
    click(cx, "favorite-8");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_space), 1);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    // Favorites come before the Space's Pinned and Today items by default.
    cx.simulate_keystrokes("ctrl-1 cmd-2");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("chromium");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::SwitchToTab { space, .. } if space == "Favorites"))));
    bar(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
}

#[gpui::test]
fn closing_a_favorite_unloads_it_and_clicking_it_reopens_the_original_url(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://example.com/favorite-navigation".into() });
    fake.take_calls();
    let archive = view.read_with(cx, |w, _| w.browser.archive.len());
    // Favorites have no hover close button; the context menu closes them.
    assert!(cx.debug_bounds("close-favorite-8").is_none());
    right_click(cx, "favorite-8");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert!(fake.take_calls().contains(&Call::Close(1)));
    assert!(active_url(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), archive);
    assert_eq!(view.read_with(cx, |w, _| w.browser.favorites.len()), 1);
    click(cx, "favorite-8");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
    assert!(fake.take_calls().contains(&Call::Create(2, original)));
}

#[gpui::test]
fn unfavorite_returns_the_shared_tab_to_the_current_spaces_today(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unfavorite tab");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert!(w.browser.favorites.is_empty());
        assert_eq!(w.browser.space().today.len(), 1);
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert!(restored.favorites.is_empty());
        assert_eq!(restored.active_tab().unwrap().id.get(), 8);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_favorite_can_move_to_the_current_spaces_pinned_section(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("move to pinned in work");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert!(w.browser.favorites.is_empty());
        assert_eq!(w.browser.space().pinned[0].id.get(), 8);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert_ne!(w.browser.spaces[0].active_tab, w.browser.space().active_tab);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn engine_favicons_follow_tabs_into_favorites_and_survive_closing_and_sessions(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx).unwrap();
    let png = include_bytes!("../testdata/favicon.png").to_vec();
    fake.send(cx, Event::Favicon { view: page(1), page_url: original.clone(), png: png.clone() });
    assert_eq!(view.read_with(cx, |w, _| w.browser.favicon(&original).map(Vec::from)), Some(png.clone()));
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1");
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://elsewhere.example/page".into() });
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorites[0].url, original);
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(restored.favicon(&original), Some(png.as_slice()));
    });
    click(cx, "favorite-8");
    assert_eq!(active_url(&view, cx).as_deref(), Some(original.as_str()));
}

fn right_click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let center = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_move(center, None, Modifiers::none());
    cx.simulate_mouse_down(center, gpui::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(center, gpui::MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

fn tab_menu(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |w, _| w.tab_menu.as_ref().and_then(|menu| menu.overlay)).unwrap();
    VisualTestContext::from_window(overlay.into(), cx)
}

#[gpui::test]
fn context_copy_uses_the_clicked_tab_and_keeps_the_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    assert_eq!(active_url(&view, cx), original);
    let mut menu = tab_menu(&view, cx);
    let panel = menu.debug_bounds("tab-context-menu").unwrap();
    let size = menu.update(|window, _| window.bounds().size);
    assert!(panel.origin.x >= gpui::px(0.) && panel.right() <= size.width);
    assert!(panel.origin.y >= gpui::px(0.) && panel.bottom() <= size.height);
    menu.simulate_keystrokes("down enter");
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())).as_deref(), Some("https://github.com"));
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::Close(..))));
}

#[gpui::test]
fn context_pin_and_rename_do_not_activate_the_clicked_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-pin");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned.iter().any(|tab| tab.id.get() == 7)));
    assert_eq!(active_url(&view, cx), original);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-rename");
    assert_eq!(inline_name(&view, cx).as_deref(), Some("GPUI"));
    click(cx, "inline-tab-rename");
    assert_eq!(active_url(&view, cx), original);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Renamed from menu");
    cx.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().find(|tab| tab.id.get() == 7).unwrap().title.clone()), "Renamed from menu");
    assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
}

#[gpui::test]
fn context_favorite_and_unfavorite_preserve_the_other_active_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-favorite");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.favorites[0].id.get()), 7);
    right_click(cx, "favorite-7");
    click(&mut tab_menu(&view, cx), "tab-menu-favorite");
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |w, _| w.browser.favorites.is_empty()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today[0].id.get()), 7);
}

#[gpui::test]
fn context_archive_targets_the_clicked_tab_and_escape_or_outside_click_cancel(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let original = active_url(&view, cx);
    for _ in 0..3 {
        right_click(cx, "tab-7");
        tab_menu(&view, cx).simulate_keystrokes("escape");
        assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
        assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
    }
    right_click(cx, "tab-7");
    let mut menu = tab_menu(&view, cx);
    menu.simulate_click(gpui::point(gpui::px(900.), gpui::px(650.)), Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty()));
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive[0].tab.id.get()), 7);
}

#[gpui::test]
fn context_reset_and_replace_original_url_work_on_an_inactive_pinned_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-4");
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("https://github.com/elsewhere");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "tab-8");
    let original = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-edit-pinned-page");
    click(&mut menu, "tab-menu-reset");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned[0].url.clone()), "https://github.com");
    assert!(fake.take_calls().contains(&Call::Close(2)), "reset unloads an inactive page immediately");
    click(cx, "tab-4");
    cx.simulate_keystrokes("cmd-l");
    bar(&view, cx).simulate_input("https://github.com/new-original");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "tab-8");
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-edit-pinned-page");
    click(&mut menu, "tab-menu-replace");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned[0].pinned_url.clone()).as_deref(), Some("https://github.com/new-original"));
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-close");
    assert!(fake.take_calls().contains(&Call::Close(3)));
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().pinned[0].url.clone()), "https://github.com/new-original");
}

#[gpui::test]
fn mute_command_and_sidebar_button_control_the_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("mute tab");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(fake.take_calls().contains(&Call::Mute(1, true)));
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
    click(cx, "unmute-8");
    assert!(fake.take_calls().contains(&Call::Mute(1, false)));
    assert_eq!(active_url(&view, cx), original);
    assert!(!view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
}

#[gpui::test]
fn context_muting_a_background_page_does_not_activate_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-7");
    click(cx, "tab-8");
    fake.take_calls();
    let original = active_url(&view, cx);
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-mute");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Mute(2, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::FocusPage(2))));
    assert_eq!(active_url(&view, cx), original);
    click(cx, "unmute-7");
    assert!(fake.take_calls().contains(&Call::Mute(2, false)));
    assert_eq!(active_url(&view, cx), original);
}

#[gpui::test]
fn mute_all_reaches_unloaded_tabs_and_unmute_all_updates_background_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Mute All Tabs");
    assert!(view.read_with(cx, |w, _| w.browser.all_tabs().all(|tab| tab.muted)));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Mute(1, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..))));
    cx.simulate_keystrokes("cmd-1");
    let calls = fake.take_calls();
    let mute = calls.iter().position(|call| *call == Call::Mute(2, true)).unwrap();
    let show = calls.iter().position(|call| *call == Call::Visible(2, true)).unwrap();
    assert!(mute < show, "a restored muted page is muted before being shown: {calls:?}");
    dispatch_menu(cx, "Tabs", "Unmute All Tabs");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Mute(1, false)) && calls.contains(&Call::Mute(2, false)));
    assert!(!view.read_with(cx, |w, _| w.browser.all_tabs().any(|tab| tab.muted)));
}

#[gpui::test]
fn a_closed_muted_favorite_reopens_muted(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    dispatch_menu(cx, "Tabs", "Mute/Unmute Tab");
    cx.simulate_keystrokes("ctrl-2 cmd-1 cmd-w");
    fake.take_calls();
    click(cx, "favorite-8");
    let calls = fake.take_calls();
    let mute = calls.iter().position(|call| *call == Call::Mute(2, true)).unwrap();
    let focus = calls.iter().position(|call| *call == Call::FocusPage(2)).unwrap();
    assert!(mute < focus, "mute is applied before returning keyboard focus: {calls:?}");
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
}

#[gpui::test]
fn internal_history_pages_do_not_offer_mute_commands(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-y");
    dispatch_menu(cx, "Tabs", "Mute/Unmute Tab");
    assert!(!view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("mute tab");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|s| matches!(s, Suggestion::Action { command: crate::window::Command::ToggleMute, .. }))));
}

#[gpui::test]
fn engine_initialization_cannot_clear_a_saved_mute_intent(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Mute/Unmute Tab");
    fake.take_calls();
    // CEF initialization may replace the host's initial audio flag after creation.
    fake.0.borrow_mut().muted.clear();
    fake.send(cx, Event::LoadingChanged { view: page(1), loading: false, can_go_back: false, can_go_forward: false });
    assert!(fake.take_calls().contains(&Call::Mute(1, true)));
    assert!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().muted));
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().audio_muted(w.browser.active_tab().unwrap().id)), Some(true));
}

fn open_split_pages(cx: &mut TestAppContext, count: usize) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    let now = std::time::SystemTime::now();
    let first = browser.new_tab("https://pane0.example", now);
    for index in 1..count {
        let next = browser.new_tab(format!("https://pane{index}.example"), now);
        browser.add_split(first, next, browser_core::splits::Placement::Right, now).unwrap();
    }
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx, fake)
}

#[gpui::test]
fn split_pages_are_visible_together_with_nonoverlapping_native_surfaces(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 4);
    let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    assert_eq!(surfaces.len(), 4);
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 4);
    for id in 1..=4 { assert!(calls.contains(&Call::Visible(id, true))); }
    for pair in surfaces.windows(2) {
        assert!(pair[0].1.x + pair[0].1.width + crate::split_layout::GAP <= pair[1].1.x + 0.001);
        assert_eq!(pair[0].1.y, pair[1].1.y);
    }
    assert!(calls.contains(&Call::FocusPage(2)), "the last added pane is inserted beside the anchor and focused");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 4);
}

#[gpui::test]
fn cef_pane_focus_updates_shortcut_targets_without_refocusing_or_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    fake.send(cx, Event::Focused { view: page(1) });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    assert!(fake.take_calls().is_empty(), "native focus is already correct; do not bounce it back to CEF");
    cx.simulate_keystrokes("cmd-r");
    assert!(fake.take_calls().contains(&Call::Reload(1)));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("unfinished query");
    fake.send(cx, Event::Focused { view: page(2) });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    assert!(bar_input_focused(&view, cx));
}

#[gpui::test]
fn switching_split_orientation_keeps_existing_views_and_navigation_state(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    view.update(cx, |w, cx| { let id = w.browser.active_tab().unwrap().id; w.browser.set_split_axis(id, browser_core::splits::Axis::Vertical); cx.notify(); });
    cx.run_until_parked();
    let frames = fake.0.borrow().bounds.clone();
    assert_eq!(frames[&1].x, frames[&2].x);
    assert!(frames[&1].y + frames[&1].height < frames[&2].y);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn closing_and_restoring_a_split_closes_and_recreates_all_its_engine_views(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor.get());
    click(cx, test_selector(format!("close-{anchor}")));
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)) && calls.contains(&Call::Close(2)));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
    assert!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().is_empty()));
    cx.simulate_keystrokes("cmd-shift-t");
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 2);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
}

#[gpui::test]
fn hidden_split_pages_cannot_steal_the_new_tabs_shortcut_target(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://outside.example");
    bar(&view, cx).simulate_keystrokes("enter");
    fake.take_calls();
    fake.send(cx, Event::Focused { view: page(1) });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://outside.example"));
    cx.simulate_keystrokes("cmd-r");
    assert!(fake.take_calls().contains(&Call::Reload(3)));
}

fn run_named_command(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext, text: &str) {
    cx.simulate_keystrokes("cmd-t");
    bar(view, cx).simulate_input(text);
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).first(), Some(Suggestion::Action { .. }))), "missing command: {text}");
    bar(view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn split_picker_is_cancelable_and_can_use_an_existing_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    cx.simulate_keystrokes("ctrl-shift-=");
    assert!(bar_input_focused(&view, cx));
    assert!(view.read_with(cx, |w, _| matches!(w.command_bar.as_ref().unwrap().target, crate::window::Target::Split { .. })));
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), before);
    run_named_command(&view, cx, "add split view");
    bar(&view, cx).simulate_input("github");
    bar(&view, cx).simulate_keystrokes("down enter");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_ne!(w.browser.active_tab().unwrap().id.get(), 4);
        assert_eq!(w.browser.active_tab().unwrap().url, "https://github.com");
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id.get() == 4));
        assert_eq!(w.browser.all_tabs().count(), before + 1);
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.browser.is_split_secondary(w.browser.active_tab().unwrap().id)));
    assert!(cx.debug_bounds("tab-8").is_some());
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(..))));
}

#[gpui::test]
fn split_picker_adds_urls_in_the_requested_direction_and_enforces_four_panes(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    run_named_command(&view, cx, "add top split");
    bar(&view, cx).simulate_input("https://third.example");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.axis, browser_core::splits::Axis::Vertical);
        assert_eq!(split.panes[1], w.browser.active_tab().unwrap().id);
    });
    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_input("https://fourth.example");
    bar(&view, cx).simulate_keystrokes("enter");
    let count = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    cx.simulate_keystrokes("ctrl-shift-=");
    assert_eq!(bar_text(&view, cx), None);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 4);
        assert_eq!(w.browser.all_tabs().count(), count);
        assert_eq!(w.toast.as_ref().unwrap().message, "Cannot Add New Pane");
    });
}

#[gpui::test]
fn pane_header_focus_and_chrome_match_native_geometry_in_both_axes(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    fake.take_calls();
    click(cx, "pane-header-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 2);
    assert!(fake.take_calls().contains(&Call::FocusPage(1)));
    for vertical in [false, true] {
        if vertical { run_named_command(&view, cx, "convert to vertical split"); }
        let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
        for (id, body) in surfaces {
            let selector = match id.get() { 2 => "pane-header-2", 3 => "pane-header-3", 5 => "pane-header-5", _ => panic!("unexpected pane") };
            let header = cx.debug_bounds(selector).unwrap();
            assert!(header.origin.y >= gpui::px(8.));
            assert!((f64::from(f32::from(header.origin.x)) - body.x).abs() < 0.6);
            assert!((f64::from(f32::from(header.bottom())) - body.y).abs() < 0.6, "vertical={vertical} header={header:?} body={body:?}");
            assert!((f64::from(f32::from(header.size.width)) - body.width).abs() < 0.6);
        }
    }
}

#[gpui::test]
fn pane_close_shortcut_archives_only_the_selected_page_and_header_separates_without_reload(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let closed = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    cx.simulate_keystrokes("ctrl-shift--");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.archive.last().unwrap().tab.id, closed);
    });
    fake.take_calls();
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-separate-pane");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 2);
    assert!(cx.debug_bounds("tab-2").is_some());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn split_sidebar_item_remembers_its_pane_and_favorites_show_one_group(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    click(cx, "pane-header-2");
    run_named_command(&view, cx, "favorite tab");
    assert!(cx.debug_bounds("favorite-2").is_some());
    assert!(cx.debug_bounds("favorite-3").is_none());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://outside.example");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "favorite-2");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    run_named_command(&view, cx, "rename tab");
    bar(&view, cx).simulate_input("Comparison");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| assert_eq!(w.browser.item_title(w.browser.active_tab().unwrap().id), "Comparison"));
    run_named_command(&view, cx, "reset tab name");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().title.is_none()));
}

#[gpui::test]
fn split_commands_move_focus_order_and_separate_without_navigation(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    fake.take_calls();
    run_named_command(&view, cx, "focus pane 1");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    run_named_command(&view, cx, "move pane right");
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().panes[1], w.browser.active_tab().unwrap().id));
    run_named_command(&view, cx, "focus next pane");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane1.example"));
    run_named_command(&view, cx, "separate all tabs");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn expanding_a_split_from_the_menu_and_command_bar_keeps_panes_loaded(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let original = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.take_calls();
    dispatch_menu(cx, "View", "Expand Current Split");
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        let active = split.panes.iter().position(|id| *id == split.active).unwrap();
        assert!(split.weights.iter().enumerate().all(|(index, weight)| index == active || *weight < split.weights[active]));
    });
    for vertical in [false, true] {
        if vertical { run_named_command(&view, cx, "convert to vertical split"); }
        click(cx, "pane-header-2");
        run_named_command(&view, cx, "expand current split");
        let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
        let active = surfaces.iter().find(|(id, _)| id.get() == 2).unwrap().1;
        for (_, bounds) in surfaces.iter().filter(|(id, _)| id.get() != 2) {
            let (large, small) = if vertical { (active.height, bounds.height) } else { (active.width, bounds.width) };
            assert!(large >= small, "active pane should expand: {surfaces:?}");
            assert!((small - if vertical { 220. - crate::split_layout::HEADER_HEIGHT } else { 220. }).abs() < 1.);
        }
        view.read_with(cx, |w, _| {
            let split = w.browser.active_split().unwrap();
            assert_eq!(split.id, original.id);
            assert_eq!(split.panes, original.panes);
            assert_eq!(split.active.get(), 2);
            let restored = browser_core::session::from_json(&w.session_json()).unwrap();
            assert_eq!(restored.active_split().unwrap().weights, split.weights);
        });
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::Reload(..))));
}

#[gpui::test]
fn expand_current_split_does_nothing_to_a_standalone_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    dispatch_menu(cx, "View", "Expand Current Split");
    assert_eq!(active_url(&view, cx), original);
    assert!(fake.take_calls().is_empty());
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("expand current split");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::ExpandCurrentSplit, .. }))));
}

#[gpui::test]
fn expanding_four_panes_in_a_small_window_keeps_every_page_visible(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 4);
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(480.)));
    cx.run_until_parked();
    fake.take_calls();
    dispatch_menu(cx, "View", "Expand Current Split");
    let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    assert_eq!(surfaces.len(), 4);
    for (_, bounds) in &surfaces {
        assert!(bounds.width > 1. && bounds.height > 1.);
        assert!((bounds.width - surfaces[0].1.width).abs() < 1.);
    }
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Visible(_, false))));
}

#[gpui::test]
fn command_search_and_archive_search_preserve_group_name_and_match_secondary_urls(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    run_named_command(&view, cx, "rename tab");
    bar(&view, cx).simulate_input("Comparison");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pane1");
    let rows = view.read_with(cx, |w, cx| w.suggestions(cx));
    assert_eq!(rows.iter().filter(|row| matches!(row, Suggestion::SwitchToTab { .. })).count(), 1);
    assert!(rows.iter().any(|row| matches!(row, Suggestion::SwitchToTab { title, .. } if title == "Comparison")));
    bar(&view, cx).simulate_keystrokes("escape");
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor.get());
    click(cx, test_selector(format!("close-{anchor}")));
    open_archive(&view, cx);
    bar(&view, cx).simulate_input("pane1");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).first(), Some(Suggestion::ArchivedTab { title, .. }) if title == "Comparison")));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 2);
}

#[gpui::test]
fn internal_pages_cannot_join_split_view_and_native_menu_uses_the_same_picker(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    dispatch_menu(cx, "View", "Add Split View…");
    bar(&view, cx).simulate_input("browser://history");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    bar(&view, cx).simulate_keystrokes("escape");
    dispatch_menu(cx, "Archive", "View History");
    cx.simulate_keystrokes("ctrl-shift-=");
    assert_eq!(bar_text(&view, cx), None);
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
}

#[gpui::test]
fn split_shortcuts_from_native_page_focus_use_the_same_picker_and_close_action(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let shortcut = |key: &str| Shortcut { ctrl: true, shift: true, key: key.into(), ..Default::default() };
    let shortcuts = crate::window::page_shortcuts();
    assert!(shortcuts.contains(&shortcut("=")));
    assert!(shortcuts.contains(&shortcut("-")));
    fake.send(cx, Event::Shortcut(shortcut("=")));
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_keystrokes("escape");
    fake.send(cx, Event::Shortcut(shortcut("-")));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

#[gpui::test]
fn pane_menu_targets_an_inactive_pane_and_moves_it_without_changing_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let active = active_url(&view, cx);
    fake.take_calls();
    click(cx, "pane-menu-2");
    assert_eq!(active_url(&view, cx), active);
    click(&mut tab_menu(&view, cx), "tab-menu-move-forward");
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().panes[1].get(), 2));
    right_click(cx, "pane-header-2");
    click(&mut tab_menu(&view, cx), "tab-menu-orientation");
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().axis, browser_core::splits::Axis::Vertical));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pane_menu_and_sidebar_tab_options_mute_or_close_only_their_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-mute");
    view.read_with(cx, |w, _| {
        assert!(w.browser.all_tabs().find(|tab| tab.id.get() == 2).unwrap().muted);
        assert_eq!(w.browser.all_tabs().filter(|tab| tab.muted).count(), 1);
    });
    // The sidebar row's Tab Options submenu acts on one pane.
    right_click(cx, "tab-2");
    let mut menu = tab_menu(&view, cx);
    let pane = view.read_with(cx, |w, _| *w.browser.active_split().unwrap().panes.iter().find(|id| id.get() != 2).unwrap());
    let title = view.read_with(cx, |w, _| crate::tab_menu::short_title(&w.browser.all_tabs().find(|tab| tab.id == pane).unwrap().title));
    let selector = format!("tab-menu-{}", title.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>());
    let selector = selector.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
    let row = menu.debug_bounds(test_selector(selector)).unwrap();
    menu.simulate_mouse_move(row.center(), None, Modifiers::none());
    menu.run_until_parked();
    click(&mut menu, "tab-menu-mute");
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().filter(|tab| tab.muted).count()), 2);
    assert!(view.read_with(cx, |w, _| w.browser.all_tabs().find(|tab| tab.id == pane).unwrap().muted));
    let active = active_url(&view, cx);
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-close-pane");
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.archive.last().unwrap().tab.id.get(), 2);
    });
    assert!(fake.take_calls().contains(&Call::Close(1)));
}

#[gpui::test]
fn pane_menu_adds_at_the_clicked_pane_and_separates_it_with_its_existing_engine(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-add-left");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("https://left.example");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes[0], w.browser.active_tab().unwrap().id);
        assert_eq!(w.browser.active_split().unwrap().panes[1].get(), 2);
    });
    fake.take_calls();
    click(cx, "pane-menu-2");
    click(&mut tab_menu(&view, cx), "tab-menu-separate-pane");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn sidebar_split_menu_resets_group_name_and_separates_the_whole_group(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 2);
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-rename");
    bar(&view, cx).simulate_input("Comparison");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-reset-name");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().title.is_none()));
    right_click(cx, "tab-2");
    click(&mut tab_menu(&view, cx), "tab-menu-separate-all");
    assert!(view.read_with(cx, |w, _| w.browser.space().splits.is_empty()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 2);
}

#[gpui::test]
fn duplicate_command_creates_an_independent_page_beside_the_source(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let source = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id);
    let url = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "duplicate current tab");
    view.read_with(cx, |w, _| {
        assert_ne!(w.browser.active_tab().unwrap().id, source);
        assert_eq!(w.browser.space().today[0].id, source);
        assert_eq!(w.browser.space().today[1].id, w.browser.active_tab().unwrap().id);
    });
    assert_eq!(active_url(&view, cx), url);
    assert_eq!(bar_text(&view, cx), None);
    let calls = fake.take_calls();
    assert!(calls.iter().any(|call| matches!(call, Call::Create(2, _))));
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-w");
    cx.run_until_parked();
    assert!(!fake.take_calls().contains(&Call::Close(1)));
    click(cx, "tab-8");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), source);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..))));
}

#[gpui::test]
fn context_duplicate_targets_the_clicked_pin_and_preserves_its_original_url(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-duplicate");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().pinned.len(), 3);
        assert_eq!(w.browser.space().pinned[0].id.get(), 4);
        assert_eq!(w.browser.space().pinned[1].id, w.browser.active_tab().unwrap().id);
        assert_eq!(w.browser.active_tab().unwrap().pinned_url.as_deref(), Some("https://github.com"));
    });
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://github.com"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn duplicate_menu_copies_the_whole_split_and_closing_it_keeps_original_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let source = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Duplicate Current Tab");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().splits.len(), 2);
        let copy = w.browser.active_split().unwrap();
        assert_ne!(copy.id, source.id);
        assert_eq!(copy.panes.len(), 3);
        assert!(copy.panes.iter().all(|id| !source.panes.contains(id)));
    });
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 3);
    let anchor = view.read_with(cx, |w, _| w.browser.active_split().unwrap().anchor.get());
    click(cx, test_selector(format!("close-{anchor}")));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().id, source.id);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 3);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Close(..))).count(), 3);
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn native_duplicate_menu_dismisses_command_input_and_regular_context_menu_can_start_a_split(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    cx.simulate_keystrokes("cmd-t");
    dispatch_menu(&mut bar(&view, cx), "Tabs", "Duplicate Current Tab");
    assert_eq!(bar_text(&view, cx), None);
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-add-right");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("https://beside-pin.example");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert_ne!(w.browser.active_split().unwrap().anchor.get(), 4);
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id.get() == 4));
        assert_eq!(w.browser.space().today.iter().filter(|tab| w.browser.related_tabs(w.browser.active_tab().unwrap().id).contains(&tab.id)).count(), 2);
    });
}

#[gpui::test]
fn context_archive_above_removes_the_active_page_and_focuses_its_reference_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-7");
    click(cx, "tab-8");
    fake.take_calls();
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-archive-above");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.archive.len(), 1);
        assert_eq!(w.browser.archive[0].tab.id.get(), 8);
        assert_eq!(w.browser.space().pinned.len(), 2);
        assert_eq!(w.browser.space().today.len(), 2);
        assert_eq!(w.toast.as_ref().unwrap().message, "Archived 1 tab");
    });
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(1)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn context_archive_below_an_inactive_reference_keeps_the_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "tab-6");
    click(cx, "tab-8");
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-7");
    click(&mut tab_menu(&view, cx), "tab-menu-archive-below");
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive[0].tab.id.get()), 6);
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Close(2)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..) | Call::FocusPage(2))));
}

#[gpui::test]
fn archive_other_command_closes_a_split_atomically_and_restore_reopens_its_panes(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://reference.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    run_named_command(&view, cx, "archive other tabs");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.all_tabs().count(), 1);
        assert_eq!(w.browser.archive.len(), 1);
        assert_eq!(w.browser.archive[0].split.as_ref().unwrap().view.panes.len(), 3);
    });
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Close(..))).count(), 3);
    cx.simulate_keystrokes("cmd-shift-t");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.len()), 3);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 3);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 3);
}

#[gpui::test]
fn a_tall_context_menu_fits_a_small_window_and_keyboard_selection_scrolls_to_the_last_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(320.)));
    cx.run_until_parked();
    right_click(cx, "tab-4");
    let mut menu = tab_menu(&view, cx);
    let panel = menu.debug_bounds("tab-context-menu").unwrap();
    let size = menu.update(|window, _| window.bounds().size);
    assert!(panel.origin.y >= gpui::px(8.) && panel.bottom() <= size.height - gpui::px(8.));
    menu.simulate_keystrokes("up");
    menu.run_until_parked();
    let close = menu.debug_bounds("tab-menu-close").unwrap();
    assert!(close.origin.y >= panel.origin.y && close.bottom() <= panel.bottom());
    menu.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none() && w.browser.archive.is_empty()));
}

fn open_long_sidebar(cx: &mut TestAppContext, pinned: bool) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    let now = std::time::SystemTime::now();
    let target = browser.new_tab("https://target.example", now);
    if pinned { browser.toggle_pin(target); }
    else {
        let second = browser.new_tab("https://secondary.example", now);
        browser.add_split(target, second, browser_core::splits::Placement::Right, now).unwrap();
    }
    for index in 0..30 {
        let id = browser.new_tab(format!("https://sidebar{index}.example"), now);
        if pinned { browser.toggle_pin(id); }
    }
    if pinned {
        let target = browser.space().pinned.last().unwrap().id;
        browser.activate_tab(target, now);
        browser.spaces[0].pinned_collapsed = true;
    }
    // Focus the group after the standalone pages have taken focus.
    if !pinned {
        let second = browser.space().splits[0].panes[1];
        browser.activate_tab(second, now);
    }
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx, fake)
}

#[gpui::test]
fn reveal_tab_unfolds_and_scrolls_pinned_without_changing_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_long_sidebar(cx, true);
    let active = active_url(&view, cx);
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    fake.take_calls();
    run_named_command(&view, cx, "reveal tab in sidebar");
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(w.sidebar_visible);
        assert!(!w.browser.space().pinned_collapsed);
    });
    assert_eq!(active_url(&view, cx), active);
    let row = cx.debug_bounds("tab-32").unwrap();
    let list = cx.debug_bounds("pinned-list").unwrap();
    assert!(row.origin.y >= list.origin.y && row.bottom() <= list.bottom(), "row={row:?}, list={list:?}");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reveal_tab_scrolls_to_the_split_anchor_and_keeps_its_selected_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_long_sidebar(cx, false);
    let active = active_url(&view, cx);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Reveal Tab in Sidebar");
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let row = cx.debug_bounds("tab-2").unwrap();
    let list = cx.debug_bounds("today-list").unwrap();
    assert!(row.origin.y >= list.origin.y && row.bottom() <= list.bottom(), "row={row:?}, list={list:?}");
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 3);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reveal_tab_from_a_floating_sidebar_closes_the_overlay_and_restores_the_docked_sidebar(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    cx.simulate_keystrokes("cmd-s");
    cx.simulate_mouse_move(gpui::point(gpui::px(2.), gpui::px(200.)), None, Modifiers::none());
    let mut sidebar = floating(&view, cx).unwrap();
    let active = active_url(&view, cx);
    fake.take_calls();
    std::thread::sleep(Duration::from_millis(200));
    sidebar.update(|_, cx| cx.refresh_windows());
    sidebar.run_until_parked();
    click(&mut sidebar, "address");
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("reveal tab in sidebar");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.sidebar_visible && w.floating_sidebar.is_none()));
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(cx.cx.update(|cx| cx.windows().len()), 1);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn archive_reason_filters_preserve_the_query_and_combine_with_space_filters(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w");
    view.update(cx, |w, _| { w.browser.archive[0].reason = browser_core::ArchiveReason::Automatic; });
    open_archive(&view, cx);
    click(&mut bar(&view, cx), "archive-reason-automatic");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).as_slice(), [Suggestion::ArchivedTab { title, .. }] if title == "Chromium")));
    bar(&view, cx).simulate_input("chromium");
    click(&mut bar(&view, cx), "archive-reason-manual");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    assert_eq!(bar_text(&view, cx).as_deref(), Some("chromium"));
    click(&mut bar(&view, cx), "archive-reason-all");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 1);
    bar(&view, cx).simulate_keystrokes("cmd-a backspace");
    click(&mut bar(&view, cx), "archive-reason-manual");
    click(&mut bar(&view, cx), "archive-this-space");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).as_slice(), [Suggestion::ArchivedTab { title, .. }] if title == "GPUI")));
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.gpui.rs"));
    assert_eq!(view.read_with(cx, |w, _| w.browser.archive.len()), 1);
}

#[gpui::test]
fn archive_date_headings_do_not_change_keyboard_restore_indices(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-w cmd-w");
    view.update(cx, |w, _| {
        use chrono::{Local, TimeZone};
        let yesterday = Local::now().date_naive().pred_opt().unwrap().and_hms_opt(12, 0, 0).unwrap();
        w.browser.archive[0].archived_at = Local.from_local_datetime(&yesterday).single().unwrap().into();
    });
    open_archive(&view, cx);
    let mut overlay = bar(&view, cx);
    assert!(overlay.debug_bounds("archive-date-0").is_some());
    assert!(overlay.debug_bounds("archive-date-1").is_some());
    view.read_with(cx, |w, cx| {
        let rows = w.suggestions(cx);
        assert!(matches!(&rows[0], Suggestion::ArchivedTab { date, .. } if date == "Today"));
        assert!(matches!(&rows[1], Suggestion::ArchivedTab { date, .. } if date == "Yesterday"));
    });
    overlay.simulate_keystrokes("down enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://www.chromium.org"));
}

#[gpui::test]
fn archive_show_more_reaches_older_entries_and_scrolls_inside_a_short_window(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, _| {
        let now = std::time::SystemTime::now();
        for index in 0..20 {
            let id = w.browser.new_tab(format!("https://archived{index}.example"), now);
            w.browser.close_tab(id, now);
        }
    });
    cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(480.)));
    open_archive(&view, cx);
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).last(), Some(Suggestion::MoreArchive { remaining: 12 }))));
    let mut overlay = bar(&view, cx);
    for _ in 0..8 { overlay.simulate_keystrokes("down"); }
    overlay.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |w, cx| matches!(w.suggestions(cx).last(), Some(Suggestion::MoreArchive { remaining: 4 }))));
    for _ in 0..8 { overlay.simulate_keystrokes("down"); }
    overlay.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, cx| w.suggestions(cx).len()), 20);
    for _ in 0..3 { overlay.simulate_keystrokes("down"); }
    overlay.run_until_parked();
    let last = overlay.debug_bounds("suggestion-19").unwrap();
    let list = overlay.debug_bounds("command-suggestions").unwrap();
    assert!(last.origin.y >= list.origin.y && last.bottom() <= list.bottom());
    let panel = overlay.debug_bounds("command-bar").unwrap();
    let size = overlay.update(|window, _| window.bounds().size);
    assert!(panel.bottom() <= size.height);
    overlay.simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://archived0.example"));
}

#[gpui::test]
fn space_header_visibility_changes_only_the_current_space_and_survives_sessions(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    let before = cx.debug_bounds("pinned-list").unwrap().origin.y;
    fake.take_calls();
    run_named_command(&view, cx, "hide space header");
    let after = cx.debug_bounds("pinned-list").unwrap().origin.y;
    assert!(after < before);
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| {
        assert!(!w.browser.space().show_header);
        assert!(w.browser.spaces[1].show_header);
        let saved = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert!(!saved.space().show_header);
    });
    dispatch_menu(cx, "Spaces", "Show/Hide Space Header");
    assert!(view.read_with(cx, |w, _| w.browser.space().show_header));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn numbered_sidebar_shortcuts_can_exclude_and_include_favorites(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    dispatch_menu(cx, "Tabs", "Favorite/Unfavorite Tab");
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 8);
    run_named_command(&view, cx, "exclude favorites from ordering");
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 4);
    cx.simulate_keystrokes("cmd-9");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 6);
    view.read_with(cx, |w, _| {
        let saved = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert!(!saved.include_favorites_in_ordering);
    });
    dispatch_menu(cx, "Tabs", "Include/Exclude Favorites in Ordering");
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 8);
}

#[gpui::test]
fn hidden_cycle_shortcuts_work_from_ui_and_native_page_focus_and_keep_split_groups_together(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    let split_page = active_url(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("https://next.example");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.simulate_keystrokes("ctrl-cmd-left");
    assert_eq!(active_url(&view, cx), split_page);
    cx.simulate_keystrokes("ctrl-cmd-right");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://next.example"));
    let shortcut = |key: &str| Shortcut { ctrl: true, cmd: true, key: key.into(), ..Default::default() };
    assert!(crate::window::page_shortcuts().contains(&shortcut("left")));
    assert!(crate::window::page_shortcuts().contains(&shortcut("right")));
    fake.send(cx, Event::Shortcut(shortcut("left")));
    assert_eq!(active_url(&view, cx), split_page);
    fake.send(cx, Event::Shortcut(shortcut("right")));
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://next.example"));
}

#[gpui::test]
fn tab_icon_editor_accepts_complex_emoji_and_invalid_input_does_not_change_the_icon(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "customize tab icon");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("two symbols");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(bar_text(&view, cx).is_some());
    bar(&view, cx).simulate_keystrokes("cmd-a");
    bar(&view, cx).simulate_input("👨‍👩‍👧‍👦");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.item_icon(w.browser.active_tab().unwrap().id).map(str::to_string)).as_deref(), Some("👨‍👩‍👧‍👦"));
    assert!(cx.debug_bounds("item-icon-8").is_some());
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn context_icon_editor_targets_an_inactive_tab_and_reset_name_and_icon_restores_its_defaults(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-icon");
    bar(&view, cx).simulate_input("★");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(active_url(&view, cx), active);
    view.read_with(cx, |w, _| assert_eq!(w.browser.space().pinned[0].custom_icon.as_deref(), Some("★")));
    right_click(cx, "tab-4");
    click(&mut tab_menu(&view, cx), "tab-menu-reset-customization");
    assert_eq!(active_url(&view, cx), active);
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned[0].custom_icon.is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn split_group_icon_survives_favorite_and_separation_reveals_the_original_page_icons(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    dispatch_menu(cx, "Tabs", "Customize Tab Icon…");
    bar(&view, cx).simulate_input("🔬");
    bar(&view, cx).simulate_keystrokes("enter");
    run_named_command(&view, cx, "favorite tab");
    view.read_with(cx, |w, _| assert_eq!(w.browser.active_split().unwrap().custom_icon.as_deref(), Some("🔬")));
    cx.run_until_parked();
    assert!(cx.debug_bounds("item-icon-2").is_some());
    dispatch_menu(cx, "Tabs", "Reset Name and Icon");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().unwrap().custom_icon.is_none()));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_icon_picker_persists_symbols_and_blank_input_restores_the_default(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    bar(&view, cx).simulate_input("🧭");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("space-icon-0").is_some());
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().custom_icon.as_deref(), Some("🧭"));
        assert_eq!(w.browser.spaces[1].custom_icon, None);
        let saved = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(saved.space().custom_icon.as_deref(), Some("🧭"));
    });
    click(cx, "space-1");
    run_named_command(&view, cx, "change space icon");
    bar(&view, cx).simulate_input("👩🏽‍💻");
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().custom_icon.is_none()));
    click(cx, "space-0");
    dispatch_menu(cx, "Spaces", "Change Space Icon…");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("🧭"));
    bar(&view, cx).simulate_keystrokes("backspace enter");
    assert!(view.read_with(cx, |w, _| w.browser.space().custom_icon.is_none()));
}

fn open_folder_pages(cx: &mut TestAppContext) -> (Entity<BrowserWindow>, &mut VisualTestContext, FakeEngine) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    let now = std::time::SystemTime::now();
    let space = browser.space().id;
    let root = browser.create_folder(space, None, "Projects").unwrap();
    let nested = browser.create_folder(space, Some(root), "Research").unwrap();
    let a = browser.new_tab("https://a.example", now);
    browser.set_title(a, "A");
    browser.toggle_pin(a);
    let b = browser.new_tab("https://b.example", now);
    browser.set_title(b, "B");
    browser.add_split(a, b, browser_core::splits::Placement::Right, now).unwrap();
    browser.move_item_to_folder(a, nested);
    browser.new_tab("https://outside.example", now);
    browser.activate_tab(b, now);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    (view, cx, fake)
}

#[gpui::test]
fn folder_creation_and_nested_creation_keep_the_page_and_cancel_without_creating_items(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "new folder");
    assert!(view.read_with(cx, |w, cx| w.suggestions(cx).is_empty()));
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.is_empty()));
    dispatch_menu(cx, "Spaces", "New Folder…");
    bar(&view, cx).simulate_input("Projects");
    bar(&view, cx).simulate_keystrokes("enter");
    click(cx, "folder-new-9");
    bar(&view, cx).simulate_input("Research");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().folders.len(), 2);
        assert_eq!(w.browser.folder_path(w.browser.space().folders[1].id), "Projects / Research");
    });
    assert!(cx.debug_bounds("folder-10").unwrap().origin.x > cx.debug_bounds("folder-9").unwrap().origin.x);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn context_new_folder_pins_the_clicked_tab_while_preserving_the_other_active_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "tab-7");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-move-to");
    click(&mut menu, "tab-menu-new-folder-with-tab");
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        let folder = &w.browser.space().folders[0];
        assert_eq!(folder.name, "Reading");
        assert_eq!(w.browser.folder_tabs(folder.id).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![7]);
        assert!(!w.browser.space().today.iter().any(|tab| tab.id.get() == 7));
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(cx.debug_bounds("tab-7").unwrap().origin.x > cx.debug_bounds("folder-9").unwrap().origin.x);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn collapsing_a_folder_keeps_its_active_split_visible_and_reveal_unfolds_its_ancestors(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    fake.take_calls();
    click(cx, "folder-2");
    view.read_with(cx, |w, _| {
        assert!(w.browser.space().folders[0].collapsed);
        assert_eq!(w.browser.pinned_rows(w.browser.space().id, false).len(), 2);
    });
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 5);
    assert!(cx.debug_bounds("tab-4").is_some());
    click(cx, "tab-7");
    fake.take_calls();
    assert_eq!(view.read_with(cx, |w, _| w.browser.pinned_rows(w.browser.space().id, false).len()), 1);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("b.example");
    bar(&view, cx).simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id.get()), 5);
    assert!(view.read_with(cx, |w, _| w.browser.space().folders[0].collapsed));
    run_named_command(&view, cx, "reveal tab in sidebar");
    view.read_with(cx, |w, _| {
        assert!(w.browser.space().folders.iter().all(|folder| !folder.collapsed));
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 5);
    });
    assert!(cx.debug_bounds("tab-4").unwrap().origin.x > cx.debug_bounds("folder-3").unwrap().origin.x);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn moving_a_split_to_a_closed_folder_in_another_space_keeps_its_engine_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, _| {
        let space = w.browser.add_space("Work", 100.);
        let folder = w.browser.create_folder(space, None, "Saved").unwrap();
        w.browser.set_folder_collapsed(folder, true);
        folder
    });
    fake.take_calls();
    run_named_command(&view, cx, "move to folder");
    bar(&view, cx).simulate_input("Work");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert!(w.browser.folder(destination).unwrap().collapsed);
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 5);
        assert_eq!(w.browser.folder_ancestors(w.browser.active_tab().unwrap().id), vec![destination]);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    right_click(cx, "folder-9");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-move");
    bar(&view, cx).simulate_input("pinned in personal");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Personal");
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 5);
        assert_eq!(w.browser.folder_space(destination), Some(w.browser.space().id));
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn folder_context_actions_rename_customize_and_copy_all_nested_page_links(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "folder-3");
    tab_menu(&view, cx).simulate_keystrokes("down enter");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Research"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::FocusPage(_))), "the closing menu must not focus Chromium behind its replacement editor");
    bar(&view, cx).simulate_input("References");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "folder-3");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-icon");
    bar(&view, cx).simulate_input("📚");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        let folder = &w.browser.space().folders[1];
        assert_eq!(w.browser.folder_path(folder.id), "Projects / References");
        assert_eq!(folder.custom_icon.as_deref(), Some("📚"));
    });
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-copy-markdown");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("[A](https://a.example)\n[B](https://b.example)"));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn folder_destination_picker_rejects_cycles_and_empty_folder_deletion_keeps_other_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-move");
    view.read_with(cx, |w, cx| {
        let root = w.browser.space().folders[0].id;
        let nested = w.browser.space().folders[1].id;
        assert!(!w.suggestions(cx).iter().any(|suggestion| matches!(suggestion, Suggestion::FolderDestination { parent: Some(parent), .. } if *parent == root || *parent == nested)));
    });
    bar(&view, cx).simulate_keystrokes("escape");
    run_named_command(&view, cx, "new folder");
    bar(&view, cx).simulate_input("Empty");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "folder-8");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-delete");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().folders.len()), 2);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn named_folder_expansion_and_collapse_all_leave_the_selected_page_intact(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "collapse all folders");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.iter().all(|folder| folder.collapsed)));
    run_named_command(&view, cx, "expand folder Projects / Research");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.iter().all(|folder| !folder.collapsed)));
    dispatch_menu(cx, "Tabs", "Collapse All Folders");
    dispatch_menu(cx, "Tabs", "Expand All Folders");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.iter().all(|folder| !folder.collapsed)));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn moving_to_a_folder_when_none_exist_creates_one_around_the_active_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "move to folder");
    assert!(bar_input_focused(&view, cx));
    assert!(view.read_with(cx, |w, _| matches!(w.command_bar.as_ref().unwrap().target, crate::window::Target::NewFolder { .. })));
    bar(&view, cx).simulate_input("Current project");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_tab().unwrap().id.get(), 8);
        assert_eq!(w.browser.folder_tabs(w.browser.space().folders[0].id).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![8]);
        assert_eq!(w.browser.space().pinned.iter().find(|tab| tab.id.get() == 8).unwrap().pinned_url.as_deref(), active.as_deref());
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn duplicating_a_closed_folder_keeps_focus_and_loads_independent_split_pages_only_when_selected(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let active = active_url(&view, cx);
    run_named_command(&view, cx, "collapse all folders");
    fake.take_calls();
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-duplicate");
    cx.run_until_parked();
    let (folder, nested, copy, focused) = view.read_with(cx, |w, _| {
        let folders = &w.browser.space().folders;
        assert_eq!(folders.len(), 4);
        let copy = folders[2].id;
        let nested = folders[3].id;
        assert!(folders[2].collapsed && folders[3].collapsed);
        let pages = w.browser.folder_tabs(copy);
        assert_eq!(pages.len(), 2);
        assert_eq!(w.browser.all_tabs().count(), 5);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        let focused = w.browser.split_for(pages[0]).unwrap().active;
        assert!(!pages.iter().any(|id| id.get() == 4 || id.get() == 5));
        (copy, nested, pages[0], focused)
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    click(cx, Box::leak(format!("folder-{}", folder.get()).into_boxed_str()));
    click(cx, Box::leak(format!("folder-{}", nested.get()).into_boxed_str()));
    click(cx, Box::leak(format!("tab-{}", copy.get()).into_boxed_str()));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab, Some(focused));
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        let restored = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(restored.folder_tabs(folder), w.browser.folder_tabs(folder));
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 2);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn duplicating_an_empty_folder_preserves_the_active_page_and_can_be_deleted_independently(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    run_named_command(&view, cx, "new folder");
    bar(&view, cx).simulate_input("Empty");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    right_click(cx, "folder-9");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-duplicate");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().folders.len()), 2);
    right_click(cx, "folder-10");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-delete");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().folders.iter().map(|folder| folder.id.get()).collect::<Vec<_>>()), vec![9]);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn turning_a_folder_into_a_space_keeps_the_active_split_and_its_native_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let surfaces = view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout());
    fake.take_calls();
    right_click(cx, "folder-2");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-into-space");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Projects");
        assert_eq!(w.browser.spaces.len(), 2);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout(), surfaces);
        assert_eq!(w.browser.space().folders.len(), 1);
        assert_eq!(w.browser.space().folders[0].id.get(), 3);
        assert!(w.browser.spaces[0].pinned.is_empty());
        assert_eq!(w.browser.spaces[0].today[0].id.get(), 7);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn turning_an_empty_folder_into_a_space_leaves_other_tabs_and_their_engine_views_intact(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    run_named_command(&view, cx, "new folder");
    bar(&view, cx).simulate_input("Empty");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    fake.take_calls();
    right_click(cx, "folder-9");
    click(&mut tab_menu(&view, cx), "tab-menu-folder-into-space");
    cx.run_until_parked();
    assert!(active_url(&view, cx).is_none());
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Empty");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("alt-cmd-right");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pin_to_a_named_nested_folder_in_another_space_preserves_the_active_split_and_closed_folders(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, _| {
        let work = w.browser.add_space("Work", 100.);
        let root = w.browser.create_folder(work, None, "Saved").unwrap();
        let nested = w.browser.create_folder(work, Some(root), "Reference").unwrap();
        w.browser.set_folder_collapsed(root, true);
        w.browser.set_folder_collapsed(nested, true);
        nested
    });
    fake.take_calls();
    run_named_command(&view, cx, "pin to saved / reference");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).last(), Some(&destination));
        assert!(w.browser.space().folders.iter().all(|folder| folder.collapsed));
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.command_bar.is_none());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pin_to_new_folder_edits_a_name_then_pins_the_current_page_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    run_named_command(&view, cx, "pin to new folder");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders.is_empty()));
    run_named_command(&view, cx, "pin to new folder");
    bar(&view, cx).simulate_input("Reading");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        let folder = &w.browser.space().folders[0];
        assert_eq!(folder.name, "Reading");
        assert_eq!(w.browser.folder_tabs(folder.id), vec![w.browser.space().active_tab.unwrap()]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn named_folder_commands_disambiguate_duplicate_names_by_space_and_skip_the_current_parent(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, cx| {
        let work = w.browser.add_space("Work", 100.);
        let folder = w.browser.create_folder(work, None, "Research").unwrap();
        let outside = w.browser.space().today[0].id;
        w.browser.activate_tab(outside, std::time::SystemTime::now());
        cx.notify();
        folder
    });
    cx.run_until_parked();
    fake.take_calls();
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin to research");
    let index = view.read_with(cx, |w, cx| {
        let suggestions = w.suggestions(cx);
        let labels: Vec<_> = suggestions.iter().filter_map(|row| match row { Suggestion::Action { command: crate::window::Command::PinToFolder(_), label } => Some(label.as_str()), _ => None }).collect();
        assert_eq!(labels, vec!["Pin to Projects / Research · Personal", "Pin to Research · Work"]);
        suggestions.iter().position(|row| matches!(row, Suggestion::Action { command: crate::window::Command::PinToFolder(id), .. } if *id == destination)).unwrap()
    });
    for _ in 0..index { bar(&view, cx).simulate_keystrokes("down"); }
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("pin to research");
    assert!(!view.read_with(cx, |w, cx| w.suggestions(cx).iter().any(|row| matches!(row, Suggestion::Action { command: crate::window::Command::PinToFolder(id), .. } if *id == destination))));
}

#[gpui::test]
fn space_header_context_menu_can_rename_or_cancel_without_changing_the_active_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Personal"));
    bar(&view, cx).simulate_input("Cancel this");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Personal");
    right_click(cx, "space-header-0");
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    bar(&view, cx).simulate_input("Home");
    bar(&view, cx).simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Home");
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn inactive_space_context_actions_change_only_the_clicked_spaces_name_and_icon(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-1");
    click(&mut tab_menu(&view, cx), "tab-menu-space-icon");
    assert!(bar_input_focused(&view, cx));
    bar(&view, cx).simulate_input("🧭");
    bar(&view, cx).simulate_keystrokes("enter");
    right_click(cx, "space-1");
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Work"));
    bar(&view, cx).simulate_input("Office");
    bar(&view, cx).simulate_keystrokes("enter");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_space, 0);
        assert_eq!(w.browser.spaces[1].name, "Office");
        assert_eq!(w.browser.spaces[1].custom_icon.as_deref(), Some("🧭"));
        assert!(w.browser.spaces[0].custom_icon.is_none());
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_context_menu_remains_available_in_the_dock_after_hiding_its_header(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let active = active_url(&view, cx);
    let pinned_top = cx.debug_bounds("pinned-list").unwrap().top();
    fake.take_calls();
    // The reference's Space menu has no header toggle; the command remains.
    run_named_command(&view, cx, "hide space header");
    assert!(!view.read_with(cx, |w, _| w.browser.space().show_header));
    assert!(cx.debug_bounds("pinned-list").unwrap().top() < pinned_top);
    right_click(cx, "space-0");
    assert!(tab_menu(&view, cx).debug_bounds("tab-menu-space-rename").is_some());
    cx.simulate_keystrokes("escape");
    view.update(cx, |w, cx| w.close_tab_menu(cx));
    cx.run_until_parked();
    run_named_command(&view, cx, "show space header");
    assert!(view.read_with(cx, |w, _| w.browser.space().show_header));
    assert_eq!(cx.debug_bounds("pinned-list").unwrap().top(), pinned_top);
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn new_folder_from_an_inactive_space_menu_uses_that_space_and_retains_the_source_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    right_click(cx, "space-1");
    click(&mut tab_menu(&view, cx), "tab-menu-space-new-folder");
    assert!(bar_input_focused(&view, cx));
    assert_eq!(active_url(&view, cx), original);
    bar(&view, cx).simulate_input("Projects");
    bar(&view, cx).simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().folders[0].name, "Projects");
        assert!(w.browser.spaces[0].folders.is_empty());
        assert_eq!(w.browser.all_tabs().count(), 5);
    });
    cx.simulate_keystrokes("alt-cmd-left");
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

fn drag_between(cx: &mut VisualTestContext, source: &'static str, destination: &'static str) {
    cx.run_until_parked();
    let end = cx.debug_bounds(destination).unwrap().center();
    drag_to_point(cx, source, end);
}

fn drag_to_point(cx: &mut VisualTestContext, source: &'static str, end: gpui::Point<gpui::Pixels>) {
    begin_drag_to_point(cx, source, end);
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn begin_drag_to_point(cx: &mut VisualTestContext, source: &'static str, end: gpui::Point<gpui::Pixels>) {
    cx.run_until_parked();
    let start = cx.debug_bounds(source).unwrap().center();
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(start + gpui::point(gpui::px(5.), gpui::px(0.)), gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn drag_to_row_edge(cx: &mut VisualTestContext, source: &'static str, destination: &'static str, after: bool) {
    cx.run_until_parked();
    let bounds = cx.debug_bounds(destination).unwrap();
    let y = if after { bounds.bottom() - gpui::px(4.) } else { bounds.top() + gpui::px(4.) };
    drag_to_point(cx, source, gpui::point(bounds.center().x, y));
}

#[gpui::test]
fn dragging_the_active_space_reorders_the_dock_without_switching_pages_or_animating_other_spaces(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "space-0", "space-2");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.iter().map(|space| space.name.as_str()).collect::<Vec<_>>(), vec!["Work", "Research", "Personal"]);
        assert_eq!(w.browser.active_space, 2);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.space().name, "Personal");
        assert_eq!(loaded.active_space, 2);
    });
    let header = cx.debug_bounds("space-header-2").unwrap();
    assert!(header.left() >= gpui::px(0.) && header.right() <= gpui::px(240.));
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_an_inactive_space_keeps_the_focused_split_and_all_its_page_views(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.browser.add_space("Work", 100.); w.browser.add_space("Research", 200.); cx.notify(); });
    cx.run_until_parked();
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "space-2", "space-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.iter().map(|space| space.name.as_str()).collect::<Vec<_>>(), vec!["Research", "Personal", "Work"]);
        assert_eq!(w.browser.active_space, 1);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    let header = cx.debug_bounds("space-header-1").unwrap();
    assert!(header.left() >= gpui::px(0.) && header.right() <= gpui::px(240.));
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_space_outside_the_dock_cancels_and_leaves_click_navigation_working(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "space-2", "address");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.spaces.iter().map(|space| space.name.as_str()).collect::<Vec<_>>(), vec!["Personal", "Work", "Research"]);
        assert_eq!(w.browser.active_space, 0);
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    click(cx, "space-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_today_tab_into_a_closed_folder_pins_it_and_reuses_the_other_loaded_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { let folder = w.browser.space().folders[1].id; w.browser.set_folder_collapsed(folder, true); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-7", "folder-3");
    view.read_with(cx, |w, _| {
        let tab = w.browser.space().active_tab.unwrap();
        assert_eq!(tab.get(), 7);
        assert!(w.browser.space().today.is_empty());
        assert_eq!(w.browser.folder_ancestors(tab).last().unwrap().get(), 3);
        assert!(w.browser.space().folders[1].collapsed);
        assert_eq!(w.browser.all_tabs().count(), 3);
        assert_eq!(w.browser.space().pinned.iter().find(|tab| tab.id.get() == 7).unwrap().pinned_url.as_deref(), Some("https://outside.example"));
        assert_eq!(w.browser.space().splits[0].active.get(), 5);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    run_named_command(&view, cx, "expand folder projects / research");
    click(cx, "tab-4");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert_eq!(view.read_with(cx, |w, _| w.pages.as_ref().unwrap().borrow().surface_layout().len()), 2);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_split_into_another_closed_folder_moves_the_complete_item_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let folder = view.update(cx, |w, cx| {
        let folder = w.browser.create_folder(w.browser.space().id, None, "Saved").unwrap();
        w.browser.set_folder_collapsed(folder, true);
        cx.notify();
        folder
    });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "tab-4", "folder-8");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.folder_tabs(folder).iter().map(|tab| tab.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert!(w.browser.folder(folder).unwrap().collapsed);
        assert!(w.browser.space().folders[1].items.is_empty());
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.folder_tabs(folder), w.browser.folder_tabs(folder));
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_favorite_into_a_folder_preserves_the_loaded_page_and_original_url(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    run_named_command(&view, cx, "favorite tab");
    let folder = view.update(cx, |w, cx| {
        let folder = w.browser.create_folder(w.browser.space().id, None, "Saved").unwrap();
        w.browser.set_folder_collapsed(folder, true);
        cx.notify();
        folder
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "favorite-8", "folder-9");
    view.read_with(cx, |w, _| {
        assert!(w.browser.favorites.is_empty());
        assert_eq!(w.browser.folder_tabs(folder).iter().map(|tab| tab.get()).collect::<Vec<_>>(), vec![8]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert!(w.browser.folder(folder).unwrap().collapsed);
        assert_eq!(w.browser.space().pinned.iter().find(|tab| tab.id.get() == 8).unwrap().pinned_url.as_deref(), original.as_deref());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn cancelling_an_inactive_tab_drag_does_not_activate_or_load_it(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    let before = today_urls(&view, cx);
    fake.take_calls();
    drag_between(cx, "tab-6", "address");
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(today_urls(&view, cx), before);
    assert!(bar_text(&view, cx).is_none());
    click(cx, "tab-8");
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_folder_with_the_focused_split_moves_its_subtree_without_recreating_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let destination = view.update(cx, |w, cx| {
        let folder = w.browser.create_folder(w.browser.space().id, None, "Saved").unwrap();
        w.browser.set_folder_collapsed(folder, true);
        cx.notify();
        folder
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "folder-2", "folder-8");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![8, 2, 3]);
        assert_eq!(w.browser.folder_tabs(destination).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert!(w.browser.folder(destination).unwrap().collapsed);
        assert_eq!(w.browser.all_tabs().count(), 3);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.folder_tabs(destination), w.browser.folder_tabs(destination));
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn folder_drags_onto_self_or_descendants_are_rejected_without_toggling_disclosure(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let before = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    fake.take_calls();
    drag_between(cx, "folder-2", "folder-3");
    assert_eq!(view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser)), before);
    drag_between(cx, "folder-2", "folder-2");
    assert_eq!(view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser)), before);
    click(cx, "folder-3");
    assert!(view.read_with(cx, |w, _| w.browser.space().folders[1].collapsed));
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_an_empty_inactive_folder_preserves_the_active_item_and_the_destination_stays_closed(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let (empty, destination) = view.update(cx, |w, cx| {
        let empty = w.browser.create_folder(w.browser.space().id, None, "Empty").unwrap();
        let destination = w.browser.create_folder(w.browser.space().id, None, "Destination").unwrap();
        w.browser.set_folder_collapsed(destination, true);
        cx.notify();
        (empty, destination)
    });
    cx.run_until_parked();
    let active = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "folder-8", "folder-9");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.folder_path(empty), "Destination / Empty");
        assert!(w.browser.folder(destination).unwrap().collapsed);
        assert!(w.browser.folder(empty).unwrap().items.is_empty());
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_between_pinned_and_today_keeps_page_identity_and_does_not_trigger_header_clicks(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "tab-8", "space-header-0");
    view.read_with(cx, |w, _| {
        assert!(!w.browser.space().pinned_collapsed);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert_eq!(w.browser.space().pinned.last().unwrap().pinned_url.as_deref(), original.as_deref());
    });
    drag_between(cx, "tab-8", "today-header");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        assert!(w.browser.space().today[0].pinned_url.is_none());
        assert_eq!(w.browser.all_tabs().count(), 5);
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn pinned_root_drop_targets_extract_split_items_and_nested_folders_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "tab-4", "space-header-0");
    view.read_with(cx, |w, _| {
        assert!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).is_empty());
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert!(w.browser.space().folders[1].items.is_empty());
    });
    drag_between(cx, "folder-3", "space-header-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.folder_path(w.browser.space().folders[1].id), "Research");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_on_a_space_header_creates_the_first_pin_without_creating_a_new_page(cx: &mut TestAppContext) {
    cx.update(init);
    let fake = FakeEngine::default();
    let mut browser = browser_core::Browser::new();
    browser.new_tab("https://single.example", std::time::SystemTime::now());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = BrowserWindow::new(browser, Some(Box::new(fake.clone())), window, cx);
        view.focus(window);
        view
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-2", "space-header-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().pinned[0].id.get(), 2);
        assert_eq!(w.browser.space().pinned[0].pinned_url.as_deref(), Some("https://single.example"));
        assert!(w.browser.space().today.is_empty());
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 2);
        assert!(w.command_bar.is_none());
    });
    assert!(cx.debug_bounds("pinned-list").is_some());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_into_the_favorites_dock_and_back_to_today_reuses_the_live_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let pin = w.browser.space().pinned[0].id;
        w.browser.favorite_tab(pin);
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    // A lone favorite fills the row, so the drop lands on its tile.
    drag_between(cx, "tab-8", "favorites");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
    });
    drag_between(cx, "favorite-8", "new-tab");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4]);
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        assert!(w.browser.space().today[0].pinned_url.is_none());
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_full_favorites_dock_rejects_a_drop_without_moving_or_loading_the_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        for index in 0..browser_core::favorites::MAX_FAVORITES {
            let id = w.browser.new_tab(format!("https://favorite{index}.example"), std::time::SystemTime::now());
            assert!(w.browser.favorite_tab(id));
        }
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-8", "favorites");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorite_count(), browser_core::favorites::MAX_FAVORITES);
        assert!(w.browser.space().today.iter().any(|tab| tab.id.get() == 8));
        assert!(!w.browser.is_favorite(w.browser.space().active_tab.unwrap()));
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_split_into_a_scrolled_today_list_reveals_its_new_first_row_and_keeps_both_pages(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        for index in 0..40 { w.browser.new_tab(format!("https://long{index}.example"), std::time::SystemTime::now()); }
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    scroll(cx, "today-list", gpui::TouchPhase::Moved, 0., -5000.);
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-4", "new-tab");
    let row = cx.debug_bounds("tab-4").unwrap();
    let list = cx.debug_bounds("today-list").unwrap();
    assert!(row.top() >= list.top() && row.bottom() <= list.bottom());
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().today[0].id.get(), 4);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert!(w.browser.space().today.iter().filter(|tab| tab.id.get() == 4 || tab.id.get() == 5).all(|tab| tab.pinned_url.is_none()));
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.command_bar.is_none());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reordering_favorite_icons_preserves_the_current_page_for_both_inactive_and_active_items(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let pins: Vec<_> = w.browser.space().pinned.iter().map(|tab| tab.id).collect();
        for tab in pins { w.browser.favorite_tab(tab); }
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "favorite-5", "favorite-4");
    assert_eq!(view.read_with(cx, |w, _| w.browser.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![5, 4]);
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    click(cx, "favorite-5");
    fake.take_calls();
    drag_between(cx, "favorite-5", "favorite-4");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn reordering_a_favorite_split_keeps_pane_focus_and_numbered_navigation_uses_the_new_order(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    run_named_command(&view, cx, "favorite tab");
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let other = w.browser.space().today[0].id;
        w.browser.favorite_tab(other);
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "favorite-4", "favorite-7");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 4, 5]);
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("cmd-1");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 7);
    fake.take_calls();
    cx.simulate_keystrokes("cmd-2");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_new_favorite_on_an_icon_inserts_it_at_that_position_without_copying_the_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let pin = w.browser.space().pinned[0].id;
        w.browser.favorite_tab(pin);
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-8", "favorite-4");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4]);
        assert_eq!(w.browser.all_tabs().count(), 5);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        let loaded = browser_core::session::from_json(&browser_core::session::to_json(&w.browser)).unwrap();
        assert_eq!(loaded.favorites.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4]);
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_full_favorites_dock_still_allows_existing_items_to_be_reordered(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        for index in 0..browser_core::favorites::MAX_FAVORITES {
            let id = w.browser.new_tab(format!("https://favorite{index}.example"), std::time::SystemTime::now());
            w.browser.favorite_tab(id);
        }
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "favorite-9", "favorite-20");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.favorite_count(), browser_core::favorites::MAX_FAVORITES);
        assert_eq!(w.browser.favorites.last().unwrap().id.get(), 9);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_today_rows_before_or_after_an_item_preserves_the_loaded_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_to_row_edge(cx, "tab-8", "tab-6", true);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![7, 6, 8]);
    drag_to_row_edge(cx, "tab-8", "tab-7", false);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![8, 7, 6]);
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_split_after_a_folder_sibling_and_before_today_keeps_both_panes_and_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| {
        let active = w.browser.space().active_tab.unwrap();
        let folder = w.browser.space().folders[1].id;
        let other = w.browser.new_tab("https://sibling.example", std::time::SystemTime::now());
        w.browser.move_item_to_folder(other, folder);
        w.browser.activate_tab(active, std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_to_row_edge(cx, "tab-4", "tab-8", true);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![8, 4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().id, split);
    });
    drag_to_row_edge(cx, "tab-4", "tab-7", false);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5, 7]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert!(w.browser.space().today.iter().filter(|tab| tab.id.get() == 4 || tab.id.get() == 5).all(|tab| tab.pinned_url.is_none()));
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.command_bar.is_none());
    });
    let row = cx.debug_bounds("tab-4").unwrap();
    let list = cx.debug_bounds("today-list").unwrap();
    assert!(row.top() >= list.top() && row.bottom() <= list.bottom());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_before_a_closed_folder_peek_item_pins_in_that_folder_and_preserves_its_split(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { let folder = w.browser.space().folders[1].id; w.browser.set_folder_collapsed(folder, true); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_to_row_edge(cx, "tab-7", "tab-4", false);
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 7);
        assert_eq!(w.browser.folder_ancestors(w.browser.space().active_tab.unwrap()).last().unwrap().get(), 3);
        assert!(w.browser.space().folders[1].collapsed);
        assert_eq!(w.browser.space().splits[0].active.get(), 5);
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 4, 5]);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    run_named_command(&view, cx, "expand folder projects / research");
    click(cx, "tab-4");
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_favorite_after_a_today_row_unfavorites_without_reloading_or_opening_the_target(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    run_named_command(&view, cx, "favorite tab");
    fake.take_calls();
    drag_to_row_edge(cx, "favorite-8", "tab-6", true);
    view.read_with(cx, |w, _| {
        assert!(w.browser.favorites.is_empty());
        assert_eq!(w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![7, 6, 8]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 8);
        assert!(w.browser.space().today.last().unwrap().pinned_url.is_none());
        assert!(w.command_bar.is_none());
    });
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_an_inactive_today_tab_focuses_the_moved_page_and_keeps_the_previous_page_loaded(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_to_row_edge(cx, "tab-6", "tab-8", false);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().today.iter().map(|tab| tab.id.get()).collect::<Vec<_>>()), vec![6, 8, 7]);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 6);
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Create(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Close(..) | Call::Navigate(..))));
    click(cx, "tab-8");
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_today_tab_on_a_space_icon_preserves_its_section_and_live_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let original = active_url(&view, cx);
    fake.take_calls();
    drag_between(cx, "tab-8", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().today[0].id.get(), 8);
        assert!(w.browser.space().pinned.is_empty());
        assert!(w.browser.space().today[0].pinned_url.is_none());
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_pinned_split_on_a_space_icon_moves_all_panes_without_recreating_them(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.browser.add_space("Work", 100.); cx.notify(); });
    cx.run_until_parked();
    let split = view.read_with(cx, |w, _| w.browser.active_split().unwrap().id);
    fake.take_calls();
    drag_between(cx, "tab-4", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().id, split);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.browser.spaces[0].pinned.is_empty());
        assert_eq!(w.browser.spaces[0].today[0].id.get(), 7);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

fn add_drag_destination(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |w, cx| {
        let work = w.browser.add_space("Work", 100.);
        let folder = w.browser.create_folder(work, None, "Saved").unwrap();
        w.browser.set_folder_collapsed(folder, true);
        w.browser.switch_space(1);
        w.browser.new_tab("https://work.example", std::time::SystemTime::now());
        w.browser.switch_space(0);
        cx.notify();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn hovering_a_space_during_drag_waits_then_opens_it_and_accepts_a_split_in_its_closed_folder(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    add_drag_destination(&view, cx);
    fake.take_calls();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "tab-4", dock);
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Personal");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Create(..))).count(), 1, "hover loads only the destination's current page");
    let destination = cx.debug_bounds("folder-9").unwrap().center();
    cx.simulate_mouse_move(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert!(w.browser.space().folders[0].collapsed);
        assert_eq!(w.browser.folder_tabs(w.browser.space().folders[0].id).iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.browser.spaces[0].pinned.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn leaving_a_space_icon_before_its_drag_hover_delay_cancels_navigation_and_loading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.browser.add_space("Work", 100.); cx.notify(); });
    cx.run_until_parked();
    let original = active_url(&view, cx);
    fake.take_calls();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "tab-7", dock);
    cx.executor().advance_clock(Duration::from_millis(250));
    let outside = cx.debug_bounds("address").unwrap().center();
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Personal");
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(active_url(&view, cx), original);
    assert!(bar_text(&view, cx).is_none());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dropping_a_folder_on_another_space_moves_the_complete_focused_subtree(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    view.update(cx, |w, cx| { w.browser.add_space("Work", 100.); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "folder-2", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.space().folders.len(), 2);
        assert!(w.browser.spaces[0].folders.is_empty());
        assert_eq!(w.browser.active_split().unwrap().panes.iter().map(|id| id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn hovering_with_an_empty_folder_then_nesting_it_preserves_the_destinations_current_page(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    add_drag_destination(&view, cx);
    view.update(cx, |w, cx| { w.browser.create_folder(w.browser.space().id, None, "Empty"); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "folder-11", dock);
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().name.clone()), "Work");
    fake.take_calls();
    let destination = cx.debug_bounds("folder-9").unwrap().center();
    cx.simulate_mouse_move(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(destination, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 10);
        assert!(w.browser.space().folders[0].collapsed);
        assert_eq!(w.browser.folder_path(w.browser.space().folders[1].id), "Saved / Empty");
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    cx.simulate_keystrokes("alt-cmd-left");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 5);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dragging_a_favorite_split_to_a_space_icon_turns_it_into_local_pins_without_losing_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    run_named_command(&view, cx, "favorite tab");
    view.update(cx, |w, cx| { w.browser.add_space("Work", 100.); cx.notify(); });
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "favorite-4", "space-1");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert!(w.browser.favorites.is_empty());
        assert_eq!(w.browser.space().pinned.iter().map(|tab| tab.id.get()).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn hovering_with_a_focused_folder_then_dropping_it_restores_its_original_split_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    add_drag_destination(&view, cx);
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "folder-2", dock);
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 10);
    fake.take_calls();
    cx.simulate_mouse_up(dock, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
        assert!(w.browser.spaces[0].pinned.is_empty());
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn a_shared_favorite_drag_keeps_its_source_pane_when_hovering_another_spaces_pane(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_folder_pages(cx);
    run_named_command(&view, cx, "favorite tab");
    view.update(cx, |w, cx| {
        w.browser.add_space("Work", 100.);
        let panes = w.browser.favorite_splits[0].panes.clone();
        w.browser.switch_space(1);
        w.browser.activate_tab(panes[0], std::time::SystemTime::now());
        w.browser.switch_space(0);
        w.browser.activate_tab(panes[1], std::time::SystemTime::now());
        cx.notify();
    });
    cx.run_until_parked();
    let dock = cx.debug_bounds("space-1").unwrap().center();
    begin_drag_to_point(cx, "favorite-4", dock);
    cx.executor().advance_clock(Duration::from_millis(550));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    // Native focus follows the destination Space's other pane while hovering.
    fake.send(cx, Event::Focused { view: page(1) });
    assert_eq!(view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap().get()), 4);
    fake.take_calls();
    cx.simulate_mouse_up(dock, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.space().name, "Work");
        assert!(w.browser.favorites.is_empty());
        assert_eq!(w.browser.space().active_tab.unwrap().get(), 5);
        assert_eq!(w.browser.active_split().unwrap().active.get(), 5);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn dock_reopen_restores_a_closed_window_and_accepts_keyboard_input(cx: &mut TestAppContext) {
    use gpui::AppContext;
    cx.update(init);
    let create = |cx: &mut gpui::App| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let view = BrowserWindow::new(demo_browser(), None, window, cx);
                view.focus(window);
                view
            })
        }).unwrap();
    };
    cx.update(|cx| crate::reopen_with(cx, create));
    let first = cx.read(|cx| cx.windows()[0]);
    let visual = VisualTestContext::from_window(first, cx);
    visual.run_until_parked();
    first.update(cx, |_, window, _| window.remove_window()).unwrap();
    cx.run_until_parked();
    assert!(cx.read(|cx| cx.windows().is_empty()));

    cx.update(|cx| crate::reopen_with(cx, create));
    let reopened = cx.read(|cx| {
        assert_eq!(cx.windows().len(), 1);
        cx.windows()[0]
    });
    assert_ne!(first.window_id(), reopened.window_id());
    let mut visual = VisualTestContext::from_window(reopened, cx);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-t");
    let view = reopened.downcast::<BrowserWindow>().unwrap().root(cx).unwrap();
    bar(&view, &mut visual).simulate_input("example.com");
    bar(&view, &mut visual).simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().url, "https://example.com");
    });
    cx.update(|cx| crate::reopen_with(cx, |_| panic!("must reuse the existing browser window")));
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
}

#[gpui::test]
fn closing_the_window_saves_changes_before_the_periodic_save(cx: &mut TestAppContext) {
    let path = std::env::temp_dir().join(format!("browser-close-session-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let (view, visual) = open(cx);
    view.update(visual, |view, cx| {
        view.save_session_to(path.clone(), cx);
        view.browser.new_tab("https://example.com/just-before-close", std::time::SystemTime::now());
    });
    visual.update(|window, _| window.remove_window());
    drop(view);
    visual.cx.update(|_| {});
    visual.run_until_parked();
    let restored = browser_core::session::load(&path).unwrap().unwrap();
    assert_eq!(restored.active_tab().unwrap().url, "https://example.com/just-before-close");
    std::fs::remove_file(path).unwrap();
}

#[gpui::test]
fn cmd_w_closes_only_the_focused_split_pane_from_ui_and_native_page_focus(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 3);
    let first = view.read_with(cx, |w, _| w.browser.space().active_tab.unwrap());
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.archive.last().unwrap().tab.id, first);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 2);
    });
    let calls = fake.take_calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, Call::Close(..))).count(), 1);
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
    fake.send(cx, Event::Shortcut(Shortcut { cmd: true, key: "w".into(), ..Default::default() }));
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), 1);
    assert_eq!(fake.take_calls().iter().filter(|call| matches!(call, Call::Close(..))).count(), 1);
}

#[gpui::test]
fn add_split_immediately_shows_an_empty_pane_with_search_inside_it_and_escape_rolls_back(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |w, _| w.browser.all_tabs().count());
    let active = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-shift-=");
    let (anchor, pane) = view.read_with(cx, |w, _| {
        let draft = w.split_draft.as_ref().unwrap();
        assert_eq!(w.browser.active_tab().unwrap().url, crate::split::EMPTY_PANE_URL);
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 2);
        assert_eq!(w.browser.all_tabs().count(), before + 1);
        assert_eq!(w.pages.as_ref().unwrap().borrow().surface_layout().len(), 1);
        (draft.anchor, draft.pane)
    });
    assert!(cx.debug_bounds(test_selector(format!("sidebar-pane-{}", anchor.get()))).is_some());
    assert!(cx.debug_bounds(test_selector(format!("sidebar-pane-{}", pane.get()))).is_some());
    let pane_header = cx.debug_bounds(test_selector(format!("pane-header-{}", pane.get()))).unwrap();
    let command = bar(&view, cx).debug_bounds("command-bar").unwrap();
    assert!(command.origin.x >= pane_header.origin.x);
    assert!(command.right() <= pane_header.right());
    assert!(bar_input_focused(&view, cx));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(active_url(&view, cx), active);
    assert_eq!(view.read_with(cx, |w, _| w.browser.all_tabs().count()), before);
    assert!(view.read_with(cx, |w, _| w.browser.archive.is_empty() && w.browser.active_split().is_none()));
}

#[gpui::test]
fn splitting_a_pin_or_favorite_keeps_the_original_and_creates_independent_today_panes(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    click(cx, "tab-4");
    let source = view.read_with(cx, |w, _| w.browser.active_tab().unwrap().clone());
    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |w, _| w.browser.active_tab().unwrap().id), source.id);
    for favorite in [false, true] {
        if favorite { run_named_command(&view, cx, "favorite tab"); }
        cx.simulate_keystrokes("ctrl-shift-=");
        bar(&view, cx).simulate_input("https://second.example");
        bar(&view, cx).simulate_keystrokes("enter");
        let anchor = view.read_with(cx, |w, _| {
            let split = w.browser.active_split().unwrap();
            assert!(!split.panes.contains(&source.id));
            assert!(split.panes.iter().all(|id| w.browser.space().today.iter().any(|tab| tab.id == *id)));
            let original = w.browser.all_tabs().find(|tab| tab.id == source.id).unwrap();
            assert_eq!(original.url, source.url);
            assert_eq!(original.pinned_url, source.pinned_url);
            assert!(w.browser.split_for(source.id).is_none());
            split.anchor
        });
        click(cx, test_selector(format!("close-{}", anchor.get())));
        if favorite { click(cx, "favorite-4"); } else { click(cx, "tab-4"); }
    }
}

#[gpui::test]
fn pending_split_is_excluded_from_sessions_and_cancel_restores_axis_weights_and_focus(cx: &mut TestAppContext) {
    let (view, cx, _) = open_split_pages(cx, 3);
    view.update(cx, |w, _| {
        let active = w.browser.space().active_tab.unwrap();
        w.browser.set_split_weights(active, &[0.2, 0.3, 0.5]);
    });
    let original = view.read_with(cx, |w, _| w.browser.active_split().unwrap().clone());
    run_named_command(&view, cx, "add bottom split");
    view.read_with(cx, |w, _| {
        assert_eq!(w.browser.active_split().unwrap().panes.len(), 4);
        let saved = browser_core::session::from_json(&w.session_json()).unwrap();
        let split = saved.active_split().unwrap();
        assert_eq!(split.panes, original.panes);
        assert_eq!(split.axis, original.axis);
        assert_eq!(split.weights, original.weights);
        assert!(!saved.all_tabs().any(|tab| tab.url == crate::split::EMPTY_PANE_URL));
    });
    bar(&view, cx).simulate_keystrokes("escape");
    view.read_with(cx, |w, _| {
        let split = w.browser.active_split().unwrap();
        assert_eq!(split.panes, original.panes);
        assert_eq!(split.axis, original.axis);
        assert_eq!(split.weights, original.weights);
        assert_eq!(split.active, original.active);
        assert!(w.browser.archive.is_empty());
    });
}

#[gpui::test]
fn sidebar_split_segments_focus_individual_panes_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    fake.take_calls();
    click(cx, "sidebar-pane-2");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane0.example"));
    click(cx, "sidebar-pane-3");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane1.example"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn cmd_w_in_the_empty_split_search_cancels_without_archiving_the_source(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let original = active_url(&view, cx);
    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_keystrokes("cmd-w");
    assert_eq!(bar_text(&view, cx), None);
    assert_eq!(active_url(&view, cx), original);
    assert!(view.read_with(cx, |w, _| w.split_draft.is_none() && w.browser.active_split().is_none() && w.browser.archive.is_empty()));
}

#[gpui::test]
fn empty_vertical_pane_search_stays_inside_the_pane(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    run_named_command(&view, cx, "add bottom split");
    let pane = view.read_with(cx, |w, _| w.split_draft.as_ref().unwrap().pane);
    let header = cx.debug_bounds(test_selector(format!("pane-header-{}", pane.get()))).unwrap();
    let search = bar(&view, cx).debug_bounds("command-bar").unwrap();
    assert!(search.origin.y >= header.bottom());
    let pane_bounds = cx.debug_bounds(test_selector(format!("pane-body-{}", pane.get()))).unwrap();
    assert!(search.bottom() <= pane_bounds.bottom());
    bar(&view, cx).simulate_keystrokes("escape");
}

#[gpui::test]
fn split_headers_and_pages_share_one_outer_corner_contour_without_recreating_views(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    assert_eq!(fake.0.borrow().rounded_top[&1], true);
    fake.take_calls();
    cx.simulate_keystrokes("ctrl-shift-=");
    assert!(fake.take_calls().contains(&Call::TopCorners(1, false)));
    bar(&view, cx).simulate_keystrokes("escape");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::TopCorners(1, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));

    cx.simulate_keystrokes("ctrl-shift-=");
    bar(&view, cx).simulate_input("https://beside.example");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(fake.0.borrow().rounded_top.values().all(|rounded| !rounded));
    let panes = view.read_with(cx, |w, _| w.browser.active_split().unwrap().panes.clone());
    for pane in panes {
        let frame = cx.debug_bounds(test_selector(format!("split-pane-{}", pane.get()))).unwrap();
        let header = cx.debug_bounds(test_selector(format!("pane-header-{}", pane.get()))).unwrap();
        let body = cx.debug_bounds(test_selector(format!("pane-body-{}", pane.get()))).unwrap();
        assert_eq!(frame.origin, header.origin);
        assert_eq!(header.bottom(), body.origin.y);
        assert_eq!(frame.bottom(), body.bottom());
        assert_eq!(frame.size.width, header.size.width);
        assert_eq!(frame.size.width, body.size.width);
    }
    run_named_command(&view, cx, "convert to vertical split");
    assert!(fake.0.borrow().rounded_top.values().all(|rounded| !rounded));
    fake.take_calls();
    cx.simulate_keystrokes("cmd-w");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::TopCorners(1, true)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_name_retracts_pins_progressively_and_reverses_mid_animation_without_reloading(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    assert!(cx.debug_bounds("pinned-section").is_none());
    let expanded = cx.debug_bounds("new-tab").unwrap().origin.y;
    let active = active_url(&view, cx);
    fake.take_calls();
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(48));
    cx.run_until_parked();
    let intermediate = cx.debug_bounds("new-tab").unwrap().origin.y;
    assert!(intermediate < expanded);
    assert!(cx.debug_bounds("pinned-reveal").unwrap().size.height > gpui::px(0.));
    click(cx, "space-header-0");
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, intermediate);
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, expanded);
    assert!(!view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    assert_eq!(active_url(&view, cx), active);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..) | Call::FocusPage(..))));
}

#[gpui::test]
fn dropping_on_a_collapsed_space_name_reveals_the_moved_pin(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    fake.take_calls();
    drag_between(cx, "tab-8", "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    view.read_with(cx, |w, _| {
        assert!(!w.browser.space().pinned_collapsed);
        assert!(w.browser.space().pinned.iter().any(|tab| tab.id.get() == 8));
    });
    let pin = cx.debug_bounds("tab-8").unwrap();
    let header = cx.debug_bounds("space-header-0").unwrap();
    assert!(pin.origin.y >= header.bottom());
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}

#[gpui::test]
fn space_header_matches_reference_control_positions_and_more_does_not_toggle_pins(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    view.update(cx, |w, cx| {
        w.browser.set_space_icon(w.browser.space().id, Some("🍕"));
        cx.notify();
    });
    cx.run_until_parked();
    let header = cx.debug_bounds("space-header-0").unwrap();
    let icon = cx.debug_bounds("space-header-icon-0").unwrap();
    let name = cx.debug_bounds("space-header-name-0").unwrap();
    assert_eq!(header.size.height, gpui::px(38.));
    assert_eq!(icon.origin.x, header.origin.x + gpui::px(9.));
    assert_eq!(name.origin.x, icon.right() + gpui::px(9.));
    assert!(icon.right() < name.origin.x);
    let original = active_url(&view, cx);
    fake.take_calls();
    cx.simulate_mouse_move(header.center(), None, Modifiers::none());
    cx.run_until_parked();
    let chevron = cx.debug_bounds("space-header-chevron-0").unwrap();
    let more = cx.debug_bounds("space-header-more-0").unwrap();
    assert_eq!(chevron.origin, icon.origin);
    assert_eq!(chevron.size, icon.size);
    assert!(chevron.right() < name.origin.x);
    assert!(more.origin.x >= name.right());
    assert_eq!(cx.debug_bounds("space-header-name-0").unwrap(), name);
    click(cx, "space-header-more-0");
    view.read_with(cx, |w, _| {
        assert_eq!(w.tab_menu.as_ref().unwrap().target, crate::tab_menu::MenuTarget::Space(w.browser.space().id));
        assert!(!w.browser.space().pinned_collapsed);
    });
    click(&mut tab_menu(&view, cx), "tab-menu-space-rename");
    assert_eq!(bar_text(&view, cx).as_deref(), Some("Personal"));
    bar(&view, cx).simulate_keystrokes("escape");
    click(cx, "space-header-0");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    assert_eq!(cx.debug_bounds("space-header-name-0").unwrap(), name);
    assert_eq!(active_url(&view, cx), original);
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
}


#[gpui::test]
fn sidebar_slide_reverses_without_jumping_and_moves_content_with_it(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_millis(48));
    cx.run_until_parked();
    let closing = cx.debug_bounds("sidebar-clip").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert!(closing.size.width > gpui::px(0.) && closing.size.width < gpui::px(240.));
    assert_eq!(sidebar.right(), closing.right());
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("sidebar-clip").unwrap(), closing, "reverses from the visible position");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("sidebar-clip").unwrap().size.width, gpui::px(240.));
}

#[gpui::test]
fn floating_sidebar_keeps_a_pointer_buffer_without_covering_the_page(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("cmd-s");
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    move_mouse(cx, 2., 300.);
    let mut overlay = floating(&view, cx).unwrap();
    move_mouse(&mut overlay, 100., 300.);
    move_mouse(cx, 300., 300.);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(floating(&view, cx).is_some(), "54 px outside the panel stays open");
    assert_eq!(overlay.update(|window, _| window.viewport_size().width), gpui::px(270.));
    move_mouse(cx, 350., 300.);
    cx.executor().advance_clock(Duration::from_millis(240));
    cx.run_until_parked();
    assert!(floating(&view, cx).is_none(), "beyond the buffer it closes");
}


#[gpui::test]
fn sidebar_toolbar_routes_navigation_and_switches_reload_to_stop(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.take_calls();
    click(cx, "go-back");
    click(cx, "go-forward");
    click(cx, "reload-page");
    let calls = fake.take_calls();
    assert!(calls.contains(&Call::Back(1)));
    assert!(calls.contains(&Call::Forward(1)));
    assert!(calls.contains(&Call::Reload(1)));
    fake.send(cx, Event::LoadingChanged { view: page(1), loading: true, can_go_back: true, can_go_forward: false });
    click(cx, "reload-page");
    assert!(fake.take_calls().contains(&Call::StopLoading(1)));
    click(cx, "toggle-sidebar");
    assert!(!view.read_with(cx, |w, _| w.sidebar_visible));
}

#[gpui::test]
fn split_sidebar_segments_fill_the_row_and_long_titles_stay_inside(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_split_pages(cx, 2);
    view.update(cx, |w, cx| {
        let ids = w.browser.active_split().unwrap().panes.clone();
        for id in ids { w.browser.set_title(id, "A very long page title that should truncate inside its own split segment"); }
        cx.notify();
    });
    cx.run_until_parked();
    move_mouse(cx, 400., 400.);
    let row = cx.debug_bounds("tab-2").unwrap();
    let contents = cx.debug_bounds("sidebar-split-2").unwrap();
    let first = cx.debug_bounds("sidebar-pane-2").unwrap();
    let last = cx.debug_bounds("sidebar-pane-3").unwrap();
    assert_eq!(contents.origin.x, row.origin.x + gpui::px(4.));
    assert_eq!(contents.right(), row.right() - gpui::px(4.));
    assert_eq!(first.size.width, last.size.width);
    assert_eq!(first.origin.x, contents.origin.x);
    assert_eq!(last.right(), contents.right());
    move_mouse(cx, f32::from(row.center().x), f32::from(row.center().y));
    assert_eq!(cx.debug_bounds("sidebar-pane-3").unwrap(), last, "hover close must not shrink a segment");
    let close = cx.debug_bounds("close-2").unwrap();
    assert!(close.origin.x >= last.origin.x && close.right() <= last.right());
    let title = cx.debug_bounds("sidebar-pane-title-3").unwrap();
    assert!(title.right() <= close.origin.x, "the hover close must not cover the title");
    fake.take_calls();
    click(cx, "sidebar-pane-3");
    assert_eq!(active_url(&view, cx).as_deref(), Some("https://pane1.example"));
    assert!(!fake.take_calls().iter().any(|call| matches!(call, Call::Create(..) | Call::Close(..) | Call::Navigate(..))));
    click(cx, "close-2");
    assert!(view.read_with(cx, |w, _| w.browser.active_split().is_none()));
}

#[gpui::test]
fn folder_hover_replaces_its_icon_without_shifting_the_title(cx: &mut TestAppContext) {
    let (view, cx, _) = open_folder_pages(cx);
    let folder = view.read_with(cx, |w, _| w.browser.space().folders[0].id);
    let key = folder.get();
    move_mouse(cx, 400., 400.);
    let name = cx.debug_bounds(test_selector(format!("folder-name-{key}"))).unwrap();
    let icon = cx.debug_bounds(test_selector(format!("folder-icon-{key}"))).unwrap();
    let row = cx.debug_bounds(test_selector(format!("folder-{key}"))).unwrap();
    assert_eq!(icon.origin.x, row.origin.x + gpui::px(9.));
    assert_eq!(name.origin.x, icon.right() + gpui::px(9.));
    move_mouse(cx, f32::from(row.center().x), f32::from(row.center().y));
    assert_eq!(cx.debug_bounds(test_selector(format!("folder-chevron-{key}"))).unwrap(), icon);
    assert_eq!(cx.debug_bounds(test_selector(format!("folder-name-{key}"))).unwrap(), name);
    let was_collapsed = view.read_with(cx, |w, _| w.browser.folder(folder).unwrap().collapsed);
    click(cx, test_selector(format!("folder-menu-{key}")));
    assert_eq!(view.read_with(cx, |w, _| w.browser.folder(folder).unwrap().collapsed), was_collapsed);
}

#[gpui::test]
fn sidebar_matches_reference_metrics_and_hover_controls(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    fake.send(cx, Event::UrlChanged { view: page(1), url: "https://www.youtube.com/watch?v=1".into() });
    view.update(cx, |w, cx| {
        for tab in w.browser.space().pinned.iter().map(|tab| tab.id).collect::<Vec<_>>() { w.browser.favorite_tab(tab); }
        cx.notify();
    });
    cx.run_until_parked();
    // The address shows only the site.
    assert_eq!(crate::window::address_host("https://www.youtube.com/watch?v=1"), "youtube.com");
    assert_eq!(crate::window::address_host("about:blank"), "about:blank");
    let address = cx.debug_bounds("address").unwrap();
    assert_eq!(address.size.height, gpui::px(36.));
    // Favorites stretch across the row with equal tiles.
    let first = cx.debug_bounds("favorite-4").unwrap();
    let second = cx.debug_bounds("favorite-5").unwrap();
    assert_eq!(first.size, second.size);
    assert_eq!(first.size.height, gpui::px(46.));
    assert_eq!(first.origin.x, address.origin.x);
    assert!(address.right() - second.right() < gpui::px(1.));
    // Rows are 38 px with a 3 px gap.
    let today = view.read_with(cx, |w, _| w.browser.space().today[0].id.get());
    let tab = cx.debug_bounds(test_selector(format!("tab-{today}"))).unwrap();
    assert_eq!(tab.size.height, gpui::px(38.));
    let new_tab = cx.debug_bounds("new-tab").unwrap();
    assert_eq!(new_tab.size.height, gpui::px(38.));
    // Traffic lights light up only while the pointer is over them.
    move_mouse(cx, 400., 400.);
    assert!(!view.read_with(cx, |w, _| w.traffic_lights_hovered));
    let lights = cx.debug_bounds("traffic-lights").unwrap();
    move_mouse(cx, f32::from(lights.center().x), f32::from(lights.center().y));
    assert!(view.read_with(cx, |w, _| w.traffic_lights_hovered));
    move_mouse(cx, 400., 400.);
    assert!(!view.read_with(cx, |w, _| w.traffic_lights_hovered));
    // The hover-only copy link button copies the page URL.
    click(cx, "address-copy-link");
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard, active_url(&view, cx));
    assert!(view.read_with(cx, |w, _| w.command_bar.is_none()));
}

#[gpui::test]
fn space_bar_and_site_controls_match_the_reference_layout(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    // Library on the left, Spaces centered, New Space on the right, all on one axis.
    let library = cx.debug_bounds("library").unwrap();
    let space = cx.debug_bounds("space-0").unwrap();
    let plus = cx.debug_bounds("new-space").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(library.center().y, space.center().y);
    assert_eq!(plus.center().y, space.center().y);
    assert!(library.right() < space.origin.x && space.right() < plus.origin.x);
    assert_eq!(library.origin.x - sidebar.origin.x, sidebar.right() - plus.right());
    // The site controls button opens the panel and Escape-like dismissal closes it.
    click(cx, "address-site-controls");
    assert!(view.read_with(cx, |w, _| w.site_controls.is_some()));
    view.update(cx, |w, cx| w.close_site_controls(cx));
    assert!(view.read_with(cx, |w, _| w.site_controls.is_none()));
}

#[gpui::test]
fn new_menu_offers_reference_items_and_opens_the_matching_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let menu = |view: &Entity<BrowserWindow>, cx: &mut VisualTestContext| {
        cx.run_until_parked();
        let overlay = view.read_with(cx, |w, _| w.new_menu.as_ref().and_then(|menu| menu.overlay)).expect("new menu is open");
        VisualTestContext::from_window(overlay.into(), cx)
    };
    click(cx, "new-space");
    let mut overlay = menu(&view, cx);
    for item in ["new-menu-space", "new-menu-folder", "new-menu-boost", "new-menu-split", "new-menu-tab"] {
        assert!(overlay.debug_bounds(item).is_some(), "{item}");
    }
    assert!(overlay.debug_bounds("new-menu-easel").is_none());
    click(&mut overlay, "new-menu-tab");
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.new_menu.is_none()));
    assert_eq!(bar_text(&view, cx).as_deref(), Some(""));
    bar(&view, cx).simulate_keystrokes("escape");
    // The keyboard walks the rows; Enter runs the selected one.
    click(cx, "new-space");
    let mut overlay = menu(&view, cx);
    overlay.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(view.read_with(cx, |w, _| w.new_menu.is_none() && matches!(w.command_bar.as_ref().map(|bar| bar.target), Some(crate::window::Target::NewSpace))));
}

#[gpui::test]
fn sidebar_controls_animate_feedback_without_changing_layout(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let copy = cx.debug_bounds("address-copy-link").unwrap();
    click(cx, "address-copy-link");
    assert_eq!(view.read_with(cx, |w, _| w.copy_feedback.map(|(count, _)| count)), Some(0));
    assert_eq!(cx.debug_bounds("address-copy-link").unwrap(), copy);
    cx.executor().advance_clock(Duration::from_millis(1400));
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("address-copy-link").unwrap(), copy);
    // The plus turns while the New menu is open and back when it closes.
    let plus = cx.debug_bounds("new-space").unwrap();
    click(cx, "new-space");
    assert_eq!(view.read_with(cx, |w, _| w.new_menu_toggles), 1);
    view.update(cx, |w, cx| w.close_new_menu(cx));
    assert_eq!(view.read_with(cx, |w, _| w.new_menu_toggles), 2);
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("new-space").unwrap(), plus);
}

#[gpui::test]
fn finished_copy_feedback_does_not_replay_when_the_controls_reappear(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    click(cx, "address-copy-link");
    assert!(cx.debug_bounds("copy-check").is_some());
    // Feedback that already ran out stays finished across hover changes.
    view.update(cx, |w, cx| { w.copy_feedback = w.copy_feedback.map(|(count, _)| (count, std::time::Instant::now() - Duration::from_secs(5))); cx.notify(); });
    cx.run_until_parked();
    assert!(cx.debug_bounds("copy-check").is_none());
    move_mouse(cx, 600., 400.);
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    move_mouse(cx, f32::from(sidebar.center().x), f32::from(sidebar.center().y));
    assert!(cx.debug_bounds("copy-check").is_none());
}

#[gpui::test]
fn sidebar_context_menus_follow_the_reference_layout(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    // The address field has its own menu; Copy URL copies the current page.
    right_click(cx, "address");
    let mut menu = tab_menu(&view, cx);
    for row in ["tab-menu-copy", "tab-menu-share", "tab-menu-capture", "tab-menu-capture-full-page", "tab-menu-show-as-toolbar", "tab-menu-disable-shared-quotes"] {
        assert!(menu.debug_bounds(row).is_some(), "{row}");
    }
    click(&mut menu, "tab-menu-copy");
    assert_eq!(cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())), active_url(&view, cx));
    // Empty sidebar space opens the Space menu.
    let page = cx.debug_bounds("space-page").unwrap();
    let empty = gpui::point(page.center().x, page.bottom() - gpui::px(4.));
    cx.simulate_mouse_move(empty, None, Modifiers::none());
    cx.simulate_mouse_down(empty, gpui::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(empty, gpui::MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |w, _| w.tab_menu.as_ref().map(|menu| menu.target)), Some(crate::tab_menu::MenuTarget::Space(view.read_with(cx, |w, _| w.browser.space().id))));
    let mut menu = tab_menu(&view, cx);
    for row in ["tab-menu-space-icon", "tab-menu-space-rename", "tab-menu-edit-theme-color", "tab-menu-set-profile", "tab-menu-space-new-folder", "tab-menu-live-folders", "tab-menu-share-space", "tab-menu-manage-spaces", "tab-menu-delete-space"] {
        assert!(menu.debug_bounds(row).is_some(), "{row}");
    }
    // Submenus open with the right arrow and close with the left one.
    assert!(menu.debug_bounds("tab-context-submenu").is_none());
    menu.simulate_keystrokes("down down down down right");
    menu.run_until_parked();
    assert!(menu.debug_bounds("tab-context-submenu").is_some());
    assert!(menu.debug_bounds("tab-menu-default").is_some());
    menu.simulate_keystrokes("left");
    menu.run_until_parked();
    assert!(menu.debug_bounds("tab-context-submenu").is_none());
    menu.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |w, _| w.tab_menu.is_none()));
}
