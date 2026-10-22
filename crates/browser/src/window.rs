//! Arc-style browser window: vertical sidebar, content card, command bar.
//! Web content is not wired up yet; the card shows the active tab's URL.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use browser_core::omnibox::{self, Resolved};
use browser_core::{Browser, Closed, Section, SpaceId, Tab, TabId};
use engine::Event;
use gpui::{
    actions, canvas, div, ease_out_quint, point, size, Animation, AnimationExt, AnyElement, Bounds,
    MouseMoveEvent, TitlebarOptions, AnyWindowHandle, Task, WeakEntity, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, hsla, linear_color_stop, linear_gradient, prelude::*, px, App, Context, Entity,
    FocusHandle, Focusable, Hsla, KeyBinding, Keystroke, Modifiers, ScrollDelta, ScrollWheelEvent,
    SharedString, Subscription, TouchPhase, Window,
};

use crate::pages::{Pages, WebEngine, CONTENT_CORNER_RADIUS, CONTENT_INSET, WINDOW_CORNER_RADIUS};
use crate::text_input::{self, TextInput};

const SIDEBAR_WIDTH: f32 = 240.0;
const COMMAND_BAR_WIDTH: f32 = 620.0;
const MAX_TAB_SUGGESTIONS: usize = 6;
const ARCHIVE_CHECK_INTERVAL: Duration = Duration::from_secs(60);
const ENGINE_PUMP_INTERVAL: Duration = Duration::from_millis(8);
const SESSION_SAVE_INTERVAL: Duration = Duration::from_secs(1);
/// Fraction of the sidebar width a swipe must travel to change space.
const SPACE_SWIPE_COMMIT: f32 = 0.25;
/// Finger speed (px/s) that changes space regardless of distance (a flick).
const SPACE_FLICK_SPEED: f32 = 400.0;
/// How much the pager follows the fingers past the first and last space.
const PAGER_RUBBER_BAND: f32 = 0.3;
const PAGER_FRAME: Duration = Duration::from_millis(16);
/// Pointer distance (px) from the window's left edge that reveals a hidden sidebar.
const SIDEBAR_REVEAL_EDGE: f32 = 8.0;
const FLOATING_SIDEBAR_INSET: f32 = 6.0;
/// Room for the floating sidebar's shadow inside its window.
const FLOATING_SIDEBAR_SHADOW: f32 = 18.0;
const FLOATING_SIDEBAR_SLIDE: Duration = Duration::from_millis(180);
/// How long a revealed sidebar waits for the pointer before hiding.
const FLOATING_SIDEBAR_GRACE: Duration = Duration::from_millis(800);
pub const TRAFFIC_LIGHT_X: f32 = 14.0;
pub const TRAFFIC_LIGHT_Y: f32 = 14.0;
/// Share of the remaining distance covered per frame when settling (ease-out).
const PAGER_SETTLE_RATE: f32 = 0.28;

actions!(
    browser,
    [
        OpenCommandBar,
        EditCurrentUrl,
        CloseTab,
        NextTab,
        PreviousTab,
        GoBack,
        GoForward,
        Reload,
        TogglePin,
        ToggleSidebar,
        NextSpace,
        PreviousSpace,
        ClearToday,
        RestoreTab,
        SelectNext,
        SelectPrevious,
        Confirm,
        Dismiss,
    ]
);

/// Jumps to the space at this index (⌃1…⌃9).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = browser, no_json)]
pub struct SwitchToSpace(pub usize);

/// Focuses the sidebar item at this index, pinned first; `None` is the
/// last one (⌘1…⌘8, ⌘9).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = browser, no_json)]
pub struct GoToItem(pub Option<usize>);

const WINDOW_CONTEXT: &str = "BrowserWindow";
const COMMAND_BAR_CONTEXT: &str = "CommandBar";

/// Window shortcuts. Defined once so the same list is bound in GPUI and
/// intercepted by the engine while a page has keyboard focus.
macro_rules! window_shortcuts {
    ($($keystroke:literal => $action:expr),* $(,)?) => {
        pub const WINDOW_SHORTCUTS: &[&str] = &[$($keystroke),*];

        fn bind_window_keys(cx: &mut App) {
            cx.bind_keys([$(KeyBinding::new($keystroke, $action, Some(WINDOW_CONTEXT))),*]);
        }
    };
}

window_shortcuts! {
    "cmd-t" => OpenCommandBar,
    "cmd-l" => EditCurrentUrl,
    "cmd-w" => CloseTab,
    "cmd-shift-t" => RestoreTab,
    "cmd-shift-k" => ClearToday,
    "ctrl-tab" => NextTab,
    "ctrl-shift-tab" => PreviousTab,
    "cmd-alt-down" => NextTab,
    "cmd-alt-up" => PreviousTab,
    "cmd-[" => GoBack,
    "cmd-]" => GoForward,
    "cmd-r" => Reload,
    "cmd-d" => TogglePin,
    "cmd-s" => ToggleSidebar,
    "cmd-alt-right" => NextSpace,
    "cmd-alt-left" => PreviousSpace,
    "ctrl-1" => SwitchToSpace(0),
    "ctrl-2" => SwitchToSpace(1),
    "ctrl-3" => SwitchToSpace(2),
    "ctrl-4" => SwitchToSpace(3),
    "ctrl-5" => SwitchToSpace(4),
    "ctrl-6" => SwitchToSpace(5),
    "ctrl-7" => SwitchToSpace(6),
    "ctrl-8" => SwitchToSpace(7),
    "ctrl-9" => SwitchToSpace(8),
    "cmd-1" => GoToItem(Some(0)),
    "cmd-2" => GoToItem(Some(1)),
    "cmd-3" => GoToItem(Some(2)),
    "cmd-4" => GoToItem(Some(3)),
    "cmd-5" => GoToItem(Some(4)),
    "cmd-6" => GoToItem(Some(5)),
    "cmd-7" => GoToItem(Some(6)),
    "cmd-8" => GoToItem(Some(7)),
    "cmd-9" => GoToItem(None),
}

