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
    FocusPage(u64),
    FocusUi,
    Navigate(u64, String),
    Back(u64),
    Forward(u64),
    ForceReload(u64),
    StopLoading(u64),
    Zoom(u64, i32),
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
    next_view: u64,
    events: Vec<Event>,
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
        state.calls.push(Call::Create(id, url.to_string()));
        Some(ViewId::from_raw(id))
    }
    fn set_bounds(&mut self, view: ViewId, bounds: Bounds) {
        self.0.borrow_mut().bounds.insert(raw(view), bounds);
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
    fn zoom(&mut self, view: ViewId, direction: i32) { self.call(Call::Zoom(raw(view), direction)); }
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
    assert!(cx.debug_bounds("new-tab").is_none());
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
    move_mouse(&mut overlay, f32::from(panel.right()) + 10.0, 300.0);
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
    click(cx, "pinned-section");
    assert!(view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    // GPUI retains old debug bounds, so check the actual layout movement.
    cx.update(|window, cx| { window.draw(cx); });
    assert!(cx.debug_bounds("new-tab").unwrap().origin.y < expanded_y);
    let saved = view.read_with(cx, |w, _| browser_core::session::to_json(&w.browser));
    let restored = browser_core::session::from_json(&saved).unwrap();
    assert!(restored.space().pinned_collapsed);
    assert!(!restored.spaces[1].pinned_collapsed);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("expand pinned section");
    bar(&view, cx).simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |w, _| w.browser.space().pinned_collapsed));
    assert_eq!(cx.debug_bounds("new-tab").unwrap().origin.y, expanded_y);
}
