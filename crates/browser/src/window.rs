//! Arc-style browser window: vertical sidebar, content card, command bar.
//! Native CEF pages sit below browser-owned GPUI controls.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use browser_core::omnibox::{self, Resolved};
use browser_core::{Browser, Section, SpaceId, Tab, TabId};
use browser_core::folders::{FolderId, PinnedItem};
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
const MAX_HISTORY_SUGGESTIONS: usize = 4;
const MAX_COMMAND_SUGGESTIONS: usize = 12;
const ARCHIVE_CHECK_INTERVAL: Duration = Duration::from_secs(60);
const ENGINE_PUMP_INTERVAL: Duration = Duration::from_millis(8);
const SESSION_SAVE_INTERVAL: Duration = Duration::from_secs(1);
const ARCHIVE_OPTIONS: &[(&str, Option<u64>)] = &[
    ("Never", None), ("1 hour", Some(3600)), ("6 hours", Some(21600)),
    ("12 hours", Some(43200)), ("24 hours", Some(86400)),
    ("7 days", Some(604800)), ("30 days", Some(2592000)),
];
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
        CopyUrl,
        CopyUrlAsMarkdown,
        NewFolder,
        CollapseAllFolders,
        ExpandAllFolders,
        NewSpace,
        EditCurrentUrl,
        CloseTab,
        AddSplit,
        CloseSplitPane,
        NextTab,
        PreviousTab,
        GoBack,
        GoForward,
        Reload,
        ForceReload,
        StopLoading,
        ZoomIn,
        ZoomOut,
        ResetZoom,
        DuplicateTab,
        TogglePin,
        ToggleFavorite,
        ToggleMute,
        MuteAllTabs,
        UnmuteAllTabs,
        TogglePinnedSection,
        CustomizeTabIcon,
        CustomizeSpaceIcon,
        ResetNameAndIcon,
        ToggleSpaceHeader,
        ToggleFavoritesOrdering,
        RevealTab,
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
    "cmd-y" => crate::history::ViewHistory,
    "cmd-m" => crate::menus::Minimize,
    "ctrl-cmd-f" => crate::menus::ToggleFullscreen,
    "cmd-l" => EditCurrentUrl,
    "cmd-shift-c" => CopyUrl,
    "cmd-alt-shift-c" => CopyUrlAsMarkdown,
    "cmd-w" => CloseTab,
    "ctrl-shift-=" => AddSplit,
    "ctrl-shift--" => CloseSplitPane,
    "cmd-shift-t" => RestoreTab,
    "cmd-shift-k" => ClearToday,
    "ctrl-tab" => NextTab,
    "ctrl-shift-tab" => PreviousTab,
    "cmd-alt-down" => NextTab,
    "cmd-alt-up" => PreviousTab,
    "ctrl-cmd-right" => NextTab,
    "ctrl-cmd-left" => PreviousTab,
    "cmd-[" => GoBack,
    "cmd-]" => GoForward,
    "cmd-f" => crate::find::FindPage,
    "cmd-g" => crate::find::NextMatch,
    "cmd-shift-g" => crate::find::PreviousMatch,
    "cmd-r" => Reload,
    "cmd-shift-r" => ForceReload,
    "cmd-." => StopLoading,
    "cmd-=" => ZoomIn,
    "cmd-+" => ZoomIn,
    "cmd-shift-=" => ZoomIn,
    "cmd--" => ZoomOut,
    "cmd-0" => ResetZoom,
    "cmd-left" => GoBack,
    "cmd-right" => GoForward,
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
        KeyBinding::new("cmd-t", OpenCommandBar, bar),
        KeyBinding::new("cmd-l", EditCurrentUrl, bar),
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
    fn speed(&self, now: Instant) -> f32 {
        let Some(&(last, _)) = self.samples.last() else {
            return 0.0;
        };
        if now.saturating_duration_since(last) > Duration::from_millis(50) {
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
    Split { anchor: TabId, placement: browser_core::splits::Placement },
    Tab(TabId),
    NewSpace,
    RenameSpace(SpaceId),
    RenameTab(TabId),
    Icon(crate::icons::IconTarget),
    NewFolder { space: SpaceId, parent: Option<FolderId>, tab: Option<TabId> },
    RenameFolder(FolderId),
    MoveToFolder(TabId),
    MoveFolder(FolderId),
    ArchiveInterval,
    Archive(Option<SpaceId>),
    ConfirmClearArchive,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Suggestion {
    Load(Resolved),
    SplitTab { id: TabId, title: String, space: String },
    History { title: String, url: String },
    SwitchToTab { id: TabId, title: String, url: String, space: String },
    Action { command: Command, label: String },
    SaveSpace { name: String },
    SaveFolderName { name: String },
    FolderDestination { space: SpaceId, parent: Option<FolderId>, label: String },
    SaveTabName { name: String },
    SaveIcon { icon: Option<String> },
    ArchiveAfter { label: String, seconds: Option<u64> },
    ArchivedTab { id: TabId, title: String, detail: String, date: String },
    MoreArchive { remaining: usize },
    ConfirmClearArchive,
}

/// Commands currently implemented by the browser, also exposed by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    AddSplit(Option<browser_core::splits::Placement>),
    CloseSplitPane,
    SplitAxis(browser_core::splits::Axis),
    SeparateSplit,
    SeparatePane,
    FocusPane(usize),
    CyclePane(isize),
    MovePane(isize),
    DuplicateTab,
    NewFolder,
    MoveToFolder,
    PinToFolder(FolderId),
    NewFolderWithTab,
    CollapseAllFolders,
    ExpandAllFolders,
    RenameFolder(FolderId),
    ExpandFolder(FolderId),
    NewSpace,
    RenameSpace,
    FindPage,
    ArchiveSettings,
    ViewArchive,
    ViewHistory,
    RequestClearArchive,
    ResetPinnedTab,
    ResetSpace,
    RenameTab,
    CustomizeTabIcon,
    CustomizeSpaceIcon,
    ResetNameAndIcon,
    ResetTabName,
    ReplacePinnedUrl,
    CopyUrl,
    CopyUrlAsMarkdown,
    ToggleSpaceHeader,
    ToggleFavoritesOrdering,
    RevealTab,
    ToggleSidebar,
    TogglePin,
    ToggleFavorite,
    ToggleMute,
    MuteAllTabs,
    UnmuteAllTabs,
    TogglePinnedSection,
    CloseTab,
    ArchiveRelative(browser_core::bulk_archive::Relative),
    ClearToday,
    RestoreTab,
    Reload,
    ForceReload,
    StopLoading,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    EnterFullscreen,
    ExitFullscreen,
    MinimizeWindow,
    ZoomWindow,
    GoBack,
    GoForward,
    NextTab,
    PreviousTab,
    FocusSpace(SpaceId),
    MoveTab { space: SpaceId, section: Section },
}

/// Exact command names win over abbreviated prefixes of longer actions.
fn command_match_score(query: &str, names: &str) -> Option<usize> {
    let words: Vec<_> = query.split_whitespace().collect();
    names.split('|').filter_map(|name| {
        let name = name.trim().trim_end_matches('…').to_lowercase();
        if name == query { return Some(0); }
        let parts: Vec<_> = name.split_whitespace().collect();
        if !words.is_empty() && words.len() <= parts.len()
            && words.iter().zip(&parts).all(|(word, part)| part.starts_with(word)) {
            Some(1 + parts.len() - words.len())
        } else { None }
    }).min()
}

pub struct CommandBar {
    pub input: Entity<TextInput>,
    pub target: Target,
    pub selected: usize,
    pub(crate) archive_reason: Option<browser_core::ArchiveReason>,
    pub(crate) archive_limit: usize,
    pub(crate) rows_scroll: gpui::ScrollHandle,
    /// The bar is drawn in its own transparent window above the browser
    /// window, since pages are native views that cover anything drawn below.
    pub overlay: Option<WindowHandle<Overlay>>,
    _changed: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    CommandBar,
    Find,
    Toast,
    /// The hidden sidebar, shown floating over the page.
    Sidebar,
    TabMenu,
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
    pub(crate) fn new(browser: WeakEntity<BrowserWindow>, kind: OverlayKind, cx: &mut Context<Self>) -> Self {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        self.browser
            .update(cx, |browser, cx| {
                let content = match kind {
                    OverlayKind::CommandBar => browser.render_command_bar_overlay(f32::from(window.viewport_size().height), cx),
                    OverlayKind::Find => browser.render_find_bar(cx),
                    OverlayKind::Toast => browser.render_toast(),
                    OverlayKind::Sidebar => browser.render_floating_sidebar(cx),
                    OverlayKind::TabMenu => browser.render_tab_menu(cx),
                };
                browser.render_overlay_shell(content, cx)
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}

#[derive(Default)]
struct MouseTracking {
    holes: Option<Vec<[f64; 4]>>,
    areas: Vec<usize>,
}

/// The sidebar shown over the page while it is hidden (⌘S).
pub struct FloatingSidebar {
    pub overlay: Option<WindowHandle<Overlay>>,
    /// The pointer has been over it; leaving then hides it.
    entered: bool,
    closing: bool,
}


#[derive(Default)]
struct SidebarScrolls { pinned: gpui::ScrollHandle, today: gpui::ScrollHandle, pending: Rc<Cell<Option<(Section, usize)>>> }

pub struct BrowserWindow {
    pub browser: Browser,
    pub sidebar_visible: bool,
    sidebar_scrolls: HashMap<SpaceId, SidebarScrolls>,
    pub command_bar: Option<CommandBar>,
    pub(crate) find_bar: Option<crate::find::FindBar>,
    pub(crate) toast: Option<crate::toast::Toast>,
    pub(crate) toast_generation: u64,
    pub(crate) history_page: crate::history::HistoryPage,
    pub(crate) tab_menu: Option<crate::tab_menu::TabMenu>,
    /// Web pages; `None` when running without an engine.
    pub pages: Option<Rc<RefCell<Pages>>>,
    pub floating_sidebar: Option<FloatingSidebar>,
    pub(crate) main_window: AnyWindowHandle,
    pub(crate) native_view: Option<*mut c_void>,
    /// Native mouse tracking around the page, see `native::track_mouse_outside`.
    mouse_tracking: Rc<RefCell<MouseTracking>>,
    pub pager: Pager,
    pub(crate) focus_handle: FocusHandle,
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
            sidebar_scrolls: HashMap::new(),
            command_bar: None,
            find_bar: None,
            toast: None,
            toast_generation: 0,
            history_page: crate::history::HistoryPage::new(cx),
            pages,
            floating_sidebar: None,
            tab_menu: None,
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
            if matches!(&event, Event::Focused { .. }) && (self.command_bar.is_some() || self.tab_menu.is_some()) { continue; }
            pages.borrow_mut().record(&event);
            match event {
                Event::Focused { view } => {
                    let tab = pages.borrow().tab_for(view).filter(|tab| pages.borrow().is_visible(*tab));
                    if let Some(tab) = tab {
                        let active = self.browser.space().active_tab;
                        if active.is_some_and(|id| self.browser.related_tabs(id).contains(&tab)) {
                            self.browser.activate_tab(tab, SystemTime::now());
                            if active != Some(tab) { self.close_find(cx); }
                        }
                    }
                }
                Event::TitleChanged { view, title } => {
                    if let Some(tab) = pages.borrow().tab_for(view) {
                        if !title.is_empty() {
                            self.browser.set_title(tab, title.clone());
                            let url = self.browser.all_tabs().find(|entry| entry.id == tab).map(|entry| entry.url.clone());
                            if let Some(url) = url { self.browser.update_history_title(&url, &title); }
                        }
                    }
                }
                Event::Favicon { view, page_url, png } => {
                    if pages.borrow().tab_for(view).is_some() { self.browser.cache_favicon(&page_url, png); }
                }
                Event::UrlChanged { view, url } => {
                    if let Some(tab) = pages.borrow().tab_for(view) {
                        self.browser.set_url(tab, url.clone());
                        self.browser.record_visit(&url, SystemTime::now());
                        let title = self.browser.all_tabs().find(|entry| entry.id == tab).and_then(|entry| entry.page_title.clone());
                        if let Some(title) = title { self.browser.update_history_title(&url, &title); }
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
                Event::FindResult { view, count, active, final_update } => {
                    if let Some(bar) = &mut self.find_bar {
                        if pages.borrow().tab_for(view) == Some(bar.tab) && !bar.input.read(cx).text().is_empty() {
                            if count >= 0 { bar.count = Some(count); }
                            if active >= 0 { bar.active = active; }
                            bar.searching = !final_update;
                        }
                    }
                }
                Event::LoadingChanged { view, loading: false, .. } => {
                    if let Some(tab) = pages.borrow().tab_for(view) {
                        let entry = self.browser.all_tabs().find(|entry| entry.id == tab).map(|entry| (entry.url.clone(), entry.page_title.clone()));
                        if let Some((url, Some(title))) = entry { self.browser.update_history_title(&url, &title); }
                    }
                }
                Event::LoadingChanged { .. } => {}
            }
        }
        // CEF can initialize its audio state after create_view returned. Reapply
        // persisted intent after engine events even when the window is occluded.
        pages.borrow_mut().sync_audio(&self.browser);
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
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        self.close_find(cx);
        let input = cx.new(|cx| {
            let placeholder = match target {
                Target::Split { .. } => "Add a page to Split View…",
                Target::NewFolder { .. } => "Name your Folder…",
                Target::RenameFolder(_) => "Rename Folder…",
                Target::MoveToFolder(_) | Target::MoveFolder(_) => "Choose a destination folder…",
                Target::NewSpace => "Name your Space…",
                Target::RenameSpace(_) => "Rename Space…",
                Target::RenameTab(_) => "Rename Tab…",
                Target::Icon(_) => "Enter one symbol or emoji…",
                Target::ArchiveInterval => "Archive tabs after…",
                Target::Archive(_) => "Search Archive…",
                Target::ConfirmClearArchive => "Type “Clear Archive” to confirm…",
                _ => "Search or Enter URL…",
            };
            let mut input = TextInput::new(placeholder, cx);
            input.set_text(text.to_string(), cx);
            input
        });
        let changed = cx.subscribe(&input, |this, _, _: &text_input::Changed, cx| {
            if let Some(bar) = &mut this.command_bar {
                bar.selected = 0;
                bar.rows_scroll.set_offset(point(px(0.), px(0.)));
                bar.archive_limit = 8;
            }
            cx.notify();
        });
        self.command_bar = Some(CommandBar {
            input: input.clone(),
            target,
            selected: 0,
            archive_reason: None,
            archive_limit: 8,
            rows_scroll: gpui::ScrollHandle::new(),
            overlay: None,
            _changed: changed,
        });
        cx.notify();

        // Opened after this update: the overlay renders from this view.
        let browser = cx.entity().downgrade();
        let fallback_bounds = window.bounds();
        let main = self.main_window;
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
            // A click can arrive from the narrow floating sidebar. Size the
            // popup against the browser window rather than that child window.
            let bounds = main.update(cx, |_, window, _| window.bounds()).unwrap_or(fallback_bounds);
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

    fn reset_command_bar(&mut self, target: Target, text: &str, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.command_bar {
            bar.target = target;
            bar.selected = 0;
            bar.input.update(cx, |input, cx| {
                input.set_placeholder("Search or Enter URL…", cx);
                input.set_text(text.to_string(), cx);
            });
        }
        cx.notify();
    }

    /// Repeating the opening shortcut dismisses the bar; a different mode
    /// replaces the query without creating another overlay window.
    fn toggle_command_target(&mut self, target: Target, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(bar) = &self.command_bar else { return false; };
        if bar.target == target {
            self.close_command_bar(cx);
        } else {
            self.reset_command_bar(target, text, cx);
        }
        true
    }

    pub(crate) fn close_command_bar(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = self.command_bar.take() else {
            return;
        };
        let main_window = self.main_window;
        let focus = self.focus_handle.clone();
        let pages = self.pages.clone();
        let browser = cx.entity().downgrade();
        // Deferred: this may run inside the overlay window's own event.
        cx.defer(move |cx| {
            if let Some(overlay) = bar.overlay {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            }
            if browser.read_with(cx, |this, _| this.command_bar.is_some() || this.tab_menu.is_some() || this.find_bar.is_some()).unwrap_or(false) { return; }
            let _ = main_window.update(cx, |_, window, _| {
                window.activate_window();
                focus.focus(window);
            });
            if let Some(pages) = pages {
                let _ = browser.read_with(cx, |this, _| {
                    let mut pages = pages.borrow_mut();
                    pages.sync(&this.browser, true);
                    pages.focus_shown();
                });
            }
        });
        cx.notify();
    }

    pub fn suggestions(&self, cx: &App) -> Vec<Suggestion> {
        let Some(bar) = &self.command_bar else {
            return Vec::new();
        };
        let text = bar.input.read(cx).text();
        if let Target::Split { anchor, .. } = bar.target { return self.split_suggestions(anchor, text); }
        if let Target::Archive(space) = bar.target {
            return self.archive_suggestions(space, text);
        }
        if bar.target == Target::ConfirmClearArchive {
            let mut suggestions = vec![Suggestion::Action { command: Command::ViewArchive, label: "Cancel".into() }];
            if text.trim().eq_ignore_ascii_case("Clear Archive") {
                suggestions.insert(0, Suggestion::ConfirmClearArchive);
            }
            return suggestions;
        }
        if bar.target == Target::ArchiveInterval {
            let query = text.trim().to_lowercase();
            let current = self.browser.archive_after.map(|duration| duration.as_secs());
            let mut options: Vec<_> = ARCHIVE_OPTIONS.iter().filter(|(label, _)| label.to_lowercase().contains(&query)).collect();
            // Enter on the empty picker preserves the current setting.
            options.sort_by_key(|(_, seconds)| *seconds != current);
            let mut suggestions: Vec<_> = options.into_iter().map(|(label, seconds)| Suggestion::ArchiveAfter { label: (*label).into(), seconds: *seconds }).collect();
            if query.is_empty() && !ARCHIVE_OPTIONS.iter().any(|(_, seconds)| *seconds == current) {
                suggestions.insert(0, Suggestion::ArchiveAfter { label: "Current interval".into(), seconds: current });
            }
            return suggestions;
        }
        if matches!(bar.target, Target::NewFolder { .. } | Target::RenameFolder(_)) {
            return if text.trim().is_empty() { Vec::new() } else { vec![Suggestion::SaveFolderName { name: text.trim().to_string() }] };
        }
        if matches!(bar.target, Target::MoveToFolder(_) | Target::MoveFolder(_)) { return self.folder_destinations(bar.target, text); }
        if matches!(bar.target, Target::Icon(_)) {
            return if text.trim().is_empty() { vec![Suggestion::SaveIcon { icon: None }] }
                else if browser_core::icons::valid_icon(text) { vec![Suggestion::SaveIcon { icon: Some(text.trim().to_string()) }] } else { Vec::new() };
        }
        if matches!(bar.target, Target::RenameTab(_)) {
            return if text.trim().is_empty() { Vec::new() } else { vec![Suggestion::SaveTabName { name: text.trim().to_string() }] };
        }
        if matches!(bar.target, Target::NewSpace | Target::RenameSpace(_)) {
            return if text.trim().is_empty() { Vec::new() } else { vec![Suggestion::SaveSpace { name: text.trim().to_string() }] };
        }
        let query = text.trim().to_lowercase();
        let mut suggestions = Vec::new();
        // Commands are explicit suggestions: never interpret arbitrary prose as an action.
        if !query.is_empty() {
            let active = self.browser.active_tab();
            let pinned = active.is_some_and(|tab| tab.pinned_url.is_some());
            let favorite = active.is_some_and(|tab| self.browser.is_favorite(tab.id));
            let modified_pin = active.is_some_and(|tab| tab.pinned_url.as_ref().is_some_and(|original| *original != tab.url));
            let commands = [
                (Command::NewFolder, "New Folder…", "new folder|create folder|add folder", true),
                (Command::MoveToFolder, "Move to Folder…", "move to folder|pin to folder", active.is_some()),
                (Command::NewFolderWithTab, "Pin to New Folder…", "pin to new folder|new folder with current tab", active.is_some()),
                (Command::CollapseAllFolders, "Collapse All Folders", "collapse all folders", self.browser.space().folders.iter().any(|folder| !folder.collapsed)),
                (Command::ExpandAllFolders, "Expand All Folders", "expand all folders", self.browser.space().folders.iter().any(|folder| folder.collapsed)),
                (Command::DuplicateTab, "Duplicate Current Tab", "duplicate current tab|duplicate tab|duplicate current item|duplicate split view", active.is_some()),
                (Command::RenameTab, "Rename Current Tab…", "rename current tab|rename tab|rename current item", active.is_some()),
                (Command::CustomizeTabIcon, "Customize Tab Icon…", "customize tab icon|change tab icon|customize tab", active.is_some()),
                (Command::CustomizeSpaceIcon, "Change Space Icon…", "change space icon|customize space icon", true),
                (Command::ResetNameAndIcon, "Reset Name and Icon", "reset name and icon|reset tab customization", active.is_some_and(|tab| self.browser.item_has_custom_name(tab.id) || self.browser.item_icon(tab.id).is_some())),
                (Command::ResetTabName, "Reset Tab Name", "reset tab name|reset name", active.is_some_and(|tab| self.browser.item_has_custom_name(tab.id))),
                (Command::ResetSpace, "Reset Space", "reset space|reset all pinned tabs|reset all tabs in space", self.browser.space().pinned.iter().any(|tab| tab.pinned_url.is_some())),
                (Command::ResetPinnedTab, "Reset Tab to Original URL", "reset tab original url|reset pinned tab|reset pin", modified_pin),
                (Command::ReplacePinnedUrl, "Replace Pinned URL with Current", "replace pinned url with current|update pinned url|replace pin with current page", modified_pin),
                (Command::ViewHistory, "View History", "view history|open history|browsing history", true),
                (Command::ViewArchive, "View Archive", "view archive|open archive|search archived tabs|archive", true),
                (Command::RequestClearArchive, "Clear Archive…", "clear archive|delete archived tabs", !self.browser.archive.is_empty()),
                (Command::CopyUrl, "Copy URL", "copy url|copy link|copy clean link", active.is_some()),
                (Command::CopyUrlAsMarkdown, "Copy URL as Markdown", "copy url as markdown|copy link as markdown", active.is_some()),
                (Command::ArchiveSettings, "Archive Tabs After…", "archive tabs after|auto archive settings|change auto archiving", true),
                (Command::FindPage, "Find in Page…", "find in page|search page", active.is_some()),
                (Command::NewSpace, "New Space…", "new space|create space|add space", true),
                (Command::RenameSpace, "Rename Space…", "rename space|change space name", true),
                (Command::ToggleSpaceHeader, if self.browser.space().show_header { "Hide Space Header" } else { "Show Space Header" }, if self.browser.space().show_header { "hide space header|toggle space header" } else { "show space header|toggle space header" }, true),
                (Command::ToggleFavoritesOrdering, if self.browser.include_favorites_in_ordering { "Exclude Favorites from Ordering" } else { "Include Favorites in Ordering" }, if self.browser.include_favorites_in_ordering { "exclude favorites from ordering|exclude favorites in ordering" } else { "include favorites in ordering" }, true),
                (Command::RevealTab, "Reveal Tab in Sidebar", "reveal tab in sidebar|reveal tab|show tab in sidebar", active.is_some()),
                (Command::ToggleSidebar, if self.sidebar_visible { "Hide Sidebar" } else { "Show Sidebar" }, if self.sidebar_visible { "toggle sidebar|hide sidebar|sidebar" } else { "toggle sidebar|show sidebar|sidebar" }, true),
                (Command::TogglePin, if pinned { "Unpin Tab" } else { "Pin Tab" }, if pinned { "unpin tab|move to today" } else { "pin tab|move to pinned" }, active.is_some() && !favorite),
                (Command::ToggleFavorite, if favorite { "Unfavorite Tab" } else { "Favorite Tab" }, if favorite { "unfavorite tab|remove from favorites|unfavorite current tab" } else { "favorite tab|favorite current tab|pin to top apps|add to favorites" }, active.is_some()),
                (Command::ToggleMute, if active.is_some_and(|tab| tab.muted) { "Unmute Tab" } else { "Mute Tab" }, if active.is_some_and(|tab| tab.muted) { "unmute tab|unmute current tab" } else { "mute tab|mute current tab" }, active.is_some_and(|tab| !omnibox::is_internal_url(&tab.url))),
                (Command::MuteAllTabs, "Mute All Tabs", "mute all tabs|mute all", self.browser.all_tabs().any(|tab| !tab.muted && !omnibox::is_internal_url(&tab.url))),
                (Command::UnmuteAllTabs, "Unmute All Tabs", "unmute all tabs|unmute all", self.browser.all_tabs().any(|tab| tab.muted && !omnibox::is_internal_url(&tab.url))),
                (Command::TogglePinnedSection, if self.browser.space().pinned_collapsed { "Expand Pinned Section" } else { "Collapse Pinned Section" }, if self.browser.space().pinned_collapsed { "expand pinned section|toggle pinned section" } else { "collapse pinned section|toggle pinned section" }, !self.browser.space().pinned_items.is_empty()),
                (Command::CloseTab, if pinned { "Close Tab" } else { "Archive Tab" }, "close tab|archive tab", active.is_some()),
                (Command::ClearToday, "Clear Unpinned Tabs", "clear unpinned tabs|clear today|archive all today tabs", !self.browser.space().today.is_empty()),
                (Command::RestoreTab, "Restore Last Closed Tab", "restore last closed tab|reopen closed tab|unarchive tab", !self.browser.archive.is_empty()),
                (Command::Reload, "Refresh the Page", "refresh page|reload tab", active.is_some()),
                (Command::ForceReload, "Force Refresh the Page", "force refresh page|reload without cache|hard reload", active.is_some()),
                (Command::StopLoading, "Stop Loading", "stop loading|stop page", active.is_some()),
                (Command::ZoomIn, "Zoom In", "zoom in|increase zoom", active.is_some()),
                (Command::ZoomOut, "Zoom Out", "zoom out|decrease zoom", active.is_some()),
                (Command::ResetZoom, "Zoom to Actual Size", "zoom actual size|reset zoom|100%", active.is_some()),
                (Command::EnterFullscreen, "Enter Full Screen", "enter full screen|full screen", true),
                (Command::ExitFullscreen, "Exit Full Screen", "exit full screen|leave full screen", true),
                (Command::MinimizeWindow, "Minimize Window", "minimize window|minimize browser", true),
                (Command::ZoomWindow, "Zoom Window", "zoom window", true),
                (Command::GoBack, "Go Back", "go back", active.is_some()),
                (Command::GoForward, "Go Forward", "go forward", active.is_some()),
                (Command::NextTab, "Next Tab", "next tab", active.is_some()),
                (Command::PreviousTab, "Previous Tab", "previous tab", active.is_some()),
            ];
            let mut commands = commands.to_vec();
            commands.extend(self.split_commands());
            for (relative, label, names) in [
                (browser_core::bulk_archive::Relative::Other, "Archive Other Tabs", "archive other tabs|close other tabs"),
                (browser_core::bulk_archive::Relative::Above, "Archive Tabs Above", "archive tabs above|close tabs above"),
                (browser_core::bulk_archive::Relative::Below, "Archive Tabs Below", "archive tabs below|close tabs below"),
            ] { commands.push((Command::ArchiveRelative(relative), label, names, active.is_some_and(|tab| !self.browser.tabs_to_archive(tab.id, relative).is_empty()))); }
            let mut matches: Vec<_> = commands.into_iter().filter_map(|(command, label, names, available)| {
                if !available { return None; }
                let score = command_match_score(&query, label).into_iter().chain(command_match_score(&query, names)).min()?;
                Some((score, Suggestion::Action { command, label: label.into() }))
            }).collect();
            for space in &self.browser.spaces {
                let names = format!("focus on {}|switch to {}|{}", space.name, space.name, space.name);
                if let Some(score) = command_match_score(&query, &names) {
                    matches.push((score, Suggestion::Action {
                        command: Command::FocusSpace(space.id),
                        label: format!("Focus on {}", space.name),
                    }));
                }
            }
            for space in &self.browser.spaces {
                for folder in &space.folders {
                    let path = self.browser.folder_path(folder.id);
                    for (command, label, names) in [
                        (Command::ExpandFolder(folder.id), format!("Expand Folder {path}"), format!("expand folder {path}|expand {path}")),
                        (Command::RenameFolder(folder.id), format!("Rename Folder {path}…"), format!("rename folder {path}")),
                    ] {
                        if let Some(score) = command_match_score(&query, &names) { matches.push((score, Suggestion::Action { command, label })); }
                    }
                    if active.is_some_and(|tab| self.browser.folder_ancestors(tab.id).last() != Some(&folder.id)) {
                        let names = format!("pin to {path}|pin to folder {path}|move to folder {path}|pin to {}|pin to {} {path}", folder.name, space.name);
                        if let Some(score) = command_match_score(&query, &names) {
                            matches.push((score, Suggestion::Action { command: Command::PinToFolder(folder.id), label: format!("Pin to {path} · {}", space.name) }));
                        }
                    }
                }
            }
            if let Some(tab) = active {
                let source = if favorite { None } else if self.browser.space().pinned.iter().any(|pinned| pinned.id == tab.id) { Some(Section::Pinned) } else { Some(Section::Today) };
                for space in &self.browser.spaces {
                    for section in [Section::Pinned, Section::Today] {
                        if space.id == self.browser.space().id && Some(section) == source { continue; }
                        let section_name = if section == Section::Pinned { "Pinned" } else { "Today" };
                        let label = format!("Move to {section_name} in {}", space.name);
                        let mut names = format!("{label}|{} {}", if section == Section::Pinned { "pin to" } else { "move to today in" }, space.name);
                        if Some(section) == source { names.push_str(&format!("|move to {}", space.name)); }
                        if let Some(score) = command_match_score(&query, &names) {
                            matches.push((score, Suggestion::Action { command: Command::MoveTab { space: space.id, section }, label }));
                        }
                    }
                }
            }
            matches.sort_by_key(|(score, _)| *score);
            suggestions.extend(matches.into_iter().take(5).map(|(_, suggestion)| suggestion));
        }
        suggestions.extend(omnibox::resolve(text).map(Suggestion::Load));
        if bar.target == Target::NewTab {
            // Current Space first, then every other Space, keeping duplicate URLs distinct.
            let current = self.browser.active_space;
            let spaces = std::iter::once(&self.browser.spaces[current]).chain(
                self.browser.spaces.iter().enumerate().filter(|(index, _)| *index != current).map(|(_, space)| space)
            );
            let favorites = self.browser.favorites.iter().map(|tab| ("Favorites", tab));
            let mut seen = std::collections::HashSet::new();
            suggestions.extend(favorites.chain(spaces.flat_map(|space| {
                space.pinned.iter().chain(&space.today).map(move |tab| (space.name.as_str(), tab))
            }))
            .filter(|(_, tab)| query.is_empty() || self.browser.item_title(tab.id).to_lowercase().contains(&query) || tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)
                || tab.page_title.as_ref().is_some_and(|title| title.to_lowercase().contains(&query)))
                .filter(|(_, tab)| seen.insert(self.browser.item_anchor(tab.id)))
                .take(MAX_TAB_SUGGESTIONS)
                .map(|(space, tab)| Suggestion::SwitchToTab {
                    id: tab.id, title: self.browser.item_title(tab.id), url: tab.url.clone(), space: if self.browser.split_for(tab.id).is_some() { format!("{space} · Split View") } else { space.to_string() },
                }));
        }
        if !query.is_empty() {
            let open: std::collections::HashSet<_> = self.browser.all_tabs().map(|tab| tab.url.as_str()).collect();
            suggestions.extend(self.browser.history.iter()
                .filter(|entry| !open.contains(entry.url.as_str()))
                .filter(|entry| entry.title.to_lowercase().contains(&query) || entry.url.to_lowercase().contains(&query))
                .take(MAX_HISTORY_SUGGESTIONS)
                .map(|entry| Suggestion::History { title: entry.title.clone(), url: entry.url.clone() }));
        }
        suggestions.truncate(MAX_COMMAND_SUGGESTIONS);
        suggestions
    }

    fn select(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.suggestions(cx).len() as isize;
        if let Some(bar) = &mut self.command_bar {
            if count > 0 {
                bar.selected = (bar.selected as isize + delta).rem_euclid(count) as usize;
                bar.rows_scroll.scroll_to_item(bar.selected);
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
                if let Target::Tab(id) = target {
                    if self.browser.all_tabs().any(|tab| tab.id == id && tab.url == resolved.url()) {
                        if let Some(pages) = &self.pages { pages.borrow_mut().reload(id); }
                        self.close_command_bar(cx);
                        return;
                    }
                }
                let title = match &resolved {
                    Resolved::Search { query, .. } => query.clone(),
                    Resolved::Url(url) if url.eq_ignore_ascii_case(crate::history::HISTORY_URL) => "History".into(),
                    Resolved::Url(url) => display_url(url),
                };
                let id = match target {
                    Target::Split { anchor, placement } => {
                        if !self.can_add_split(anchor) || omnibox::is_internal_url(resolved.url()) { return; }
                        let id = self.browser.new_tab(resolved.url(), now);
                        self.browser.add_split(anchor, id, placement, now).expect("validated Split View");
                        id
                    }
                    Target::NewTab => self.browser.new_tab(resolved.url(), now),
                    Target::Tab(id) => {
                        self.browser.navigate(id, resolved.url());
                        id
                    }
                    Target::NewSpace | Target::RenameSpace(_) | Target::RenameTab(_) | Target::Icon(_) | Target::NewFolder { .. } | Target::RenameFolder(_) | Target::MoveToFolder(_) | Target::MoveFolder(_) | Target::ArchiveInterval | Target::Archive(_) | Target::ConfirmClearArchive => return,
                };
                self.browser.set_title(id, title);
            }
            Suggestion::History { title, url } => {
                let id = match target {
                    Target::NewTab => self.browser.new_tab(&url, now),
                    Target::Tab(id) => { self.browser.navigate(id, &url); id }
                    _ => return,
                };
                self.browser.set_title(id, title);
            }
            Suggestion::SplitTab { id, .. } => {
                if let Target::Split { anchor, placement } = target {
                    if self.browser.add_split(anchor, id, placement, now).is_err() { return; }
                } else { return; }
            }
            Suggestion::SwitchToTab { id, .. } => {
                if self.browser.is_favorite(id) {
                    self.browser.activate_tab(id, now);
                } else if let Some(index) = self.browser.spaces.iter().position(|space| space.pinned.iter().chain(&space.today).any(|tab| tab.id == id)) {
                    self.browser.switch_space(index);
                    self.browser.activate_tab(id, now);
                    self.settle_pager(cx);
                }
            }
            Suggestion::Action { command, .. } => {
                self.run_command(command, cx);
                if self.command_bar.as_ref().is_some_and(|bar| bar.target != target) { return; }
            }
            Suggestion::ArchivedTab { id, .. } => {
                if self.browser.restore_archived(id, now).is_some() {
                    self.settle_pager(cx);
                    self.show_toast(format!("Tab restored to {}", self.browser.space().name), cx);
                }
            }
            Suggestion::MoreArchive { .. } => {
                if let Some(bar) = &mut self.command_bar { bar.archive_limit = bar.archive_limit.saturating_add(8); }
                cx.notify();
                return;
            }
            Suggestion::ConfirmClearArchive => {
                self.browser.archive.clear();
                self.show_toast("Archive cleared".into(), cx);
            }
            Suggestion::ArchiveAfter { seconds, .. } => {
                self.browser.archive_after = seconds.map(Duration::from_secs);
            }
            Suggestion::SaveFolderName { name } => {
                if !self.save_folder_name(target, &name, cx) { return; }
            }
            Suggestion::FolderDestination { space, parent, .. } => {
                if !self.confirm_folder_destination(target, space, parent, cx) { return; }
            }
            Suggestion::SaveIcon { icon } => {
                match target {
                    Target::Icon(crate::icons::IconTarget::Folder(id)) => { self.browser.set_folder_icon(id, icon.as_deref()); }
                    Target::Icon(crate::icons::IconTarget::Tab(id)) => { self.browser.set_item_icon(id, icon.as_deref()); }
                    Target::Icon(crate::icons::IconTarget::Space(id)) => { self.browser.set_space_icon(id, icon.as_deref()); }
                    _ => return,
                }
            }
            Suggestion::SaveTabName { name } => {
                if let Target::RenameTab(id) = target { self.browser.rename_tab(id, &name); }
            }
            Suggestion::SaveSpace { name } => match target {
                Target::NewSpace => {
                    let hue = (self.browser.space().hue + 137.5).rem_euclid(360.0);
                    self.browser.add_space(name, hue);
                    self.switch_space(self.browser.spaces.len() - 1, cx);
                }
                Target::RenameSpace(id) => { self.browser.rename_space(id, &name); }
                _ => return,
            },
        }
        self.close_command_bar(cx);
    }

    pub(crate) fn run_command(&mut self, command: Command, cx: &mut Context<Self>) {
        match command {
            Command::NewFolder => self.edit_new_folder(None, None, cx),
            Command::NewFolderWithTab => { if let Some(id) = self.browser.space().active_tab { self.edit_new_folder(None, Some(id), cx); } }
            Command::PinToFolder(folder) => {
                if let (Some(tab), Some(space)) = (self.browser.space().active_tab, self.browser.folder_space(folder)) {
                    self.confirm_folder_destination(Target::MoveToFolder(tab), space, Some(folder), cx);
                }
            }
            Command::MoveToFolder => { if let Some(id) = self.browser.space().active_tab { self.edit_folder_destination(Target::MoveToFolder(id), cx); } }
            Command::CollapseAllFolders => { self.browser.set_all_folders_collapsed(self.browser.space().id, true); }
            Command::ExpandAllFolders => { self.browser.set_all_folders_collapsed(self.browser.space().id, false); }
            Command::RenameFolder(id) => self.edit_folder_name(id, cx),
            Command::ExpandFolder(id) => self.expand_folder(id, cx),
            Command::AddSplit(placement) => self.edit_split(placement, cx),
            Command::CloseSplitPane => { if let Some(id) = self.browser.space().active_tab { self.close_split_pane(id, cx); } }
            Command::SplitAxis(axis) => { if let Some(id) = self.browser.space().active_tab { self.browser.set_split_axis(id, axis); } }
            Command::SeparateSplit => { if let Some(id) = self.browser.space().active_tab { if !self.browser.separate_split(id) { self.show_toast("You can have up to 12 Favorites".into(), cx); } } }
            Command::SeparatePane => { if let Some(id) = self.browser.space().active_tab { if !self.browser.separate_split_pane(id) { self.show_toast("You can have up to 12 Favorites".into(), cx); } } }
            Command::FocusPane(index) => { self.browser.focus_split_pane(index, SystemTime::now()); }
            Command::CyclePane(delta) => { self.browser.cycle_split_pane(delta, SystemTime::now()); }
            Command::MovePane(delta) => { if let Some(id) = self.browser.space().active_tab { self.browser.move_split_pane(id, delta); } }
            Command::DuplicateTab => { if let Some(id) = self.browser.space().active_tab { self.duplicate_tab(id, cx); } }
            Command::RenameTab => {
                if let Some(tab) = self.browser.active_tab() {
                    let (id, name) = (tab.id, self.browser.item_title(tab.id));
                    if let Some(bar) = &mut self.command_bar {
                        bar.target = Target::RenameTab(id);
                        bar.selected = 0;
                        bar.input.update(cx, |input, cx| { input.set_placeholder("Rename Tab…", cx); input.set_text(name, cx); });
                    }
                }
            }
            Command::CustomizeTabIcon => { if let Some(id) = self.browser.space().active_tab { self.edit_icon(crate::icons::IconTarget::Tab(id), cx); } }
            Command::CustomizeSpaceIcon => self.edit_icon(crate::icons::IconTarget::Space(self.browser.space().id), cx),
            Command::ResetNameAndIcon => { if let Some(id) = self.browser.space().active_tab { self.browser.reset_item_customization(id); } }
            Command::ResetTabName => {
                if let Some(id) = self.browser.space().active_tab { self.browser.reset_tab_name(id); }
            }
            Command::ResetSpace => {
                let tabs = self.browser.reset_space();
                if let Some(pages) = &self.pages {
                    let mut pages = pages.borrow_mut();
                    for tab in &tabs { pages.discard(*tab); }
                }
                self.show_toast(format!("Reset {} pinned {}", tabs.len(), if tabs.len() == 1 { "tab" } else { "tabs" }), cx);
            }
            Command::ResetPinnedTab => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.browser.reset_pinned_tab(id) { self.show_toast("Tab reset to its pinned URL".into(), cx); }
                }
            }
            Command::ReplacePinnedUrl => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.browser.replace_pinned_url(id) { self.show_toast("Pinned URL updated".into(), cx); }
                }
            }
            Command::ViewHistory => self.history_from_command(cx),
            Command::ViewArchive => self.edit_archive_view(None, cx),
            Command::RequestClearArchive => self.confirm_clear_archive(cx),
            Command::CopyUrl => self.copy_url(false, cx),
            Command::CopyUrlAsMarkdown => self.copy_url(true, cx),
            Command::ArchiveSettings => {
                if let Some(bar) = &mut self.command_bar {
                    bar.target = Target::ArchiveInterval;
                    bar.selected = 0;
                    bar.input.update(cx, |input, cx| {
                        input.set_placeholder("Archive tabs after…", cx);
                        input.set_text("", cx);
                    });
                }
            }
            Command::FindPage => self.find_from_command(cx),
            Command::NewSpace => self.edit_space_name(None, cx),
            Command::RenameSpace => self.edit_space_name(Some(self.browser.space().id), cx),
            Command::ToggleSpaceHeader => { self.browser.set_space_header(self.browser.space().id, !self.browser.space().show_header); }
            Command::ToggleFavoritesOrdering => { self.browser.include_favorites_in_ordering = !self.browser.include_favorites_in_ordering; }
            Command::RevealTab => self.reveal_active_tab(cx),
            Command::ToggleSidebar => self.toggle_sidebar(cx),
            Command::TogglePin => {
                if let Some(id) = self.browser.space().active_tab { self.browser.toggle_pin(id); }
            }
            Command::ToggleFavorite => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.browser.is_favorite(id) { self.browser.unfavorite_tab(id); }
                    else if !self.browser.favorite_tab(id) { self.show_toast("You can have up to 12 Favorites".into(), cx); }
                }
            }
            Command::ToggleMute => {
                if let Some(tab) = self.browser.active_tab() { self.set_tab_muted(tab.id, !tab.muted, cx); }
            }
            Command::MuteAllTabs => self.set_all_tabs_muted(true, cx),
            Command::UnmuteAllTabs => self.set_all_tabs_muted(false, cx),
            Command::TogglePinnedSection => self.toggle_pinned_section(self.browser.active_space, cx),
            Command::CloseTab => {
                if let Some(id) = self.browser.space().active_tab { self.close_tab(id, cx); }
            }
            Command::ArchiveRelative(relative) => { if let Some(id) = self.browser.space().active_tab { self.archive_relative(id, relative, cx); } }
            Command::ClearToday => { self.browser.clear_today(SystemTime::now()); }
            Command::RestoreTab => {
                if self.browser.restore_last_archived(SystemTime::now()).is_some() { self.settle_pager(cx); }
            }
            Command::Reload => self.with_active_page(|pages, tab| pages.reload(tab)),
            Command::ForceReload => self.with_active_page(|pages, tab| pages.force_reload(tab)),
            Command::StopLoading => self.with_active_page(|pages, tab| pages.stop_loading(tab)),
            Command::ZoomIn => self.with_active_page(|pages, tab| pages.zoom(tab, 1)),
            Command::ZoomOut => self.with_active_page(|pages, tab| pages.zoom(tab, -1)),
            Command::ResetZoom => self.with_active_page(|pages, tab| pages.zoom(tab, 0)),
            Command::EnterFullscreen => self.perform_window_operation(crate::menus::WindowOperation::Fullscreen(true), cx),
            Command::ExitFullscreen => self.perform_window_operation(crate::menus::WindowOperation::Fullscreen(false), cx),
            Command::MinimizeWindow => self.perform_window_operation(crate::menus::WindowOperation::Minimize, cx),
            Command::ZoomWindow => self.perform_window_operation(crate::menus::WindowOperation::Zoom, cx),
            Command::GoBack => self.with_active_page(|pages, tab| pages.go_back(tab)),
            Command::GoForward => self.with_active_page(|pages, tab| pages.go_forward(tab)),
            Command::NextTab => self.cycle_tab(1, cx),
            Command::PreviousTab => self.cycle_tab(-1, cx),
            Command::MoveTab { space, section } => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.browser.move_tab(id, space, section) {
                        if let Some(index) = self.browser.spaces.iter().position(|candidate| candidate.id == space) {
                            self.switch_space(index, cx);
                            self.browser.activate_tab(id, SystemTime::now());
                            self.show_toast(format!("Moved tab to {}", self.browser.space().name), cx);
                        }
                    }
                }
            }
            Command::FocusSpace(id) => {
                if let Some(index) = self.browser.spaces.iter().position(|space| space.id == id) { self.switch_space(index, cx); }
            }
        }
        cx.notify();
    }

    /// Reuses the focused command input so the name editor keeps keyboard focus.
    fn edit_space_name(&mut self, space: Option<SpaceId>, cx: &mut Context<Self>) {
        let text = space.and_then(|id| self.browser.spaces.iter().find(|space| space.id == id)).map(|space| space.name.clone()).unwrap_or_default();
        if let Some(bar) = &mut self.command_bar {
            bar.target = space.map_or(Target::NewSpace, Target::RenameSpace);
            bar.selected = 0;
            bar.input.update(cx, |input, cx| {
                input.set_placeholder(if space.is_some() { "Rename Space…" } else { "Name your Space…" }, cx);
                input.set_text(text, cx);
            });
        }
        cx.notify();
    }

    fn render_overlay_shell(&self, content: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        div().size_full().key_context(WINDOW_CONTEXT)
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| this.open_command_action(Command::NewFolder, window, cx)))
            .on_action(cx.listener(|this, _: &CollapseAllFolders, _, cx| this.run_command(Command::CollapseAllFolders, cx)))
            .on_action(cx.listener(|this, _: &ExpandAllFolders, _, cx| this.run_command(Command::ExpandAllFolders, cx)))
            .on_action(cx.listener(|this, _: &CustomizeTabIcon, window, cx| this.open_command_action(Command::CustomizeTabIcon, window, cx)))
            .on_action(cx.listener(|this, _: &CustomizeSpaceIcon, window, cx| this.open_command_action(Command::CustomizeSpaceIcon, window, cx)))
            .on_action(cx.listener(|this, _: &ResetNameAndIcon, _, cx| this.run_command(Command::ResetNameAndIcon, cx)))
            .on_action(cx.listener(|this, _: &ToggleSpaceHeader, _, cx| this.run_command(Command::ToggleSpaceHeader, cx)))
            .on_action(cx.listener(|this, _: &ToggleFavoritesOrdering, _, cx| this.run_command(Command::ToggleFavoritesOrdering, cx)))
            .on_action(cx.listener(|this, _: &RevealTab, _, cx| this.reveal_active_tab(cx)))
            .on_action(cx.listener(|this, _: &DuplicateTab, _, cx| this.run_command(Command::DuplicateTab, cx)))
            .on_action(cx.listener(|this, _: &AddSplit, window, cx| this.open_command_action(Command::AddSplit(None), window, cx)))
            .on_action(cx.listener(|this, _: &CloseSplitPane, _, cx| this.run_command(Command::CloseSplitPane, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::Minimize, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Minimize, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ZoomWindow, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Zoom, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ToggleFullscreen, _, cx| this.perform_window_operation(crate::menus::WindowOperation::ToggleFullscreen, cx)))
            .child(content).into_any_element()
    }

    pub(crate) fn open_command_action(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.open_command_bar(Target::NewTab, "", window, cx);
        self.run_command(command, cx);
    }

    // Window actions.

    fn on_open_command_bar(&mut self, _: &OpenCommandBar, window: &mut Window, cx: &mut Context<Self>) {
        if !self.toggle_command_target(Target::NewTab, "", cx) {
            self.open_command_bar(Target::NewTab, "", window, cx);
        }
    }

    fn on_edit_current_url(&mut self, _: &EditCurrentUrl, window: &mut Window, cx: &mut Context<Self>) {
        let (target, text) = self.browser.active_tab().map(|tab| (Target::Tab(tab.id), tab.url.clone()))
            .unwrap_or((Target::NewTab, String::new()));
        if !self.toggle_command_target(target, &text, cx) {
            self.open_command_bar(target, &text, window, cx);
        }
    }

    fn on_close_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.browser.space().active_tab {
            self.close_tab(id, cx);
        }
    }

    /// Archives a Today tab or unloads a pinned one, as in Arc.
    pub(crate) fn close_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        let related = self.browser.related_tabs(id);
        if self.browser.close_tab(id, SystemTime::now()).is_some() {
            if let Some(pages) = &self.pages {
                let mut pages = pages.borrow_mut();
                for id in related { pages.discard(id); }
                pages.sync(&self.browser, true);
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

    fn toggle_pinned_section(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(space) = self.browser.spaces.get_mut(index) {
            space.pinned_collapsed = !space.pinned_collapsed;
            cx.notify();
        }
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

    pub(crate) fn scroll_to_pinned_item(&mut self, item: PinnedItem, cx: &mut Context<Self>) {
        let space = self.browser.space().id;
        if let Some(index) = self.browser.pinned_rows(space, false).iter().position(|row| row.item == item) {
            self.sidebar_scrolls.entry(space).or_default().pending.set(Some((Section::Pinned, index)));
        }
        cx.notify();
    }

    pub(crate) fn scroll_to_today_start(&mut self, cx: &mut Context<Self>) {
        self.sidebar_scrolls.entry(self.browser.space().id).or_default().pending.set(Some((Section::Today, 0)));
        cx.notify();
    }

    fn reveal_active_tab(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.browser.space().active_tab else { return; };
        self.close_command_bar(cx);
        if !self.sidebar_visible { self.toggle_sidebar(cx); }
        self.close_floating_sidebar(cx);
        for folder in self.browser.folder_ancestors(active) { self.browser.set_folder_collapsed(folder, false); }
        let anchor = self.browser.item_anchor(active);
        let space = self.browser.space();
        let section = if space.pinned.iter().any(|tab| tab.id == anchor) { Some(Section::Pinned) }
            else if space.today.iter().any(|tab| tab.id == anchor) { Some(Section::Today) } else { None };
        if let Some(section) = section {
            let tabs = if section == Section::Pinned { &space.pinned } else { &space.today };
            let index = if section == Section::Pinned { self.browser.pinned_rows(space.id, false).iter().position(|row| self.browser.pinned_item_tab(space.id, row.item) == Some(anchor)) }
                else { tabs.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).position(|tab| tab.id == anchor) };
            let scrolls = self.sidebar_scrolls.entry(space.id).or_default();
            if let Some(index) = index { scrolls.pending.set(Some((section, index))); }
            if section == Section::Pinned { self.browser.spaces[self.browser.active_space].pinned_collapsed = false; }
        }
        self.settle_pager(cx);
        cx.notify();
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
                drag.samples.push((cx.background_executor().now(), dx));
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
                let speed = drag.speed(cx.background_executor().now());
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

    pub(crate) fn switch_space(&mut self, index: usize, cx: &mut Context<Self>) {
        self.browser.switch_space(index);
        self.settle_pager(cx);
        cx.notify();
    }

    fn reorder_space(&mut self, id: SpaceId, destination: usize, cx: &mut Context<Self>) {
        if self.browser.move_space(id, destination) {
            // The selected Space is the same object at a new index. A pager
            // animation here would briefly display a different Space's tabs.
            self.pager.settle = None;
            self.pager.drag = None;
            self.pager.position = self.browser.active_space as f32;
            cx.notify();
        }
    }

    fn cycle_space(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.browser.spaces.len() as isize;
        let index = (self.browser.active_space as isize + delta).rem_euclid(n);
        self.switch_space(index as usize, cx);
    }

    // Rendering.

    pub(crate) fn render_tab(
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
            .on_drag(self.tab_drag(tab), |drag, _, _, cx| cx.new(|_| drag.clone()))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .h(px(34.0))
            .px_2()
            .rounded_lg()
            .when(active, |this| this.bg(hsla(0.0, 0.0, 1.0, 0.85)).shadow_sm())
            .when(!active, |this| this.hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3))))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_tab_menu(id, event.position, window, cx);
            }))
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                this.browser.activate_sidebar_item(id, SystemTime::now());
                if event.click_count() == 2 {
                    let title = Some(this.browser.item_title(id));
                    if let Some(title) = title { this.open_command_bar(Target::RenameTab(id), &title, window, cx); }
                }
                cx.notify();
            }))
            .child(self.render_item_icon(tab, hue))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_sm()
                    .child(self.browser.item_title(id)),
            )
            .when(tab.muted, |this| this.child(self.render_mute_control(tab, false, cx)))
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

    fn render_favorite(&self, tab: &Tab, cx: &mut Context<Self>) -> impl IntoElement {
        let id = tab.id;
        let key = id.get() as usize;
        let group = SharedString::from(format!("favorite-{key}"));
        let active = self.browser.space().active_tab.is_some_and(|active| self.browser.item_anchor(active) == id);
        let title = self.browser.item_title(id);
        let url = tab.url.clone();
        let browser = cx.entity().downgrade();
        div().id(("favorite", key)).debug_selector(move || format!("favorite-{key}"))
            .group(group.clone()).relative().flex().items_center().justify_center()
            .on_drag(self.tab_drag(tab), |drag, _, _, cx| cx.new(|_| drag.clone()))
            .can_drop(move |value, _, cx| value.downcast_ref::<crate::tab_drag::TabDrag>().is_some_and(|drag| {
                browser.read_with(cx, |this, _| drag.id != id && (this.browser.is_favorite(drag.id) || this.browser.favorite_count() < browser_core::favorites::MAX_FAVORITES)).unwrap_or(false)
            }))
            .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.9)))
            .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_at_favorite(drag.id, id, cx)))
            .w(px(68.0)).h(px(48.0)).rounded_lg()
            .bg(hsla(0.0, 0.0, 1.0, if active { 0.85 } else { 0.3 }))
            .when(active, |this| this.shadow_sm())
            .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.65)))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_tab_menu(id, event.position, window, cx);
            }))
            .tooltip(move |_, cx| cx.new(|_| TabTooltip { title: title.clone(), url: display_url(&url) }).into())
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                this.browser.activate_sidebar_item(id, SystemTime::now());
                if event.click_count() == 2 {
                    let title = this.browser.item_title(id);
                    this.open_command_bar(Target::RenameTab(id), &title, window, cx);
                }
                cx.notify();
            }))
            .child(self.render_item_icon(tab, self.accent()))
            .when(tab.muted, |this| this.child(self.render_mute_control(tab, true, cx)))
            .child(div().id(("close-favorite", key)).debug_selector(move || format!("close-favorite-{key}"))
                .absolute().right(px(2.0)).top(px(1.0)).px_1().rounded_sm().text_xs()
                .invisible().group_hover(group, |s| s.visible())
                .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.1)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.close_tab(id, cx);
                })).child("−"))
    }

    fn render_mute_control(&self, tab: &Tab, favorite: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let id = tab.id;
        let key = id.get() as usize;
        let title = tab.title.clone();
        div().id(("unmute", key)).debug_selector(move || format!("unmute-{key}"))
            .flex_none().px_1().rounded_sm().text_sm()
            .when(favorite, |this| this.absolute().bottom(px(1.)).left(px(1.)))
            .hover(|s| s.bg(hsla(0., 0., 0., 0.1)))
            .tooltip(move |_, cx| cx.new(|_| TabTooltip { title: "Unmute Tab".into(), url: title.clone() }).into())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.set_tab_muted(id, false, cx);
            })).child("🔇")
    }

    pub(crate) fn render_favicon(&self, tab: &Tab, hue: f32) -> AnyElement {
        if let Some(icon) = &tab.custom_icon { return crate::icons::glyph(icon); }
        match self.browser.favicon(&tab.url) {
            Some(png) => {
                let title = tab.title.clone();
                gpui::img(Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png.to_vec())))
                    .size(px(18.0)).flex_none().object_fit(gpui::ObjectFit::Contain)
                    .with_fallback(move || favicon(&title, hue).into_any_element())
                    .into_any_element()
            }
            None => favicon(&tab.title, hue).into_any_element(),
        }
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
        let browser = cx.entity().downgrade();

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
            .when(!self.browser.favorites.is_empty(), |this| this.child(
                div().id("favorites").debug_selector(|| "favorites".into())
                    .flex().flex_wrap().gap_2().px_2().pb_2()
                    .rounded_lg().drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.3)))
                    .can_drop(move |value, _, cx| value.downcast_ref::<crate::tab_drag::TabDrag>().is_some_and(|drag| {
                        browser.read_with(cx, |this, _| !this.browser.is_favorite(drag.id) && this.browser.favorite_count() < browser_core::favorites::MAX_FAVORITES).unwrap_or(false)
                    }))
                    .on_drop(cx.listener(|this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_in_favorites(drag.id, cx)))
                    .children(self.browser.favorites.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).map(|tab| self.render_favorite(tab, cx))),
            ))
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

    fn sidebar_list(&self, space: SpaceId, section: Section, cx: &mut Context<Self>) -> gpui::Div {
        let Some(scrolls) = self.sidebar_scrolls.get(&space) else { return div(); };
        let scroll = match section { Section::Pinned => scrolls.pinned.clone(), Section::Today => scrolls.today.clone() };
        let pending = scrolls.pending.clone();
        let browser = cx.entity().downgrade();
        div().on_children_prepainted(move |_, _, cx| {
            if let Some((_, index)) = pending.get().filter(|(kind, _)| *kind == section) {
                pending.set(None);
                let scroll = scroll.clone();
                let browser = browser.clone();
                // The section now has child bounds, including after unfolding it.
                cx.defer(move |cx| {
                    scroll.scroll_to_item(index);
                    let _ = browser.update(cx, |_, cx| cx.notify());
                });
            }
        })
    }

    fn render_space_page(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let space = &self.browser.spaces[index];
        let space_id = space.id;
        let hue = space.hue / 360.0;
        let current = index == self.browser.active_space;
        let tabs = |tabs: &[Tab], section, cx: &mut Context<Self>| -> Vec<AnyElement> {
            tabs.iter().filter(|tab| !self.browser.is_split_secondary(tab.id))
                .map(|tab| {
                    let active = space.active_tab.is_some_and(|id| self.browser.item_anchor(id) == tab.id);
                    self.render_tab(tab, section, active, hue, cx).into_any_element()
                })
                .collect()
        };
        let pinned = self.render_folder_rows(space.id, hue, cx);
        let today = tabs(&space.today, Section::Today, cx);

        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .px_2()
            .gap_1()
            .when(space.show_header, |this| this.child(
                div().id(("space-header", index)).debug_selector(move || format!("space-header-{index}"))
                    .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag.id, space_id, Section::Pinned, cx)))
                    .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_space_menu(space_id, event.position, window, cx);
                    }))
                    .px_2()
                    .pb_1()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .child(space.name.clone()),
            ))
            .when(!space.pinned_items.is_empty(), |this| {
                this.child(div()
                    .id(("pinned-section", index))
                    .when(current, |this| this.debug_selector(|| "pinned-section".into()))
                    .flex().items_center().gap_2().h(px(24.0)).px_2().rounded_md()
                    .text_xs().text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.2)))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_pinned_section(index, cx)))
                    .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag.id, space_id, Section::Pinned, cx)))
                    .drag_over::<crate::folder_drag::FolderDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::folder_drag::FolderDrag, _, cx| { this.confirm_folder_destination(Target::MoveFolder(drag.id), space_id, None, cx); }))
                    .child(if space.pinned_collapsed { "›" } else { "⌄" })
                    .child("Pinned"))
            })
            .when(!space.pinned_collapsed && !pinned.is_empty(), |this| this.child(
                self.sidebar_list(space.id, Section::Pinned, cx).id(("pinned-list", index)).when(current, |this| this.debug_selector(|| "pinned-list".into()))
                    .flex().flex_col().flex_none().max_h(gpui::relative(0.45)).overflow_y_scroll()
                    .when_some(self.sidebar_scrolls.get(&space.id), |this, scrolls| this.track_scroll(&scrolls.pinned))
                    .children(pinned),
            ))
            .child(div().h(px(1.0)).mx_2().my_2().bg(hsla(0.0, 0.0, 0.0, 0.12)))
            .when(!space.today.is_empty(), |this| {
                this.child(div().id(("today-header", index)).when(current, |this| this.debug_selector(|| "today-header".into())).flex().items_center().justify_between().px_2().h(px(24.0))
                    .rounded_md().drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag.id, space_id, Section::Today, cx)))
                    .text_xs().text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .child("Today")
                    .child(div().id(("clear-today", index))
                        .when(current, |this| this.debug_selector(|| "clear-today".into()))
                        .px_2().py_1().rounded_md()
                        .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.browser.switch_space(index);
                            this.browser.clear_today(SystemTime::now());
                            this.settle_pager(cx);
                            cx.notify();
                        }))
                        .child("Clear")))
            })
            .child(
                div()
                    .id(("new-tab", index))
                    .when(current, |this| this.debug_selector(|| "new-tab".into()))
                    .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag.id, space_id, Section::Today, cx)))
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
                self.sidebar_list(space.id, Section::Today, cx)
                    .id(("today", index))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .overflow_y_scroll()
                    .when(current, |this| this.debug_selector(|| "today-list".into()))
                    .when_some(self.sidebar_scrolls.get(&space.id), |this, scrolls| this.track_scroll(&scrolls.today))
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
                let space_id = space.id;
                let hue = space.hue / 360.0;
                let drag = crate::space_drag::SpaceDrag { id: space_id, name: space.name.clone(), icon: space.custom_icon.clone(), hue: space.hue };
                div()
                    .id(("space", index))
                    .debug_selector(move || format!("space-{index}"))
                    .flex_none().size(px(if space.custom_icon.is_some() { 22. } else if active { 10. } else { 8. }))
                    .when(space.custom_icon.is_none(), |this| this.rounded_full().bg(hsla(hue, 0.55, 0.5, if active { 1.0 } else { 0.45 })).hover(move |s| s.bg(hsla(hue, 0.55, 0.5, 0.8))))
                    .when_some(space.custom_icon.clone(), |this, icon| this.flex().items_center().justify_center().rounded_md()
                        .when(active, |this| this.bg(hsla(0., 0., 1., 0.5))).hover(|this| this.bg(hsla(0., 0., 1., 0.3)))
                        .child(div().debug_selector(move || format!("space-icon-{index}")).child(crate::icons::glyph(&icon))))
                    .on_click(cx.listener(move |this, _, _, cx| this.switch_space(index, cx)))
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .drag_over::<crate::space_drag::SpaceDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.8)))
                    .on_drop(cx.listener(move |this, drag: &crate::space_drag::SpaceDrag, _, cx| this.reorder_space(drag.id, index, cx)))
                    .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_space_menu(space_id, event.position, window, cx);
                    }))
            }))
            .child(div().id("new-space").debug_selector(|| "new-space".into())
                .w(px(20.0)).h(px(20.0)).mt(px(-6.0)).rounded_md()
                .flex().items_center().justify_center().text_sm()
                .text_color(hsla(0.0, 0.0, 0.0, 0.55))
                .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)))
                .on_click(cx.listener(|this, _, window, cx| this.open_command_bar(Target::NewSpace, "", window, cx)))
                .child("+"))
    }

    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let split_open = self.browser.active_split().is_some();
        let history_open = self.history_is_open();
        let internal = self.browser.active_tab().is_some_and(|tab| omnibox::is_internal_url(&tab.url));
        let native_view = self.native_view;
        let mouse_tracking = self.mouse_tracking.clone();
        // The page view covers the card; text only shows when there is no page.
        let placeholder = if split_open || history_open { None } else if internal { Some(("Page not found".into(), self.browser.active_tab().unwrap().url.clone())) } else { match (self.browser.active_tab(), &self.pages) {
            (None, _) => Some(("No tab open".to_string(), "Press ⌘T to open a new tab".to_string())),
            (Some(tab), None) => Some((tab.title.clone(), tab.url.clone())),
            (Some(_), Some(_)) => None,
        }};
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
                    .when(!split_open, |this| this.px_8())
                    .overflow_hidden()
                    .rounded(px(CONTENT_CORNER_RADIUS))
                    .bg(gpui::white())
                    .shadow_md()
                    .relative()
                    .when(split_open, |this| this.child(self.render_split(cx)))
                    .when(history_open, |this| this.child(self.render_history(cx)))
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
                                let surfaces = match pages.try_borrow_mut() {
                                    Ok(mut pages) => { pages.set_bounds(bounds); Some(pages.surface_bounds()) }
                                    Err(_) => None,
                                };
                                let tracking = mouse_tracking.clone();
                                let update_tracking = move |surfaces: Vec<engine::Bounds>| {
                                    if let Some(view) = native_view {
                                        let holes: Vec<_> = surfaces.into_iter().map(|rect| [rect.x, rect.y, rect.width, rect.height]).collect();
                                        let mut tracking = tracking.borrow_mut();
                                        if tracking.holes.as_ref() != Some(&holes) {
                                            tracking.holes = Some(holes.clone());
                                            crate::native::track_mouse_outside(view, &holes, &mut tracking.areas);
                                        }
                                    }
                                };
                                if let Some(surfaces) = surfaces { update_tracking(surfaces); }
                                else {
                                    let pages = pages.clone();
                                    cx.defer(move |_| {
                                        let surfaces = { let mut pages = pages.borrow_mut(); pages.set_bounds(bounds); pages.surface_bounds() };
                                        update_tracking(surfaces);
                                    });
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

    fn render_command_bar_overlay(&self, height: f32, cx: &mut Context<Self>) -> AnyElement {
        match &self.command_bar {
            Some(bar) => self.render_command_bar(bar, height, cx).into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn render_command_bar(&self, bar: &CommandBar, height: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let accent = self.accent();
        let top = 140.0_f32.min(height * 0.2);
        let archive = matches!(bar.target, Target::Archive(_));
        let rows_height = (height - top - if archive { 150. } else { 90. } - 16.).max(40.);
        let mut previous_date = None;
        let rows: Vec<_> = self
            .suggestions(cx)
            .into_iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let selected = index == bar.selected;
                let date = match &suggestion {
                    Suggestion::ArchivedTab { date, .. } if previous_date.as_ref() != Some(date) => { previous_date = Some(date.clone()); Some(date.clone()) }
                    _ => None,
                };
                let archived_id = match &suggestion { Suggestion::ArchivedTab { id, .. } => Some(*id), _ => None };
                let history_url = match &suggestion { Suggestion::History { url, .. } => Some(url.clone()), _ => None };
                let (icon, label, detail): (&str, String, String) = match suggestion {
                    Suggestion::Load(Resolved::Search { query, .. }) => {
                        ("⌕", query, "Google Search".into())
                    }
                    Suggestion::Load(Resolved::Url(url)) => ("↗", display_url(&url), "Open".into()),
                    Suggestion::SplitTab { title, space, .. } => ("▥", title, format!("Add to Split View · {space}")),
                    Suggestion::History { title, url } => ("◷", title, format!("History · {}", display_url(&url))),
                    Suggestion::SwitchToTab { title, space, .. } => {
                        ("⇥", title, format!("Switch to Tab · {space}"))
                    }
                    Suggestion::Action { label, .. } => ("⌘", label, "Action".into()),
                    Suggestion::SaveFolderName { name } => ("▸", name, if matches!(bar.target, Target::RenameFolder(_)) { "Rename Folder".into() } else { "Create Folder".into() }),
                    Suggestion::FolderDestination { label, .. } => ("▸", label, "Move".into()),
                    Suggestion::SaveIcon { icon } => ("↵", icon.unwrap_or_else(|| "Reset Icon".into()), match bar.target { Target::Icon(crate::icons::IconTarget::Space(_)) => "Space Icon".into(), Target::Icon(crate::icons::IconTarget::Folder(_)) => "Folder Icon".into(), _ => "Tab Icon".into() }),
                    Suggestion::SaveTabName { name } => ("↵", name, "Rename Tab".into()),
                    Suggestion::SaveSpace { name } => ("+", name, if bar.target == Target::NewSpace { "Create Space".into() } else { "Rename Space".into() }),
                    Suggestion::ArchiveAfter { label, seconds } => ("◷", label, if self.browser.archive_after.map(|duration| duration.as_secs()) == seconds { "Current".into() } else { "Archive tabs after".into() }),
                    Suggestion::ArchivedTab { title, detail, .. } => ("↩", title, detail),
                    Suggestion::MoreArchive { remaining } => ("↓", "Show More".into(), format!("{remaining} more archived {}", if remaining == 1 { "tab" } else { "tabs" })),
                    Suggestion::ConfirmClearArchive => ("×", "Clear Archive".into(), "Permanently delete".into()),
                };
                let row = div()
                    .id(("suggestion", index))
                    .debug_selector(move || format!("suggestion-{index}"))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(40.0)).flex_none()
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
                            .max_w(px(260.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .when(!selected, |this| this.text_color(hsla(0.0, 0.0, 0.0, 0.45)))
                            .child(detail),
                    )
                    .when_some(archived_id, |this, id| this.child(div()
                        .id(("delete-archived", id.get() as usize))
                        .debug_selector(move || format!("delete-archived-{}", id.get()))
                        .px_2().rounded_md().hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.12)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_archive_entry(id, cx);
                        })).child("×")))
                    .when_some(history_url, |this, url| this.child(div()
                        .id(("delete-suggestion", index))
                        .debug_selector(move || format!("delete-suggestion-{index}"))
                        .px_2().rounded_md().hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.12)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.browser.delete_history_entry(&url);
                            let count = this.suggestions(cx).len();
                            if let Some(bar) = &mut this.command_bar {
                                bar.selected = bar.selected.min(count.saturating_sub(1));
                            }
                            cx.notify();
                        })).child("×")));
                div().flex().flex_col().flex_none()
                    .children(date.map(|date| div().debug_selector(move || format!("archive-date-{index}")).h(px(24.)).flex_none().px_4().text_xs().text_color(hsla(0., 0., 0., 0.5)).child(date)))
                    .child(row).into_any_element()
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
            .pt(px(top))
            .bg(hsla(0.0, 0.0, 0.0, 0.15))
            .on_click(cx.listener(|this, _, _, cx| this.close_command_bar(cx)))
            .child(
                div()
                    .id("command-bar")
                    .debug_selector(|| "command-bar".into())
                    .key_context(COMMAND_BAR_CONTEXT)
                    .on_action(cx.listener(|this, _: &OpenCommandBar, _, cx| { this.toggle_command_target(Target::NewTab, "", cx); }))
                    .on_action(cx.listener(|this, _: &EditCurrentUrl, _, cx| {
                        let (target, text) = this.browser.active_tab().map(|tab| (Target::Tab(tab.id), tab.url.clone())).unwrap_or((Target::NewTab, String::new()));
                        this.toggle_command_target(target, &text, cx);
                    }))
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
                    .when(matches!(bar.target, Target::Icon(_)), |this| this.child(div().px_4().pb_2().text_xs().child("Enter one symbol or emoji. Leave blank to restore the default icon.")))
                    .when(matches!(bar.target, Target::Archive(_)), |this| this.child(self.render_archive_controls(cx)))
                    .when(bar.target == Target::ConfirmClearArchive, |this| this.child(div().px_4().pb_2().text_sm().child("This action is permanent. Archived tabs from all Spaces will be deleted.")))
                    .when(!has_rows && matches!(bar.target, Target::Archive(_)), |this| this.child(div().px_4().py_3().text_sm().child("No archived tabs match your search.")))
                    .when(has_rows, |this| {
                        this.child(div().h(px(1.0)).mb_2().bg(hsla(0.0, 0.0, 0.0, 0.08)))
                            .text_sm()
                            .child(div().id("command-suggestions").debug_selector(|| "command-suggestions".into())
                                .flex().flex_col().max_h(px(rows_height)).overflow_y_scroll().track_scroll(&bar.rows_scroll).children(rows))
                    }),
            )
    }
}