/// [`WINDOW_SHORTCUTS`] in the engine's terms.
pub fn page_shortcuts() -> Vec<engine::Shortcut> {
    WINDOW_SHORTCUTS
        .iter()
        .map(|source| {
            let keystroke = Keystroke::parse(source).expect("invalid shortcut");
            engine::Shortcut {
                cmd: keystroke.modifiers.platform,
                ctrl: keystroke.modifiers.control,
                alt: keystroke.modifiers.alt,
                shift: keystroke.modifiers.shift,
                key: keystroke.key,
            }
        })
        .collect()
}

fn keystroke_for(shortcut: engine::Shortcut) -> Keystroke {
    Keystroke {
        modifiers: Modifiers {
            platform: shortcut.cmd,
            control: shortcut.ctrl,
            alt: shortcut.alt,
            shift: shortcut.shift,
            function: false,
        },
        key: shortcut.key,
        key_char: None,
    }
}

pub fn bind_keys(cx: &mut App) {
    bind_window_keys(cx);
    let bar = Some(COMMAND_BAR_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, bar),
        KeyBinding::new("up", SelectPrevious, bar),
        KeyBinding::new("ctrl-n", SelectNext, bar),
        KeyBinding::new("ctrl-p", SelectPrevious, bar),
        KeyBinding::new("enter", Confirm, bar),
        KeyBinding::new("escape", Dismiss, bar),
    ]);
}

/// The sidebar pages between spaces like Arc's: spaces sit side by side and
/// a two-finger swipe drags them, then they settle on one.
#[derive(Default)]
pub struct Pager {
    /// Visual position in spaces: 1.5 is halfway between the 2nd and 3rd.
    pub position: f32,
    drag: Option<Drag>,
    settle: Option<Task<()>>,
}

struct Drag {
    start: f32,
    travel: f32,
    /// Decided by the first movement; vertical gestures scroll the tab list.
    horizontal: Option<bool>,
    /// Recent (time, horizontal delta) samples for flick speed.
    samples: Vec<(Instant, f32)>,
}

impl Drag {
    /// Finger speed (px/s) over the last moments of the gesture, measured
    /// between events; zero if the fingers paused before lifting.
    fn speed(&self) -> f32 {
        let Some(&(last, _)) = self.samples.last() else {
            return 0.0;
        };
        if last.elapsed() > Duration::from_millis(50) {
            return 0.0;
        }
        let recent: Vec<_> = self
            .samples
            .iter()
            .filter(|(at, _)| last.duration_since(*at) < Duration::from_millis(100))
            .collect();
        let elapsed = last.duration_since(recent[0].0).as_secs_f32();
        // Too short to measure (e.g. synthesized input): judge by distance only.
        if elapsed < 0.01 {
            return 0.0;
        }
        recent[1..].iter().map(|(_, dx)| dx).sum::<f32>() / elapsed
    }
}

/// Where a command bar URL is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    NewTab,
    Tab(TabId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Suggestion {
    Load(Resolved),
    SwitchToTab { id: TabId, title: String, url: String, space: String },
    Action { command: Command, label: String },
}

/// Commands currently implemented by the browser, also exposed by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ToggleSidebar,
    TogglePin,
    CloseTab,
    ClearToday,
    RestoreTab,
    Reload,
    GoBack,
    GoForward,
    NextTab,
    PreviousTab,
    FocusSpace(SpaceId),
}

fn matches_command(query: &str, names: &str) -> bool {
    names.split('|').any(|name| {
        let name = name.to_lowercase();
        query.split_whitespace().all(|word| name.split_whitespace().any(|part| part.starts_with(word)))
    })
}

pub struct CommandBar {
    pub input: Entity<TextInput>,
    pub target: Target,
    pub selected: usize,
    /// The bar is drawn in its own transparent window above the browser
    /// window, since pages are native views that cover anything drawn below.
    pub overlay: Option<WindowHandle<Overlay>>,
    _changed: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    CommandBar,
    /// The hidden sidebar, shown floating over the page.
    Sidebar,
}

/// Root of a transparent child window over the browser window. Pages are
/// native views that cover anything the browser window draws, so UI that
/// lies over a page lives in one of these. Draws from the browser window.
pub struct Overlay {
    browser: WeakEntity<BrowserWindow>,
    kind: OverlayKind,
    _observe: Option<Subscription>,
}

impl Overlay {
    fn new(browser: WeakEntity<BrowserWindow>, kind: OverlayKind, cx: &mut Context<Self>) -> Self {
        let observe = browser
            .upgrade()
            .map(|browser| cx.observe(&browser, |_, _, cx| cx.notify()));
        Self {
            browser,
            kind,
            _observe: observe,
        }
    }
}

impl Render for Overlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        self.browser
            .update(cx, |browser, cx| match kind {
                OverlayKind::CommandBar => browser.render_command_bar_overlay(cx),
                OverlayKind::Sidebar => browser.render_floating_sidebar(cx),
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}

#[derive(Default)]
struct MouseTracking {
    hole: Option<[f64; 4]>,
    areas: Vec<usize>,
}

/// The sidebar shown over the page while it is hidden (⌘S).
pub struct FloatingSidebar {
    pub overlay: Option<WindowHandle<Overlay>>,
    /// The pointer has been over it; leaving then hides it.
    entered: bool,
    closing: bool,
}


pub struct BrowserWindow {
    pub browser: Browser,
    pub sidebar_visible: bool,
    pub command_bar: Option<CommandBar>,
    /// Web pages; `None` when running without an engine.
    pub pages: Option<Rc<RefCell<Pages>>>,
    pub floating_sidebar: Option<FloatingSidebar>,
    main_window: AnyWindowHandle,
    native_view: Option<*mut c_void>,
    /// Native mouse tracking around the page, see `native::track_mouse_outside`.
    mouse_tracking: Rc<RefCell<MouseTracking>>,
    pub pager: Pager,
    focus_handle: FocusHandle,
    _save_on_quit: Option<Subscription>,
}

impl BrowserWindow {
    pub fn new(
        browser: Browser,
        engine: Option<Box<dyn WebEngine>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let pages = engine.map(|engine| Rc::new(RefCell::new(Pages::new(engine))));
        if let Some(pages) = pages.clone() {
            let pump = pages.borrow().pump_fn();
            cx.spawn_in(window, async move |this, cx| loop {
                cx.background_executor().timer(ENGINE_PUMP_INTERVAL).await;
                pump();
                let Ok(events) = pages.try_borrow_mut().map(|mut pages| pages.take_events()) else {
                    continue;
                };
                if events.is_empty() {
                    continue;
                }
                let applied = this.update_in(cx, |this, window, cx| {
                    this.apply_events(events, window, cx)
                });
                if applied.is_err() {
                    break;
                }
            })
            .detach();
        }

        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(ARCHIVE_CHECK_INTERVAL).await;
            let alive = this.update(cx, |this, cx| {
                if this.browser.archive_idle(SystemTime::now()) > 0 {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();

        let position = browser.active_space as f32;
        Self {
            pager: Pager {
                position,
                ..Default::default()
            },
            browser,
            sidebar_visible: true,
            command_bar: None,
            pages,
            floating_sidebar: None,
            main_window: window.window_handle(),
            native_view: crate::native::view_of(window),
            mouse_tracking: Rc::default(),
            focus_handle: cx.focus_handle(),
            _save_on_quit: None,
        }
    }

    /// Keeps the session saved at `path`: shortly after every change and
    /// when the app quits.
    pub fn save_session_to(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        // Empty, so the first launch's content is saved too.
        let saved = Rc::new(RefCell::new(String::new()));
        let save = {
            let saved = saved.clone();
            move |browser: &Browser| {
                let json = browser_core::session::to_json(browser);
                if *saved.borrow() == json {
                    return;
                }
                match browser_core::session::save(&path, &json) {
                    Ok(()) => *saved.borrow_mut() = json,
                    Err(error) => eprintln!("Cannot save the session to {}: {error}", path.display()),
                }
            }
        };
        let on_quit = save.clone();
        self._save_on_quit = Some(cx.on_app_quit(move |this, _| {
            on_quit(&this.browser);
            async {}
        }));
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(SESSION_SAVE_INTERVAL).await;
            if this.update(cx, |this, _| save(&this.browser)).is_err() {
                break;
            }
        })
        .detach();
    }

    pub fn apply_events(&mut self, events: Vec<Event>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pages) = self.pages.clone() else {
            return;
        };
        for event in events {
            pages.borrow_mut().record(&event);
            match event {
                Event::TitleChanged { view, title } => {
                    if let Some(tab) = pages.borrow().tab_for(view) {
                        if !title.is_empty() {
                            self.browser.set_title(tab, title);
                        }
                    }
                }
                Event::UrlChanged { view, url } => {
                    if let Some(tab) = pages.borrow().tab_for(view) {
                        self.browser.set_url(tab, url);
                    }
                }
                Event::OpenInNewTab { url, .. } => {
                    let id = self.browser.new_tab(url.clone(), SystemTime::now());
                    self.browser.set_title(id, display_url(&url));
                }
                Event::Shortcut(shortcut) => {
                    // Actions update this view, so dispatch after this update ends.
                    let keystroke = keystroke_for(shortcut);
                    window.defer(cx, move |window, cx| {
                        window.dispatch_keystroke(keystroke, cx);
                    });
                }
                Event::LoadingChanged { .. } => {}
            }
        }
        cx.notify();
    }

    fn with_active_page(&self, f: impl FnOnce(&mut Pages, TabId)) {
        if let (Some(pages), Some(tab)) = (&self.pages, self.browser.space().active_tab) {
            f(&mut pages.borrow_mut(), tab);
        }
    }

    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    fn accent(&self) -> f32 {
        self.browser.space().hue / 360.0
    }

    /// Background hue at the pager position, blending neighbouring spaces.
    fn pager_hue(&self) -> f32 {
        let spaces = &self.browser.spaces;
        let position = self.pager.position.clamp(0.0, spaces.len() as f32 - 1.0);
        let (a, b) = (position.floor() as usize, position.ceil() as usize);
        let (from, to) = (spaces[a].hue, spaces[b].hue);
        // Shortest way around the color wheel.
        let diff = ((to - from + 540.0) % 360.0) - 180.0;
        (from + diff * position.fract()).rem_euclid(360.0) / 360.0
    }

    // Command bar.

    pub fn open_command_bar(
        &mut self,
        target: Target,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            let mut input = TextInput::new("Search or Enter URL…", cx);
            input.set_text(text.to_string(), cx);
            input
        });
        let changed = cx.subscribe(&input, |this, _, _: &text_input::Changed, cx| {
            if let Some(bar) = &mut this.command_bar {
                bar.selected = 0;
            }
            cx.notify();
        });
        self.command_bar = Some(CommandBar {
            input: input.clone(),
            target,
            selected: 0,
            overlay: None,
            _changed: changed,
        });
        cx.notify();

        // Opened after this update: the overlay renders from this view.
        let browser = cx.entity().downgrade();
        let bounds = window.bounds();
        let parent = self.native_view;
        cx.defer(move |cx| {
            let still_open = browser
                .read_with(cx, |this, _| {
                    this.command_bar.as_ref().is_some_and(|bar| bar.input == input)
                })
                .unwrap_or(false);
            if !still_open {
                return;
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let overlay = cx.open_window(options, |window, cx| {
                input.read(cx).focus_handle(cx).focus(window);
                if let (Some(parent), Some(overlay)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, overlay, false);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::CommandBar, cx))
            });
            if let Ok(overlay) = overlay {
                let _ = browser.update(cx, |this, _| {
                    if let Some(bar) = &mut this.command_bar {
                        bar.overlay = Some(overlay);
                    }
                });
            }
        });
    }

    fn close_command_bar(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = self.command_bar.take() else {
            return;
        };
        let main_window = self.main_window;
        let focus = self.focus_handle.clone();
        let pages = self.pages.clone();
        // Deferred: this may run inside the overlay window's own event.
        cx.defer(move |cx| {
            if let Some(overlay) = bar.overlay {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            }
            let _ = main_window.update(cx, |_, window, _| {
                window.activate_window();
                focus.focus(window);
            });
            if let Some(pages) = pages {
                pages.borrow_mut().focus_shown();
            }
        });
        cx.notify();
    }

    pub fn suggestions(&self, cx: &App) -> Vec<Suggestion> {
        let Some(bar) = &self.command_bar else {
            return Vec::new();
        };
        let text = bar.input.read(cx).text();
        let query = text.trim().to_lowercase();
        let mut suggestions = Vec::new();
        // Commands are explicit suggestions: never interpret arbitrary prose as an action.
        if !query.is_empty() {
            let active = self.browser.active_tab();
            let pinned = active.is_some_and(|tab| tab.pinned_url.is_some());
            let commands = [
                (Command::ToggleSidebar, if self.sidebar_visible { "Hide Sidebar" } else { "Show Sidebar" }, "toggle sidebar|show sidebar|hide sidebar", true),
                (Command::TogglePin, if pinned { "Unpin Tab" } else { "Pin Tab" }, if pinned { "unpin tab|move to today" } else { "pin tab|move to pinned" }, active.is_some()),
                (Command::CloseTab, if pinned { "Close Tab" } else { "Archive Tab" }, "close tab|archive tab", active.is_some()),
                (Command::ClearToday, "Clear Unpinned Tabs", "clear unpinned tabs|clear today|archive all today tabs", !self.browser.space().today.is_empty()),
                (Command::RestoreTab, "Restore Last Closed Tab", "restore last closed tab|reopen closed tab|unarchive tab", !self.browser.archive.is_empty()),
                (Command::Reload, "Refresh the Page", "refresh page|reload tab", active.is_some()),
                (Command::GoBack, "Go Back", "go back", active.is_some()),
                (Command::GoForward, "Go Forward", "go forward", active.is_some()),
                (Command::NextTab, "Next Tab", "next tab", active.is_some()),
                (Command::PreviousTab, "Previous Tab", "previous tab", active.is_some()),
            ];
            suggestions.extend(commands.into_iter()
                .filter(|(_, label, names, available)| *available && (matches_command(&query, label) || matches_command(&query, names)))
                .map(|(command, label, _, _)| Suggestion::Action { command, label: label.into() }));
            for space in &self.browser.spaces {
                let names = format!("focus on {}|switch to {}|{}", space.name, space.name, space.name);
                if matches_command(&query, &names) {
                    suggestions.push(Suggestion::Action {
                        command: Command::FocusSpace(space.id),
                        label: format!("Focus on {}", space.name),
                    });
                }
            }
            suggestions.truncate(5);
        }
        suggestions.extend(omnibox::resolve(text).map(Suggestion::Load));
        if bar.target == Target::NewTab {
            // Current Space first, then every other Space, keeping duplicate URLs distinct.
            let current = self.browser.active_space;
            let spaces = std::iter::once(&self.browser.spaces[current]).chain(
                self.browser.spaces.iter().enumerate().filter(|(index, _)| *index != current).map(|(_, space)| space)
            );
            suggestions.extend(spaces.flat_map(|space| {
                space.pinned.iter().chain(&space.today).map(move |tab| (space, tab))
            }).filter(|(_, tab)| query.is_empty() || tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query))
                .take(MAX_TAB_SUGGESTIONS)
                .map(|(space, tab)| Suggestion::SwitchToTab {
                    id: tab.id, title: tab.title.clone(), url: tab.url.clone(), space: space.name.clone(),
                }));
        }
        suggestions
    }

    fn select(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.suggestions(cx).len() as isize;
        if let Some(bar) = &mut self.command_bar {
            if count > 0 {
                bar.selected = (bar.selected as isize + delta).rem_euclid(count) as usize;
                cx.notify();
            }
        }
    }

    pub fn confirm(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(target) = self.command_bar.as_ref().map(|bar| bar.target) else {
            return;
        };
        let Some(suggestion) = self.suggestions(cx).into_iter().nth(index) else {
            return;
        };
        let now = SystemTime::now();
        match suggestion {
            Suggestion::Load(resolved) => {
                let title = match &resolved {
                    Resolved::Search { query, .. } => query.clone(),
                    Resolved::Url(url) => display_url(url),
                };
                let id = match target {
                    Target::NewTab => self.browser.new_tab(resolved.url(), now),
                    Target::Tab(id) => {
                        self.browser.navigate(id, resolved.url());
                        id
                    }
                };
                self.browser.set_title(id, title);
            }
            Suggestion::SwitchToTab { id, .. } => {
                if let Some(index) = self.browser.spaces.iter().position(|space| space.pinned.iter().chain(&space.today).any(|tab| tab.id == id)) {
                    self.browser.switch_space(index);
                    self.browser.activate_tab(id, now);
                    self.settle_pager(cx);
                }
            }
            Suggestion::Action { command, .. } => self.run_command(command, cx),
        }
        self.close_command_bar(cx);
    }

    fn run_command(&mut self, command: Command, cx: &mut Context<Self>) {
        match command {
            Command::ToggleSidebar => self.toggle_sidebar(cx),
            Command::TogglePin => {
                if let Some(id) = self.browser.space().active_tab { self.browser.toggle_pin(id); }
            }
            Command::CloseTab => {
                if let Some(id) = self.browser.space().active_tab { self.close_tab(id, cx); }
            }
            Command::ClearToday => { self.browser.clear_today(SystemTime::now()); }
            Command::RestoreTab => {
                if self.browser.restore_last_archived(SystemTime::now()).is_some() { self.settle_pager(cx); }
            }
            Command::Reload => self.with_active_page(|pages, tab| pages.reload(tab)),
            Command::GoBack => self.with_active_page(|pages, tab| pages.go_back(tab)),
            Command::GoForward => self.with_active_page(|pages, tab| pages.go_forward(tab)),
            Command::NextTab => self.cycle_tab(1, cx),
            Command::PreviousTab => self.cycle_tab(-1, cx),
            Command::FocusSpace(id) => {
                if let Some(index) = self.browser.spaces.iter().position(|space| space.id == id) { self.switch_space(index, cx); }
            }
        }
        cx.notify();
    }

    // Window actions.

    fn on_open_command_bar(&mut self, _: &OpenCommandBar, window: &mut Window, cx: &mut Context<Self>) {
        self.open_command_bar(Target::NewTab, "", window, cx);
    }

    fn on_edit_current_url(&mut self, _: &EditCurrentUrl, window: &mut Window, cx: &mut Context<Self>) {
        match self.browser.active_tab().map(|tab| (tab.id, tab.url.clone())) {
            Some((id, url)) => self.open_command_bar(Target::Tab(id), &url, window, cx),
            None => self.open_command_bar(Target::NewTab, "", window, cx),
        }
    }

    fn on_close_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.browser.space().active_tab {
            self.close_tab(id, cx);
        }
    }

    /// Archives a Today tab or unloads a pinned one, as in Arc.
    fn close_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        if self.browser.close_tab(id, SystemTime::now()) == Some(Closed::Unloaded) {
            if let Some(pages) = &self.pages {
                pages.borrow_mut().discard(id);
            }
        }
        cx.notify();
    }

    fn on_go_back(&mut self, _: &GoBack, _: &mut Window, _: &mut Context<Self>) {
        self.with_active_page(|pages, tab| pages.go_back(tab));
    }

    fn on_go_forward(&mut self, _: &GoForward, _: &mut Window, _: &mut Context<Self>) {
        self.with_active_page(|pages, tab| pages.go_forward(tab));
    }

    fn on_reload(&mut self, _: &Reload, _: &mut Window, _: &mut Context<Self>) {
        self.with_active_page(|pages, tab| pages.reload(tab));
    }

    fn on_toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.browser.space().active_tab {
            self.browser.toggle_pin(id);
            cx.notify();
        }
    }

    fn on_toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_sidebar(cx);
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        // Window buttons live in the sidebar, like Arc's.
        if let Some(view) = self.native_view {
            crate::native::set_traffic_lights_hidden(view, !self.sidebar_visible);
        }
        if self.sidebar_visible {
            self.close_floating_sidebar(cx);
        }
        cx.notify();
    }

    /// Slides the hidden sidebar in over the page, as when the pointer
    /// touches the window's left edge in Arc.
    pub fn reveal_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_visible || self.floating_sidebar.is_some() {
            return;
        }
        self.floating_sidebar = Some(FloatingSidebar {
            overlay: None,
            entered: false,
            closing: false,
        });
        cx.notify();

        let browser = cx.entity().downgrade();
        let main = window.bounds();
        let bounds = Bounds::new(
            main.origin,
            size(px(SIDEBAR_WIDTH + 2.0 * FLOATING_SIDEBAR_INSET + FLOATING_SIDEBAR_SHADOW), main.size.height),
        );
        let parent = self.native_view;
        cx.defer(move |cx| {
            let wanted = browser
                .read_with(cx, |this, _| {
                    this.floating_sidebar.as_ref().is_some_and(|f| f.overlay.is_none() && !f.closing)
                })
                .unwrap_or(false);
            if !wanted {
                return;
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(TRAFFIC_LIGHT_X), px(TRAFFIC_LIGHT_Y))),
                }),
                focus: false,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let overlay = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(overlay)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, overlay, true);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Sidebar, cx))
            });
            if let Ok(overlay) = overlay {
                let _ = browser.update(cx, |this, cx| {
                    if let Some(floating) = &mut this.floating_sidebar {
                        floating.overlay = Some(overlay);
                    }
                    // Hide again if the pointer never comes over it.
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(FLOATING_SIDEBAR_GRACE).await;
                        let _ = this.update(cx, |this, cx| {
                            if this.floating_sidebar.as_ref().is_some_and(|f| !f.entered) {
                                this.close_floating_sidebar(cx);
                            }
                        });
                    })
                    .detach();
                });
            }
        });
    }

    fn on_floating_sidebar_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let Some(floating) = &mut self.floating_sidebar else {
            return;
        };
        if hovered {
            floating.entered = true;
        } else if floating.entered {
            self.close_floating_sidebar(cx);
        }
    }

    /// Slides the floating sidebar out, then removes its window.
    pub fn close_floating_sidebar(&mut self, cx: &mut Context<Self>) {
        let Some(floating) = &mut self.floating_sidebar else {
            return;
        };
        if floating.closing {
            return;
        }
        floating.closing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLOATING_SIDEBAR_SLIDE).await;
            let overlay = this
                .update(cx, |this, cx| {
                    cx.notify();
                    this.floating_sidebar.take().and_then(|f| f.overlay)
                })
                .ok()
                .flatten();
            if let Some(overlay) = overlay {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            }
        })
        .detach();
    }

    /// Two-finger horizontal swipe on the sidebar drags between spaces.
    fn on_sidebar_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let ScrollDelta::Pixels(delta) = event.delta else {
            return;
        };
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        match event.touch_phase {
            TouchPhase::Started => {
                self.pager.settle = None;
                self.pager.drag = Some(Drag {
                    start: self.pager.position,
                    travel: 0.0,
                    horizontal: None,
                    samples: Vec::new(),
                });
            }
            TouchPhase::Moved => {
                // Momentum after the fingers lift has no drag and is ignored.
                let Some(drag) = &mut self.pager.drag else {
                    return;
                };
                if drag.horizontal.is_none() && (dx != 0.0 || dy != 0.0) {
                    drag.horizontal = Some(dx.abs() > dy.abs());
                }
                if drag.horizontal != Some(true) {
                    return;
                }
                drag.travel += dx;
                drag.samples.push((Instant::now(), dx));
                // Fingers moving left reveal the space on the right.
                let raw = drag.start - drag.travel / SIDEBAR_WIDTH;
                self.pager.position = rubber_band(raw, self.browser.spaces.len());
                cx.notify();
            }
            TouchPhase::Ended => {
                let Some(drag) = self.pager.drag.take() else {
                    return;
                };
                if drag.horizontal != Some(true) {
                    return;
                }
                let start = drag.start.round();
                let moved = self.pager.position - drag.start;
                let speed = drag.speed();
                let step = if moved > SPACE_SWIPE_COMMIT || speed < -SPACE_FLICK_SPEED {
                    1.0
                } else if moved < -SPACE_SWIPE_COMMIT || speed > SPACE_FLICK_SPEED {
                    -1.0
                } else {
                    0.0
                };
                let last = self.browser.spaces.len() as f32 - 1.0;
                let target = (start + step).clamp(0.0, last) as usize;
                self.switch_space(target, cx);
            }
        }
    }

    /// Eases the pager to the active space.
    fn settle_pager(&mut self, cx: &mut Context<Self>) {
        let target = self.browser.active_space as f32;
        if (self.pager.position - target).abs() < 0.001 {
            self.pager.position = target;
            self.pager.settle = None;
            return;
        }
        self.pager.settle = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(PAGER_FRAME).await;
            let done = this.update(cx, |this, cx| {
                let pager = &mut this.pager;
                let remaining = target - pager.position;
                if remaining.abs() < 0.002 {
                    pager.position = target;
                } else {
                    pager.position += remaining * PAGER_SETTLE_RATE;
                }
                cx.notify();
                pager.position == target
            });
            if !matches!(done, Ok(false)) {
                break;
            }
        }));
    }

    fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.browser.cycle_tab(delta, SystemTime::now());
        cx.notify();
    }

    fn switch_space(&mut self, index: usize, cx: &mut Context<Self>) {
        self.browser.switch_space(index);
        self.settle_pager(cx);
        cx.notify();
    }

    fn cycle_space(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.browser.spaces.len() as isize;
        let index = (self.browser.active_space as isize + delta).rem_euclid(n);
        self.switch_space(index as usize, cx);
    }

    // Rendering.

    fn render_tab(
        &self,
        tab: &Tab,
        section: Section,
        active: bool,
        hue: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = tab.id;
        let key = id.get() as usize;
        let group = SharedString::from(format!("tab-{key}"));

        div()
            .id(("tab", key))
            .debug_selector(|| format!("tab-{key}"))
            .group(group.clone())
            .flex()
            .items_center()
            .gap_2()
            .h(px(34.0))
            .px_2()
            .rounded_lg()
            .when(active, |this| this.bg(hsla(0.0, 0.0, 1.0, 0.85)).shadow_sm())
            .when(!active, |this| this.hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3))))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.browser.activate_tab(id, SystemTime::now());
                cx.notify();
            }))
            .child(favicon(&tab.title, hue))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_sm()
                    .child(tab.title.clone()),
            )
            .child(
                div()
                    .id(("close", key))
                    .debug_selector(|| format!("close-{key}"))
                    .flex_none()
                    .px_1()
                    .rounded_sm()
                    .text_sm()
                    .text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(id, cx);
                    }))
                    .child(match section {
                        Section::Pinned => "−",
                        Section::Today => "×",
                    }),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self
            .browser
            .active_tab()
            .map(|t| display_url(&t.url))
            .unwrap_or_default();
        let pages: Vec<_> = (0..self.browser.spaces.len())
            .map(|index| self.render_space_page(index, cx).into_any_element())
            .collect();
        let offset = -self.pager.position * SIDEBAR_WIDTH;

        div()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_sidebar_scroll(event, cx)))
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .pt(px(38.0))
            .pb_2()
            .gap_1()
            .text_color(hsla(0.0, 0.0, 0.1, 1.0))
            .child(
                div().px_2().child(
                    div()
                        .id("address")
                        .debug_selector(|| "address".into())
                        .h(px(32.0))
                        .px_3()
                        .mb_2()
                        .flex()
                        .items_center()
                        .rounded_lg()
                        .bg(hsla(0.0, 0.0, 1.0, 0.35))
                        .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.5)))
                        .text_sm()
                        .text_color(hsla(0.0, 0.0, 0.0, 0.6))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.on_edit_current_url(&EditCurrentUrl, window, cx)
                        }))
                        .child(url),
                ),
            )
            // Spaces side by side, shifted to the pager position.
            .child(
                div().flex_1().min_h_0().relative().overflow_hidden().child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(offset))
                        .flex()
                        .children(pages),
                ),
            )
            .child(self.render_space_switcher(cx))
    }

    fn render_space_page(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let space = &self.browser.spaces[index];
        let hue = space.hue / 360.0;
        let current = index == self.browser.active_space;
        let tabs = |tabs: &[Tab], section, cx: &mut Context<Self>| -> Vec<AnyElement> {
            tabs.iter()
                .map(|tab| {
                    let active = space.active_tab == Some(tab.id);
                    self.render_tab(tab, section, active, hue, cx).into_any_element()
                })
                .collect()
        };
        let pinned = tabs(&space.pinned, Section::Pinned, cx);
        let today = tabs(&space.today, Section::Today, cx);

        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .px_2()
            .gap_1()
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .child(space.name.clone()),
            )
            .children(pinned)
            .child(div().h(px(1.0)).mx_2().my_2().bg(hsla(0.0, 0.0, 0.0, 0.12)))
            .child(
                div()
                    .id(("new-tab", index))
                    .when(current, |this| this.debug_selector(|| "new-tab".into()))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(34.0))
                    .px_2()
                    .rounded_lg()
                    .text_sm()
                    .text_color(hsla(0.0, 0.0, 0.0, 0.55))
                    .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_open_command_bar(&OpenCommandBar, window, cx)
                    }))
                    .child("+  New Tab"),
            )
            .child(
                div()
                    .id(("today", index))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(today),
            )
    }

    fn render_space_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .justify_center()
            .gap_2()
            .pt_2()
            .children(self.browser.spaces.iter().enumerate().map(|(index, space)| {
                let active = index == self.pager.position.round() as usize;
                let hue = space.hue / 360.0;
                div()
                    .id(("space", index))
                    .debug_selector(move || format!("space-{index}"))
                    .size(px(if active { 10.0 } else { 8.0 }))
                    .rounded_full()
                    .bg(hsla(hue, 0.55, 0.5, if active { 1.0 } else { 0.45 }))
                    .hover(move |s| s.bg(hsla(hue, 0.55, 0.5, 0.8)))
                    .on_click(cx.listener(move |this, _, _, cx| this.switch_space(index, cx)))
            }))
    }

    fn render_content(&self) -> impl IntoElement {
        let native_view = self.native_view;
        let mouse_tracking = self.mouse_tracking.clone();
        // The page view covers the card; text only shows when there is no page.
        let placeholder = match (self.browser.active_tab(), &self.pages) {
            (None, _) => Some(("No tab open".to_string(), "Press ⌘T to open a new tab".to_string())),
            (Some(tab), None) => Some((tab.title.clone(), tab.url.clone())),
            (Some(_), Some(_)) => None,
        };
        div()
            .flex_1()
            // Never grow wider than the window because of content.
            .min_w_0()
            .h_full()
            .py(px(CONTENT_INSET))
            .pr(px(CONTENT_INSET))
            .when(!self.sidebar_visible, |this| this.pl(px(CONTENT_INSET)))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .px_8()
                    .overflow_hidden()
                    .rounded(px(CONTENT_CORNER_RADIUS))
                    .bg(gpui::white())
                    .shadow_md()
                    .relative()
                    .children(placeholder.map(|(title, url)| {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_2()
                            .max_w_full()
                            .child(div().max_w_full().truncate().text_xl().child(title))
                            .child(
                                div()
                                    .max_w_full()
                                    .truncate()
                                    .text_sm()
                                    .text_color(hsla(0.0, 0.0, 0.0, 0.45))
                                    .child(url),
                            )
                    }))
                    .children(self.pages.clone().map(|pages| {
                        // Reports the card's bounds so the native page view can sit on it.
                        canvas(
                            move |bounds, _, cx| {
                                let bounds = engine::Bounds {
                                    x: f64::from(f32::from(bounds.origin.x)),
                                    y: f64::from(f32::from(bounds.origin.y)),
                                    width: f64::from(f32::from(bounds.size.width)),
                                    height: f64::from(f32::from(bounds.size.height)),
                                };
                                if let Some(view) = native_view {
                                    let hole = [bounds.x, bounds.y, bounds.width, bounds.height];
                                    let mut tracking = mouse_tracking.borrow_mut();
                                    if tracking.hole != Some(hole) {
                                        tracking.hole = Some(hole);
                                        crate::native::track_mouse_outside(view, hole, &mut tracking.areas);
                                    }
                                }
                                match pages.try_borrow_mut() {
                                    Ok(mut pages) => pages.set_bounds(bounds),
                                    Err(_) => {
                                        let pages = pages.clone();
                                        cx.defer(move |_| pages.borrow_mut().set_bounds(bounds));
                                    }
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                    })),
            )
    }

    fn render_floating_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(floating) = &self.floating_sidebar else {
            return div().into_any_element();
        };
        let closing = floating.closing;
        let hue = self.pager_hue();
        let width = SIDEBAR_WIDTH;
        let panel = div()
            .id("floating-sidebar")
            .debug_selector(|| "floating-sidebar".into())
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.on_floating_sidebar_hover(*hovered, cx)
            }))
            .absolute()
            .top(px(FLOATING_SIDEBAR_INSET))
            .bottom(px(FLOATING_SIDEBAR_INSET))
            .w(px(width))
            // Concentric with the window corner.
            .rounded(px(WINDOW_CORNER_RADIUS - FLOATING_SIDEBAR_INSET))
            .overflow_hidden()
            .shadow_lg()
            .bg(linear_gradient(
                160.0,
                linear_color_stop(tint(hue, 0.82), 0.0),
                linear_color_stop(tint(hue, 0.72), 1.0),
            ))
            .child(self.render_sidebar(cx));
        let slide = Animation::new(FLOATING_SIDEBAR_SLIDE).with_easing(ease_out_quint());
        let travel = width + FLOATING_SIDEBAR_INSET;
        div()
            .size_full()
            .child(panel.with_animation(
                if closing { "slide-out" } else { "slide-in" },
                slide,
                move |panel, delta| {
                    let shown = if closing { 1.0 - delta } else { delta };
                    panel.left(px(FLOATING_SIDEBAR_INSET - travel * (1.0 - shown)))
                },
            ))
            .into_any_element()
    }

    fn render_command_bar_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.command_bar {
            Some(bar) => self.render_command_bar(bar, cx).into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn render_command_bar(&self, bar: &CommandBar, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = self.accent();
        let rows: Vec<_> = self
            .suggestions(cx)
            .into_iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let selected = index == bar.selected;
                let (icon, label, detail): (&str, String, String) = match suggestion {
                    Suggestion::Load(Resolved::Search { query, .. }) => {
                        ("⌕", query, "Google Search".into())
                    }
                    Suggestion::Load(Resolved::Url(url)) => ("↗", display_url(&url), "Open".into()),
                    Suggestion::SwitchToTab { title, space, .. } => {
                        ("⇥", title, format!("Switch to Tab · {space}"))
                    }
                    Suggestion::Action { label, .. } => ("⌘", label, "Action".into()),
                };
                div()
                    .id(("suggestion", index))
                    .debug_selector(move || format!("suggestion-{index}"))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(40.0))
                    .px_3()
                    .mx_2()
                    .rounded_lg()
                    .when(selected, |this| this.bg(hsla(accent, 0.6, 0.5, 1.0)).text_color(gpui::white()))
                    .when(!selected, |this| this.hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.05))))
                    .on_click(cx.listener(move |this, _, _, cx| this.confirm(index, cx)))
                    .child(div().w(px(16.0)).flex_none().child(icon))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .when(!selected, |this| this.text_color(hsla(0.0, 0.0, 0.0, 0.45)))
                            .child(detail),
                    )
                    .into_any_element()
            })
            .collect();
        let has_rows = !rows.is_empty();

        // Full-window backdrop; clicking outside the panel dismisses it.
        div()
            .id("command-bar-backdrop")
            .debug_selector(|| "command-bar-backdrop".into())
            .size_full()
            // The overlay window is square; match the browser window's corners.
            .rounded(px(WINDOW_CORNER_RADIUS))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(140.0))
            .bg(hsla(0.0, 0.0, 0.0, 0.15))
            .on_click(cx.listener(|this, _, _, cx| this.close_command_bar(cx)))
            .child(
                div()
                    .id("command-bar")
                    .debug_selector(|| "command-bar".into())
                    .key_context(COMMAND_BAR_CONTEXT)
                    .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select(1, cx)))
                    .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.select(-1, cx)))
                    .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                        let index = this.command_bar.as_ref().map_or(0, |bar| bar.selected);
                        this.confirm(index, cx)
                    }))
                    .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                        this.close_command_bar(cx)
                    }))
                    // Keep clicks inside the panel from reaching the backdrop.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .w(px(COMMAND_BAR_WIDTH))
                    .flex()
                    .flex_col()
                    .pb_2()
                    .rounded_xl()
                    .bg(hsla(0.0, 0.0, 0.99, 1.0))
                    .shadow_2xl()
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 0.0, 0.08))
                    .text_color(hsla(0.0, 0.0, 0.1, 1.0))
                    .child(
                        div()
                            .debug_selector(|| "command-bar-input".into())
                            .px_4()
                            .pt_3()
                            .pb_2()
                            .text_size(px(20.0))
                            .line_height(px(32.0))
                            .child(bar.input.clone()),
                    )
                    .when(has_rows, |this| {
                        this.child(div().h(px(1.0)).mb_2().bg(hsla(0.0, 0.0, 0.0, 0.08)))
                            .text_sm()
                            .children(rows)
                    }),
            )
    }
}