impl Render for BrowserWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for space in &self.browser.spaces { self.sidebar_scrolls.entry(space.id).or_default(); }
        self.sync_history_focus(window, cx);
        if self.tab_menu.as_ref().is_some_and(|menu| !self.menu_target_exists(menu.target)) {
            self.close_tab_menu(cx);
        }
        if self.find_bar.as_ref().is_some_and(|bar| Some(bar.tab) != self.browser.space().active_tab) {
            self.close_find(cx);
        }
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
            .on_action(cx.listener(|this, _: &crate::history::ViewHistory, window, cx| this.open_history(window, cx)))
            .on_action(cx.listener(|this, _: &AddSplit, window, cx| this.open_command_action(Command::AddSplit(None), window, cx)))
            .on_action(cx.listener(|this, _: &CloseSplitPane, _, cx| this.run_command(Command::CloseSplitPane, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::Minimize, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Minimize, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ZoomWindow, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Zoom, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ToggleFullscreen, _, cx| this.perform_window_operation(crate::menus::WindowOperation::ToggleFullscreen, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::RenameSpace, window, cx| this.open_command_action(Command::RenameSpace, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::RenameTab, window, cx| this.open_command_action(Command::RenameTab, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ViewArchive, window, cx| this.open_command_action(Command::ViewArchive, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ClearArchive, window, cx| this.open_command_action(Command::RequestClearArchive, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ArchiveSettings, window, cx| this.open_command_action(Command::ArchiveSettings, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ResetTabName, _, cx| this.run_command(Command::ResetTabName, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ResetPinnedTab, _, cx| this.run_command(Command::ResetPinnedTab, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ReplacePinnedUrl, _, cx| this.run_command(Command::ReplacePinnedUrl, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ResetSpace, _, cx| this.run_command(Command::ResetSpace, cx)))
            .on_action(cx.listener(|this, _: &NewSpace, window, cx| this.open_command_bar(Target::NewSpace, "", window, cx)))
            .on_action(cx.listener(Self::on_edit_current_url))
            .on_action(cx.listener(|this, _: &CopyUrl, _, cx| this.copy_url(false, cx)))
            .on_action(cx.listener(|this, _: &CopyUrlAsMarkdown, _, cx| this.copy_url(true, cx)))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| this.cycle_tab(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| this.cycle_tab(-1, cx)))
            .on_action(cx.listener(Self::on_go_back))
            .on_action(cx.listener(Self::on_go_forward))
            .on_action(cx.listener(|this, _: &crate::find::FindPage, window, cx| this.open_find(window, cx)))
            .on_action(cx.listener(|this, _: &crate::find::NextMatch, _, cx| this.find_next(true, cx)))
            .on_action(cx.listener(|this, _: &crate::find::PreviousMatch, _, cx| this.find_next(false, cx)))
            .on_action(cx.listener(Self::on_reload))
            .on_action(cx.listener(|this, _: &ForceReload, _, _| this.with_active_page(|pages, tab| pages.force_reload(tab))))
            .on_action(cx.listener(|this, _: &StopLoading, _, _| this.with_active_page(|pages, tab| pages.stop_loading(tab))))
            .on_action(cx.listener(|this, _: &ZoomIn, _, _| this.with_active_page(|pages, tab| pages.zoom(tab, 1))))
            .on_action(cx.listener(|this, _: &ZoomOut, _, _| this.with_active_page(|pages, tab| pages.zoom(tab, -1))))
            .on_action(cx.listener(|this, _: &ResetZoom, _, _| this.with_active_page(|pages, tab| pages.zoom(tab, 0))))
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| this.open_command_action(Command::NewFolder, window, cx)))
            .on_action(cx.listener(|this, _: &CollapseAllFolders, _, cx| this.run_command(Command::CollapseAllFolders, cx)))
            .on_action(cx.listener(|this, _: &ExpandAllFolders, _, cx| this.run_command(Command::ExpandAllFolders, cx)))
            .on_action(cx.listener(|this, _: &CustomizeTabIcon, window, cx| this.open_command_action(Command::CustomizeTabIcon, window, cx)))
            .on_action(cx.listener(|this, _: &CustomizeSpaceIcon, window, cx| this.open_command_action(Command::CustomizeSpaceIcon, window, cx)))
            .on_action(cx.listener(|this, _: &ResetNameAndIcon, _, cx| this.run_command(Command::ResetNameAndIcon, cx)))
            .on_action(cx.listener(|this, _: &ToggleSpaceHeader, _, cx| this.run_command(Command::ToggleSpaceHeader, cx)))
            .on_action(cx.listener(|this, _: &ToggleFavoritesOrdering, _, cx| this.run_command(Command::ToggleFavoritesOrdering, cx)))
            .on_action(cx.listener(|this, _: &RevealTab, _, cx| this.reveal_active_tab(cx)))
            .on_action(cx.listener(|this, _: &DuplicateTab, _, cx| this.run_command(Command::DuplicateTab, cx)))
            .on_action(cx.listener(Self::on_toggle_pin))
            .on_action(cx.listener(|this, _: &ToggleFavorite, _, cx| this.run_command(Command::ToggleFavorite, cx)))
            .on_action(cx.listener(|this, _: &ToggleMute, _, cx| this.run_command(Command::ToggleMute, cx)))
            .on_action(cx.listener(|this, _: &MuteAllTabs, _, cx| this.run_command(Command::MuteAllTabs, cx)))
            .on_action(cx.listener(|this, _: &UnmuteAllTabs, _, cx| this.run_command(Command::UnmuteAllTabs, cx)))
            .on_action(cx.listener(|this, _: &TogglePinnedSection, _, cx| this.toggle_pinned_section(this.browser.active_space, cx)))
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
            .child(self.render_content(cx))

    }
}

struct TabTooltip { title: String, url: String }

impl Render for TabTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_1().p_2().rounded_md().bg(gpui::white()).shadow_md()
            .text_sm().text_color(gpui::black()).max_w(px(320.0))
            .child(self.title.clone())
            .child(div().text_xs().text_color(hsla(0.0, 0.0, 0.0, 0.5)).child(self.url.clone()))
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