impl Render for BrowserWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(pages) = &self.pages {
            match pages.try_borrow_mut() {
                Ok(mut pages) => pages.sync(&self.browser, true),
                // Re-entered from engine work; sync on the next frame.
                Err(_) => cx.on_next_frame(window, |_, _, cx| cx.notify()),
            }
        }
        let hue = self.pager_hue();

        div()
            .key_context(WINDOW_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if !this.sidebar_visible && event.position.x < px(SIDEBAR_REVEAL_EDGE) {
                    this.reveal_sidebar(window, cx);
                }
            }))
            .on_action(cx.listener(Self::on_open_command_bar))
            .on_action(cx.listener(Self::on_edit_current_url))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.cycle_tab(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| this.cycle_tab(-1, cx)))
            .on_action(cx.listener(Self::on_go_back))
            .on_action(cx.listener(Self::on_go_forward))
            .on_action(cx.listener(Self::on_reload))
            .on_action(cx.listener(Self::on_toggle_pin))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(|this, _: &NextSpace, _, cx| this.cycle_space(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousSpace, _, cx| this.cycle_space(-1, cx)))
            .on_action(cx.listener(|this, action: &SwitchToSpace, _, cx| this.switch_space(action.0, cx)))
            .on_action(cx.listener(|this, action: &GoToItem, _, cx| {
                this.browser.activate_item(action.0, SystemTime::now());
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearToday, _, cx| {
                this.browser.clear_today(SystemTime::now());
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RestoreTab, _, cx| {
                if this.browser.restore_last_archived(SystemTime::now()).is_some() {
                    this.settle_pager(cx);
                }
                cx.notify();
            }))
            .relative()
            .flex()
            .size_full()
            .font_family(".SystemUIFont")
            .bg(linear_gradient(
                160.0,
                linear_color_stop(tint(hue, 0.82), 0.0),
                linear_color_stop(tint(hue, 0.72), 1.0),
            ))
            .when(self.sidebar_visible, |this| this.child(self.render_sidebar(cx)))
            .child(self.render_content())

    }
}

fn favicon(title: &str, hue: f32) -> impl IntoElement {
    let initial: String = title.chars().next().unwrap_or('•').to_uppercase().collect();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(18.0))
        .rounded_sm()
        .bg(hsla(hue, 0.5, 0.45, 1.0))
        .text_color(gpui::white())
        .text_xs()
        .child(initial)
}

fn tint(hue: f32, lightness: f32) -> Hsla {
    hsla(hue, 0.45, lightness, 1.0)
}

pub fn display_url(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_string()
}

/// Resists dragging past the first and last space.
fn rubber_band(position: f32, spaces: usize) -> f32 {
    let last = spaces.saturating_sub(1) as f32;
    if position < 0.0 {
        position * PAGER_RUBBER_BAND
    } else if position > last {
        last + (position - last) * PAGER_RUBBER_BAND
    } else {
        position
    }
}
