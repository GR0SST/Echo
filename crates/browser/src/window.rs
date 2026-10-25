//! Arc-style browser window: vertical sidebar, content card, command bar.
//! Native CEF pages sit below browser-owned GPUI controls.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use browser_core::omnibox::{self, Resolved};
use browser_core::{Browser, Section, SpaceId, Tab, TabId};
use browser_core::folders::{FolderId, PinnedItem};
use engine::Event;
use gpui::{
    actions, canvas, div, ease_out_quint, point, size, Animation, AnimationExt, AnyElement, Bounds,
    MouseMoveEvent, TitlebarOptions, AnyWindowHandle, Task, WeakEntity, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, hsla, prelude::*, px, App, Context, Entity,
    FocusHandle, Focusable, Hsla, KeyBinding, Keystroke, Modifiers, ScrollDelta, ScrollWheelEvent,
    SharedString, Subscription, TouchPhase, Window,
};

use crate::pages::{Pages, WebEngine, CONTENT_CORNER_RADIUS, CONTENT_INSET, WINDOW_CORNER_RADIUS};
use crate::text_input::{self, TextInput};

#[path = "window_close.rs"]
pub(crate) mod window_close;
#[path = "developer_mode.rs"]
mod developer_mode;
#[path = "download_probe.rs"]
pub(crate) mod download_probe;

pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;
/// Sidebar metrics measured from the reference at 1x.
const SIDEBAR_INSET: f32 = 8.0;
const SIDEBAR_TOOLBAR_HEIGHT: f32 = 47.0;
pub(crate) const TOP_TOOLBAR_HEIGHT: f32 = 33.;
pub(crate) const SIDEBAR_ROW_HEIGHT: f32 = 38.0;
const SIDEBAR_TAB_HEIGHT: f32 = 36.0;
pub(crate) const SIDEBAR_ROW_GAP: f32 = 3.0;
pub(crate) const SIDEBAR_ROW_RADIUS: f32 = 10.0;
const FAVORITE_HEIGHT: f32 = 46.0;
const FAVORITE_GAP: f32 = 8.0;
const FAVORITE_COLUMNS: usize = 4;
/// Room for "Tidy | Clear" at the right of the Today divider.
const TODAY_ACTIONS_WIDTH: f32 = 96.0;
const SPACE_BAR_HEIGHT: f32 = 50.0;
const COMMAND_BAR_WIDTH: f32 = 764.0;
/// The search field's band, separator included.
const COMMAND_BAR_HEADER: f32 = 64.0;
const COMMAND_BAR_RADIUS: f32 = 12.0;
const MAX_TAB_SUGGESTIONS: usize = 6;
const MAX_HISTORY_SUGGESTIONS: usize = 4;
const MAX_COMMAND_SUGGESTIONS: usize = 12;
const ENGINE_PUMP_INTERVAL: Duration = Duration::from_millis(8);
/// Fraction of the sidebar width a swipe must travel to change space.
const SPACE_SWIPE_COMMIT: f32 = 0.5;
/// Arc moves 60 logical pixels for 120 precise scroll units in native captures.
const SPACE_SCROLL_GAIN: f32 = 0.5;
/// Finger speed (px/s) that changes space regardless of distance (a flick).
const SPACE_FLICK_SPEED: f32 = 300.0;
/// How much the pager follows the fingers past the first and last space.
const PAGER_RUBBER_BAND: f32 = 0.3;
const PAGER_FRAME: Duration = Duration::from_millis(16);
/// Pointer distance (px) from the window's left edge that reveals a hidden sidebar.
const SIDEBAR_REVEAL_EDGE: f32 = 8.0;
/// Logical pixels beyond the revealed panel; this does not intercept page input.
const SIDEBAR_DISMISS_BUFFER: f32 = 100.0;
const FLOATING_SIDEBAR_INSET: f32 = 6.0;
/// Room for the floating sidebar's shadow inside its window.
const FLOATING_SIDEBAR_SHADOW: f32 = 18.0;
const FLOATING_SIDEBAR_SLIDE: Duration = Duration::from_millis(180);
/// How long a revealed sidebar waits for the pointer before hiding.
const FLOATING_SIDEBAR_GRACE: Duration = Duration::from_millis(800);
pub const TRAFFIC_LIGHT_X: f32 = 12.5;
pub const TRAFFIC_LIGHT_Y: f32 = 17.0;
/// Critical spring fitted to the installed Arc's keyboard slide (24.7/s).
/// Evaluate in elapsed time so missed frames do not slow the animation down.
const PAGER_SPRING_FREQUENCY: f32 = 24.7;
/// The short-drag return in the same capture settles faster than key navigation.
pub(crate) const PAGER_RETURN_FREQUENCY: f32 = 34.3;

actions!(
    browser,
    [
        OpenCommandBar,
        CopyUrl,
        CopyUrlAsMarkdown,
        CopyUrlAsQuote,
        CapturePage,
        NewBoost,
        NewFolder,
        CollapseAllFolders,
        ExpandAllFolders,
        NewSpace,
        EditCurrentUrl,
        CloseTab,
        AddSplit,
        CloseSplitPane,
        ExpandCurrentSplit,
        ViewSource,
        ViewLibrary,
        ViewDownloads,
        PrintPage,
        SavePage,
        SharePage,
        DeveloperTools,
        InspectElements,
        JavaScriptConsole,
        NetworkInspector,
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
        ToggleToolbar,
        ToggleDeveloperMode,
        NextSpace,
        PreviousSpace,
        ClearToday,
        RestoreTab,
        SelectNext,
        SelectPrevious,
        Confirm,
        ConfirmBackground,
        ConfirmInstant,
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
    "cmd-z" => crate::text_input::Undo,
    "cmd-shift-z" => crate::text_input::Redo,
    "cmd-shift-v" => crate::text_input::PasteAndMatchStyle,
    "cmd-t" => OpenCommandBar,
    "cmd-," => crate::menus::Settings,
    "cmd-y" => crate::history::ViewHistory,
    "cmd-m" => crate::menus::Minimize,
    "ctrl-cmd-f" => crate::menus::ToggleFullscreen,
    "cmd-l" => EditCurrentUrl,
    "cmd-shift-l" => ViewLibrary,
    "cmd-shift-j" => ViewDownloads,
    "cmd-p" => PrintPage,
    "cmd-shift-s" => SavePage,
    "cmd-shift-c" => CopyUrl,
    "cmd-shift-2" => CapturePage,
    "cmd-alt-shift-c" => CopyUrlAsMarkdown,
    "cmd-w" => CloseTab,
    "cmd-shift-w" => crate::menus::CloseWindow,
    "ctrl-cmd-n" => crate::menus::BlankWindow,
    "cmd-n" => crate::menus::NewWindow,
    "cmd-alt-n" => crate::transient::NewLittleWindow,
    "cmd-o" => crate::transient::OpenInSpace,
    "cmd-shift-n" => crate::menus::NewIncognitoWindow,
    "ctrl-shift-=" => AddSplit,
    "ctrl-shift--" => CloseSplitPane,
    "cmd-shift-t" => RestoreTab,
    "cmd-shift-k" => ClearToday,
    "ctrl-tab" => crate::tab_switcher::NextRecentItem,
    "ctrl-shift-tab" => crate::tab_switcher::PreviousRecentItem,
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
    "cmd-alt-u" => ViewSource,
    "cmd-alt-i" => DeveloperTools,
    "f12" => DeveloperTools,
    "cmd-alt-c" => InspectElements,
    "cmd-alt-j" => JavaScriptConsole,
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
    "cmd-shift-d" => ToggleToolbar,
    "ctrl-d" => ToggleDeveloperMode,
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
    crate::inline_rename::bind_keys(cx);
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
        KeyBinding::new("cmd-enter", ConfirmBackground, bar),
        KeyBinding::new("shift-enter", ConfirmInstant, bar),
        KeyBinding::new("escape", Dismiss, bar),
    ]);
}

/// The sidebar pages between spaces like Arc's: spaces sit side by side and
/// a two-finger swipe drags them, then they settle on one.
#[derive(Default)]
pub struct Pager {
    /// Visual position in spaces: 1.5 is halfway between the 2nd and 3rd.
    pub position: f32,
    velocity: f32,
    drag: Option<Drag>,
    settle: Option<Task<()>>,
}

/// Analytic critically damped spring, in pages and pages/second. Keeping the
/// release velocity makes a flick and an interrupted snap continuous.
pub(crate) fn pager_spring(from: f32, target: f32, velocity: f32, frequency: f32, seconds: f32) -> (f32, f32) {
    let displacement = from - target;
    let coefficient = velocity + frequency * displacement;
    let decay = (-frequency * seconds).exp();
    (
        target + (displacement + coefficient * seconds) * decay,
        (velocity - frequency * coefficient * seconds) * decay,
    )
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

struct PinnedReveal {
    amount: f32,
    collapsed: bool,
    _task: Task<()>,
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
    Archive(Option<SpaceId>),
    ConfirmClearArchive,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Suggestion {
    Load(Resolved),
    AskChatGpt { query: String },
    Calculator { answer: String },
    SplitTab { id: TabId, title: String, space: String },
    History { title: String, url: String },
    SwitchToTab { id: TabId, title: String, url: String, space: String },
    Action { command: Command, label: String },
    SaveSpace { name: String },
    SaveFolderName { name: String },
    FolderDestination { space: SpaceId, parent: Option<FolderId>, label: String },
    SaveTabName { name: String },
    SaveIcon { icon: Option<String> },
    ArchivedTab { id: TabId, title: String, detail: String, date: String },
    MoreArchive { remaining: usize },
    ConfirmClearArchive,
}

/// Commands currently implemented by the browser, also exposed by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewBoost,
    AddSplit(Option<browser_core::splits::Placement>),
    CloseSplitPane,
    SplitAxis(browser_core::splits::Axis),
    SeparateSplit,
    SeparatePane,
    ExpandCurrentSplit,
    ViewSource,
    DevTools(crate::devtools::Command),
    ToggleDeveloperMode,
    FocusPane(usize),
    CyclePane(isize),
    MovePane(isize),
    DuplicateTab,
    NewFolder,
    MoveToFolder,
    PinToFolder(FolderId),
    NewFolderWithTab,
    NewWindow,
    NewIncognitoWindow,
    NewLittleWindow,
    NewProfile(SpaceId),
    ChangeProfile(SpaceId),
    BlankWindow,
    CollapseAllFolders,
    ExpandAllFolders,
    RenameFolder(FolderId),
    ExpandFolder(FolderId),
    NewSpace,
    RenameSpace,
    FindPage,
    Settings,
    ViewArchive,
    ViewHistory,
    OpenLibrary,
    CopyBrowserInfo,
    RevealData,
    ViewMedia,
    ViewBoosts,
    ClearCookiesAndRefresh,
    ClearCacheAndRefresh,
    ManageSpaces,
    PrintPage,
    SavePage,
    SharePage,
    AutomaticAppearance,
    LightAppearance,
    DarkAppearance,
    ViewDownloads,
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
    CopyUrlAsQuote,
    CapturePage,
    OpenTaskManager,
    AirTrafficControl,
    SearchSettings,
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
    /// The user's draft while arrow navigation displays a selected page URL.
    pub(crate) navigation_query: Option<String>,
    pub target: Target,
    pub selected: usize,
    pub(crate) site_search: Option<omnibox::SiteSearch>,
    pub(crate) site_search_profile: Option<browser_core::UserProfileId>,
    pub(crate) remote: Vec<String>,
    pub(crate) remote_generation: u64,
    pub(crate) remote_task: Option<Task<()>>,
    pub(crate) suggestions_privacy: bool,
    pub(crate) material_backdrop: Rc<Cell<bool>>,
    material_alive: Rc<Cell<bool>>,
    pub(crate) shadow_cache: crate::space_theme::ThemeShadowCache,
    pub(crate) archive_reason: Option<browser_core::ArchiveReason>,
    pub(crate) archive_limit: usize,
    pub(crate) rows_scroll: gpui::ScrollHandle,
    pub(crate) icon_picker: crate::icon_catalog::Picker,
    pub(crate) icon_anchor: Option<Bounds<gpui::Pixels>>,
    /// The bar is drawn in its own transparent window above the browser
    /// window, since pages are native views that cover anything drawn below.
    pub overlay: Option<WindowHandle<Overlay>>,
    _changed: Subscription,
    _empty_backspace: Subscription,
}

impl CommandBar {
    pub(crate) fn query<'a>(&'a self, cx: &'a App) -> &'a str {
        self.navigation_query.as_deref().unwrap_or_else(|| self.input.read(cx).text())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    CommandBar,
    RecentItems,
    Find,
    Toast(u64),
    LinkStatus,
    MaxPreview,
    MaxPreviewMenu,
    PageZoom,
    PageSwipe,
    SplitDrop,
    /// The hidden sidebar, shown floating over the page.
    Sidebar,
    TabMenu,
    SiteControls,
    Capture,
    NewMenu,
    UserProfileDialog,
    PasswordPrompt,
    Setup,
    ThemeEditor,
    TransientPicker,
}

/// Who restores native focus after an overlay has been removed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupFocus {
    Restore,
    OnActivation,
}

/// Root of a transparent child window over the browser window. Pages are
/// native views that cover anything the browser window draws, so UI that
/// lies over a page lives in one of these. Draws from the browser window.
pub struct Overlay {
    browser: WeakEntity<BrowserWindow>,
    kind: OverlayKind,
    _observe: Option<Subscription>,
    preview_focus: Option<FocusHandle>,
}

impl Overlay {
    pub(crate) fn draws_split_drag_card(&self)->bool {matches!(self.kind,OverlayKind::SplitDrop)}

    pub(crate) fn belongs_to(&self,owner:gpui::EntityId)->bool {self.browser.entity_id()==owner}

    pub(crate) fn new(browser: WeakEntity<BrowserWindow>, kind: OverlayKind, cx: &mut Context<Self>) -> Self {
        let observe = browser
            .upgrade()
            .map(|browser| cx.observe(&browser, |_, _, cx| cx.notify()));
        Self {
            browser,
            kind,
            _observe: observe,
            preview_focus: matches!(kind,OverlayKind::MaxPreview|OverlayKind::MaxPreviewMenu).then(||cx.focus_handle()),
        }
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        // Passive animation reads its snapshot without updating the shared
        // browser entity during another window's render pass.
        if let OverlayKind::Toast(generation) = kind {
            let now = cx.background_executor().now();
            let width = f32::from(window.viewport_size().width);
            return self.browser.read_with(cx, |browser, _| browser.render_toast(generation, width, now, self.browser.clone()))
                .unwrap_or_else(|_| div().into_any_element());
        }
        let preview_focus=self.preview_focus.clone();
        if let Some(focus)=&preview_focus {focus.focus(window);}
        let content=self.browser
            .update(cx, |browser, cx| {
                let content = match kind {
                    OverlayKind::RecentItems => browser.render_recent_items(cx),
                    OverlayKind::CommandBar => browser.render_command_bar_overlay(f32::from(window.viewport_size().width), f32::from(window.viewport_size().height), cx),
                    OverlayKind::Find => browser.render_find_bar(cx),
                    OverlayKind::Toast(_) => unreachable!("passive toast renders above"),
                    OverlayKind::MaxPreviewMenu => browser.render_max_preview_menu(cx),
                    OverlayKind::MaxPreview => browser.render_max_preview(window, cx),
                    OverlayKind::LinkStatus => browser.render_link_status(window),
                    OverlayKind::PageSwipe => browser.render_page_swipe(cx),
                    OverlayKind::PageZoom => browser.render_page_zoom(),
                    OverlayKind::SplitDrop => browser.render_split_drop(window),
                    OverlayKind::Sidebar => browser.render_floating_sidebar(cx),
                    OverlayKind::TabMenu => browser.render_tab_menu(window, cx),
                    OverlayKind::SiteControls => browser.render_site_controls(window, cx),
                    OverlayKind::Capture => browser.render_capture(window, cx),
                    OverlayKind::NewMenu => browser.render_new_menu(cx),
                    OverlayKind::ThemeEditor => { browser.sync_theme_editor_backdrop(window); browser.render_theme_editor(cx) },
                    OverlayKind::UserProfileDialog => browser.render_user_profile_dialog(),
                    OverlayKind::PasswordPrompt => browser.render_password_prompt(),
                    OverlayKind::Setup => browser.render_setup(),
                    OverlayKind::TransientPicker => browser.render_transient_picker(window, cx),
                };
                browser.render_overlay_shell(content, preview_focus.as_ref(), cx)
            })
            .unwrap_or_else(|_| div().into_any_element());
        content
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
    height: f32,
    _pointer_task: Option<Task<()>>,
}


#[derive(Default)]
struct SidebarScrolls { pinned: gpui::ScrollHandle, today: gpui::ScrollHandle, pending: Rc<Cell<Option<(Section, usize)>>> }

pub struct BrowserWindow {
    pub browser: browser_core::windows::WindowProjection,
    pub(crate) profile: crate::profile::ProfileBinding,
    pub(crate) sidebar_navigation: crate::sidebar_navigation::SidebarNavigation,
    pub(crate) mini_player: crate::mini_player::MiniPlayer,
    pub(crate) max: crate::max::State,
    pub(crate) sidebar_debug_namespace: Option<String>,
    pub(crate) sidebar_page_interactive: bool,
    pub sidebar_visible: bool,
    pub(crate) transient: Option<crate::transient::Transient>,
    pub(crate) peek_child: Option<WindowHandle<BrowserWindow>>,
    pub(crate) peek_owner: Option<crate::peek::PeekOwner>,
    sidebar_width: f32,
    sidebar_resize: Option<sidebar_resize::SidebarResizeGesture>,
    sidebar_resize_generation: u64,
    sidebar_resize_focus_on_activation: Option<(crate::pages::ResizePageFocus, u64)>,
    pub(crate) sidebar_render_width: Option<f32>,
    sidebar_reveal: Option<PinnedReveal>,
    sidebar_scrolls: HashMap<SpaceId, SidebarScrolls>,
    pub(crate) icon_anchors: Rc<RefCell<HashMap<crate::icons::IconTarget, Bounds<gpui::Pixels>>>>,
    pub(crate) last_icon_catalog: crate::icon_catalog::Catalog,
    pinned_reveals: HashMap<SpaceId, PinnedReveal>,
    tab_drop_edges: RefCell<HashMap<TabId, Rc<Cell<bool>>>>,
    pub(crate) tab_drag_active: bool,
    tab_drag_press: Rc<Cell<Option<gpui::Point<gpui::Pixels>>>>,
    sidebar_press: Option<(TabId, f32, f32, Instant)>,
    sidebar_press_task: Option<Task<()>>,
    space_drag_hover: Option<(SpaceId, Task<()>)>,
    pub(crate) split_drop: Option<crate::split_drop::SplitDropPreview>,
    pub(crate) split_drop_generation: u64,
    pub(crate) split_draft: Option<crate::split::SplitDraft>,
    pub(crate) hovered_row: Option<TabId>,
    pub(crate) sidebar_selection: crate::multi_selection::SidebarSelection,
    /// Layout that changes on sidebar hover (Today actions) needs state:
    /// hover styles alone repaint without a new layout.
    pub(crate) sidebar_hovered: bool,
    pub(crate) traffic_lights_hovered: bool,
    favicon_styles: RefCell<crate::favicon_style::FaviconStyles>,
    pub(crate) snapshot_images: RefCell<crate::window_snapshots::SnapshotImages>,
    pub command_bar: Option<CommandBar>,
    pub(crate) theme_editor: Option<crate::theme_editor::ThemeEditor>,
    pub(crate) theme_paint_cache: crate::space_theme::ThemePaintCache,
    pub(crate) theme_system_dark: bool,
    theme_background_blurred: Option<bool>,
    pub(crate) passwords: crate::password_flow::PasswordFlow,
    pub(crate) user_profile_dialog: Option<crate::user_profiles::ProfileDialogHost>,
    pub(crate) setup: Option<crate::setup::SetupHost>,
    pub(crate) tab_rename: Option<crate::inline_rename::TabRename>,
    pub(crate) tab_rename_pending: Option<TabId>,
    pub(crate) tab_rename_generation: u64,
    pub(crate) find_bar: Option<crate::find::FindBar>,
    pub(crate) toast: Option<crate::toast::Toast>,
    pub(crate) toast_generation: u64,
    pub(crate) quote_request: u64,
    pub(crate) pending_context_image: Option<(u64, crate::pages::PageIdentity, String, std::time::Instant)>,
    pub(crate) pending_quote: Option<(u64, crate::pages::PageIdentity, String)>,
    pub(crate) retiring_toasts: Vec<crate::toast::Toast>,
    pub(crate) share_picker: Option<crate::native::sharing::SharePicker>,
    pub(crate) page_share_generation: u64,
    pub(crate) page_swipe: Option<crate::page_swipe::PageSwipeFeedback>,
    pub(crate) link_status: Option<crate::link_status::LinkStatus>,
    pub(crate) link_status_generation: u64,
    pub(crate) page_zoom: Option<crate::page_zoom::PageZoom>,
    pub(crate) page_zoom_observed: Vec<(browser_core::UserProfileId, TabId, String, u16)>,
    pub(crate) page_zoom_generation: u64,
    pub(crate) page_zoom_refresh: bool,
    pub(crate) loading_indicators: crate::page_loading::LoadingIndicators,
    pub(crate) history_page: crate::history::HistoryPage,
    pub(crate) search_settings: crate::search_settings::State,
    pub(crate) tab_menu: Option<crate::tab_menu::TabMenu>,
    pub(crate) site_controls: Option<crate::site_controls::SiteControls>,
    pub(crate) site_settings_page: crate::site_settings_page::State,
    pub(crate) capture: Option<crate::capture::Capture>,
    pub(crate) site_controls_anchor: crate::site_controls::Anchor,
    pub(crate) new_menu: Option<crate::new_menu::NewMenu>,
    pub(crate) boost_editor: Option<WindowHandle<crate::boosts::BoostEditor>>,
    pub(crate) boost_creation_pending: bool,
    pub(crate) new_menu_anchor: crate::site_controls::Anchor,
    /// Times the New menu opened or closed, keying the plus rotation.
    pub(crate) new_menu_toggles: u64,
    /// The control last clicked and its click count, keying its press bounce.
    pressed_control: Option<(&'static str, u64, Instant)>,
    /// Copy URL clicks, keying the link → checkmark feedback.
    pub(crate) copy_feedback: Option<(u64, Instant)>,
    /// When the New menu last opened or closed.
    new_menu_toggled_at: Option<Instant>,
    /// Web pages; `None` when running without an engine.
    pub pages: Option<Rc<RefCell<Pages>>>,
    /// Native callbacks waiting behind a browser shortcut's GPUI dispatch.
    pending_engine_events: std::collections::VecDeque<crate::pages::PageEvent>,
    engine_shortcut_pending: bool,
    pub floating_sidebar: Option<FloatingSidebar>,
    pub(crate) main_window: AnyWindowHandle,
    pub(crate) native_view: Option<*mut c_void>,
    /// Native mouse tracking around the page, see `native::track_mouse_outside`.
    mouse_tracking: Rc<RefCell<MouseTracking>>,
    pub pager: Pager,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) native_action_focus: FocusHandle,
    print_generation: u64,
    save_page_generation: u64,
    save_page_task: Option<Task<()>>,
    pub(crate) save_prompt_active: bool,
    popup_focus_generation: u64,
    popup_focus_pending: usize,
    popup_focus_on_activation: bool,
    pub(crate) recent_items: browser_core::recent_items::RecentItems,
    pub(crate) tab_switcher: Option<crate::tab_switcher::Switcher>,
}

impl BrowserWindow {
    pub(crate) fn favicon_image_for(&self, tab: &browser_core::Tab) -> Option<std::sync::Arc<gpui::Image>> {
        let profile = self.browser.user_profile_id_for_tab(tab.id)?;
        let png = self.browser.favicon(profile, &tab.url)?;
        Some(self.favicon_styles.borrow_mut().get(png).image)
    }

    pub(crate) fn window_controls_offset(&self) -> f32 {
        if self.transient.is_some() || !self.page_content_visible() { 0. } else { self.sidebar_width() * (self.sidebar_amount() - 1.) }
    }

    pub(crate) fn sidebar_debug_name(&self, name: String) -> String {
        self.sidebar_debug_namespace.as_ref().map_or_else(|| name.clone(), |namespace| format!("{namespace}/{name}"))
    }

    pub(crate) fn reset_profile_pager(&mut self, index: usize) {
        self.pager.settle = None;
        self.pager.drag = None;
        self.pager.velocity = 0.;
        self.pager.position = if self.browser.is_blank() || self.browser.is_private() {
            index
        } else {
            self.sidebar_page_index()
        } as f32;
    }

    pub(crate) fn receive_profile_projection(&mut self, projection: browser_core::windows::WindowProjection) {
        let selected = self.sidebar_navigation.controller.selected();
        let previous_index = self.sidebar_navigation.controller.pages(&self.browser).iter().position(|page| *page == selected);
        self.recent_items.record(&projection);
        self.browser = projection;
        self.sidebar_navigation.downloads.dirty = true;
        self.reconcile_sidebar_navigation();
        let current_index = self.sidebar_navigation.controller.pages(&self.browser).iter().position(|page| *page == selected);
        // A retained draft follows its origin through external reorders even
        // when the underlying real Space and its model index did not change.
        let derived_moved = previous_index.is_some() && previous_index != current_index;
        if derived_moved { self.reset_profile_pager(self.browser.active_space); }
        self.invalidate_history_confirmation();
        if let Some(pages) = &self.pages { pages.borrow_mut().reconcile_inventory(&self.browser); }
    }

    pub(crate) fn invalidate_sidebar_focus(&mut self) {
        self.popup_focus_generation = self.popup_focus_generation.wrapping_add(1);
        self.popup_focus_pending = 0;
    }
    pub(crate) fn settle_sidebar_pager(&mut self, cx: &mut Context<Self>) { self.settle_pager(cx); }
    pub(crate) fn reset_sidebar_motion(&mut self) { self.pager.drag = None; self.pager.settle = None; self.pager.velocity = 0.; }

    pub fn new(
        browser: Browser,
        engine: Option<Box<dyn WebEngine>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let profile = crate::profile::ProfileHandle::new(browser, cx);
        let selection = profile.selection();
        Self::with_profile(profile, Some(selection), engine, window, cx)
    }

    pub(crate) fn with_profile(
        profile: crate::profile::ProfileHandle,
        selection: Option<browser_core::windows::WindowSelection>,
        engine: Option<Box<dyn WebEngine>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_profile_pages(profile, selection, engine.map(Pages::new), window, cx)
    }

    pub(crate) fn with_profile_pages(
        profile: crate::profile::ProfileHandle,
        selection: Option<browser_core::windows::WindowSelection>,
        pages: Option<Pages>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selection = selection.unwrap_or_else(|| profile.new_window_selection());
        let browser = profile.projection(&selection);
        let profile = crate::profile::ProfileBinding::new(profile, selection, cx);
        let pages = pages.map(|mut pages| {
            if let Some(parent) = crate::native::view_of(window) {
                crate::ordinary_downloads::bind(parent, profile.handle.clone(), window.window_handle(), cx);
                pages.set_parent(parent);
            }
            Rc::new(RefCell::new(pages))
        });
        let closing_root = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            // AppKit must not destroy the parent before the outside-App
            // preparation boundary. Red close and Cmd+Shift+W share one path.
            let _ = closing_root.update(cx, |this, cx| this.close_window(cx));
            false
        });
        cx.observe_window_bounds(window, |this, window, cx| {
            this.resize_peek(window, cx);
            this.resize_setup(window,cx);
            this.resize_password_prompt(window,cx);
        }).detach();
        cx.observe_window_appearance(window, |this, window, cx| {
            this.theme_system_dark = matches!(window.appearance(), gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark);
            cx.notify();
        }).detach();
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() { this.restore_site_settings_peek_focus(cx); this.maybe_password_prompt(cx); }
            this.set_little_archive_active(window.is_window_active(),cx);
            this.max_preview_activation(window.is_window_active(),cx);
            this.on_sidebar_resize_activation(window.window_handle(), window.is_window_active(), cx);
        }).detach();
        if let Some(pages) = pages.clone() {
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refresh_profile_projection();
                    if std::mem::take(&mut this.popup_focus_on_activation) && !this.popup_owns_focus() {
                        this.focus_handle.focus(window);
                        if this.page_content_visible() { if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); } }
                    }
                    if let Some(pages) = &this.pages { pages.borrow_mut().activate_host_with_visibility(&this.browser, this.page_content_visible()); }
                    this.restore_abandoned_resize_focus();
                    if !this.page_content_visible() && this.sidebar_root_may_take_focus()
                        && !this.pages.as_ref().is_some_and(|pages| pages.borrow().capture_resize_focus().has_native_focus())
                    { this.focus_sidebar_page(window, cx, None); }
                    cx.notify();
                }
            }).detach();
            let pump = pages.borrow().pump_fn();
            let pages = Rc::downgrade(&pages);
            cx.spawn_in(window, async move |this, cx| {
                let mut mirror_revision = 0;
                loop {
                    cx.background_executor().timer(ENGINE_PUMP_INTERVAL).await;
                    if crate::quit::is_quitting() || this.upgrade().is_none() {
                        break;
                    }
                    pump();
                    let _ = cx.update(|_, cx| { if let Some(downloads) = cx.try_global::<crate::downloads::AppDownloads>().cloned() { downloads.wake(cx); } });
                    if crate::quit::is_quitting() { break; }
                    let Some(pages) = pages.upgrade() else { break; };
                    let Ok((events, revision, retry_creation)) = pages.try_borrow_mut().map(|mut pages| {
                        let events = pages.take_routed_events();
                        (events, pages.mirror_revision(), pages.creation_retry_pending())
                    }) else {
                        continue;
                    };
                    let mirrors_changed = mirror_revision != revision;
                    mirror_revision = revision;
                    // CEF consumes mouse events, including exits from the rail.
                    // Reconcile both edges against the native pointer rather than
                    // leaving GPUI's cached hover stuck after a page interaction.
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.poll_download_ui(cx);
                        this.poll_download_destinations(window,cx);
                        this.poll_max_preview_modifiers(window,cx);
                        if let Some((x, y)) = this.native_view.and_then(crate::native::pointer_in_view) {
                            let hovered = (window.is_window_active() || this.peek_child.is_some())
                                && x >= 0. && x < this.sidebar_width() && y >= 0. && y < f32::from(window.viewport_size().height);
                            if this.sidebar_hovered != hovered { this.sidebar_hovered = hovered; cx.notify(); }
                        }
                    });
                    if events.is_empty() && !mirrors_changed && !retry_creation {
                        continue;
                    }
                    let applied = this.update_in(cx, |this, window, cx| {
                        this.apply_page_events(events, window, cx);
                        let retried = if retry_creation {
                            this.refresh_profile_projection();
                            this.pages.as_ref().is_some_and(|pages| pages.borrow_mut().retry_pending_creation_with_visibility(&this.browser, this.page_content_visible()))
                        } else { false };
                        if mirrors_changed || retried { cx.notify(); }
                    });
                    if applied.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }

        let sidebar_navigation = crate::sidebar_navigation::SidebarNavigation::new(&browser);
        let position = if browser.is_blank() || browser.is_private() { browser.active_space } else { browser.active_space + 1 } as f32;
        cx.on_release(|this, cx| {
            this.cancel_split_draft();
            if let Some(pages) = &this.pages { pages.borrow_mut().release_host(); }
            this.profile.flush(cx);
            this.remove_child_windows(cx);
        }).detach();
        let mut recent_items = browser_core::recent_items::RecentItems::default();
        recent_items.record(&browser);
        Self {
            recent_items, tab_switcher: None,
            pager: Pager {
                position,
                ..Default::default()
            },
            browser,
            profile,
            sidebar_navigation,
            mini_player: Default::default(),
            max: Default::default(),
            sidebar_debug_namespace: None,
            sidebar_page_interactive: true,
            sidebar_visible: true,
            transient: None, peek_child: None, peek_owner: None,
            sidebar_width: SIDEBAR_WIDTH,
            sidebar_resize: None,
            sidebar_resize_generation: 0,
            sidebar_resize_focus_on_activation: None,
            sidebar_render_width: None,
            sidebar_reveal: None,
            sidebar_scrolls: HashMap::new(),
            icon_anchors: Default::default(),
            last_icon_catalog: Default::default(),
            pinned_reveals: HashMap::new(),
            tab_drop_edges: RefCell::new(HashMap::new()),
            tab_drag_active: false,
            tab_drag_press: Rc::new(Cell::new(None)),
            sidebar_press: None,
            sidebar_press_task: None,
            space_drag_hover: None,
            split_drop: None,
            split_drop_generation: 0,
            split_draft: None,
            hovered_row: None,
            sidebar_selection: Default::default(),
            sidebar_hovered: false,
            traffic_lights_hovered: false,
            favicon_styles: Default::default(),
            snapshot_images: Default::default(),
            command_bar: None,
            theme_editor: None,
            theme_paint_cache: Default::default(),
            theme_background_blurred: None,
            theme_system_dark: matches!(window.appearance(), gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark),
            passwords: Default::default(),
            user_profile_dialog: None,
            setup: None,
            tab_rename: None,
            tab_rename_pending: None,
            tab_rename_generation: 0,
            find_bar: None,
            toast: None,
            toast_generation: 0,
            quote_request: 0,
            pending_context_image: None,
            pending_quote: None,
            retiring_toasts: Vec::new(),
            share_picker: None,
            page_share_generation: 0,
            page_swipe: None,
            link_status: None,
            link_status_generation: 0,
            page_zoom: None,
            page_zoom_observed: Vec::new(),
            page_zoom_generation: 0,
            page_zoom_refresh: false,
            loading_indicators: Default::default(),
            history_page: crate::history::HistoryPage::new(cx),
            search_settings: crate::search_settings::State::new(cx),
            pages,
            pending_engine_events: Default::default(),
            engine_shortcut_pending: false,
            floating_sidebar: None,
            tab_menu: None,
            site_controls: None,
            site_settings_page: Default::default(),
            capture: None,
            site_controls_anchor: Default::default(),
            new_menu: None,
            boost_editor: None,
            boost_creation_pending: false,
            new_menu_anchor: Default::default(),
            new_menu_toggles: 0,
            pressed_control: None,
            copy_feedback: None,
            new_menu_toggled_at: None,
            main_window: window.window_handle(),
            native_view: crate::native::view_of(window),
            mouse_tracking: Rc::default(),
            focus_handle: cx.focus_handle(),
            native_action_focus: cx.focus_handle(),
            print_generation: 0,
            save_page_generation: 0, save_page_task: None, save_prompt_active: false,
            popup_focus_generation: 0,
            popup_focus_pending: 0,
            popup_focus_on_activation: false,
        }
    }

    /// Keeps the session saved at `path`: shortly after every change and
    /// when the app quits.
    pub fn save_session_to(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.profile.save_session_to(path, cx);
    }

    pub(crate) fn prepare_quit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.commit_theme_editor();
        self.profile.handle.flush_for_quit(cx)
    }

    pub(crate) fn detach_for_quit(&mut self, cx: &mut Context<Self>) {
        // Parent windows stay alive until all engine-tracked children are closed.
        self.remove_child_windows(cx);
        self.pages.take();
        self.native_view = None;
        self.pending_engine_events.clear();
        cx.notify();
    }

    /// Closing a window keeps its tabs in the session, unlike Archive Tab.
    pub(crate) fn close_window(&mut self, cx: &mut Context<Self>) {
        self.close_theme_editor_with_focus(PopupFocus::OnActivation, cx);
        if crate::quit::is_quitting() || crate::quit::last_window_close(cx) { return; }
        if !window_close::claim(self.main_window, cx) { return; }
        self.detach_peek(cx);
        self.cancel_split_draft();
        // Parent remains alive. release_host only queues retirement; the
        // engine's native handoff must run in the task, outside this update.
        if let Some(pages) = &self.pages { pages.borrow_mut().release_host(); }
        if let Some(downloads) = cx.try_global::<crate::downloads::AppDownloads>().cloned() { downloads.wake(cx); }
        self.remove_child_windows(cx);
        // Native teardown is complete while the NSView/HWND is still alive.
        // Entity release can happen after GPUI destroys it; never reuse that
        // raw pointer from the release callback or a late metadata observer.
        self.native_view = None;
        window_close::prepare(self.main_window, cx);
    }

    /// Automation-only explicit admission. Ordinary creators remain denied.
    pub(crate) fn arm_download_probe(&mut self, config: download_probe::Config,
        cx: &mut Context<Self>) -> Result<(), String> {
        if cx.has_global::<crate::downloads::AppDownloads>() || download_probe::admission::occupied() {
            return Err("Download consumer/admission occupied".into());
        }
        download_probe::init(cx);
        if crate::quit::is_quitting() || self.browser.is_private() || self.browser.is_blank() {
            return Err("Probe requires a normal workspace".into());
        }
        if !download_probe::can_arm(cx) { return Err("Probe is already armed".into()); }
        let pages = self.pages.clone().ok_or("Probe requires native pages")?;
        // No sync/creation happens inside edit_browser. Record the new Today
        // TabId before installing the one-source descriptor and calling sync.
        let url = config.fixture_url();
        let tab = self.edit_browser(|browser| browser.new_tab(&url, SystemTime::now()));
        let source = {
            let mut pages = pages.borrow_mut();
            pages.reconcile_inventory(&self.browser);
            pages.download_probe_source(&self.browser, tab, &config)?
        };
        download_probe::arm(config, source, cx)?;
        pages.borrow_mut().sync(&self.browser, self.page_content_visible());
        cx.notify();
        Ok(())
    }

    /// Prepared filesystem work is detached; only normal canonical identity is captured here.
    pub(crate) fn arm_app_download_fixture(&mut self, arm: crate::download_acceptance::PreparedArm,
        cx: &mut Context<Self>) -> Result<(), String> {
        use crate::{download_acceptance::{self, AppDownloadAcceptance, FixtureAudit}, downloads::AppDownloads};
        download_acceptance::preflight(cx)?;
        self.refresh_profile_projection();
        if self.browser.is_private() || self.browser.is_blank() { return Err("Fixture requires a normal workspace".into()); }
        let pages = self.pages.clone().ok_or("Fixture requires native pages")?;
        if !pages.borrow().has_native_parent() { return Err("Fixture requires a native parent".into()); }
        let captured = self.browser.active_user_profile_id();
        let handle = self.profile.handle.clone();
        let (normal, id, blank) = handle.download_normal_owner(captured).ok_or("Canonical owner unavailable")?;
        if blank || id != captured || !normal.same_workspace(&handle) { return Err("Canonical owner mismatch".into()); }
        let downloads = AppDownloads::default();
        let owner = downloads.service.borrow_mut().register_source(&handle, captured)?;
        let config = arm.config.clone();
        let audit = Rc::new(RefCell::new(FixtureAudit::new(arm, captured, owner)));
        downloads.service.borrow_mut().attach_fixture(audit.clone())?;
        // Claim once before model edits. Any subsequent failure stays observable;
        // never reset, adopt an existing descriptor or start a replacement run.
        cx.set_global(AppDownloadAcceptance::default());
        cx.set_global(downloads.clone());
        let result: Result<(), String> = (|| {
            let tab = self.edit_browser(|browser| browser.new_tab(&config.fixture_url(), SystemTime::now()));
            let source = {
                let mut pages = pages.borrow_mut();
                pages.reconcile_inventory(&self.browser);
                pages.download_probe_source(&self.browser, tab, &config)?
            };
            if source.identity.profile != captured { return Err("Fixture source profile changed".into()); }
            let descriptor = download_probe::admission::try_install(config, source, owner)?;
            audit.borrow_mut().admission = Some(descriptor.clone());
            cx.global_mut::<AppDownloadAcceptance>().descriptor = Some(descriptor);
            pages.borrow_mut().sync(&self.browser, self.page_content_visible());
            Ok(())
        })();
        if let Err(error) = &result { audit.borrow_mut().fail(error); }
        downloads.wake(cx);
        cx.notify();
        result
    }

    /// Automation closes only the retained fixture, even after selection changes.
    pub(crate) fn close_app_download_source(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if crate::quit::is_quitting() { return Err("Quit sealed".into()); }
        let descriptor = cx.try_global::<crate::download_acceptance::AppDownloadAcceptance>()
            .and_then(|run| run.descriptor.clone()).ok_or("Fixture source unavailable")?;
        let (source, url) = { let d = descriptor.borrow(); (d.source.clone(), d.config.fixture_url()) };
        self.refresh_profile_projection();
        let scoped = !self.browser.is_private() && !self.browser.is_blank() && self.browser.spaces.iter().any(|space|
            space.id == source.space && space.user_profile_id == source.identity.profile
                && space.today.iter().any(|tab| tab.id == source.identity.tab && tab.url == url));
        if !scoped || !self.pages.as_ref().is_some_and(|p| p.borrow().owns_download_fixture_source(&source)) {
            return Err("Fixture source identity/host changed".into());
        }
        self.close_tab(source.identity.tab, cx);
        Ok(())
    }

    /// Child windows can outlive their parent on GPUI's test platform and
    /// must also be removed when a native titlebar close releases the root.
    fn remove_child_windows(&mut self, cx: &mut App) {
        self.save_page_generation = self.save_page_generation.wrapping_add(1);
        self.save_prompt_active=false;
        if let Some(task)=self.save_page_task.take() {cx.spawn(async move |_| {drop(task);}).detach();}
        crate::peek_history::discard_owner(self.main_window.window_id(), cx);
        self.page_share_generation = self.page_share_generation.wrapping_add(1);
        if let Some(picker) = self.share_picker.take() { cx.spawn(async move |_| drop(picker)).detach(); }
        self.discard_sidebar_resize();
        if let Some(bar) = &self.command_bar { bar.material_alive.set(false); }
        self.retire_max_preview(cx);
        self.clear_command_bar_backdrop();
        self.clear_site_controls_backdrop();
        let transient_child = self.transient.as_mut().and_then(|state| state.picker.take()).and_then(|picker| picker.overlay);
        let password_child = self.take_password_child(cx);
        let profile_child = self.take_user_profile_child(cx);
        let setup_child = self.take_setup_child(cx);
        let theme_child = self.take_theme_editor();
        if let Some(child) = self.peek_child.take() {
            // Peek owns native pages, so it needs its own retirement boundary.
            cx.defer(move |cx| { let _ = child.update(cx, |view, _, cx| view.close_window(cx)); });
        }
        let children = [
            self.tab_switcher.take().and_then(|state|state.overlay).map(|handle|handle.into()),
            self.boost_editor.take().map(|handle|handle.into()),
            transient_child.map(|handle| handle.into()),
            profile_child.map(|handle| handle.into()),
            password_child.map(|handle|handle.into()),
            setup_child.map(|handle| handle.into()),
            theme_child.map(|handle| handle.into()),
            self.command_bar.take().and_then(|bar| bar.overlay).map(|handle| handle.into()),
            self.find_bar.take().and_then(|bar| bar.overlay).map(|handle| handle.into()),
            self.tab_menu.take().and_then(|menu| menu.overlay).map(|handle| handle.into()),
            self.site_controls.take().and_then(|menu| menu.overlay).map(|handle| handle.into()),
            self.capture.take().and_then(|state|state.overlay).map(|handle|handle.into()),
            self.new_menu.take().and_then(|menu| menu.overlay).map(|handle| handle.into()),
            self.toast.take().and_then(|toast| toast.overlay).map(|handle| handle.into()),
            self.link_status.take().and_then(|status| status.overlay).map(|handle| handle.into()),
            self.page_swipe.take().and_then(|state|state.overlay).map(|handle|handle.into()),
            self.page_zoom.take().and_then(|state| state.overlay).map(|handle| handle.into()),
            self.split_drop.take().and_then(|preview| preview.overlay).map(|handle| handle.into()),
            self.floating_sidebar.take().and_then(|sidebar| sidebar.overlay).map(|handle| handle.into()),
        ];
        let outgoing_toasts = std::mem::take(&mut self.retiring_toasts).into_iter()
            .filter_map(|toast| toast.overlay.map(|handle| handle.into()));
        for child in children.into_iter().flatten().chain(outgoing_toasts) {
            let child: AnyWindowHandle = child;
            cx.defer(move |cx| { let _ = child.update(cx, |_, window, _| window.remove_window()); });
        }
    }

    pub fn apply_events(&mut self, events: Vec<Event>, window: &mut Window, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() { return; }
        self.refresh_profile_projection();
        if let Some(pages) = &self.pages {
            // Reconcile ownership before accepting even an already drained callback.
            pages.borrow_mut().reconcile_inventory(&self.browser);
            let pages = pages.borrow();
            self.pending_engine_events.extend(events.into_iter().filter_map(|event| pages.capture_event(event)));
        }
        if !self.engine_shortcut_pending { self.drain_engine_events(window, cx); }
    }

    pub(crate) fn apply_page_events(&mut self, events: Vec<crate::pages::PageEvent>, window: &mut Window, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() { return; }
        self.pending_engine_events.extend(events);
        if !self.engine_shortcut_pending { self.drain_engine_events(window, cx); }
    }

    fn drain_engine_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pages) = self.pages.clone() else {
            self.pending_engine_events.clear();
            return;
        };
        self.refresh_profile_projection();
        pages.borrow_mut().reconcile_inventory(&self.browser);
        'drain: while let Some(queued) = self.pending_engine_events.pop_front() {
            let ready = if queued.is_metadata() { pages.borrow().claim_metadata_through(&queued) } else { vec![queued] };
            for queued in ready {
            let crate::pages::PageEvent { event, identity, source_space, input_origin, .. } = queued;
            if input_origin.is_some_and(|origin| !pages.borrow().accepts_input_origin(origin)) { continue; }
            let identity = Some(identity);
            let source_space = Some(source_space);
            if let Some(identity) = identity {
                if !pages.borrow().accepts_identity(identity)
                    || self.browser.user_profile_id_for_tab(identity.tab) != Some(identity.profile) { continue; }
            }
            let page_profile = identity.map(|identity| identity.profile);
            if matches!(&event, Event::Focused { .. }) && (self.save_prompt_active || !self.native_page_content_visible() || self.theme_editor.is_some() || self.setup.is_some() || self.passwords.prompt.is_some() || self.user_profile_dialog.is_some() || self.command_bar.is_some() || self.tab_switcher.is_some() || self.find_bar.is_some() || self.tab_menu.is_some() || self.site_controls.is_some() || self.new_menu.is_some()) { continue; }
            pages.borrow_mut().record(&event);
            match event {
                Event::Password { event, .. } => { if let Some(identity)=identity {self.password_event(identity,event,cx);} },
                Event::LinkPreviewHover { view, token, source_url, hover } => self.max_preview_hover(view,token,source_url,hover,window,cx),
                Event::PreviewModifiers { view, shift } => self.max_preview_modifiers(view,shift,window,cx),
                Event::PictureInPictureReturnRequested { .. } => {
                    if let Some(identity)=identity { self.return_to_media_tab(identity.tab,cx);cx.notify(); }
                }
                Event::Focused { view } => {
                    self.finish_tab_rename(true, cx);
                    let tab = pages.borrow().tab_for(view).filter(|tab| pages.borrow().is_visible(*tab));
                    if let Some(tab) = tab {
                        self.restore_site_settings_peek_focus(cx);
                        // Native content owns the responder. Clear a retained
                        // GPUI editor's logical action target without calling
                        // focus_ui or changing the native first responder.
                        self.native_action_focus.focus(window);
                        // A newer visible native responder supersedes both queued
                        // sidebar focus and an already scheduled popup dismissal.
                        self.invalidate_sidebar_focus();
                        self.invalidate_navigation_focus();
                        self.popup_focus_on_activation = false;
                        self.sidebar_navigation.media_search_focus = false;
                        self.sidebar_navigation.downloads.focus = None;
                        self.sidebar_navigation.preview_ui_focus = None;
                        self.sidebar_navigation.preview_native_focus = None;
                        let active = self.browser.space().active_tab;
                        if active.is_some_and(|id| self.browser.related_tabs(id).contains(&tab)) {
                            self.edit_browser(|browser| browser.activate_tab(tab, SystemTime::now()));
                            if active != Some(tab) { self.close_find(cx); }
                        }
                        // Retain the latest validated native intent at its
                        // shared source, so other popup owners cannot lose it
                        // by clearing Pages focus before their next frame.
                        self.remember_library_native_popup_focus();
                    }
                }
                Event::MediaStateChanged { state, .. } => {
                    if let Some(identity)=identity { self.mini_player.record(identity.tab,&state); }
                    cx.notify();
                }
                Event::PageSwipe { state,.. } => { if let Some(identity)=identity {self.update_page_swipe(identity.tab,state,window,cx);} }
                Event::AudioStateChanged { .. } | Event::MediaCaptureChanged { .. } => { cx.notify(); }
                Event::TitleChanged { view, title } => {
                    if let Some(tab) = identity.map(|identity| identity.tab) {
                        if !title.is_empty() {
                            self.edit_profile(|browser| browser.set_title(tab, title.clone()));
                            let url = self.browser.all_tabs().find(|entry| entry.id == tab).map(|entry| entry.url.clone());
                            if let Some(url) = url { self.edit_profile(|browser| browser.update_history_title(page_profile.expect("page provenance"), &url, &title)); }
                        }
                    }
                }
                Event::Favicon { view, page_url, png } => {
                    if identity.is_some() { self.edit_profile(|browser| browser.cache_favicon(page_profile.expect("page provenance"), &page_url, png)); }
                }
                Event::UrlChanged { view, url } => {
                    if let Some(tab) = identity.map(|identity| identity.tab) {
                        self.edit_profile(|browser| browser.set_url(tab, url.clone()));
                        self.edit_profile(|browser| browser.record_visit(page_profile.expect("page provenance"), &url, SystemTime::now()));
                        let title = self.browser.all_tabs().find(|entry| entry.id == tab).and_then(|entry| entry.page_title.clone());
                        if let Some(title) = title { self.edit_profile(|browser| browser.update_history_title(page_profile.expect("page provenance"), &url, &title)); }
                    }
                }
                Event::ContextImage { request, page_url, png, .. } => { if let Some(identity)=identity { self.complete_context_image(identity,request,&page_url,png,cx); } }
                Event::PageContext { request, .. } => { if let Some(identity) = identity { self.confirm_page_context(identity, request, window, cx); } }
                Event::QuoteSelection { request, text, source_url, .. } => {
                    self.complete_quote(request, identity, &text, &source_url, cx);
                }
                Event::BoostZap { selector, .. } => {
                    if let (Some(identity),Some(editor))=(identity,self.boost_editor) {
                        let source=self.main_window.window_id();
                        cx.defer(move |cx| { let _=editor.update(cx,|editor,_,cx|editor.receive_zap(source,identity.profile,identity.tab,selector,cx)); });
                    }
                }
                Event::PermissionRequested { request, .. } => {
                    if let Some(identity) = identity {
                        self.show_site_permission_request(identity.tab, identity.profile, request, window, cx);
                    }
                }
                Event::PermissionDismissed { request, .. } => self.site_permission_dismissed(request, cx),
                Event::StatusChanged { view, text } => self.update_link_status(view, text, window, cx),
                Event::EditFocusChanged { .. } => {},
                Event::OpenPeek { url, .. } => {
                    if identity.is_some_and(|identity| Some(identity.tab) == self.browser.space().active_tab)
                        && source_space == Some(self.browser.space().id) {
                        self.open_peek(url, window, cx);
                    }
                }
                event @ (Event::OpenInNewTab { .. } | Event::OpenInBackgroundTab { .. }) => {
                    let requested_background = matches!(event, Event::OpenInBackgroundTab { .. });
                    let (Event::OpenInNewTab { url, .. } | Event::OpenInBackgroundTab { url, .. }) = event else { unreachable!() };
                    let Some(source) = source_space.and_then(|id| self.browser.spaces.iter().position(|space| space.id == id && Some(space.user_profile_id) == page_profile)) else { continue; };
                    let id = self.edit_browser(|browser| {
                        let mut selection = browser.window_selection();
                        let background = requested_background || browser.active_space != source;
                        browser.switch_space(source);
                        let id = browser.new_tab(url.clone(), SystemTime::now());
                        if background { browser.edit_in_window(&mut selection, |_| {}); }
                        id
                    });
                    self.edit_profile(|browser| browser.set_title(id, display_url(&url)));
                    if requested_background { self.show_background_tab(id, cx); }
                }
                Event::InspectorInput(input) => {
                    let input = pages.borrow_mut().record_inspector_input(&input);
                    match input {
                        Some(crate::devtools::Input::Focus(tab)) => {
                            self.native_action_focus.focus(window);
                            if self.browser.active_tab().is_some_and(|active| self.browser.related_tabs(active.id).contains(&tab)) {
                                self.edit_browser(|browser| browser.activate_tab(tab, SystemTime::now()));
                            }
                            self.remember_library_native_popup_focus();
                        }
                        Some(crate::devtools::Input::Shortcut(shortcut)) => {
                            self.defer_engine_shortcut(shortcut, input_origin, window, cx);
                            break 'drain;
                        }
                        None => {}
                    }
                }
                Event::Shortcut { shortcut, .. } => {
                    self.defer_engine_shortcut(shortcut, input_origin, window, cx);
                    break 'drain;
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
                    if let Some(tab) = identity.map(|identity| identity.tab) {
                        let entry = self.browser.all_tabs().find(|entry| entry.id == tab).map(|entry| (entry.url.clone(), entry.page_title.clone()));
                        if let Some((url, Some(title))) = entry { self.edit_profile(|browser| browser.update_history_title(page_profile.expect("page provenance"), &url, &title)); }
                    }
                }
                Event::DocumentColor { .. } | Event::LoadingProgress { .. } | Event::LoadingChanged { .. } => {}
            }
        }
        }
        // CEF can initialize its audio state after create_view returned. Reapply
        // persisted intent after engine events even when the window is occluded.
        pages.borrow_mut().sync_audio(&self.browser);
        cx.notify();
    }

    fn defer_engine_shortcut(&mut self, shortcut: engine::Shortcut, source: Option<crate::pages::InputOrigin>, window: &mut Window, cx: &mut Context<Self>) {
        if shortcut.key=="escape" && self.max.preview.open.is_some() { self.dismiss_max_preview(cx); return; }
        self.engine_shortcut_pending = true;
        let browser = cx.entity().downgrade();
        let keystroke = keystroke_for(shortcut);
        // Release this entity before GPUI dispatches its action. Later native
        // callbacks stay queued, including callbacks from another engine pump.
        window.defer(cx, move |window, cx| {
            let current = browser.update(cx, |this, _| {
                this.refresh_profile_projection();
                let Some(source) = source else { return false; };
                let Some(pages) = &this.pages else { return false; };
                let mut pages = pages.borrow_mut();
                pages.reconcile_inventory(&this.browser);
                this.native_page_content_visible() && pages.accepts_input_origin(source) && this.browser.user_profile_id_for_tab(source.identity.tab) == Some(source.identity.profile)
            }).unwrap_or(false);
            // No owner/model/PageStore borrow survives action dispatch.
            if current { window.dispatch_keystroke(keystroke, cx); }
            // Actions may defer their own work (for example restoring an overlay
            // before opening DevTools). Enqueue continuation after those effects.
            window.defer(cx, move |window, cx| {
                let _ = browser.update(cx, |this, cx| {
                    this.engine_shortcut_pending = false;
                    this.drain_engine_events(window, cx);
                });
            });
        });
    }

    fn with_active_page(&self, f: impl FnOnce(&mut Pages, TabId)) {
        if !self.native_page_content_visible() { return; }
        if let (Some(pages), Some(tab)) = (&self.pages, self.browser.space().active_tab) {
            f(&mut pages.borrow_mut(), tab);
        }
    }

    fn change_page_zoom(&mut self, direction: i32, cx: &mut Context<Self>) {
        self.page_zoom_refresh = true;
        self.with_active_page(|pages, tab| pages.zoom(tab, direction));
        cx.notify();
    }

    fn edit_undo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !redo {
            if let Some(editor)=self.boost_editor.filter(|editor|editor.read_with(cx,|editor,_|editor.picking).unwrap_or(false)) {
                cx.defer(move|cx|{let _=editor.update(cx,|editor,_,cx|editor.undo_last_zap(cx));});return;
            }
        }
        let before = self.browser.all_tabs().count();
        let editable = self.browser.active_tab().is_some_and(|tab|
            self.pages.as_ref().is_some_and(|pages| pages.borrow().state(tab.id).editable));
        if !editable && self.peek_owner.is_some() { self.edit_peek_history(redo, window, cx); return; }
        if !editable && if redo { self.edit_browser(|browser| browser.redo_sidebar(SystemTime::now())) } else { self.edit_browser(|browser| browser.undo_sidebar(SystemTime::now())) } {
            if !redo { self.dismiss_archive_feedback(cx); }
            if let Some(pages) = &self.pages { pages.borrow_mut().reconcile_inventory(&self.browser); }
            self.settle_pager(cx);
            if self.browser.all_tabs().count() < before { self.after_tab_removal(cx); }
            cx.notify();
        } else if !editable && self.edit_peek_history(redo, window, cx) {
            cx.notify();
        } else {
            self.with_active_page(|pages, tab| if redo { pages.redo(tab) } else { pages.undo(tab) });
        }
    }

    pub fn focus(&self, window: &mut Window) {
        if self.focus_site_settings(window){return;}
        self.focus_handle.focus(window);
    }

    pub(crate) fn accent(&self) -> f32 {
        self.browser.space().accent_hue() / 360.0
    }

    /// Content of an already positioned region in a passive overlay. Bounds
    /// are window-relative; the caller owns rounded clipping and the contour.
    /// Translate the full viewport field instead of stretching it to the pane.
    pub(crate) fn theme_region(&self, bounds: Bounds<gpui::Pixels>, viewport: gpui::Size<gpui::Pixels>) -> AnyElement {
        div().relative().w(bounds.size.width).h(bounds.size.height).overflow_hidden()
            .debug_selector(|| "theme-region".into())
            .child(div().absolute().left(-bounds.origin.x).top(-bounds.origin.y)
                .debug_selector(|| "theme-region-field".into())
                .w(viewport.width).h(viewport.height).child(self.theme_surface(false)))
            .into_any_element()
    }

    /// Paint the continuous field through window-relative rectangular masks.
    /// The caller positions this region; masks can describe rounded or inverse
    /// corner strips without affecting document layout or its native view.
    pub(crate) fn theme_surface_mask(&self, bounds: Bounds<gpui::Pixels>, viewport: gpui::Size<gpui::Pixels>, masks: Vec<Bounds<gpui::Pixels>>) -> AnyElement {
        div().relative().w(bounds.size.width).h(bounds.size.height).overflow_hidden()
            .children(masks.into_iter().map(|mask| div().absolute()
                .left(mask.left() - bounds.left()).top(mask.top() - bounds.top())
                .w(mask.size.width).h(mask.size.height)
                .child(self.theme_region(mask, viewport))))
            .into_any_element()
    }

    // Command bar.

    pub fn open_command_bar(
        &mut self,
        target: Target,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_password_prompt(PopupFocus::OnActivation,cx);
        self.close_theme_editor(cx);
        if target == Target::NewSpace { self.begin_space_creation(self.browser.space().id, cx); return; }
        if self.browser.is_blank() && !matches!(target, Target::NewTab | Target::Tab(_) | Target::Split { .. } | Target::RenameTab(_) | Target::Archive(_) | Target::ConfirmClearArchive | Target::Icon(crate::icons::IconTarget::Tab(_))) { return; }
        if crate::quit::is_quitting() { return; }
        self.remember_library_popup_focus(window, cx);
        self.close_transient_picker(cx);
        self.close_user_profile_dialog(cx);
        self.close_setup(cx);
        self.finish_tab_rename(true, cx);
        self.close_link_status(cx);
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        self.close_find(cx);
        if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
        let input = cx.new(|cx| {
            let placeholder = match target {
                Target::Split { .. } => "Search or Enter URL…",
                Target::NewFolder { .. } => "Name your Folder…",
                Target::RenameFolder(_) => "Rename Folder…",
                Target::MoveToFolder(_) | Target::MoveFolder(_) => "Choose a destination folder…",
                Target::NewSpace => "Name your Space…",
                Target::RenameSpace(_) => "Rename Space…",
                Target::RenameTab(_) => "Rename Tab…",
                Target::Icon(_) => "Search",
                Target::Archive(_) => "Search Archive…",
                Target::ConfirmClearArchive => "Type “Clear Archive” to confirm…",
                _ => "Search or Enter URL…",
            };
            let mut input = TextInput::new(placeholder, cx);
            input.set_text(if matches!(target, Target::Icon(_)) { String::new() } else { text.to_string() }, cx);
            input
        });
        let changed = cx.subscribe(&input, |this, _, _: &text_input::Changed, cx| {
            if let Some(bar) = &mut this.command_bar {
                bar.navigation_query = None;
                bar.selected = 0;
                bar.rows_scroll.set_offset(point(px(0.), px(0.)));
                bar.archive_limit = 8;
                bar.icon_picker.keyboard = false;
            }
            this.update_command_suggestions(cx);
            cx.notify();
        });
        let empty_backspace = cx.subscribe(&input, |this, input, _: &text_input::EmptyBackspace, cx| {
            if let Some(bar) = &mut this.command_bar {
                if bar.site_search.take().is_some() {
                    input.update(cx, |input, cx| input.set_placeholder("Search or Enter URL…", cx));
                    cx.notify();
                }
            }
        });
        self.command_bar = Some(CommandBar {
            input: input.clone(),
            navigation_query: None,
            target,
            selected: 0,
            site_search: None,
            site_search_profile: None,
            remote: Vec::new(),
            remote_generation: 0,
            remote_task: None,
            suggestions_privacy: false,
            material_backdrop: Rc::new(Cell::new(false)),
            material_alive: Rc::new(Cell::new(true)),
            shadow_cache: Default::default(),
            archive_reason: None,
            archive_limit: 8,
            rows_scroll: gpui::ScrollHandle::new(),
            icon_picker: crate::icon_catalog::Picker { catalog: if matches!(target, Target::Icon(crate::icons::IconTarget::Folder(_))) { crate::icon_catalog::Catalog::Symbols } else { self.last_icon_catalog }, folder: matches!(target, Target::Icon(crate::icons::IconTarget::Folder(_))), ..Default::default() },
            icon_anchor: if let Target::Icon(target) = target { self.icon_anchors.borrow().get(&target).copied() } else { None },
            overlay: None,
            _changed: changed,
            _empty_backspace: empty_backspace,
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
                // Attaching a key window as a child can make AppKit return
                // key status to its parent. Activate after attachment.
                let _ = overlay.update(cx, |_, window, _| window.activate_window());
            }
        });
    }

    fn reset_command_bar(&mut self, target: Target, text: &str, cx: &mut Context<Self>) {
        if !matches!(target, Target::Split { .. }) { self.cancel_split_draft(); }
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
        self.close_command_bar_with_focus(PopupFocus::Restore, cx);
    }

    pub(crate) fn suspend_popup_focus_for_utility(&mut self) {
        self.popup_focus_on_activation |= self.command_bar.is_some() || self.tab_switcher.is_some() || self.popup_focus_pending > 0;
        self.invalidate_sidebar_focus();
        self.invalidate_navigation_focus();
    }

    pub(crate) fn close_command_bar_with_focus(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        self.cancel_split_draft();
        let Some(bar) = self.command_bar.take() else { return; };
        bar.material_alive.set(false);
        self.clear_command_bar_backdrop();
        self.close_popup_overlay(bar.overlay, focus, true, cx);
        cx.notify();
    }

    pub(crate) fn popup_owns_focus(&self) -> bool {
        self.save_prompt_active || self.capture.is_some() || self.tab_switcher.is_some() || self.transient.as_ref().is_some_and(|state| state.picker.is_some()) || !self.page_content_visible() || self.theme_editor.is_some() || self.setup.is_some() || self.passwords.prompt.is_some() || self.user_profile_dialog.is_some() || self.site_controls.is_some() || self.new_menu.is_some() || self.command_bar.is_some() || self.tab_menu.is_some() || self.find_bar.is_some()
            || self.tab_rename.is_some() || self.tab_rename_pending.is_some()
    }

    /// Native activation may be asynchronous (GPUI's macOS backend queues
    /// makeKeyAndOrderFront). Minimizing must never enqueue that activation,
    /// including from an earlier dismissal in the same command dispatch.
    pub(crate) fn close_popups_for_window_operation(&mut self, minimize: bool, cx: &mut Context<Self>) {
        if minimize && self.max.preview.open.is_some() {self.popup_focus_on_activation=true;}
        self.retire_max_preview(cx);
        if minimize { self.abandon_sidebar_resize(); self.queue_floating_resize(cx); }
        let focus = if minimize {
            // A root with no popup already owns its responder intent (for
            // example History search or docked DevTools). Only replace it when
            // this operation actually takes over a popup's restoration.
            self.popup_focus_on_activation |= self.tab_switcher.is_some() || self.theme_editor.is_some() || self.setup.is_some() || self.passwords.prompt.is_some() || self.user_profile_dialog.is_some() || self.site_controls.is_some() || self.new_menu.is_some() || self.command_bar.is_some()
                || self.find_bar.is_some() || self.tab_menu.is_some()
                || self.popup_focus_pending > 0;
            self.popup_focus_generation = self.popup_focus_generation.wrapping_add(1);
            self.popup_focus_pending = 0;
            self.invalidate_navigation_focus();
            PopupFocus::OnActivation
        } else {
            PopupFocus::Restore
        };
        self.finish_recent_items(false, focus, cx);
        self.close_theme_editor_with_focus(focus, cx);
        self.close_transient_picker_with_focus(focus, cx);
        self.close_password_prompt(focus,cx);
        self.close_user_profile_dialog_with_focus(focus, cx);
        self.close_setup_with_focus(focus,cx);
        self.close_site_controls_with_focus(focus, cx);
        self.close_capture(focus,cx);
        self.close_new_menu_with_focus(focus, cx);
        self.close_command_bar_with_focus(focus, cx);
        self.close_find_with_focus(focus, cx);
        self.close_tab_menu_with_focus(focus, cx);
        if minimize {
            if let Some((_, generation)) = &mut self.sidebar_resize_focus_on_activation {
                *generation = self.popup_focus_generation;
            }
        }
    }

    pub(crate) fn restore_passive_overlay_focus(&self,cx:&mut Context<Self>){
        let generation=self.popup_focus_generation;let main=self.main_window;let browser=cx.weak_entity();
        cx.defer(move|cx|{
            if !crate::native::application_active() || cx.active_window().is_some_and(|active|active.window_id()!=main.window_id()){return;}
            if !browser.read_with(cx,|b,_|b.popup_focus_generation==generation&&!b.popup_owns_focus()).unwrap_or(false){return;}
            let _=main.update(cx,|_,w,_|w.activate_window());
            let _=browser.update(cx,|b,cx|{if b.popup_focus_generation==generation&&!b.popup_owns_focus(){if let Some(pages)=&b.pages {pages.borrow_mut().request_page_focus();}cx.notify();}});
        });
    }

    pub(crate) fn close_popup_overlay(&mut self, overlay: Option<WindowHandle<Overlay>>, policy: PopupFocus, sync_pages: bool, cx: &mut Context<Self>) {
        let generation = self.popup_focus_generation;
        if policy == PopupFocus::Restore { self.popup_focus_pending += 1; }
        let main = self.main_window;
        let focus = self.focus_handle.clone();
        let browser = cx.entity().downgrade();
        // Deferred: this may run inside the overlay window's own event.
        cx.defer(move |cx| {
            if let Some(overlay) = overlay {
                let _ = overlay.update(cx, |_, window, _| window.remove_window());
            }
            if policy == PopupFocus::OnActivation { return; }
            let restore = browser.update(cx, |this, _| {
                if this.popup_focus_generation != generation { return false; }
                this.popup_focus_pending -= 1;
                !this.popup_owns_focus() || (!this.page_content_visible() && this.sidebar_root_may_take_focus())
            }).unwrap_or(false);
            if !restore { return; }
            let _ = main.update(cx, |_, window, cx| {
                window.activate_window();
                let _ = browser.update(cx, |this, cx| {
                    // Reconcile external edits before restoring a retained
                    // native or GPUI preview responder, even before repaint.
                    this.refresh_profile_projection();
                    if this.popup_focus_generation != generation || !this.sidebar_root_may_take_focus() { return; }
                    if !this.page_content_visible() {
                        this.sync_navigation_visibility();
                        this.remember_library_preview_focus(window, cx);
                        this.focus_sidebar_page(window, cx, None);
                    } else { focus.focus(window); }
                });
            });
            let _ = browser.update(cx, |this, _| {
                // Another host may have reassigned this Space while removal
                // waited in the foreground queue. Never revive an old context
                // from this window's previously painted projection.
                this.refresh_profile_projection();
                if this.popup_focus_generation != generation || this.popup_owns_focus() { return; }
                if let Some(pages) = &this.pages {
                    let mut pages = pages.borrow_mut();
                    pages.reconcile_inventory(&this.browser);
                    if sync_pages { pages.sync(&this.browser, this.page_content_visible()); }
                    if this.page_content_visible() { pages.focus_shown(); }
                }
            });
        });
    }

    pub fn suggestions(&self, cx: &App) -> Vec<Suggestion> {
        let Some(bar) = &self.command_bar else {
            return Vec::new();
        };
        let text = bar.query(cx);
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
        if matches!(bar.target, Target::NewFolder { .. } | Target::RenameFolder(_)) {
            return if text.trim().is_empty() { Vec::new() } else { vec![Suggestion::SaveFolderName { name: text.trim().to_string() }] };
        }
        if matches!(bar.target, Target::MoveToFolder(_) | Target::MoveFolder(_)) { return self.folder_destinations(bar.target, text); }
        if matches!(bar.target, Target::Icon(_)) {
            return bar.icon_picker.entries(text).into_iter().map(|entry| Suggestion::SaveIcon { icon: Some(entry.value) }).collect();
        }
        if matches!(bar.target, Target::RenameTab(_)) {
            return if text.trim().is_empty() { Vec::new() } else { vec![Suggestion::SaveTabName { name: text.trim().to_string() }] };
        }
        if matches!(bar.target, Target::NewSpace | Target::RenameSpace(_)) {
            return if text.trim().is_empty() { Vec::new() } else { vec![Suggestion::SaveSpace { name: text.trim().to_string() }] };
        }
        let query = text.trim().to_lowercase();
        let mut suggestions = Vec::new();
        let chatgpt_enabled = self.profile.handle.read_settings(|b| b.settings.max.enabled(echo_ai::MaxFeature::ChatGptCommandBar));
        let chatgpt_keyword = chatgpt_enabled && query == "chatgpt";
        if let Some(site) = bar.site_search {
            if matches!(site, omnibox::SiteSearch::Configured(_)) && bar.site_search_profile != Some(self.browser.active_user_profile_id()) { return Vec::new(); }
            if site == omnibox::SiteSearch::ChatGpt && !chatgpt_enabled { return Vec::new(); }
            if !query.is_empty() { return site.search(text.trim(), &self.browser.active_user_profile().settings.search).map(Suggestion::Load).into_iter().collect(); }
            if site != omnibox::SiteSearch::ChatGpt { return Vec::new(); }
            let mut rows = vec![Suggestion::Load(Resolved::Url("https://chatgpt.com/".into())),
                Suggestion::Load(self.browser.active_user_profile().settings.search.search("ChatGPT"))];
            rows.extend(self.browser.active_user_profile().history.iter()
                .filter(|entry| entry.title.to_lowercase().contains("chatgpt") || entry.url.contains("chatgpt.com") || entry.url.contains("chat.openai.com"))
                .take(3).map(|entry| Suggestion::History {title:entry.title.clone(),url:entry.url.clone()}));
            return rows;
        }
        if chatgpt_keyword { suggestions.push(Suggestion::Load(Resolved::Url("https://chatgpt.com/".into()))); }
        if let Some(answer) = omnibox::calculator::answer(text) {
            suggestions.push(Suggestion::Calculator { answer });
        }
        // Commands are explicit suggestions: never interpret arbitrary prose as an action.
        if !query.is_empty() {
            let active = self.browser.active_tab();
            let pinned = active.is_some_and(|tab| tab.pinned_url.is_some());
            let favorite = active.is_some_and(|tab| self.browser.is_favorite(tab.id));
            let modified_pin = active.is_some_and(|tab| tab.pinned_url.as_ref().is_some_and(|original| *original != tab.url));
            let commands = [
                (Command::RevealData, "Reveal Echo Data", "reveal echo data|reveal browser data|open data folder", true),
                (Command::CopyBrowserInfo, "Copy Echo Info", "copy echo info|copy browser info|copy info|diagnostic info", true),
                (Command::AutomaticAppearance, "Switch to Automatic Appearance", "switch to automatic appearance|automatic mode", true),
                (Command::LightAppearance, "Switch to Light Mode", "switch to light mode|light mode|light appearance", true),
                (Command::DarkAppearance, "Switch to Dark Mode", "switch to dark mode|dark mode|dark appearance|night mode", true),
                (Command::NewBoost, "New Boost", "new boost|customize page", true),
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
                (Command::OpenLibrary, "Open Library", "open library|view library|library", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ManageSpaces, "Manage Spaces", "manage spaces|space management|organize|toggle space manager|view spaces", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ViewMedia, "View Media", "view media|screenshots|captures|videos|media", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ClearCookiesAndRefresh, "Clear Cookies and Refresh", "clear cookies and refresh|clear cookies", self.can_clear_active_site()),
                (Command::ClearCacheAndRefresh, "Clear Cache and Refresh", "clear cache and refresh|clear cache", self.can_clear_active_site()),
                (Command::ViewBoosts, "View Boosts", "view boosts|boosts", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ViewDownloads, "View Downloads", "view downloads|downloads", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ViewArchive, "View Archive", "view archive|open archive|search archived tabs|archive", true),
                (Command::RequestClearArchive, "Clear Archive…", "clear archive|delete archived tabs", !self.browser.archive.is_empty()),
                (Command::CopyUrl, "Copy URL", "copy url|copy link|copy clean link", active.is_some()),
                (Command::OpenTaskManager, "Open Task Manager", "open task manager|process manager|task manager", true),
                (Command::CapturePage, "Capture", "capture|capture page|take screenshot", self.capture_available()),
                (Command::CopyUrlAsQuote, "Copy Current URL as Quote", "copy current url as quote|copy url as quote|copy quote as url", active.is_some()),
                (Command::CopyUrlAsMarkdown, "Copy URL as Markdown", "copy url as markdown|copy link as markdown", active.is_some()),
                (Command::Settings, "Settings…", "settings|preferences", true),
                (Command::SearchSettings, "Search Settings", "search settings|manage search engines|site search", true),
                (Command::AirTrafficControl, "Air Traffic Control", "air traffic control|link routing|routes", true),
                (Command::FindPage, "Find in Page…", "find in page|search page", active.is_some()),
                (Command::NewProfile(self.browser.space().id), "New Profile", "new profile|create profile|add profile", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ChangeProfile(self.browser.space().id), "Change Profile", "change profile|set profile|switch profile", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::NewSpace, "New Space…", "new space|create space|add space", true),
                (Command::BlankWindow, "Blank Window", "blank window|new blank window", true),
                (Command::NewLittleWindow, "New Little Echo Window", "new little echo window|new little window|little arc|quick lookup", true),
                (Command::NewWindow, "New Window", "new window|open new window|create window|copy window|duplicate window", true),
                (Command::NewIncognitoWindow, "New Incognito Window", "new incognito window|incognito|private window", true),
                (Command::RenameSpace, "Rename Space…", "rename space|change space name", true),
                (Command::ToggleSpaceHeader, if self.browser.space().show_header { "Hide Space Header" } else { "Show Space Header" }, if self.browser.space().show_header { "hide space header|toggle space header" } else { "show space header|toggle space header" }, true),
                (Command::ToggleFavoritesOrdering, if self.browser.active_user_profile().include_favorites_in_ordering { "Exclude Favorites from Ordering" } else { "Include Favorites in Ordering" }, if self.browser.active_user_profile().include_favorites_in_ordering { "exclude favorites from ordering|exclude favorites in ordering" } else { "include favorites in ordering" }, true),
                (Command::RevealTab, "Reveal Tab in Sidebar", "reveal tab in sidebar|reveal tab|show tab in sidebar", active.is_some()),
                (Command::ToggleSidebar, if self.sidebar_visible { "Hide Sidebar" } else { "Show Sidebar" }, if self.sidebar_visible { "toggle sidebar|hide sidebar|sidebar" } else { "toggle sidebar|show sidebar|sidebar" }, true),
                (Command::TogglePin, if pinned { "Unpin Tab" } else { "Pin Tab" }, if pinned { "unpin tab|move to today" } else { "pin tab|move to pinned" }, active.is_some() && !favorite),
                (Command::ToggleFavorite, if favorite { "Unfavorite Tab" } else { "Favorite Tab" }, if favorite { "unfavorite tab|remove from favorites|unfavorite current tab" } else { "favorite tab|favorite current tab|pin to top apps|add to favorites" }, active.is_some()),
                (Command::ToggleMute, if active.is_some_and(|tab| tab.muted) { "Unmute Tab" } else { "Mute Tab" }, if active.is_some_and(|tab| tab.muted) { "unmute tab|unmute current tab" } else { "mute tab|mute current tab" }, active.is_some_and(|tab| !omnibox::is_internal_url(&tab.url))),
                (Command::MuteAllTabs, "Mute All Tabs", "mute all tabs|mute all", self.browser.all_tabs().any(|tab| !tab.muted && !omnibox::is_internal_url(&tab.url))),
                (Command::UnmuteAllTabs, "Unmute All Tabs", "unmute all tabs|unmute all", self.browser.all_tabs().any(|tab| tab.muted && !omnibox::is_internal_url(&tab.url))),
                (Command::TogglePinnedSection, if self.browser.space().pinned_collapsed { "Expand Pinned Section" } else { "Collapse Pinned Section" }, if self.browser.space().pinned_collapsed { "expand pinned section|toggle pinned section" } else { "collapse pinned section|toggle pinned section" }, !self.browser.space().pinned_items.is_empty()),
                (Command::CloseTab, if pinned { "Close Tab" } else { "Archive Tab" }, "close tab|archive tab", active.is_some()),
                (Command::ClearToday, "Clear Today", "clear unpinned tabs|clear today|archive background today tabs", !self.browser.space().today.is_empty()),
                (Command::RestoreTab, "Restore Last Closed Tab", "restore last closed tab|reopen closed tab|unarchive tab", !self.browser.archive.is_empty()),
                (Command::PrintPage, "Print Page", "print|print page", self.print_available()),
                (Command::SharePage, "Share URL", "share url|share page|share", self.can_share_page()),
                (Command::SavePage, "Save Page As", "save page as|save page|save file", self.save_page_available()),
                (Command::Reload, "Refresh the Page", "refresh page|reload tab", active.is_some()),
                (Command::ForceReload, "Force Refresh the Page", "force refresh page|reload without cache|hard reload", active.is_some()),
                (Command::StopLoading, "Stop Loading", "stop loading|stop page", active.is_some()),
                (Command::ViewSource, "View Source", "view source|page source|show source", self.current_source_url().is_some()),
                (Command::ToggleDeveloperMode, "Toggle Developer Mode", "developer mode|turn on developer mode|turn off developer mode", self.developer_site().is_some()),
                (Command::DevTools(crate::devtools::Command::Toggle), "Developer Tools", "developer tools|devtools", self.devtools_available()),
                (Command::DevTools(crate::devtools::Command::Elements), "Inspect Elements", "inspect elements|element inspector", self.devtools_available()),
                (Command::DevTools(crate::devtools::Command::Console), "JavaScript Console", "javascript console|console", self.devtools_available()),
                (Command::DevTools(crate::devtools::Command::Network), "Network Inspector", "network inspector|network", self.devtools_available()),
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
                if !available || !self.blank_allows_command(command) { return None; }
                let score = command_match_score(&query, label).into_iter().chain(command_match_score(&query, names)).min()?;
                Some((score, Suggestion::Action { command, label: label.into() }))
            }).collect();
            for space in self.browser.spaces.iter().filter(|_| !self.browser.is_blank()) {
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
                        if space.user_profile_id != self.browser.active_user_profile_id() || (space.id == self.browser.space().id && Some(section) == source) { continue; }
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
            matches.retain(|(_, suggestion)| match suggestion { Suggestion::Action { command, .. } => self.blank_allows_command(*command), _ => true });
            matches.sort_by_key(|(score, _)| *score);
            suggestions.extend(matches.into_iter().take(5).map(|(_, suggestion)| suggestion));
        }
        let preferences = &self.browser.active_user_profile().settings.search;
        suggestions.extend(preferences.resolve(text).map(Suggestion::Load));
        if bar.target == Target::NewTab {
            // Current Space first, then every other Space, keeping duplicate URLs distinct.
            let current = self.browser.active_space;
            let spaces = std::iter::once(&self.browser.spaces[current]).chain(
                self.browser.spaces.iter().enumerate().filter(|(index, _)| *index != current).map(|(_, space)| space)
            );
            let favorites = self.browser.active_user_profile().favorites.iter().map(|tab| ("Favorites", tab));
            let mut seen = std::collections::HashSet::new();
            suggestions.extend(favorites.chain(spaces.filter(|space| space.user_profile_id == self.browser.active_user_profile_id()).flat_map(|space| {
                space.pinned.iter().chain(&space.today).map(move |tab| (space.name.as_str(), tab))
            }))
            .filter(|(_, tab)| Some(tab.id) != self.browser.space().active_tab || self.browser.split_for(tab.id).is_some())
            .filter(|(_, tab)| query.is_empty() || self.browser.item_title(tab.id).to_lowercase().contains(&query) || tab.title.to_lowercase().contains(&query) || tab.url.to_lowercase().contains(&query)
                || tab.page_title.as_ref().is_some_and(|title| title.to_lowercase().contains(&query)))
                .filter(|(_, tab)| seen.insert(self.browser.item_anchor(tab.id)))
                .take(if query.is_empty() { 5 } else { MAX_TAB_SUGGESTIONS })
                .map(|(space, tab)| Suggestion::SwitchToTab {
                    id: tab.id, title: self.browser.item_title(tab.id), url: tab.url.clone(), space: if self.browser.split_for(tab.id).is_some() { format!("{space} · Split View") } else { space.to_string() },
                }));
        }
        if !query.is_empty() {
            let open: std::collections::HashSet<_> = self.browser.all_tabs().filter(|tab| self.browser.user_profile_id_for_tab(tab.id) == Some(self.browser.active_user_profile_id())).map(|tab| tab.url.as_str()).collect();
            suggestions.extend(self.browser.active_user_profile().history.iter()
                .filter(|entry| self.browser.active_tab().is_some_and(|tab| tab.url == entry.url) || !open.contains(entry.url.as_str()))
                .filter(|entry| entry.title.to_lowercase().contains(&query) || entry.url.to_lowercase().contains(&query))
                .take(MAX_HISTORY_SUGGESTIONS)
                .map(|entry| Suggestion::History { title: entry.title.clone(), url: entry.url.clone() }));
        }
        if preferences.suggestions && preferences.suggestion_engine().is_some() && !self.browser.is_private() {
            suggestions.extend(bar.remote.iter().map(|query| Suggestion::Load(preferences.search(query))));
        }
        let ask_chatgpt = chatgpt_enabled && !chatgpt_keyword
            && bar.target == Target::NewTab
            && matches!(preferences.resolve(text), Some(Resolved::Search { .. }))
            && !suggestions.iter().any(|s| matches!(s, Suggestion::Action { .. } | Suggestion::Calculator { .. }));
        suggestions.truncate(MAX_COMMAND_SUGGESTIONS - usize::from(ask_chatgpt));
        if ask_chatgpt { suggestions.push(Suggestion::AskChatGpt { query: text.trim().to_owned() }); }
        suggestions
    }

    fn select(&mut self, delta: isize, cx: &mut Context<Self>) {
        let suggestions = self.suggestions(cx);
        let count = suggestions.len() as isize;
        if let Some(bar) = &mut self.command_bar {
            if count > 0 {
                bar.selected = (bar.selected as isize + delta).rem_euclid(count) as usize;
                bar.rows_scroll.scroll_to_item(bar.selected);
                if matches!(bar.target, Target::NewTab | Target::Tab(_)) && bar.site_search.is_none() {
                    let query = bar.navigation_query.get_or_insert_with(|| bar.input.read(cx).text().to_owned());
                    let preview = match &suggestions[bar.selected] {
                        Suggestion::SwitchToTab { url, .. } | Suggestion::History { url, .. }
                        | Suggestion::Load(Resolved::Url(url)) => display_url(url),
                        Suggestion::Load(Resolved::Search { query, .. }) => query.clone(),
                        _ => query.clone(),
                    };
                    bar.input.update(cx, |input, cx| input.preview_navigation_text(&preview, cx));
                }
                cx.notify();
            }
        }
    }

    fn new_command_tab(&mut self, url: &str, background: bool, cx: &mut Context<Self>) -> TabId {
        let id = self.edit_browser(|browser| {
            let mut selection = browser.window_selection();
            let id = browser.new_tab(url, SystemTime::now());
            if background { browser.edit_in_window(&mut selection, |_| {}); }
            id
        });
        if background { self.show_background_tab(id, cx); }
        id
    }

    pub fn confirm(&mut self, index: usize, cx: &mut Context<Self>) { self.confirm_with_background(index, false, cx); }

    fn confirm_with_background(&mut self, index: usize, background: bool, cx: &mut Context<Self>) {
        let Some(target) = self.command_bar.as_ref().map(|bar| bar.target) else {
            return;
        };
        let Some(suggestion) = self.suggestions(cx).into_iter().nth(index) else {
            return;
        };
        let now = SystemTime::now();
        match suggestion {
            Suggestion::AskChatGpt { query } => {
                if target != Target::NewTab || !self.profile.handle.read_settings(|b| b.settings.max.enabled(echo_ai::MaxFeature::ChatGptCommandBar)) { return; }
                let resolved = omnibox::SiteSearch::ChatGpt.search(&query, &self.browser.active_user_profile().settings.search).expect("ChatGPT has a fixed search endpoint");
                if !self.page_content_visible() { self.sidebar_navigation.controller.select(&self.browser, crate::sidebar_pages::SidebarPage::Space(self.browser.space().id)); }
                let id = self.new_command_tab(resolved.url(), background, cx);
                self.edit_browser(|b| b.set_title(id, query));
            }
            Suggestion::Calculator { answer } => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(answer));
                self.close_command_bar(cx);
                return;
            }
            Suggestion::Load(resolved) => {
                if !self.page_content_visible() { self.sidebar_navigation.controller.select(&self.browser, crate::sidebar_pages::SidebarPage::Space(self.browser.space().id)); }
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
                    Target::Split { .. } => {
                        if omnibox::is_internal_url(resolved.url()) { return; }
                        let Some(draft) = self.split_draft.take() else { return; };
                        let id = draft.pane;
                        self.edit_browser(|browser| browser.navigate(id, resolved.url()));
                        id
                    }
                    Target::NewTab => self.new_command_tab(resolved.url(), background, cx),
                    Target::Tab(id) => {
                        self.edit_browser(|browser| browser.navigate(id, resolved.url()));
                        id
                    }
                    Target::NewSpace | Target::RenameSpace(_) | Target::RenameTab(_) | Target::Icon(_) | Target::NewFolder { .. } | Target::RenameFolder(_) | Target::MoveToFolder(_) | Target::MoveFolder(_) | Target::Archive(_) | Target::ConfirmClearArchive => return,
                };
                self.edit_browser(|browser| browser.set_title(id, title));
            }
            Suggestion::History { title, url } => {
                if !self.page_content_visible() { self.sidebar_navigation.controller.select(&self.browser, crate::sidebar_pages::SidebarPage::Space(self.browser.space().id)); }
                let id = match target {
                    Target::NewTab => self.new_command_tab(&url, background, cx),
                    Target::Tab(id) => { self.edit_browser(|browser| browser.navigate(id, &url)); id }
                    Target::Split { .. } => {
                        let Some(draft) = self.split_draft.take() else { return; };
                        self.edit_browser(|browser| browser.navigate(draft.pane, &url));
                        draft.pane
                    }
                    _ => return,
                };
                self.edit_browser(|browser| browser.set_title(id, title));
            }
            Suggestion::SplitTab { id, .. } => {
                if let Target::Split { anchor, placement } = target {
                    let Some(draft) = self.split_draft.take() else { return; };
                    let id = if self.browser.is_favorite(id) || self.browser.spaces.iter().any(|space| space.pinned.iter().any(|tab| tab.id == id)) {
                        self.edit_browser(|browser| browser.copy_item_to_today(id, now)).expect("existing suggested tab")
                    } else { id };
                    self.edit_browser(|browser| browser.discard_split_draft(draft.pane, None, now));
                    self.edit_browser(|browser| browser.add_split(anchor, id, placement, now)).expect("validated suggested pane");
                } else { return; }
            }
            Suggestion::SwitchToTab { id, .. } => {
                if !self.page_content_visible() { self.sidebar_navigation.controller.select(&self.browser, crate::sidebar_pages::SidebarPage::Space(self.browser.space().id)); }
                if self.browser.is_favorite(id) {
                    self.edit_browser(|browser| browser.activate_tab(id, now));
                } else if let Some(index) = self.browser.spaces.iter().position(|space| space.pinned.iter().chain(&space.today).any(|tab| tab.id == id)) {
                    self.edit_browser(|browser| { browser.switch_space(index); browser.activate_tab(id, now); });
                    self.settle_pager(cx);
                }
            }
            Suggestion::Action { command, .. } => {
                self.run_command(command, cx);
                if self.command_bar.as_ref().is_some_and(|bar| bar.target != target) { return; }
            }
            Suggestion::ArchivedTab { id, .. } => {
                if self.edit_browser(|browser| browser.restore_archived(id, now)).is_some() {
                    self.settle_pager(cx);
                    self.show_reveal_toast(format!("Tab restored to {}. Click to view.", self.browser.space().name), id, cx);
                }
            }
            Suggestion::MoreArchive { .. } => {
                if let Some(bar) = &mut self.command_bar { bar.archive_limit = bar.archive_limit.saturating_add(8); }
                cx.notify();
                return;
            }
            Suggestion::ConfirmClearArchive => {
                self.edit_browser(|browser| { let profile = browser.active_user_profile_id(); browser.archive.retain(|entry| entry.user_profile_id != profile); });
                self.show_toast("Archive cleared".into(), cx);
            }
            Suggestion::SaveFolderName { name } => {
                if !self.save_folder_name(target, &name, cx) { return; }
            }
            Suggestion::FolderDestination { space, parent, .. } => {
                if !self.confirm_folder_destination(target, space, parent, cx) { return; }
            }
            Suggestion::SaveIcon { icon } => {
                match target {
                    Target::Icon(crate::icons::IconTarget::Folder(id)) => { self.edit_browser(|browser| browser.set_folder_icon(id, icon.as_deref())); }
                    Target::Icon(crate::icons::IconTarget::Tab(id)) => { self.edit_browser(|browser| browser.set_item_icon(id, icon.as_deref())); }
                    Target::Icon(crate::icons::IconTarget::Space(id)) => { self.edit_browser(|browser| browser.set_space_icon(id, icon.as_deref())); }
                    _ => return,
                }
            }
            Suggestion::SaveTabName { name } => {
                if let Target::RenameTab(id) = target { self.edit_browser(|browser| browser.rename_tab(id, &name)); }
            }
            Suggestion::SaveSpace { name } => match target {
                Target::NewSpace => { self.begin_space_creation(self.browser.space().id, cx); return; }
                Target::RenameSpace(id) => { self.edit_browser(|browser| browser.rename_space(id, &name)); }
                _ => return,
            },
        }
        self.close_command_bar(cx);
        if self.boost_creation_pending && self.browser.active_tab().is_some_and(|tab|tab.url.starts_with("http://")||tab.url.starts_with("https://")) { self.boost_creation_pending=false; self.new_boost(cx); }
    }

    pub(crate) fn run_command(&mut self, command: Command, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() { return; }
        if !self.blank_allows_command(command) { return; }
        match command {
            Command::NewBoost => self.new_boost(cx),
            Command::NewFolder => self.edit_new_folder(None, None, cx),
            Command::NewFolderWithTab => { if let Some(id) = self.browser.space().active_tab { self.edit_new_folder(None, Some(id), cx); } }
            Command::PinToFolder(folder) => {
                if let (Some(tab), Some(space)) = (self.browser.space().active_tab, self.browser.folder_space(folder)) {
                    self.confirm_folder_destination(Target::MoveToFolder(tab), space, Some(folder), cx);
                }
            }
            Command::MoveToFolder => { if let Some(id) = self.browser.space().active_tab { self.edit_folder_destination(Target::MoveToFolder(id), cx); } }
            Command::CollapseAllFolders => { self.edit_browser(|browser| browser.set_all_folders_collapsed(browser.space().id, true)); }
            Command::ExpandAllFolders => { self.edit_browser(|browser| browser.set_all_folders_collapsed(browser.space().id, false)); }
            Command::RenameFolder(id) => self.edit_folder_name(id, cx),
            Command::ExpandFolder(id) => self.expand_folder(id, cx),
            Command::AddSplit(placement) => self.edit_split(placement, cx),
            Command::CloseSplitPane => { if let Some(id) = self.browser.space().active_tab { self.close_split_pane(id, cx); } }
            Command::SplitAxis(axis) => { if let Some(id) = self.browser.space().active_tab { self.edit_browser(|browser| browser.set_split_axis(id, axis)); } }
            Command::SeparateSplit => { if let Some(id) = self.browser.space().active_tab { if !self.edit_browser(|browser| browser.separate_split(id)) { self.show_toast("You can have up to 12 Favorites".into(), cx); } } }
            Command::SeparatePane => { if let Some(id) = self.browser.space().active_tab { if !self.edit_browser(|browser| browser.separate_split_pane(id)) { self.show_toast("You can have up to 12 Favorites".into(), cx); } } }
            Command::ExpandCurrentSplit => self.expand_current_split(cx),
            Command::ViewSource => self.view_source(cx),
            Command::DevTools(command) => self.run_devtools(command, cx),
            Command::ToggleDeveloperMode => self.toggle_developer_mode(cx),
            Command::FocusPane(index) => { self.edit_browser(|browser| browser.focus_split_pane(index, SystemTime::now())); }
            Command::CyclePane(delta) => { self.edit_browser(|browser| browser.cycle_split_pane(delta, SystemTime::now())); }
            Command::MovePane(delta) => { if let Some(id) = self.browser.space().active_tab { self.edit_browser(|browser| browser.move_split_pane(id, delta)); } }
            Command::DuplicateTab => { if let Some(id) = self.browser.space().active_tab { self.duplicate_tab(id, cx); } }
            Command::RenameTab => {
                if let Some(tab) = self.browser.active_tab() {
                    let (id, name) = (tab.id, self.browser.item_title(tab.id));
                    if self.can_rename_tab_inline(id) { self.rename_tab_inline(id, cx); }
                    else if let Some(bar) = &mut self.command_bar {
                        bar.target = Target::RenameTab(id);
                        bar.selected = 0;
                        bar.input.update(cx, |input, cx| { input.set_placeholder("Rename Tab…", cx); input.set_text(name, cx); });
                    }
                }
            }
            Command::CustomizeTabIcon => { if let Some(id) = self.browser.space().active_tab { self.edit_icon(crate::icons::IconTarget::Tab(id), cx); } }
            Command::CustomizeSpaceIcon => self.edit_icon(crate::icons::IconTarget::Space(self.browser.space().id), cx),
            Command::ResetNameAndIcon => { if let Some(id) = self.browser.space().active_tab { self.edit_browser(|browser| browser.reset_item_customization(id)); } }
            Command::ResetTabName => {
                if let Some(id) = self.browser.space().active_tab { self.edit_browser(|browser| browser.reset_tab_name(id)); }
            }
            Command::ResetSpace => {
                let tabs = self.edit_browser(|browser| browser.reset_space());
                if let Some(pages) = &self.pages {
                    let mut pages = pages.borrow_mut();
                    for tab in &tabs { pages.discard(*tab); }
                }
                self.show_toast(format!("Reset {} pinned {}", tabs.len(), if tabs.len() == 1 { "tab" } else { "tabs" }), cx);
            }
            Command::ResetPinnedTab => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.edit_browser(|browser| browser.reset_pinned_tab(id)) { self.show_toast("Tab reset to its pinned URL".into(), cx); }
                }
            }
            Command::ReplacePinnedUrl => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.edit_browser(|browser| browser.replace_pinned_url(id)) { self.show_toast("Pinned URL updated".into(), cx); }
                }
            }
            Command::ViewHistory => self.history_from_command(cx),
            Command::CopyBrowserInfo => self.copy_browser_info(cx),
            Command::RevealData => self.open_menu_utility(crate::diagnostics::reveal_data, cx),
            Command::OpenLibrary => self.select_sidebar_page(crate::sidebar_pages::SidebarPage::Library, cx),
            Command::ManageSpaces => self.open_library_spaces(cx),
            Command::ViewDownloads => self.open_library_downloads(cx),
            Command::ViewBoosts => self.open_library_boosts(cx),
            Command::ViewMedia => self.open_library_media(cx),
            Command::ClearCookiesAndRefresh => self.clear_active_site(engine::ClearSiteData::Cookies, cx),
            Command::ClearCacheAndRefresh => self.clear_active_site(engine::ClearSiteData::Cache, cx),
            Command::ViewArchive => self.edit_archive_view(None, cx),
            Command::RequestClearArchive => self.confirm_clear_archive(cx),
            Command::CopyUrl => self.copy_url(false, cx),
            Command::CopyUrlAsMarkdown => self.copy_url(true, cx),
            Command::CopyUrlAsQuote => self.copy_url_as_quote(cx),
            Command::CapturePage => self.capture_page(cx),
            Command::OpenTaskManager => self.open_menu_utility(crate::task_manager::open, cx),
            Command::Settings => self.open_settings(cx),
            Command::AirTrafficControl => self.open_air_traffic_control(cx),
            Command::SearchSettings => self.search_settings_from_command(cx),
            Command::FindPage => self.find_from_command(cx),
            Command::NewSpace => self.begin_space_creation(self.browser.space().id, cx),
            Command::NewProfile(space) => self.open_user_profile_creation(crate::user_profile_dialog::CreationSource::File(space), cx),
            Command::ChangeProfile(space) => self.open_user_profile_switch(space, cx),
            Command::BlankWindow => self.blank_window(cx),
            Command::NewWindow => self.new_window(cx),
            Command::NewLittleWindow => self.new_little_window(cx),
            Command::NewIncognitoWindow => {
                self.close_command_bar(cx);
                cx.defer(crate::open_incognito_window);
            }
            Command::RenameSpace => self.edit_space_name(Some(self.browser.space().id), cx),
            Command::ToggleSpaceHeader => { self.edit_browser(|browser| browser.set_space_header(browser.space().id, !browser.space().show_header)); }
            Command::ToggleFavoritesOrdering => { self.edit_browser(|browser| browser.active_user_profile_mut().include_favorites_in_ordering = !browser.active_user_profile().include_favorites_in_ordering); }
            Command::RevealTab => self.reveal_active_tab(cx),
            Command::ToggleSidebar => self.toggle_sidebar(cx),
            Command::TogglePin => {
                if let Some(id) = self.browser.space().active_tab { self.toggle_tab_pin(id, cx); }
            }
            Command::ToggleFavorite => {
                if let Some(id) = self.browser.space().active_tab {
                    if self.browser.is_favorite(id) { self.edit_browser(|browser| browser.unfavorite_tab(id)); }
                    else if !self.edit_browser(|browser| browser.favorite_tab(id)) { self.show_toast("You can have up to 12 Favorites".into(), cx); }
                }
            }
            Command::ToggleMute => {
                if let Some(tab) = self.browser.active_tab() { self.set_tab_muted(tab.id, !tab.muted, cx); }
            }
            Command::MuteAllTabs => self.set_all_tabs_muted(true, cx),
            Command::UnmuteAllTabs => self.set_all_tabs_muted(false, cx),
            Command::TogglePinnedSection => self.toggle_pinned_section(self.browser.active_space, cx),
            Command::CloseTab => self.close_focused_tab(cx),
            Command::ArchiveRelative(relative) => { if let Some(id) = self.browser.space().active_tab { self.archive_relative(id, relative, cx); } }
            Command::ClearToday => { self.clear_today_tabs(cx); }
            Command::RestoreTab => {
                if self.edit_browser(|browser| { let id = browser.archive.iter().rev().find(|entry| entry.user_profile_id == browser.active_user_profile_id()).map(|entry| entry.tab.id)?; browser.restore_archived(id, SystemTime::now()) }).is_some() { self.settle_pager(cx); }
            }
            Command::PrintPage => self.print_page(cx),
            Command::SavePage => self.save_page(cx),
            Command::SharePage => self.share_page(cx),
            Command::AutomaticAppearance => self.set_website_appearance(browser_core::themes::ThemeAppearance::Automatic, cx),
            Command::LightAppearance => self.set_website_appearance(browser_core::themes::ThemeAppearance::Light, cx),
            Command::DarkAppearance => self.set_website_appearance(browser_core::themes::ThemeAppearance::Dark, cx),
            Command::Reload => self.with_active_page(|pages, tab| pages.reload(tab)),
            Command::ForceReload => self.with_active_page(|pages, tab| pages.force_reload(tab)),
            Command::StopLoading => self.with_active_page(|pages, tab| pages.stop_loading(tab)),
            Command::ZoomIn => self.change_page_zoom(1, cx),
            Command::ZoomOut => self.change_page_zoom(-1, cx),
            Command::ResetZoom => self.change_page_zoom(0, cx),
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
                    if !self.browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == self.browser.active_user_profile_id()) { return; }
                    if self.edit_browser(|browser| browser.move_tab(id, space, section)) {
                        if let Some(index) = self.browser.spaces.iter().position(|candidate| candidate.id == space) {
                            self.switch_space(index, cx);
                            self.edit_browser(|browser| browser.activate_tab(id, SystemTime::now()));
                            self.show_tab_move(id, cx);
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
        if self.browser.is_blank() { return; }
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

    pub(crate) fn render_overlay_shell(&self, content: AnyElement, focus:Option<&FocusHandle>, cx: &mut Context<Self>) -> AnyElement {
        div().size_full().when_some(focus,|root,focus|root.track_focus(focus)).key_context(if self.sidebar_resize.is_some() { "BrowserWindow SidebarResizing" } else { WINDOW_CONTEXT })
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| { if !this.capture_max_preview_key(event,cx) {this.capture_sidebar_resize_key(event, window, cx);} }))
            .on_action(cx.listener(|this, _: &crate::menus::BlankWindow, _, cx| this.blank_window(cx)))
            .when(self.peek_owner.is_some(), |this| this
                .on_action(cx.listener(|this, _: &crate::text_input::Undo, window, cx| this.edit_undo(false, window, cx)))
                .on_action(cx.listener(|this, _: &crate::text_input::Redo, window, cx| this.edit_undo(true, window, cx)))
                .on_action(cx.listener(Self::on_go_back))
                .on_action(cx.listener(Self::on_go_forward))
                .on_action(cx.listener(Self::on_reload))
                .on_action(cx.listener(Self::on_open_command_bar))
                .on_action(cx.listener(Self::on_edit_current_url))
                .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.change_page_zoom(1, cx)))
                .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.change_page_zoom(-1, cx)))
                .on_action(cx.listener(|this, _: &ResetZoom, _, cx| this.change_page_zoom(0, cx))))
            .on_action(cx.listener(|this, _: &crate::peek::ExitPeek, _, cx| this.exit_peek(cx)))
            .on_action(cx.listener(|this, _: &crate::transient::NewLittleWindow, _, cx| this.new_little_window(cx)))
            .on_action(cx.listener(|this, _: &crate::transient::OpenInSpace, _, cx| this.promote_transient(None, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::NewWindow, _, cx| this.new_window(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::Settings, _, cx| this.open_settings(cx)))
            .on_modifiers_changed(cx.listener(|this, event: &gpui::ModifiersChangedEvent, _, cx| {
                if this.tab_switcher.is_some() && !event.modifiers.control { this.finish_recent_items(true,PopupFocus::Restore,cx); }
            }))
            .on_action(cx.listener(|this, _: &crate::tab_switcher::NextRecentItem, window, cx| this.show_recent_items(false,window,cx)))
            .on_action(cx.listener(|this, _: &crate::tab_switcher::PreviousRecentItem, window, cx| this.show_recent_items(true,window,cx)))
            .on_action(cx.listener(|this, _: &crate::menus::About, _, cx| this.open_menu_utility(crate::about::open, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::CopyBrowserInfo, _, cx| this.copy_browser_info(cx)))
            .on_action(cx.listener(|this, action: &crate::standard_edit::Perform, window, cx| this.perform_standard_edit(action.0, window, cx)))
            .on_action(cx.listener(|this, _: &CapturePage, _, cx| this.capture_page(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::RevealData, _, cx| this.open_menu_utility(crate::diagnostics::reveal_data, cx)))
            .on_action(cx.listener(|this, _: &crate::task_manager::OpenTaskManager, _, cx| this.open_menu_utility(crate::task_manager::open, cx)))
            .on_action(cx.listener(|this, _: &crate::essential_shortcuts::EssentialShortcuts, _, cx| this.open_menu_utility(crate::essential_shortcuts::open, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ImportFromBrowser, _, cx| this.open_setup(false,cx)))
            .on_action(cx.listener(|this, _: &crate::menus::SetDefaultBrowser, _, cx| this.request_default_browser(cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::NewProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_creation(crate::user_profile_dialog::CreationSource::File(space), cx); }))
                .on_action(cx.listener(|this, _: &crate::menus::ChangeProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_switch(space, cx); })))
            .on_action(cx.listener(|this, _: &crate::menus::EditTheme, window, cx| { let space = this.browser.space().id; this.open_theme_editor(space, window, cx); }))
            .on_action(cx.listener(|this, _: &crate::menus::CloseWindow, _, cx| this.close_window(cx)))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                if this.peek_owner.is_some() { this.retain_closed_peek(cx); this.close_window(cx); }
                else if this.theme_editor.is_some() { this.close_theme_editor(cx); }
                else if this.setup.is_some() { this.close_setup(cx); }
                else if this.user_profile_dialog.is_some() { this.close_user_profile_dialog(cx); }
                else if this.command_bar.is_some() { this.close_command_bar(cx); }
                else { this.close_focused_tab(cx); }
            }))
            .on_action(cx.listener(|this, _: &ToggleToolbar, _, cx| this.toggle_toolbar(cx)))
            .on_action(cx.listener(|this, _: &ToggleDeveloperMode, _, cx| this.toggle_developer_mode(cx)))
            .on_action(cx.listener(|this, _: &NewBoost, _, cx|this.new_boost(cx)))
            .when(!self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &NewFolder, window, cx| this.open_command_action(Command::NewFolder, window, cx))))
            .on_action(cx.listener(|this, _: &CollapseAllFolders, _, cx| this.run_command(Command::CollapseAllFolders, cx)))
            .on_action(cx.listener(|this, _: &ExpandAllFolders, _, cx| this.run_command(Command::ExpandAllFolders, cx)))
            .on_action(cx.listener(|this, _: &CustomizeTabIcon, window, cx| this.open_command_action(Command::CustomizeTabIcon, window, cx)))
            .on_action(cx.listener(|this, _: &CustomizeSpaceIcon, window, cx| this.open_command_action(Command::CustomizeSpaceIcon, window, cx)))
            .on_action(cx.listener(|this, _: &ResetNameAndIcon, _, cx| this.run_command(Command::ResetNameAndIcon, cx)))
            .on_action(cx.listener(|this, _: &ToggleSpaceHeader, _, cx| this.run_command(Command::ToggleSpaceHeader, cx)))
            .on_action(cx.listener(|this, _: &ToggleFavoritesOrdering, _, cx| this.run_command(Command::ToggleFavoritesOrdering, cx)))
            .on_action(cx.listener(|this, _: &RevealTab, _, cx| this.reveal_active_tab(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::AutomaticAppearance, _, cx| this.set_website_appearance(browser_core::themes::ThemeAppearance::Automatic, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::LightAppearance, _, cx| this.set_website_appearance(browser_core::themes::ThemeAppearance::Light, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::DarkAppearance, _, cx| this.set_website_appearance(browser_core::themes::ThemeAppearance::Dark, cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::ViewMedia, _, cx| this.open_library_media(cx)))
                .on_action(cx.listener(|this, _: &crate::menus::ViewBoosts, _, cx| this.open_library_boosts(cx))))
            .when(self.can_clear_active_site(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::ClearCookiesAndRefresh, _, cx| this.clear_active_site(engine::ClearSiteData::Cookies, cx)))
                .on_action(cx.listener(|this, _: &crate::menus::ClearCacheAndRefresh, _, cx| this.clear_active_site(engine::ClearSiteData::Cache, cx))))
            .on_action(cx.listener(|this, _: &DuplicateTab, _, cx| this.run_command(Command::DuplicateTab, cx)))
            .on_action(cx.listener(|this, _: &AddSplit, window, cx| this.open_command_action(Command::AddSplit(None), window, cx)))
            .on_action(cx.listener(|this, _: &CloseSplitPane, _, cx| this.run_command(Command::CloseSplitPane, cx)))
            .on_action(cx.listener(|this, _: &ExpandCurrentSplit, _, cx| this.run_command(Command::ExpandCurrentSplit, cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &ViewLibrary, _, cx| this.run_command(Command::OpenLibrary, cx))).on_action(cx.listener(|this, _: &ViewDownloads, _, cx| this.run_command(Command::ViewDownloads, cx))).on_action(cx.listener(|this, _: &crate::menus::ManageSpaces, _, cx| this.run_command(Command::ManageSpaces, cx))))
            .on_action(cx.listener(|this, _: &crate::text_input::PasteAndMatchStyle, _, cx| this.paste_and_match_style(cx)))
            .when(self.save_page_available(), |this| this.on_action(cx.listener(|this, _: &SavePage, _, cx| this.save_page(cx))))
            .when(self.can_share_page(), |this| this.on_action(cx.listener(|this, _: &SharePage, _, cx| this.share_page(cx))))
            .when(self.print_available(), |this| this.on_action(cx.listener(|this, _: &PrintPage, _, cx| this.print_page(cx))))
            .when(self.current_source_url().is_some(), |this| this.on_action(cx.listener(|this, _: &ViewSource, _, cx| this.view_source(cx))))
            .when(self.devtools_available(), |this| this
                .on_action(cx.listener(|this, _: &DeveloperTools, _, cx| this.run_devtools(crate::devtools::Command::Toggle, cx)))
                .on_action(cx.listener(|this, _: &InspectElements, _, cx| this.run_devtools(crate::devtools::Command::Elements, cx)))
                .on_action(cx.listener(|this, _: &JavaScriptConsole, _, cx| this.run_devtools(crate::devtools::Command::Console, cx)))
                .on_action(cx.listener(|this, _: &NetworkInspector, _, cx| this.run_devtools(crate::devtools::Command::Network, cx))))
            .on_action(cx.listener(|this, _: &crate::menus::Minimize, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Minimize, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ZoomWindow, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Zoom, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ToggleFullscreen, _, cx| this.perform_window_operation(crate::menus::WindowOperation::ToggleFullscreen, cx)))
            .child(content).child(release_hover_on_exit()).into_any_element()
    }

    pub(crate) fn open_command_action(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if !self.blank_allows_command(command) { return; }
        if command == Command::NewSpace { self.begin_space_creation(self.browser.space().id, cx); return; }
        self.open_command_bar(Target::NewTab, "", window, cx);
        self.run_command(command, cx);
    }

    // Window actions.

    fn current_source_url(&self) -> Option<String> {
        let url = &self.browser.active_tab()?.url;
        let (scheme, _) = url.split_once(':')?;
        ["http", "https", "file", "data"].iter().any(|allowed| scheme.eq_ignore_ascii_case(allowed))
            .then(|| format!("view-source:{url}"))
    }

    pub(crate) fn view_source(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.current_source_url() else { return; };
        self.close_command_bar(cx);
        self.close_find(cx);
        let id = self.edit_browser(|browser| browser.new_tab(&url, SystemTime::now()));
        self.edit_browser(|browser| browser.set_title(id, url));
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }

    fn on_open_command_bar(&mut self, _: &OpenCommandBar, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_bar.as_ref().is_some_and(|bar| matches!(bar.target, Target::Icon(_))) { self.close_command_bar(cx); }
        if !self.toggle_command_target(Target::NewTab, "", cx) {
            self.open_command_bar(Target::NewTab, "", window, cx);
        }
    }

    fn on_edit_current_url(&mut self, _: &EditCurrentUrl, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_bar.as_ref().is_some_and(|bar| matches!(bar.target, Target::Icon(_))) { self.close_command_bar(cx); }
        let (target, text) = self.browser.active_tab().map(|tab| (Target::Tab(tab.id), tab.url.clone()))
            .unwrap_or((Target::NewTab, String::new()));
        if !self.toggle_command_target(target, &text, cx) {
            self.open_command_bar(target, &text, window, cx);
        }
    }

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        if self.transient.is_some() { self.close_window(cx); return; }
        if self.setup.is_some() { self.close_setup(cx); return; }
        if self.user_profile_dialog.is_some() { self.close_user_profile_dialog(cx); return; }
        let editing_name = self.tab_rename.is_some();
        self.close_focused_tab(cx);
        if editing_name { self.focus_handle.focus(window); }
    }

    fn close_focused_tab(&mut self, cx: &mut Context<Self>) {
        if !self.page_content_visible() { return; }
        if self.split_draft.is_some() { self.close_command_bar(cx); return; }
        if let Some(id) = self.browser.space().active_tab {
            if self.browser.split_for(id).is_some() { self.close_split_pane(id, cx); }
            else { self.close_tab(id, cx); }
        }
    }

    /// Archives a Today tab or unloads a pinned one, as in Arc.
    pub(crate) fn close_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.close_sidebar_item(id, false, cx);
    }

    pub(crate) fn remove_pin(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.close_sidebar_item(id, true, cx);
    }

    fn close_sidebar_item(&mut self, id: TabId, remove_pin: bool, cx: &mut Context<Self>) {
        self.finish_tab_rename(true, cx);
        let related = self.browser.related_tabs(id);
        let title = self.browser.item_title(id);
        let archived_before = self.browser.archive.len();
        if self.edit_browser(|browser| if remove_pin { browser.remove_pin(id, SystemTime::now()) } else { browser.close_tab(id, SystemTime::now()) }).is_some() {
            self.recent_items.forget(&related);
            if let Some(pages) = &self.pages {
                let mut pages = pages.borrow_mut();
                for id in related { pages.discard(id); }
                pages.sync(&self.browser, self.page_content_visible());
            }
            self.after_tab_removal(cx);
            if self.browser.archive.len() > archived_before { self.show_archive_undo(&title, cx); }
            if let Some(downloads) = cx.try_global::<crate::downloads::AppDownloads>().cloned() { downloads.wake(cx); }
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
        if self.browser.is_blank() { return; }
        let Some(space) = self.browser.spaces.get(index) else { return; };
        let id = space.id;
        let was_collapsed = space.pinned_collapsed;
        let from = self.pinned_reveals.remove(&id)
            .filter(|reveal| reveal.collapsed == was_collapsed)
            .map_or(if was_collapsed { 0. } else { 1. }, |reveal| reveal.amount);
        let collapsed = !was_collapsed;
        self.edit_browser(|browser| {
            if let Some(space) = browser.spaces.iter_mut().find(|space| space.id == id) { space.pinned_collapsed = collapsed; }
        });
        let to = if collapsed { 0. } else { 1. };
        if !crate::native::prefers_reduced_motion() {
            let started = cx.background_executor().now();
            let task = cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let elapsed = cx.background_executor().now().duration_since(started).as_secs_f32();
                let progress = (elapsed / 0.18).clamp(0., 1.);
                let amount = from + (to - from) * (1. - (1. - progress).powi(3));
                if this.update(cx, |this, cx| {
                    if let Some(reveal) = this.pinned_reveals.get_mut(&id) { reveal.amount = amount; }
                    cx.notify();
                }).is_err() || progress >= 1. { break; }
            });
            self.pinned_reveals.insert(id, PinnedReveal { amount: from, collapsed, _task: task });
        }
        cx.notify();
    }

    fn on_toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        if self.browser.is_blank() { return; }
        if let Some(id) = self.browser.space().active_tab {
            self.toggle_tab_pin(id, cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_toolbar(&mut self, cx: &mut Context<Self>) {
        let shown = self.toolbar_shown();
        self.edit_browser(|browser| browser.settings.set_enabled(browser_core::settings::Toggle::ShowToolbar, !shown));
        self.close_site_controls(cx);
        cx.notify();
    }

    pub(crate) fn toolbar_shown(&self) -> bool {
        self.browser.settings.enabled(browser_core::settings::Toggle::ShowToolbar)
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

    pub(crate) fn scroll_to_today_item(&mut self, tab: TabId, cx: &mut Context<Self>) {
        let anchor = self.browser.item_anchor(tab);
        if let Some(index) = self.today_row_index(self.browser.space().id, anchor) {
            self.sidebar_scrolls.entry(self.browser.space().id).or_default().pending.set(Some((Section::Today, index)));
        }
        cx.notify();
    }

    fn can_clear_active_site(&self) -> bool {
        self.native_page_content_visible() && self.pages.is_some() && self.browser.active_tab()
            .is_some_and(|tab| browser_core::site_settings::SiteSettings::origin(&tab.url).is_some())
    }
    fn clear_active_site(&mut self, kind: engine::ClearSiteData, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if !self.can_clear_active_site() { return; }
        let Some(tab) = self.browser.active_tab() else { return; };
        let Some(origin) = browser_core::site_settings::SiteSettings::origin(&tab.url) else { return; };
        let accepted = self.pages.as_ref().is_some_and(|pages| pages.borrow_mut().clear_site_data(tab.id, &origin, kind));
        if accepted {
            self.close_command_bar(cx);
            self.close_site_controls(cx);
        }
    }

    fn set_website_appearance(&mut self, mode: browser_core::themes::ThemeAppearance, cx: &mut Context<Self>) {
        self.edit_profile(|browser| browser.settings.theme_appearance = mode);
        if let Some(pages) = &self.pages { pages.borrow_mut().sync_color_schemes(self.theme_appearance()); }
        crate::menus::sync_appearance(self.theme_appearance(), cx);
        cx.notify();
    }

    fn reveal_active_tab(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.browser.space().active_tab else { return; };
        self.reveal_tab_row(active, cx);
    }

    pub(crate) fn reveal_tab_row(&mut self, active: TabId, cx: &mut Context<Self>) {
        self.close_command_bar(cx);
        if !self.sidebar_visible { self.toggle_sidebar(cx); }
        self.close_floating_sidebar(cx);
        for folder in self.browser.folder_ancestors(active) { self.edit_browser(|browser| browser.set_folder_collapsed(folder, false)); }
        let groups = self.browser.today_groups(self.browser.space().id);
        let active_anchor = self.browser.item_anchor(active);
        for group in groups.into_iter().filter(|g|g.collapsed && g.tabs.contains(&active_anchor)) {
            let space=self.browser.space().id;
            self.edit_browser(|b|b.toggle_today_group(space,group.id));
        }
        let anchor = self.browser.item_anchor(active);
        let space = self.browser.space();
        let section = if space.pinned.iter().any(|tab| tab.id == anchor) { Some(Section::Pinned) }
            else if space.today.iter().any(|tab| tab.id == anchor) { Some(Section::Today) } else { None };
        if let Some(section) = section {
            let index = if section == Section::Pinned { self.browser.pinned_rows(space.id, false).iter().position(|row| self.browser.pinned_item_tab(space.id, row.item) == Some(anchor)) }
                else { self.today_row_index(space.id, anchor) };
            let scrolls = self.sidebar_scrolls.entry(space.id).or_default();
            if let Some(index) = index { scrolls.pending.set(Some((section, index))); }
            if section == Section::Pinned { self.edit_browser(|browser| browser.spaces[browser.active_space].pinned_collapsed = false); }
        }
        self.settle_pager(cx);
        cx.notify();
    }

    pub(crate) fn sidebar_amount(&self) -> f32 {
        if self.transient.is_some() { return 0.; }
        self.sidebar_reveal.as_ref().filter(|reveal| reveal.collapsed == !self.sidebar_visible)
            .map_or(if self.sidebar_visible { 1. } else { 0. }, |reveal| reveal.amount)
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.transient.is_some() { return; }
        self.cancel_sidebar_resize_for_active_command(cx);
        self.finish_tab_rename(true, cx);
        let from = self.sidebar_amount();
        self.sidebar_visible = !self.sidebar_visible;
        self.sidebar_reveal = None;
        if self.sidebar_visible { self.close_floating_sidebar(cx); }
        if !crate::native::prefers_reduced_motion() {
            let collapsed = !self.sidebar_visible;
            let to = if collapsed { 0. } else { 1. };
            let started = cx.background_executor().now();
            let task = cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(PAGER_FRAME).await;
                let progress = (cx.background_executor().now().duration_since(started).as_secs_f32() / FLOATING_SIDEBAR_SLIDE.as_secs_f32()).clamp(0., 1.);
                let amount = from + (to - from) * (1. - (1. - progress).powi(5));
                if this.update(cx, |this, cx| {
                    if let Some(reveal) = &mut this.sidebar_reveal { reveal.amount = amount; }
                    cx.notify();
                }).is_err() || progress >= 1. { break; }
            });
            self.sidebar_reveal = Some(PinnedReveal { amount: from, collapsed, _task: task });
        }
        cx.notify();
    }

    /// Slides the hidden sidebar in over the page, as when the pointer
    /// touches the window's left edge in Arc.
    pub fn reveal_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.transient.is_some() || self.sidebar_visible || self.floating_sidebar.is_some() {
            return;
        }
        self.floating_sidebar = Some(FloatingSidebar {
            overlay: None,
            entered: false,
            closing: false,
            height: f32::from(window.viewport_size().height),
            _pointer_task: None,
        });
        cx.notify();

        let sidebar_width = self.sidebar_width();
        let browser = cx.entity().downgrade();
        let main = window.bounds();
        let bounds = Bounds::new(
            main.origin,
            size(px(self.sidebar_width() + 2.0 * FLOATING_SIDEBAR_INSET + FLOATING_SIDEBAR_SHADOW), main.size.height),
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
                    crate::native::position_traffic_lights(overlay, TRAFFIC_LIGHT_X - sidebar_width - FLOATING_SIDEBAR_INSET);
                }
                cx.new(|cx| {
                    cx.observe_window_activation(window, |overlay: &mut Overlay, window, cx| {
                        let source = window.window_handle();
                        let active = window.is_window_active();
                        let _ = overlay.browser.update(cx, |this, cx| this.on_sidebar_resize_activation(source, active, cx));
                    }).detach();
                    Overlay::new(browser.clone(), OverlayKind::Sidebar, cx)
                })
            });
            if let Ok(overlay) = overlay {
                let _ = browser.update(cx, |this, cx| {
                    if let Some(floating) = &mut this.floating_sidebar {
                        floating.overlay = Some(overlay);
                        // Native pages cover GPUI. Read pointer position without extending
                        // the child window into an invisible input-blocking region.
                        if let Some(parent) = this.native_view {
                            floating._pointer_task = Some(cx.spawn(async move |this, cx| loop {
                                cx.background_executor().timer(PAGER_FRAME).await;
                                let keep = this.update(cx, |this, cx| {
                                    if this.floating_sidebar.as_ref().is_none_or(|floating| floating.closing) { return false; }
                                    if let Some([x, y]) = crate::native::pointer_position(parent) { this.on_floating_sidebar_pointer(x, y, cx); }
                                    true
                                }).unwrap_or(false);
                                if !keep { break; }
                            }));
                        }
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

    fn on_floating_sidebar_pointer(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if self.sidebar_resize.is_some() { return; }
        let right = FLOATING_SIDEBAR_INSET + self.sidebar_width();
        let Some(floating) = &mut self.floating_sidebar else { return; };
        if (0. ..=right).contains(&x) && (0. ..=floating.height).contains(&y) { floating.entered = true; }
        if floating.entered && (x > right + SIDEBAR_DISMISS_BUFFER || y < 0. || y > floating.height)
            && self.tab_menu.is_none() && self.command_bar.is_none() {
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
        if self.browser.is_blank() { return; }
        let ScrollDelta::Pixels(delta) = event.delta else {
            return;
        };
        let (dx, dy) = (f32::from(delta.x) * SPACE_SCROLL_GAIN, f32::from(delta.y) * SPACE_SCROLL_GAIN);
        if self.sidebar_resize.is_some() { return; }
        let sidebar_width = self.sidebar_width();
        let page_count = if self.browser.is_private() {
            self.browser.spaces.len()
        } else {
            self.sidebar_navigation.controller.pages(&self.browser).len()
        };
        let now = cx.background_executor().now();
        if matches!(event.touch_phase, TouchPhase::Started) {
            self.pager.settle = None;
            self.pager.velocity = 0.;
            self.pager.drag = Some(Drag {
                start: self.pager.position,
                travel: 0.,
                horizontal: None,
                samples: Vec::new(),
            });
        }
        // Include movement in both the initial and release events. Ignore
        // inertial events after release, which no longer have a finger drag.
        let Some(drag) = &mut self.pager.drag else { return; };
        if drag.horizontal.is_none() && (dx != 0. || dy != 0.) {
            drag.horizontal = Some(dx.abs() > dy.abs());
        }
        if drag.horizontal == Some(true) && dx != 0. {
            drag.travel += dx;
            drag.samples.push((now, dx));
            let raw = drag.start - drag.travel / sidebar_width.max(1.);
            self.pager.position = rubber_band(raw, page_count);
            cx.notify();
        }
        match event.touch_phase {
            TouchPhase::Started | TouchPhase::Moved => {}
            TouchPhase::Ended => {
                let Some(drag) = self.pager.drag.take() else {
                    return;
                };
                if drag.horizontal != Some(true) {
                    self.settle_pager(cx);
                    return;
                }
                let start = drag.start.round();
                let moved = self.pager.position - drag.start;
                let speed = drag.speed(cx.background_executor().now());
                self.pager.velocity = -speed / sidebar_width.max(1.);
                let step = if speed.abs() > SPACE_FLICK_SPEED {
                    -speed.signum()
                } else if moved > SPACE_SWIPE_COMMIT {
                    1.0
                } else if moved < -SPACE_SWIPE_COMMIT {
                    -1.0
                } else {
                    0.0
                };
                let last = page_count.saturating_sub(1) as f32;
                let target = (start + step).clamp(0.0, last) as usize;
                if self.browser.is_private() {
                    self.switch_space(target, cx);
                } else if let Some(page) = self.sidebar_navigation.controller.pages(&self.browser).get(target).copied() {
                    if page == crate::sidebar_pages::SidebarPage::CreateSpace {
                        self.begin_space_creation(self.browser.space().id, cx);
                    } else {
                        self.select_sidebar_page(page, cx);
                    }
                }
                self.animate_pager(if step == 0. { PAGER_RETURN_FREQUENCY } else { PAGER_SPRING_FREQUENCY }, cx);
            }
        }
    }

    /// Springs to the selected page in the current derived order.
    pub(crate) fn settle_pager(&mut self, cx: &mut Context<Self>) {
        self.animate_pager(PAGER_SPRING_FREQUENCY, cx);
    }

    fn animate_pager(&mut self, frequency: f32, cx: &mut Context<Self>) {
        let target = if self.browser.is_blank() || self.browser.is_private() {
            self.browser.active_space
        } else {
            self.sidebar_page_index()
        } as f32;
        if (self.pager.position - target).abs() < 0.001 && self.pager.velocity.abs() < 0.01 {
            self.pager.position = target;
            self.pager.velocity = 0.;
            self.pager.settle = None;
            return;
        }
        let from = self.pager.position;
        let velocity = self.pager.velocity;
        let started = cx.background_executor().now();
        self.pager.settle = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(PAGER_FRAME).await;
            let done = this.update(cx, |this, cx| {
                let pager = &mut this.pager;
                let elapsed = cx.background_executor().now().saturating_duration_since(started).as_secs_f32();
                let (position, velocity) = pager_spring(from, target, velocity, frequency, elapsed);
                let finished = (target - position).abs() < 0.001 && velocity.abs() < 0.01;
                if finished {
                    pager.position = target;
                    pager.velocity = 0.;
                } else {
                    pager.position = position;
                    pager.velocity = velocity;
                }
                cx.notify();
                finished
            });
            if !matches!(done, Ok(false)) {
                break;
            }
        }));
    }

    fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.edit_browser(|browser| browser.cycle_tab(delta, SystemTime::now()));
        cx.notify();
    }

    pub(crate) fn switch_space(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.browser.is_blank() { return; }
        let Some(id) = self.browser.spaces.get(index).map(|space| space.id) else { return; };
        if !self.browser.is_private() {
            self.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(id), cx);
            return;
        }
        self.finish_tab_rename(true, cx);
        self.edit_browser(|browser| browser.switch_space(index));
        self.settle_pager(cx);
        cx.notify();
    }

    pub(crate) fn reorder_space(&mut self, id: SpaceId, destination: usize, cx: &mut Context<Self>) {
        if self.browser.is_blank() { return; }
        if self.edit_browser(|browser| browser.move_space(id, destination)) {
            // The selected Space is the same object at a new index. A pager
            // animation here would briefly display a different Space's tabs.
            self.reconcile_sidebar_navigation();
            self.reset_profile_pager(self.browser.active_space);
            cx.notify();
        }
    }

    pub(crate) fn cancel_space_drag_hover(&mut self) { self.space_drag_hover = None; }

    fn hover_space_drag(&mut self, space: SpaceId, hovered: bool, cx: &mut Context<Self>) {
        if !hovered {
            if self.space_drag_hover.as_ref().is_some_and(|(id, _)| *id == space) { self.space_drag_hover = None; }
            return;
        }
        if self.browser.space().id == space || self.space_drag_hover.as_ref().is_some_and(|(id, _)| *id == space) { return; }
        self.space_drag_hover = Some((space, cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let _ = this.update(cx, |this, cx| {
                if !this.space_drag_hover.as_ref().is_some_and(|(id, _)| *id == space) { return; }
                if !cx.has_active_drag() { this.space_drag_hover = None; return; }
                if let Some(index) = this.browser.spaces.iter().position(|entry| entry.id == space) { this.switch_space(index, cx); }
            });
        })));
    }

    fn cycle_space(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.browser.is_blank() { return; }
        // The reference keyboard route wraps only through real Spaces.
        let index = (self.browser.active_space as isize + delta).rem_euclid(self.browser.spaces.len() as isize);
        self.switch_space(index as usize, cx);
    }

    // Rendering.

    pub(crate) fn sidebar_press_inset(&self, id: TabId, cx: &Context<Self>) -> f32 {
        self.sidebar_press.filter(|press| press.0 == id).map_or(0., |(_, from, to, started)| {
            let t = (cx.background_executor().now().saturating_duration_since(started).as_secs_f32() / 0.06).clamp(0., 1.);
            from + (to - from) * t * t * (3. - 2. * t)
        })
    }

    fn set_sidebar_pressed(&mut self, id: TabId, pressed: bool, cx: &mut Context<Self>) {
        if !pressed && !self.sidebar_press.is_some_and(|press| press.0 == id) { return; }
        let from = self.sidebar_press_inset(id, cx);
        let to = if pressed { 1. } else { 0. };
        if crate::native::prefers_reduced_motion() {
            self.sidebar_press_task = None;
            self.sidebar_press = pressed.then(|| (id, to, to, cx.background_executor().now()));
            cx.notify();
            return;
        }
        let started = cx.background_executor().now();
        self.sidebar_press = Some((id, from, to, started));
        self.sidebar_press_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(8)).await;
            let done = this.update(cx, |this, cx| {
                let done = cx.background_executor().now().saturating_duration_since(started) >= Duration::from_millis(60);
                if done && !pressed { this.sidebar_press = None; }
                cx.notify();
                done
            });
            if !matches!(done, Ok(false)) { break; }
        }));
        cx.notify();
    }

    pub(crate) fn render_tab(
        &self,
        tab: &Tab,
        section: Section,
        active: bool,
        hue: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = tab.id;
        let painted_space = self.browser.space().id;
        let painted_profile = self.browser.active_user_profile_id();
        let key = id.get() as usize;
        let private = self.theme_uses_light_ink_for(self.browser.space().id);
        let segmented = self.browser.split_for(id).is_some_and(|split| split.title.is_none());
        let renaming = self.tab_rename.as_ref().is_some_and(|editor| editor.tab == id);
        let reserve_close = !segmented && !renaming;
        let group = SharedString::from(format!("tab-{key}"));
        // Keep the current edge across view refreshes caused by drag hover.
        let edge = self.tab_drop_edges.borrow_mut().entry(id).or_default().clone();
        let drag_moves_active = self.sidebar_page_interactive;
        let move_edge = edge.clone();
        let paint_edge = edge.clone();
        let drag_owner = cx.entity().downgrade();
        let drag_row_width = (self.sidebar_width() - 16.).max(1.);
        let drag_press = self.tab_drag_press.clone();
        let record_drag_press = drag_press.clone();
        let press_inset = if segmented { 0. } else { self.sidebar_press_inset(id, cx) };

        div()
            .id(("tab", key))
            .relative().child(self.track_icon_anchor(crate::icons::IconTarget::Tab(id)))
            .debug_selector(|| self.sidebar_debug_name((|| format!("tab-{key}"))()))
            .group(group.clone())
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                record_drag_press.set(Some(event.position));
                if !segmented { this.set_sidebar_pressed(id, true, cx); }
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| this.set_sidebar_pressed(id, false, cx)))
            .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| this.set_sidebar_pressed(id, false, cx)))
            .on_drag(self.tab_drag(tab), move |drag, offset, window, cx| {
                // GPUI reports the first move past its threshold, rather than
                // the press. Preserve the original grab point on the larger card.
                let offset = offset - (window.mouse_position() - drag_press.get().unwrap_or(window.mouse_position()));
                // The pressed row moves right during compression. Normalize the
                // grab point back to its uncompressed frame for the drag card.
                let offset = offset + gpui::point(px(press_inset), px(0.));
                drag.start_at(drag_owner.clone(),offset,drag_row_width,if segmented {36.} else {SIDEBAR_ROW_HEIGHT},cx)
            })
            .can_drop(move |value, _, _| value.downcast_ref::<crate::tab_drag::TabDrag>().is_some_and(|drag| drag.id != id))
            .on_drag_move::<crate::tab_drag::TabDrag>(cx.listener(move |this,event:&gpui::DragMoveEvent<crate::tab_drag::TabDrag>,_,cx| {
                let drag=event.drag(cx);
                if !drag_moves_active||!this.accepts_tab_drag(drag)||drag.id==id{return;}
                let after=event.event.position.y>event.bounds.center().y;
                if event.bounds.contains(&event.event.position){move_edge.set(after);this.reorder_feedback(drag,Some((id,after)),cx);}
                else if drag.reorder_target.get().is_some_and(|(tab,_)|tab==id){this.reorder_feedback(drag,None,cx);}
            }))
            .drag_over::<crate::tab_drag::TabDrag>(move |style, _, _, _| {
                let style = style.border_color(hsla(0., 0., 0.2, 0.6));
                if paint_edge.get() { style.border_b_2() } else { style.border_t_2() }
            })
            .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| { if this.prepare_sidebar_target(painted_space, painted_profile) { this.drop_tab_relative(drag, id, edge.get(), cx); } }))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(SIDEBAR_ROW_HEIGHT))
            .relative()
            .pl(px(9.))
            // Titles run to the edge; the close control takes room only on
            // hover. Audio takes an inline slot between favicon and title.
            // Hover alone only repaints, so the padding follows hover state.
            .pr(px(if reserve_close && self.hovered_row == Some(id) { 34. } else { 6. }))
            .text_size(px(14.)).line_height(px(18.))
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered { this.hovered_row = Some(id); }
                else if this.hovered_row == Some(id) { this.hovered_row = None; }
                cx.notify();
            }))
            .when(segmented, |this| this.px_1().gap_0().h(px(36.)))
            .rounded(px(SIDEBAR_ROW_RADIUS))
            // Arc eases ordinary rows into a one-pixel inset at each edge while held.
            .mx(px(press_inset))
            .text_color(self.theme_foreground_for(painted_space))
            // Paint a 36 px fill inside the unchanged hit/scroll slot.
            .child(div().debug_selector(|| self.sidebar_debug_name(format!("tab-fill-{key}")))
                .absolute().left_0().right_0()
                .top(px(if segmented { 0. } else { (SIDEBAR_ROW_HEIGHT - SIDEBAR_TAB_HEIGHT) / 2. }))
                .bottom(px(if segmented { 0. } else { (SIDEBAR_ROW_HEIGHT - SIDEBAR_TAB_HEIGHT) / 2. }))
                .rounded(px(SIDEBAR_ROW_RADIUS))
                .when(active, |fill| fill.bg(self.theme_tab_selection_for(painted_space)))
                .when(!active && self.sidebar_item_selected(id), |fill| fill.bg(hsla(0., 0., 1., if private { 0.1 } else { 0.3 })))
                .when(active && segmented && self.theme_uses_light_ink_for(painted_space), |fill| fill.bg(hsla(0.,0.,1.,0.2)))
                .when(!active, |fill| fill.group_hover(group.clone(), move |fill| fill.bg(hsla(0.,0.,1.,if private {0.1} else {0.3})))))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                this.open_tab_menu(id, event.position, window, cx);
            }))
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                if this.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Library { this.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(painted_space), cx); }
                if this.select_sidebar_item(id, event.modifiers(), cx) { return; }
                if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                this.edit_browser(|browser| browser.activate_sidebar_item(id, SystemTime::now()));
                if event.click_count() == 2 {
                    this.open_tab_rename(id, window, cx);
                }
                cx.notify();
            }))
            .child(self.render_sidebar_item_contents(tab, hue, cx))
            .when(!segmented, |this| this.child(
                div()
                    .id(("close", key))
                    .debug_selector(|| self.sidebar_debug_name((|| format!("close-{key}"))()))
                    .absolute().top(px(if segmented { 10. } else { (SIDEBAR_ROW_HEIGHT - 24.) / 2. })).right(px(if segmented { 4. } else { 6. }))
                    .flex_none()
                    .when(segmented, |this| this.px_1().rounded_sm().text_sm())
                    .when(!segmented, |this| this.size(px(24.)).flex().items_center().justify_center().rounded(px(5.)))
                    .text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .when(private, |this| this.text_color(hsla(0., 0., 1., 0.6)))
                    .invisible()
                    .when(!renaming && !segmented, |this| this.group_hover(group, |s| s.visible()))
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                        this.close_tab(id, cx);
                    }))
                    .child(crate::icons::control_sized(match section {
                        Section::Pinned => crate::icons::Icon::Minus,
                        Section::Today => crate::icons::Icon::Close,
                    }, if !segmented && matches!(section, Section::Today) { 22. } else { 18. })),
            ))
    }

    pub(crate) fn render_favorite(&self, tab: &Tab, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let id = tab.id;
        let painted_space = self.browser.space().id;
        let painted_profile = self.browser.active_user_profile_id();
        let key = id.get() as usize;
        let group = SharedString::from(format!("favorite-{key}"));
        let active = self.browser.space().active_tab.is_some_and(|active| self.browser.item_anchor(active) == id);
        let browser = cx.entity().downgrade();
        let drag_owner = browser.clone();
        // The active Favorite stays translucent with an outline tinted by its
        // icon, so light icons keep their contrast.
        let outline = self.browser.favicon(self.browser.active_user_profile_id(), &tab.url)
            .and_then(|png| self.favicon_styles.borrow_mut().get(png).accent)
            .map_or(hsla(0., 0., 1., 0.95), |accent| accent.opacity(0.85));
        div().id(("favorite", key)).debug_selector(|| self.sidebar_debug_name((move || format!("favorite-{key}"))()))
            .group(group.clone()).relative().flex().items_center().justify_center()
            .on_drag(self.tab_drag(tab), move |drag, _, _, cx| crate::tab_drag::TabDrag::start(drag, drag_owner.clone(), cx))
            .can_drop(move |value, _, cx| value.downcast_ref::<crate::tab_drag::TabDrag>().is_some_and(|drag| {
                browser.read_with(cx, |this, _| drag.id != id && (this.browser.user_profile(painted_profile).is_some_and(|profile| profile.favorites.iter().any(|tab| tab.id == drag.id) || profile.favorites.len() < browser_core::favorites::MAX_FAVORITES))).unwrap_or(false)
            }))
            .on_drag_move::<crate::tab_drag::TabDrag>(cx.listener(move |this,event:&gpui::DragMoveEvent<crate::tab_drag::TabDrag>,_,cx| {
                let drag=event.drag(cx);
                if !this.accepts_tab_drag(drag)||drag.id==id{return;}
                if event.bounds.contains(&event.event.position){this.reorder_feedback(drag,Some((id,false)),cx);}
                else if drag.reorder_target.get().is_some_and(|(tab,_)|tab==id){this.reorder_feedback(drag,None,cx);}
            }))
            .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.9)))
            .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| { if this.prepare_sidebar_target(painted_space, painted_profile) { this.drop_tab_at_favorite(drag, id, cx); } }))
            .w(px(width)).h(px(FAVORITE_HEIGHT)).rounded(px(SIDEBAR_ROW_RADIUS))
            .bg(hsla(0.0, 0.0, 1.0, if active { 0.5 } else { 0.3 }))
            .when(active, |this| this.border(px(1.5)).border_color(outline).shadow_sm())
            .hover(move |s| s.bg(hsla(0.0, 0.0, 1.0, if active { 0.55 } else { 0.42 })))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                this.open_tab_menu(id, event.position, window, cx);
            }))
            // No tooltip: the page view covers the sidebar's popups.
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                if this.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Library { this.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(painted_space), cx); }
                if this.select_sidebar_item(id, event.modifiers(), cx) { return; }
                if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                this.edit_browser(|browser| browser.activate_sidebar_item(id, SystemTime::now()));
                if event.click_count() == 2 {
                    let title = this.browser.item_title(id);
                    this.open_command_bar(Target::RenameTab(id), &title, window, cx);
                }
                cx.notify();
            }))
            .child(self.render_item_icon(tab, self.accent()))
            .when(self.show_tab_audio(tab), |this| this.child(self.render_mute_control(tab, true, cx)))
    }

    pub(crate) fn render_mute_control(&self, tab: &Tab, favorite: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let id = tab.id;
        let painted_space = self.browser.space().id;
        let painted_profile = self.browser.active_user_profile_id();
        let key = id.get() as usize;
        let muted = tab.muted;
        let foreground = self.theme_foreground_for(painted_space);
        div().id(("unmute", key)).debug_selector(|| self.sidebar_debug_name((move || format!("unmute-{key}"))()))
            .flex_none().w(px(18.)).h(px(18.)).rounded(px(4.)).text_color(foreground.opacity(0.65))
            .when(favorite, |this| this.absolute().bottom(px(1.)).left(px(1.)))
            .hover(move |s| s.bg(foreground.opacity(0.1)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                this.set_tab_muted(id, !muted, cx);
            })).child(crate::icons::speaker(muted))
    }

    pub(crate) fn render_favicon(&self, tab: &Tab, hue: f32) -> AnyElement { self.render_favicon_on(tab, hue, self.browser.is_private()) }

    fn render_command_page_icon(&self, url: &str, tab: Option<TabId>, dark: bool) -> AnyElement {
        if let Some(tab) = tab.and_then(|id| self.browser.all_tabs().find(|tab| tab.id == id && tab.custom_icon.is_some())) {
            return self.render_favicon_on(tab, self.accent(), dark);
        }
        if let Some(png) = self.browser.favicon(self.browser.active_user_profile_id(), url) {
            let style = self.favicon_styles.borrow_mut().get(png);
            return gpui::img(if dark { style.on_dark } else { style.image })
                .size(px(16.)).flex_none().object_fit(gpui::ObjectFit::Contain).into_any_element();
        }
        crate::icons::control_sized(crate::icons::Icon::Globe, 16.)
    }

    /// `tab`'s favicon for a light surface, or a dark one when `dark`.
    pub(crate) fn render_favicon_on(&self, tab: &Tab, _hue: f32, dark: bool) -> AnyElement {
        self.render_favicon_for_profile(tab,self.browser.active_user_profile_id(),dark)
    }
    pub(crate) fn render_favicon_for_profile(&self, tab: &Tab, profile: browser_core::UserProfileId, dark: bool) -> AnyElement {
        self.render_favicon_styled(tab,profile,dark,false)
    }
    pub(crate) fn render_media_favicon(&self, tab: &Tab, profile: browser_core::UserProfileId) -> AnyElement {
        self.render_favicon_styled(tab,profile,true,true)
    }
    fn render_favicon_styled(&self, tab: &Tab, profile: browser_core::UserProfileId, dark: bool, media: bool) -> AnyElement {
        if let Some(icon) = &tab.custom_icon { return crate::icons::glyph(icon); }
        let icon = match self.browser.favicon(profile, &tab.url) {
            Some(png) => {
                let style = self.favicon_styles.borrow_mut().get(png);
                gpui::img(if dark { style.on_dark } else { style.image })
                    .size(px(16.0)).flex_none().object_fit(gpui::ObjectFit::Contain).rounded(px(if media{3.}else{0.}))
                    .with_fallback(move || favicon_placeholder(dark).into_any_element())
                    .into_any_element()
            }
            None => favicon_placeholder(dark).into_any_element(),
        };
        // Favicons are 16 px, centered in the 18 px slot shared with symbols.
        div().size(px(if media{16.}else{18.})).flex_none().flex().items_center().justify_center().child(icon).into_any_element()
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.browser.is_blank() || self.browser.is_private() { return div().h_full().on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_sidebar_scroll(event, cx))).child(self.render_workspace_sidebar(None, cx)).into_any_element(); }
        if self.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::CreateSpace {
            return div().id("sidebar-viewport").debug_selector(|| "sidebar".into()).relative().w(px(self.sidebar_width())).h_full().overflow_hidden()
                .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_sidebar_scroll(event, cx)))
                .when_some(self.sidebar_navigation.form.clone(), |this, form| this.child(form))
                .child(div().absolute().bottom_0().left_0().right_0().child(self.render_space_switcher(cx)))
                .into_any_element();
        }
        let pages = self.sidebar_navigation.controller.pages(&self.browser);
        let saved = self.browser.clone();
        let saved_namespace = self.sidebar_debug_namespace.clone();
        let saved_interactive = self.sidebar_page_interactive;
        let mut rendered = Vec::with_capacity(pages.len());
        for (page_index, page) in pages.into_iter().enumerate() {
            let content = match page {
                crate::sidebar_pages::SidebarPage::Space(id) => {
                    let mut model: Browser = (*saved).clone();
                    if let Some(index) = model.spaces.iter().position(|space| space.id == id) { model.switch_space(index); }
                    self.browser = model.window_projection(&model.window_selection());
                    let active = id == saved.space().id;
                    self.sidebar_debug_namespace = if active { saved_namespace.clone() } else { Some(format!("inactive-{id:?}")) };
                    self.sidebar_page_interactive = active;
                    self.render_space_page(self.browser.active_space, cx).into_any_element()
                }
                crate::sidebar_pages::SidebarPage::CreateSpace | crate::sidebar_pages::SidebarPage::Library => div().into_any_element(),
            };
            rendered.push(div().id(("sidebar-page", page_index)).relative().flex_none().w(px(self.sidebar_width())).h_full().child(content).into_any_element());
        }
        self.browser = saved;
        self.sidebar_debug_namespace = saved_namespace;
        self.sidebar_page_interactive = saved_interactive;
        // Toolbar, Favorites and Space dock remain stationary while the
        // window-local Space contents follow the fingers between real IDs.
        div().id("sidebar-viewport").debug_selector(|| "sidebar".into()).relative().w(px(self.sidebar_width())).h_full().overflow_hidden()
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_sidebar_scroll(event, cx)))
            .child(self.render_workspace_sidebar(Some((rendered, -self.pager.position * self.sidebar_width())), cx))
            .into_any_element()
    }

    pub(crate) fn render_workspace_sidebar(&self, pager_pages: Option<(Vec<AnyElement>, f32)>, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self
            .browser
            .active_tab()
            .map(|t| if t.url == crate::split::EMPTY_PANE_URL || self.has_frozen_active_page() { "Search or Enter URL…".into() } else { address_host(&t.url) })
            .unwrap_or_else(|| "Search or Enter URL…".into());
        let url = self.peek_child.and_then(|child| child.read(cx).ok().and_then(|view|
            view.browser.active_tab().map(|tab| crate::transient::lookup_host(&tab.url)))).unwrap_or(url);
        let address_placeholder = url == "Search or Enter URL…";
        let private = self.browser.is_private();
        let dark = self.theme_uses_light_ink_for(self.browser.space().id);
        let blank = self.browser.is_blank();
        let (pages, offset) = pager_pages.unwrap_or_else(|| (vec![if blank { self.render_blank_tabs(cx).into_any_element() } else if private { self.render_private_tabs(cx).into_any_element() } else { self.render_space_page(self.browser.active_space, cx).into_any_element() }], 0.));
        let browser = cx.entity().downgrade();
        let state = self.browser.space().active_tab.and_then(|id| self.pages.as_ref()?.try_borrow().ok().map(|pages| pages.state(id))).unwrap_or_default();

        let loading = state.loading;
        let control = |id: &'static str| div().id(id).debug_selector(|| self.sidebar_debug_name((move || id.into())())).flex_none().size(px(28.)).rounded_md()
            .flex().items_center().justify_center().text_color(hsla(0., 0., if dark { 1. } else { 0. }, 0.6)).hover(|style| style.bg(hsla(0., 0., 1., 0.25)));
        let controls_open = self.site_controls.is_some();
        let anchor = if self.sidebar_debug_namespace.is_none() { self.site_controls_anchor.clone() } else { Default::default() };
        let address_action = |id: &'static str, active: bool| div().id(id).debug_selector(|| self.sidebar_debug_name((move || id.into())()))
            .group(id).relative().flex_none().size(px(26.)).flex().items_center().justify_center()
            .child(self.render_press_highlight(id, hsla(0., 0., 0., 0.08), active));
        let address_actions_shown = controls_open || self.sidebar_hovered;
        let favorites: Vec<_> = self.browser.active_user_profile().favorites.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).collect();
        // Favorites stretch to fill each row, as in the reference.
        let columns = favorites.len().clamp(1, FAVORITE_COLUMNS) as f32;
        let favorite_width = ((self.sidebar_width() - 2. * SIDEBAR_INSET - FAVORITE_GAP * (columns - 1.)) / columns).floor();
        div()
            .id("sidebar")
            .debug_selector(|| self.sidebar_debug_name(if private || blank { "sidebar".into() } else { "workspace-sidebar".into() }))
            .group("sidebar")
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.sidebar_hovered != *hovered { this.sidebar_hovered = *hovered; cx.notify(); }
            }))
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.sidebar_width()))
            .h_full()
            .pt(px(SIDEBAR_TOOLBAR_HEIGHT - if self.toolbar_shown() { 3. } else { 1. }))
            .gap(px(8.))
            .text_color(hsla(0.0, 0.0, if dark { 0.95 } else { 0.1 }, 1.0))
            .child(div().absolute().top_0().left_0().right(px(8.)).h(px(SIDEBAR_TOOLBAR_HEIGHT)).flex().items_center().gap(px(6.))
                .child(div().flex_none().w(px(74.)).h_full())
                .child(control("toggle-sidebar").ml(px(-1.))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
                    .child(crate::icons::control(crate::icons::Icon::Sidebar)))
                .child(div().flex_1())
                .when(!self.toolbar_shown() || self.browser.active_split().is_some(), |this| this.child(control("go-back")
                    .text_color(hsla(0., 0., if dark { 1. } else { 0. }, if state.can_go_back { 0.6 } else { 0.25 }))
                    .on_click(cx.listener(|this, _, _, _| { this.with_active_page(|pages, tab| pages.go_back(tab)); }))
                    .child(crate::icons::control(crate::icons::Icon::Back)))
                .child(control("go-forward")
                    .text_color(hsla(0., 0., if dark { 1. } else { 0. }, if state.can_go_forward { 0.6 } else { 0.25 }))
                    .on_click(cx.listener(|this, _, _, _| { this.with_active_page(|pages, tab| pages.go_forward(tab)); }))
                    .child(crate::icons::control(crate::icons::Icon::Forward)))
                .child(control("reload-page")
                    .on_click(cx.listener(move |this, _, _, _| { this.with_active_page(|pages, tab| { if loading { pages.stop_loading(tab); } else { pages.reload(tab); } }); }))
                    .child(crate::icons::control(if state.loading { crate::icons::Icon::Close } else { crate::icons::Icon::Reload })))))
            .when(!self.toolbar_shown(), |this| this.child(
                div().px(px(SIDEBAR_INSET)).child(
                    div()
                        .id("address")
                        .debug_selector(|| self.sidebar_debug_name((|| "address".into())()))
                        .h(px(36.0))
                        .pl(px(13.))
                        .pr(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .rounded(px(SIDEBAR_ROW_RADIUS))
                        .bg(hsla(0.0, 0.0, 1.0, 0.35))
                        .hover(move |s| s.bg(hsla(0.0, 0.0, 1.0, if dark { 0.2 } else { 0.5 })))
                        .text_size(px(13.))
                        .text_color(hsla(0.0, 0.0, 0.0, 0.6))
                        .when(dark, |this| this.bg(hsla(0., 0., 1., 0.14)).text_color(hsla(0., 0., 1., 0.65)))
                        .when_some(self.theme_address_fill(), |this, fill| this.bg(fill))
                        .whitespace_nowrap()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.on_edit_current_url(&EditCurrentUrl, window, cx)
                        }))
                        .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_sidebar_menu(crate::tab_menu::MenuTarget::Address, event.position, window, cx);
                        }))
                        .when(address_placeholder, |this| this.pl(px(4.)).gap(px(12.))
                            .child(div().debug_selector(|| self.sidebar_debug_name((|| "address-search-icon".into())()))
                                .flex_none().child(crate::icons::control_sized(crate::icons::Icon::Search, 16.))))
                        .child(div().flex_1().min_w_0().overflow_hidden()
                            .when(address_placeholder, |this| this.debug_selector(|| self.sidebar_debug_name((|| "address-placeholder".into())())))
                            .when(!address_placeholder, |this| this.debug_selector(|| self.sidebar_debug_name((|| "address-host".into())())))
                            .child(url))
                        // Page actions appear only while the pointer is over the sidebar.
                        .when(self.browser.active_tab().is_some(), |this| this.child(hover_fade(div().flex().items_center().gap(px(4.))
                            // No tooltip: the page view would cover it.
                            .child(address_action("address-copy-link", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.press_control("address-copy-link");
                                    this.copy_url(false, cx);
                                    this.copy_feedback = Some((this.copy_feedback.map_or(0, |(generation, _)| generation + 1), Instant::now()));
                                }))
                                .child(self.render_copy_icon()))
                            .child(address_action("address-site-controls", controls_open)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.press_control("address-site-controls");
                                    this.toggle_site_controls(window, cx);
                                }))
                                .child(self.render_pressed_icon("address-site-controls", crate::icons::Icon::SiteControls))
                                .child(canvas(|_, _, _| (), move |bounds, _, _, _| anchor.set(Some(bounds))).absolute().inset_0())),
                            "address-actions", address_actions_shown))),
                ),
            ))
            .when(!blank && (!favorites.is_empty() || (!private && (!self.browser.active_user_profile().empty_favorites_dismissed || self.tab_drag_active))), |this| this.child(
                div().id("favorites").debug_selector(|| self.sidebar_debug_name((|| "favorites".into())()))
                    .flex().flex_wrap().gap(px(FAVORITE_GAP)).px(px(SIDEBAR_INSET))
                    .when(favorites.is_empty(), |this| this.flex_none()
                        .child(self.render_empty_favorites(cx)))
                    .rounded_lg().drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.3)))
                    .can_drop(move |value, _, cx| value.downcast_ref::<crate::tab_drag::TabDrag>().is_some_and(|drag| {
                        browser.read_with(cx, |this, _| this.can_drop_in_favorites(drag)).unwrap_or(false)
                    }))
                    .on_drop(cx.listener(|this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_in_favorites(drag, cx)))
                    .children(favorites.iter().map(|tab| self.render_favorite(tab, favorite_width, cx))),
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
            .when_some(self.render_mini_player(cx),|this,player|this.child(player))
            .when(!private && !blank, |this| this.child(self.render_space_switcher(cx)))
            .when(private, |this| this.child(div().h(px(SPACE_BAR_HEIGHT)).flex_none().px(px(7.)).flex().items_center()
                .text_color(hsla(0., 0., 1., 0.5))
                .child(div().size(px(32.)).flex().items_center().justify_center().child(crate::icons::control(crate::icons::Icon::Library)))
                .child(div().flex_1())
                .child(div().id("private-plus").size(px(32.)).flex().items_center().justify_center()
                    .on_click(cx.listener(|this, _, window, cx| this.open_private_command_bar(window, cx)))
                    .child(crate::icons::control(crate::icons::Icon::Plus)))))
    }

    /// The supplied Arc reference shows this card at rest. After dismissal,
    /// reuse the same card during tab drags; Arc's drag presentation is still
    /// pending native observation, so no additional transition is introduced.
    fn render_empty_favorites(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.theme_colors();
        div().id("empty-favorites").debug_selector(|| self.sidebar_debug_name((|| "empty-favorites".into())()))
            .relative().w_full().h(px(95.)).flex_none()
            .rounded(px(12.)).border_1().border_dashed()
            .border_color(colors.border)
            .flex().flex_col().items_center().pt(px(11.))
            .text_color(colors.foreground.opacity(0.78))
            .child(canvas(|_, _, _| (), |bounds, _, window, _| {
                let center = bounds.center();
                window.paint_quad(gpui::fill(Bounds::new(
                    center - point(px(8.), px(8.)), size(px(16.), px(16.))),
                    hsla(0.67, 0.8, 0.91, 1.)).corner_radii(px(8.)));
                let mut star = gpui::PathBuilder::fill();
                for i in 0..10 {
                    let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.;
                    let radius = if i % 2 == 0 { 5.3 } else { 2.3 };
                    let p = center + point(px(angle.cos() * radius), px(angle.sin() * radius));
                    if i == 0 { star.move_to(p); } else { star.line_to(p); }
                }
                star.close();
                if let Ok(star) = star.build() { window.paint_path(star, hsla(0.67, 0.45, 0.38, 1.)); }
            }).size(px(16.)).flex_none())
            .child(div().mt(px(5.)).text_size(px(13.)).line_height(px(16.))
                .font_weight(gpui::FontWeight::BOLD).child("Drag to add Favorites"))
            .child(div().mt(px(2.)).text_center().text_size(px(12.)).line_height(px(15.))
                .child("Favorites keep your most").child(div().child("used sites and apps close")))
            .child(div().id("dismiss-empty-favorites").debug_selector(|| self.sidebar_debug_name((|| "dismiss-empty-favorites".into())()))
                .absolute().top(px(4.)).right(px(4.)).size(px(20.))
                .flex().items_center().justify_center().cursor_pointer()
                .text_color(colors.muted)
                .hover(move |style| style.text_color(colors.foreground))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.edit_browser(|browser| browser.active_user_profile_mut().empty_favorites_dismissed = true);
                    cx.notify();
                }))
                .child(crate::icons::control_sized(crate::icons::Icon::Close, 12.)))
    }

    /// Window controls stay native for their behavior, but idle as neutral
    /// dots and show their colors only while the pointer is over them.
    pub(crate) fn render_traffic_lights(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hovered = self.traffic_lights_hovered;
        div().id("traffic-lights").debug_selector(|| self.sidebar_debug_name((|| "traffic-lights".into())())).flex_none().w(px(74.)).h_full()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.traffic_lights_hovered != *hovered {
                    this.traffic_lights_hovered = *hovered;
                    cx.notify();
                }
            }))
            .child(canvas(|_, _, _| (), move |_, _, window, _| {
                let Some(view) = crate::native::view_of(window) else { return; };
                // Fullscreen AppKit owns a separate toolbar window. Its
                // buttons cannot be projected into this content view.
                let native_controls = hovered || window.is_fullscreen();
                crate::native::set_traffic_lights_alpha(view, if native_controls { 1. } else { 0. });
                let Some(frames) = crate::native::traffic_light_frames(view).filter(|_| !native_controls) else { return; };
                for (frame, rim) in frames.iter().zip([0xff5f57, 0xfebc2e, 0x28c840]) {
                    let diameter = frame[2].min(frame[3]);
                    let origin = point(px(frame[0] + (frame[2] - diameter) / 2.), px(frame[1] + (frame[3] - diameter) / 2.));
                    window.paint_quad(gpui::quad(Bounds::new(origin, size(px(diameter), px(diameter))), px(diameter / 2.),
                        hsla(0., 0., 0.47, 1.), px(1.), Hsla::from(gpui::rgb(rim)).opacity(0.45), gpui::BorderStyle::Solid));
                }
            }).size_full())
    }

    fn press_control(&mut self, name: &'static str) {
        let count = self.pressed_control.map_or(0, |(_, count, _)| count + 1);
        self.pressed_control = Some((name, count, Instant::now()));
    }

    pub(crate) fn mark_new_menu_toggled(&mut self) {
        self.new_menu_toggles += 1;
        self.new_menu_toggled_at = Some(Instant::now());
    }

    /// A control's hover/active background, which dips and springs back
    /// with the icon after a click.
    fn render_press_highlight(&self, name: &'static str, color: Hsla, active: bool) -> AnyElement {
        let layer = div().absolute().inset_0().rounded_md()
            .when(active, |layer| layer.bg(color))
            .group_hover(name, |style| style.bg(color));
        let Some((_, count, started)) = self.pressed_control.filter(|(pressed, _, _)| *pressed == name) else { return layer.into_any_element(); };
        if started.elapsed() >= PRESS_BOUNCE { return layer.into_any_element(); }
        // While it bounces the pointer is on the control, so the background
        // shows; it is painted directly for smooth sub-pixel scaling.
        clocked(div().absolute().inset_0(), format!("{name}-highlight-{count}"), started, PRESS_BOUNCE, move |slot, t| {
            let scale = press_scale(t, HIGHLIGHT_DIP);
            slot.child(canvas(|_, _, _| (), move |bounds, _, window, _| {
                let size = bounds.size.map(|side| side * scale);
                let origin = bounds.center() - gpui::point(size.width / 2., size.height / 2.);
                window.paint_quad(gpui::fill(Bounds::new(origin, size), color).corner_radii(px(6. * scale)));
            }).size_full())
        })
    }

    /// A control icon that dips and springs back after a click.
    fn render_pressed_icon(&self, name: &'static str, icon: crate::icons::Icon) -> AnyElement {
        let Some((_, count, started)) = self.pressed_control.filter(|(pressed, _, _)| *pressed == name) else { return crate::icons::control(icon); };
        clocked(div().size(px(18.)).flex_none(), format!("{name}-press-{count}"), started, PRESS_BOUNCE,
            move |slot, t| slot.child(crate::icons::scaled(icon, press_scale(t, ICON_DIP))))
    }

    /// Copy URL feedback: the link crossfades into a checkmark, the check
    /// erases along its stroke while shrinking, then the link springs back.
    fn render_copy_icon(&self) -> AnyElement {
        use crate::icons::Icon;
        let Some((count, started)) = self.copy_feedback else { return crate::icons::control(Icon::Link); };
        let total = COPY_FEEDBACK.as_secs_f32() * 1000.;
        let check_selector = self.sidebar_debug_name("copy-check".into());
        clocked(div().size(px(18.)).flex_none().relative(), format!("copy-feedback-{count}"), started, COPY_FEEDBACK, move |slot, t| {
            let ms = t * total;
            let phase = |from: f32, to: f32| ((ms - from) / (to - from)).clamp(0., 1.);
            let (link_alpha, link_scale) = if ms < 960. {
                let p = phase(0., 160.);
                (1. - p, 1. - 0.2 * p)
            } else {
                let r = phase(960., 1300.);
                ((r * 2.).min(1.), 0.6 + 0.4 * back_out(r, 1.4))
            };
            // The click itself gets the same light press bounce as other controls.
            let press = press_scale(phase(0., PRESS_BOUNCE.as_secs_f32() * 1000.), ICON_DIP);
            let link_scale = if ms < 960. { link_scale * press } else { link_scale };
            let appear = phase(0., 160.);
            let erase = phase(700., 960.);
            let check_scale = (0.8 + 0.2 * appear) * (1. - 0.12 * erase) * press;
            slot.child(div().absolute().inset_0().opacity(link_alpha).child(crate::icons::scaled(Icon::Link, link_scale)))
                .when(ms < 960., |slot| slot.child(div().debug_selector(|| check_selector.clone()).absolute().inset_0().opacity(appear)
                    .child(crate::icons::check(gpui::ease_in_out(erase), check_scale))))
        })
    }

    /// The Space bar plus turns into a cross while the New menu is open.
    fn render_new_menu_plus(&self) -> AnyElement {
        let open = self.new_menu.is_some();
        let quarter = std::f32::consts::FRAC_PI_4;
        let Some(started) = self.new_menu_toggled_at else { return crate::icons::plus(0., 1.); };
        clocked(div().size(px(18.)).flex_none(), format!("new-menu-plus-{}", self.new_menu_toggles), started, PLUS_TURN, move |slot, t| {
            // An elastic turn with a slight overshoot; the plus keeps its size.
            let turn = back_out(t, 1.3);
            slot.child(crate::icons::plus(quarter * if open { turn } else { 1. - turn }, 1.))
        })
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
        let ink = if self.theme_uses_light_ink_for(space.id) {1.} else {0.};
        let space_id = space.id;
        let hue = space.accent_hue() / 360.0;
        let current = index == self.browser.active_space;
        let pinned = self.render_folder_rows(space.id, hue, cx);
        let today = self.render_today_rows(space.id, hue, cx);
        let reveal = self.pinned_reveals.get(&space.id)
            .filter(|reveal| reveal.collapsed == space.pinned_collapsed)
            .map_or(if space.pinned_collapsed { 0. } else { 1. }, |reveal| reveal.amount);
        let pinned_height = pinned.len() as f32 * (SIDEBAR_ROW_HEIGHT + SIDEBAR_ROW_GAP) - SIDEBAR_ROW_GAP;
        let header_group = SharedString::from(format!("space-header-{index}"));
        let has_today = !space.today.is_empty();
        let can_tidy = self.browser.can_tidy_tabs(space.id);
        let today_actions = has_today && self.sidebar_hovered && !self.max.is_tidying(space.id);
        let action_width = if can_tidy { TODAY_ACTIONS_WIDTH } else { 44. };
        let line_end = action_width + 11.;
        let header_menu_open = self.tab_menu.as_ref().is_some_and(|menu| menu.target == crate::tab_menu::MenuTarget::Space(space_id));
        let icon_picker_open = self.command_bar.as_ref().is_some_and(|bar| bar.target == Target::Icon(crate::icons::IconTarget::Space(space_id)));

        div()
            .id(("space-page", index))
            .when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "space-page".into())())))
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.sidebar_width()))
            .h_full()
            .px(px(SIDEBAR_INSET))
            .gap(px(SIDEBAR_ROW_GAP))
            // Empty sidebar space opens the Space's menu, as in the reference.
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_space_menu(space_id, event.position, window, cx);
            }))
            .when(space.show_header, |this| this.child(
                div().id(("space-header", index)).debug_selector(|| self.sidebar_debug_name((move || format!("space-header-{index}"))()))
                    .group(header_group.clone()).relative()
                    .child(div().absolute().left(px(if space.custom_icon.is_some() { 9. } else { 8. })).top(px(10.)).size(px(18.))
                        .child(self.track_icon_anchor(crate::icons::IconTarget::Space(space_id))))
                    .flex().items_center().gap(px(9.)).h(px(SIDEBAR_ROW_HEIGHT)).pl(px(if space.custom_icon.is_some() { 9. } else { 8. })).pr(px(6.)).rounded(px(SIDEBAR_ROW_RADIUS))
                    .when(header_menu_open, |style| style.bg(hsla(0., 0., 1., 0.15)))
                    .on_click(cx.listener(move |this, _, _, cx| { if let Some(index) = this.browser.spaces.iter().position(|space| space.id == space_id) { this.toggle_pinned_section(index, cx); } }))
                    .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag, space_id, Section::Pinned, cx)))
                    .drag_over::<crate::folder_drag::FolderDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::folder_drag::FolderDrag, _, cx| this.drop_folder_to(drag, space_id, None, cx)))
                    .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_space_menu(space_id, event.position, window, cx);
                    }))
                    .text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(hsla(0., 0., ink, 0.5))
                    .when(space.custom_icon.is_some() || icon_picker_open, |this| this.child(div().relative().size(px(18.)).flex_none()
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("space-header-icon-{index}"))()))
                            .size_full().flex().items_center().justify_center()
                            .when(space.pinned_collapsed || header_menu_open, |this| this.invisible())
                            .group_hover(header_group.clone(), |style| style.invisible())
                            .when_some(space.custom_icon.as_deref(), |this, icon| this.child(crate::icons::glyph(icon)))
                            .when(space.custom_icon.is_none() && icon_picker_open, |this| this.child(div().size(px(18.)).rounded(px(3.)).border_1().border_dashed().border_color(hsla(0., 0., 0., 0.25)))))
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("space-header-chevron-{index}"))()))
                            .absolute().top_0().left_0().size_full().invisible()
                            .when(space.pinned_collapsed || header_menu_open, |this| this.visible())
                            .group_hover(header_group.clone(), |style| style.visible())
                            .child(crate::icons::disclosure(space.pinned_collapsed)))))
                    .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("space-header-name-{index}"))()))
                        .flex_1().min_w_0().truncate().child(space.name.clone()))
                    .child(div().id(("space-header-more", index)).debug_selector(|| self.sidebar_debug_name((move || format!("space-header-more-{index}"))()))
                        .flex_none().size(px(18.)).invisible()
                        .when(header_menu_open, |this| this.visible())
                        .group_hover(header_group, |style| style.visible())
                        .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_space_menu(space_id, event.position(), window, cx);
                        }))
                        .child(crate::icons::more())),
            ))
            .when(!pinned.is_empty(), |this| this.child(
                div().id(("pinned-reveal", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "pinned-reveal".into())())))
                    .flex_none().h(px(pinned_height * reveal)).max_h(gpui::relative(0.45 * reveal))
                    .mb(px(-4. * (1. - reveal))).overflow_hidden()
                    .child(self.sidebar_list(space.id, Section::Pinned, cx).id(("pinned-list", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "pinned-list".into())())))
                        .relative().top(px(-12. * (1. - reveal))).size_full().flex().flex_col().gap(px(SIDEBAR_ROW_GAP)).overflow_y_scroll()
                        .when_some(self.sidebar_scrolls.get(&space.id), |this, scrolls| this.track_scroll(&scrolls.pinned))
                        .children(pinned)),
            ))
            // The divider doubles as the Today drop target. Tidy and Clear show on
            // sidebar hover, and the line shortens to make room for them.
            .child(div().id(("today-header", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "today-header".into())())))
                .relative().flex_none().h(px(if has_today { 22. } else { 0. })).rounded_md()
                .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag, space_id, Section::Today, cx)))
                .when(has_today, |this| this.child(div().debug_selector(|| self.sidebar_debug_name((move || format!("today-divider-{index}"))())).absolute().left(px(10.)).top(px(11.)).h(px(1.)).bg(hsla(0., 0., ink, 0.12))
                    .with_animation(SharedString::from(format!("today-line-{index}-{today_actions}")), hover_animation(), move |line, delta| {
                        let shown = if today_actions { delta } else { 1. - delta };
                        line.right(px(10. + line_end * shown))
                    })))
                .when(!space.today.is_empty(), |this| this.child(hover_fade(div().absolute().right(px(10.)).top_0().h_full().w(px(action_width))
                    .flex().items_center().justify_end()
                    .text_size(px(11.)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(hsla(0., 0., ink, 0.35))
                    .when(can_tidy, |this| this.child(div().id(("tidy-today", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "tidy-today".into())())))
                        .flex().items_center().gap(px(3.)).hover(move |s| s.text_color(hsla(0., 0., ink, 0.6)))
                        .on_click(cx.listener(move |this, _, _, cx| this.tidy_today(space_id, cx)))
                        .child(crate::icons::control_sized(crate::icons::Icon::Tidy, 12.)).child("Tidy").mr(px(16.))))
                    .child(div().id(("clear-today", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "clear-today".into())())))
                        .flex().items_center().gap(px(2.)).hover(move |s| s.text_color(hsla(0., 0., ink, 0.6)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let Some(index) = this.browser.spaces.iter().position(|space| space.id == space_id) else { return; };
                            this.edit_browser(|browser| browser.switch_space(index));
                            this.clear_today_tabs(cx);
                            this.settle_pager(cx);
                            cx.notify();
                        }))
                        .child(crate::icons::control_sized(crate::icons::Icon::Down, 12.))
                        .child("Clear")), SharedString::from(format!("today-actions-{index}")), today_actions))))
            .child(
                div()
                    .id(("new-tab", index))
                    .when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "new-tab".into())())))
                    .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag, space_id, Section::Today, cx)))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .h(px(SIDEBAR_ROW_HEIGHT))
                    .pl(px(9.))
                    .rounded(px(SIDEBAR_ROW_RADIUS))
                    .text_size(px(13.))
                    .text_color(hsla(0., 0., ink, 0.45))
                    .hover(move |s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_open_command_bar(&OpenCommandBar, window, cx)
                    }))
                    .child(crate::icons::control(crate::icons::Icon::Plus)).child("New Tab"),
            )
            .child(
                self.sidebar_list(space.id, Section::Today, cx)
                    .id(("today", index))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap(px(SIDEBAR_ROW_GAP))
                    .overflow_y_scroll()
                    .when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "today-list".into())())))
                    .when_some(self.sidebar_scrolls.get(&space.id), |this, scrolls| this.track_scroll(&scrolls.today))
                    .children(today),
            )
    }

    fn render_space_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ink = if self.theme_uses_light_ink_for(self.browser.space().id) {1.} else {0.};
        // Layout the stationary marks first, then paint the active dot between
        // their actual centers. The reference moves one dot along the rail.
        let dot_centers = Rc::new(RefCell::new(Vec::new()));
        let paint_centers = dot_centers.clone();
        let pager_position = self.pager.position;
        let active_dot_fill = self.theme_space_dot_fill(true);
        let drag_moves_active = self.sidebar_page_interactive;
        let new_menu_anchor = if self.sidebar_debug_namespace.is_none() { self.new_menu_anchor.clone() } else { Default::default() };
        let normal = !self.browser.is_blank() && !self.browser.is_private();
        let pages = self.sidebar_navigation.controller.pages(&self.browser);
        let selected = self.sidebar_navigation.controller.selected();
        let default_positions: Vec<_> = self.browser.spaces.iter().enumerate()
            .filter(|(_, space)| space.custom_icon.is_none())
            .map(|(index, space)| if normal {
                pages.iter().position(|page| *page == crate::sidebar_pages::SidebarPage::Space(space.id)).unwrap_or(index + 1)
            } else { index }).collect();
        let traveling_dot = default_positions.contains(&(pager_position.floor() as usize))
            && default_positions.contains(&(pager_position.ceil() as usize));
        let corner = |id: &'static str| div().id(id).debug_selector(|| self.sidebar_debug_name((move || id.into())()))
            .flex_none().size(px(32.)).rounded_md().flex().items_center().justify_center()
            .text_color(hsla(0., 0., ink, 0.5)).hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)));
        div()
            .flex_none()
            .h(px(SPACE_BAR_HEIGHT))
            .px(px(SIDEBAR_INSET))
            .flex()
            .items_center()
            .child(corner("library").relative()
                .when(normal && selected == crate::sidebar_pages::SidebarPage::Library, |this| this.bg(hsla(0., 0., 1., 0.3)))
                .when(normal, |this| this.on_click(cx.listener(|this, _, _, cx| {
                    this.select_sidebar_page(crate::sidebar_pages::SidebarPage::Library, cx);
                    this.settle_pager(cx);
                }))
            .child(canvas(|_, _, _| (), move |_, _, window, _| {
                let centers = paint_centers.borrow();
                let lower = pager_position.floor() as usize;
                let upper = pager_position.ceil() as usize;
                let left = centers.iter().find(|(index, _, _)| *index == lower);
                let right = centers.iter().find(|(index, _, _)| *index == upper);
                if let (Some((_, left, true)), Some((_, right, true))) = (left, right) {
                    let fraction = pager_position - lower as f32;
                    let center = *left + (*right - *left) * fraction;
                    window.paint_quad(gpui::fill(
                        Bounds::new(center - point(px(4.), px(4.)), size(px(8.), px(8.))),
                        active_dot_fill,
                    ).corner_radii(px(4.)));
                }
            }).absolute().inset_0()))
                .child(crate::icons::control(crate::icons::Icon::Library))
                .children(self.sidebar_navigation.downloads.progress.filter(|_|normal).map(|progress|
                    div().id("download-progress-badge").debug_selector(||"download-progress-badge".into()).absolute().right(px(-4.)).top(px(-6.)).size(px(20.))
                    .on_click(cx.listener(|this,_,_,cx|{cx.stop_propagation();this.open_library_downloads(cx);})).child(crate::downloads_view::progress_icon_sized(progress,20.))
                )))
            .child(div().relative().flex_1().min_w_0().flex().items_center().justify_center().gap(px(4.))
            .children(self.browser.spaces.iter().enumerate().flat_map(|(index, space)| {
                let space_id = space.id;
                let page_index = if normal {
                    pages.iter().position(|page| *page == crate::sidebar_pages::SidebarPage::Space(space_id)).unwrap_or(index + 1)
                } else { index };
                let active = page_index == self.pager.position.round() as usize;
                let centers = dot_centers.clone();
                let default_icon = space.custom_icon.is_none();
                let drag = crate::space_drag::SpaceDrag { id: space_id, workspace: self.profile.handle.clone(), name: space.name.clone(), icon: space.custom_icon.clone(), hue: space.hue };
                // Space marks are monochrome; only user emoji keep their colors.
                let mark = self.theme_space_dot_fill(active);
                let real_mark = div()
                    .id(("space", index))
                    .debug_selector(|| self.sidebar_debug_name((move || format!("space-{index}"))()))
                    .group(SharedString::from(format!("space-mark-{index}")))
                    .relative().flex_none().size(px(22.)).rounded_md().flex().items_center().justify_center()
                    .hover(|s| s.bg(hsla(0., 0., 1., 0.3)))
                    .when(space.custom_icon.is_none(), |this| this.child(div()
                        .debug_selector(|| self.sidebar_debug_name((move || format!("space-dot-{index}"))()))
                        .size(px(if active && (!traveling_dot || pager_position.fract() == 0.) { 8. } else { 4. })).rounded_full()
                        .bg(if active && !traveling_dot { mark } else if active && pager_position.fract() == 0. {
                            gpui::transparent_black()
                        } else { self.theme_space_dot_fill(false) })
                        .group_hover(SharedString::from(format!("space-mark-{index}")), |s| s.bg(hsla(0., 0., ink, if active { 0.8 } else { 0.5 })))))
                    .when_some(space.custom_icon.clone(), |this, icon| this.text_color(mark)
                        .opacity(if active { 1. } else { 0.5 })
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("space-icon-{index}"))())).child(crate::icons::glyph(&icon))))
                    .child(canvas(move |bounds, _, _| {
                        centers.borrow_mut().push((page_index, bounds.center(), default_icon));
                    }, |_, _, _, _| ()).absolute().inset_0())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(index) = this.browser.spaces.iter().position(|space| space.id == space_id) {
                            this.switch_space(index, cx);
                        }
                    }))
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .drag_over::<crate::space_drag::SpaceDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.8)))
                    .on_drop(cx.listener(move |this, drag: &crate::space_drag::SpaceDrag, _, cx| {
                        if this.profile.handle.same_workspace(&drag.workspace) {
                            if let Some(destination) = this.browser.spaces.iter().position(|space| space.id == space_id) {
                                this.reorder_space(drag.id, destination, cx);
                            }
                        }
                    }))
                    .on_drag_move::<crate::tab_drag::TabDrag>(cx.listener(move |this, event: &gpui::DragMoveEvent<crate::tab_drag::TabDrag>, _, cx| { if drag_moves_active { this.hover_space_drag(space_id, event.bounds.contains(&event.event.position), cx); } }))
                    .on_drag_move::<crate::folder_drag::FolderDrag>(cx.listener(move |this, event: &gpui::DragMoveEvent<crate::folder_drag::FolderDrag>, _, cx| { if drag_moves_active { this.hover_space_drag(space_id, event.bounds.contains(&event.event.position), cx); } }))
                    .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.8)))
                    .drag_over::<crate::folder_drag::FolderDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.8)))
                    .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_space(drag, space_id, cx)))
                    .on_drop(cx.listener(move |this, drag: &crate::folder_drag::FolderDrag, _, cx| this.drop_folder_on_space(drag, space_id, cx)))
                    .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_space_menu(space_id, event.position, window, cx);
                    }));
                let mut marks = vec![real_mark.into_any_element()];
                if self.sidebar_navigation.controller.draft().is_some_and(|draft| draft.origin == space_id) {
                    marks.push(div().id("space-creation-mark").debug_selector(|| "space-creation-mark".into())
                        .flex_none().size(px(22.)).rounded_md().flex().items_center().justify_center()
                        .text_color(hsla(0., 0., ink, 0.6))
                        .when(selected == crate::sidebar_pages::SidebarPage::CreateSpace, |mark| mark.bg(hsla(0., 0., 1., 0.3)))
                        .on_click(cx.listener(move |this, _, _, cx| this.begin_space_creation(space_id, cx)))
                        .child(crate::icons::control_sized(crate::icons::Icon::NewSpace, 16.)).into_any_element());
                }
                marks
            })))
            .child(div().id("new-space").debug_selector(|| self.sidebar_debug_name((|| "new-space".into())())).group("new-space")
                .relative().flex_none().size(px(32.)).flex().items_center().justify_center()
                .text_color(hsla(0., 0., ink, 0.5))
                .child(self.render_press_highlight("new-space", hsla(0.0, 0.0, 1.0, 0.3), self.new_menu.is_some()))
                .on_click(cx.listener(|this, _, window, cx| { this.press_control("new-space"); this.toggle_new_menu(window, cx); }))
                .child(self.render_new_menu_plus())
                .child(canvas(|_, _, _| (), move |bounds, _, _, _| new_menu_anchor.set(Some(bounds))).absolute().inset_0()))
    }

    /// Both normal content and Library mount this canvas inside their actual
    /// GPUI surface. Generation guards also cover deferred tracking updates.
    pub(crate) fn render_page_viewport_canvas(&self) -> Option<AnyElement> {
        let native_view = self.native_view;
        let mouse_tracking = self.mouse_tracking.clone();
        self.pages.clone().and_then(|pages| {
                        let generation = pages.try_borrow().ok()?.viewport_generation();
                        let divider_pages = pages.clone();
                        // Reports the card's bounds so the native page view can sit on it.
                        Some(canvas(
                            move |bounds, window, cx| {
                                let clip = bounds.intersect(&window.content_mask().bounds);
                                let clip = engine::Bounds { x: f64::from(f32::from(clip.origin.x)), y: f64::from(f32::from(clip.origin.y)), width: f64::from(f32::from(clip.size.width)), height: f64::from(f32::from(clip.size.height)) };
                                let bounds = engine::Bounds {
                                    x: f64::from(f32::from(bounds.origin.x)),
                                    y: f64::from(f32::from(bounds.origin.y)),
                                    width: f64::from(f32::from(bounds.size.width)),
                                    height: f64::from(f32::from(bounds.size.height)),
                                };
                                let surfaces = match pages.try_borrow_mut() {
                                    Ok(mut pages) => { if pages.set_viewport_bounds(generation, bounds) { pages.set_viewport_clip(generation, clip); Some(pages.surface_bounds()) } else { None } }
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
                                        let surfaces = { let mut pages = pages.borrow_mut(); if !pages.set_viewport_bounds(generation, bounds) { return; } pages.set_viewport_clip(generation, clip); pages.surface_bounds() };
                                        update_tracking(surfaces);
                                    });
                                }
                            },
                            move |_, _, window, _| {
                                if let Some(divider) = divider_pages.borrow().devtools_divider() {
                                    window.paint_quad(gpui::fill(Bounds::new(
                                        point(px(divider.x as f32), px(divider.y as f32)),
                                        size(px(divider.width as f32), px(divider.height as f32)),
                                    ), hsla(0., 0., 0.5, 0.3)));
                                }
                            },
                        )
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .into_any_element())
        })
    }

    fn render_top_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.developer_mode_enabled() { return self.render_developer_toolbar(cx); }
        let state = self.browser.active_tab().and_then(|tab| self.pages.as_ref()?.try_borrow().ok().map(|pages| pages.state(tab.id))).unwrap_or_default();
        let text = self.browser.active_tab().map(|tab| if self.browser.settings.enabled(browser_core::settings::Toggle::FullToolbarUrl) { tab.url.clone() } else { address_host(&tab.url) }).unwrap_or_default();
        let (background, ink) = toolbar_colors(state.document_color, self.theme_is_dark());
        let anchor = self.site_controls_anchor.clone();
        let button = |id: &'static str, icon| div().id(id).debug_selector(move || id.into()).flex_none().w(px(38.)).h(px(TOP_TOOLBAR_HEIGHT)).flex().items_center().justify_center().rounded(px(5.)).hover(move |s|s.bg(hsla(0.,0.,ink,0.08))).child(crate::icons::control_sized(icon, if matches!(icon, crate::icons::Icon::Reload) { 16. } else { 18. }));
        div().debug_selector(|| "top-toolbar".into()).absolute().top_0().left_0().right_0().h(px(TOP_TOOLBAR_HEIGHT)).px(px(2.)).flex().items_center().bg(background).border_b_1().border_color(hsla(0.,0.,ink,0.08)).text_color(hsla(0.,0.,ink,0.675))
            .child(button("toolbar-back",crate::icons::Icon::Back).opacity(if state.can_go_back {1.} else {0.37}).on_click(cx.listener(|this,_,_,_|this.with_active_page(|pages,tab|pages.go_back(tab)))))
            .child(button("toolbar-forward",crate::icons::Icon::Forward).opacity(if state.can_go_forward {1.} else {0.37}).on_click(cx.listener(|this,_,_,_|this.with_active_page(|pages,tab|pages.go_forward(tab)))))
            .child(button("toolbar-reload",if state.loading {crate::icons::Icon::Close} else {crate::icons::Icon::Reload}).on_click(cx.listener(move |this,_,_,_|this.with_active_page(|pages,tab|if state.loading {pages.stop_loading(tab)} else {pages.reload(tab)}))))
            .child(div().id("toolbar-address").debug_selector(||"toolbar-address".into()).flex_1().min_w_0().h_full().flex().items_center().justify_center().text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(hsla(0.,0.,ink,0.75))
                .on_click(cx.listener(|this,_,window,cx|this.on_edit_current_url(&EditCurrentUrl,window,cx)))
                .on_mouse_down(gpui::MouseButton::Right,cx.listener(|this,event:&gpui::MouseDownEvent,window,cx|this.open_sidebar_menu(crate::tab_menu::MenuTarget::Address,event.position,window,cx)))
                // Arc centers the hostname on the page; Copy URL precedes it.
                .child(div().relative().left(px(-32.)).px(px(32.)).min_w_0().flex().items_center()
                .child(div().id("toolbar-copy-url").text_color(hsla(0.,0.,ink,0.675)).debug_selector(||"toolbar-copy-url".into()).size(px(26.)).flex_none().flex().items_center().justify_center().on_click(cx.listener(|this,_,_,cx|{cx.stop_propagation();this.copy_url(false,cx);})).child(crate::icons::control(crate::icons::Icon::Link)))
                .child(div().min_w_0().truncate().child(text))))
            .child(button("toolbar-site-controls",crate::icons::Icon::SiteControls).on_click(cx.listener(|this,_,window,cx|this.toggle_site_controls(window,cx))).child(canvas(|_,_,_|(),move |bounds,_,_,_|anchor.set(Some(bounds))).absolute().inset_0()))
            .child(button("toolbar-split",crate::icons::Icon::Split).on_click(cx.listener(|this,_,_,cx|this.edit_split(None,cx))))
            .into_any_element()
    }

    fn render_content(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let virtual_page = !self.page_content_visible();
        let split_open = !virtual_page && self.browser.active_split().is_some();
        let has_page = !virtual_page && self.browser.active_tab().is_some();
        let empty_canvas = !virtual_page && !has_page;
        let viewport = window.viewport_size();
        let left = self.sidebar_width() * self.sidebar_amount() + CONTENT_INSET * (1. - self.sidebar_amount());
        let panel_bounds = Bounds::new(point(px(left), px(CONTENT_INSET)),
            size((viewport.width - px(left + CONTENT_INSET)).max(px(0.)), (viewport.height - px(CONTENT_INSET * 2.)).max(px(0.))));
        let panel_mask = crate::space_theme::rounded_theme_mask(panel_bounds, CONTENT_CORNER_RADIUS, window.scale_factor());
        let history_open = !virtual_page && self.history_is_open();
        let search_settings_open = !virtual_page && self.search_settings_owner().is_some();
        let more_settings_open = !virtual_page && self.more_settings_is_open();
        let site_settings_open = !virtual_page && (self.site_settings_is_open() || more_settings_open);
        let internal = self.browser.active_tab().is_some_and(|tab| omnibox::is_internal_url(&tab.url));
        // The page view covers the card; text only shows when there is no page.
        let placeholder = if virtual_page || split_open || history_open || search_settings_open || site_settings_open || (self.browser.is_private() && self.browser.active_tab().is_none()) { None } else if internal { Some(("Page not found".into(), self.browser.active_tab().unwrap().url.clone())) } else { match (self.browser.active_tab(), &self.pages) {
            (None, _) => None,
            (Some(tab), None) => Some((tab.title.clone(), tab.url.clone())),
            (Some(_), Some(_)) => None,
        }};
        div()
            .debug_selector(|| "browser-content".into())
            .flex_1()
            // Never grow wider than the window because of content.
            .min_w_0()
            .h_full()
            .py(px(CONTENT_INSET + if self.peek_child.is_some() { 9. } else { 0. }))
            .pr(px(CONTENT_INSET + if self.peek_child.is_some() { 9. } else { 0. }))
            .pl(px(CONTENT_INSET * (1. - self.sidebar_amount()) + if self.peek_child.is_some() { 9. } else { 0. }))
            .relative()
            .when(empty_canvas, |this| this.child(div()
                .debug_selector(|| "empty-space-panel-shadow".into())
                .absolute().top(px(CONTENT_INSET)).bottom(px(CONTENT_INSET))
                .right(px(CONTENT_INSET)).left(px(CONTENT_INSET * (1. - self.sidebar_amount())))
                .child(self.theme_empty_panel_shadow())))
            .child(
                div()
                    .debug_selector(|| "page-surface".into())
                    .on_drop(cx.listener(|this, drag: &crate::tab_drag::TabDrag, window, cx| this.drop_tab_into_split(drag, window.mouse_position(), window, cx)))
                    .left(px(self.library_page_offset()))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .when(!split_open, |this| this.px_8())
                    .when(!split_open, |this| this.overflow_hidden())
                    .when(has_page && !split_open, |this| this.rounded(px(CONTENT_CORNER_RADIUS)).bg(gpui::white()).shadow_md())
                    .when(self.browser.is_private() && has_page && !split_open, |this| this.bg(hsla(210. / 360., 0.035, 0.15, 1.)))
                    .when(empty_canvas, |this| this.rounded(px(CONTENT_CORNER_RADIUS)))
                    .relative()
                    .when(!has_page, |this| this.child(div().debug_selector(|| "empty-space-canvas".into()).absolute().inset_0()
                        .when(empty_canvas, |canvas| canvas
                            .child(self.theme_surface_mask(panel_bounds, viewport, panel_mask))
                            .child(div().absolute().inset_0().rounded(px(CONTENT_CORNER_RADIUS)).bg(self.theme_empty_panel_fill())))))
                    .child(div().debug_selector(|| "page-document-surface".into()).absolute().left_0().right_0().bottom_0()
                        .top(px(if has_page && self.page_toolbar_shown() && !split_open { self.page_toolbar_height() } else { 0. }))
                        .flex().flex_col().items_center().justify_center().gap_2()
                        .when(!split_open, |this| this.px_8())
                    .when(split_open, |this| this.child(self.render_split(cx)))
                    .when(history_open, |this| this.child(self.render_history(cx)))
                    .when(search_settings_open, |this| this.child(self.render_search_settings(window,cx)))
                    .when(site_settings_open, |this| this.child(if more_settings_open { self.render_more_settings_root() } else { self.render_site_settings_page(cx) }))
                    .children(placeholder.map(|(title, url)| {
                        div().debug_selector(|| "page-placeholder".into())
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
                    .children(self.render_page_viewport_canvas()))
                    .when(has_page && self.page_toolbar_shown() && !split_open, |this| this.child(self.render_top_toolbar(cx))),
            )
    }

    fn render_floating_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(floating) = &self.floating_sidebar else {
            return div().into_any_element();
        };
        let closing = floating.closing;
        let width = self.sidebar_width();
        let panel = div()
            .id("floating-sidebar")
            .debug_selector(|| self.sidebar_debug_name((|| "floating-sidebar".into())()))
            .absolute()
            .top(px(FLOATING_SIDEBAR_INSET))
            .bottom(px(FLOATING_SIDEBAR_INSET))
            .w(px(width))
            // Concentric with the window corner.
            .rounded(px(WINDOW_CORNER_RADIUS - FLOATING_SIDEBAR_INSET))
            .overflow_hidden()
            .shadow_lg()

            .when(self.browser.is_private(), |this| this.bg(hsla(210. / 360., 0.06, 0.05, 1.)))
            .child(self.theme_surface(false))
            .child(self.render_sidebar(cx))
            .child(self.render_sidebar_resize_handle(true, cx))
            // This overlay is its own native window with forwarded controls.
            // Keep its single control mount outside the page renderer too.
            .child(div().absolute().top_0().left_0().w(px(74.)).h(px(SIDEBAR_TOOLBAR_HEIGHT)).child(self.render_traffic_lights(cx)))
            .child(canvas(|_, _, _| (), |bounds, _, window, _| {
                if let Some(view) = crate::native::view_of(window) {
                    crate::native::position_traffic_lights(view, TRAFFIC_LIGHT_X + f32::from(bounds.origin.x) - FLOATING_SIDEBAR_INSET);
                }
            }).absolute().top_0().left_0().size_full());
        let slide = Animation::new(if crate::native::prefers_reduced_motion() { Duration::ZERO } else { FLOATING_SIDEBAR_SLIDE }).with_easing(ease_out_quint());
        let travel = width + FLOATING_SIDEBAR_INSET;
        div()
            .size_full()
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| { this.end_sidebar_resize(f32::from(event.position.x), window, cx); }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.sidebar_resize.is_some() { return; }
                this.on_floating_sidebar_pointer(f32::from(event.position.x), f32::from(event.position.y), cx);
            }))
            .child(self.render_sidebar_resize_tracking(cx))
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

    pub(crate) fn delete_command_history_entry(&mut self, owner: browser_core::UserProfileId, url: &str, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if self.browser.active_user_profile_id() != owner { return; }
        self.edit_browser(|browser| {
            if browser.active_user_profile_id() == owner { browser.delete_history_entry(owner, url); }
        });
        if let Some(bar) = &mut self.command_bar {
            if let Some(query) = bar.navigation_query.take() {
                bar.input.update(cx, |input, cx| input.preview_navigation_text(&query, cx));
            }
        }
        let count = self.suggestions(cx).len();
        if let Some(bar) = &mut self.command_bar { bar.selected = bar.selected.min(count.saturating_sub(1)); }
        cx.notify();
    }

    fn clear_command_bar_backdrop(&self) {
        if let Some(view) = self.native_view {
            use crate::material::*;
            crate::native::set_content_backdrop(view, "command-bar-page", None, 0., BackdropConfig {
                appearance: EffectAppearance::Dark, state: EffectState::Active,
                blending: EffectBlending::WithinWindow, emphasized: false,
                effect_alpha: 1., material: EffectMaterial::Menu,
            });
        }
    }

    fn render_command_bar_overlay(&self, width: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        match &self.command_bar {
            Some(bar) if matches!(bar.target, Target::Icon(_)) => { bar.material_alive.set(false); self.clear_command_bar_backdrop(); self.render_icon_picker(bar, width, height, cx) },
            Some(bar) => { bar.material_alive.set(true); self.render_command_bar(bar, width, height, cx).into_any_element() },
            None => div().into_any_element(),
        }
    }

    fn render_command_bar(&self, bar: &CommandBar, width: f32, height: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = self.theme_is_dark();
        let ink = if dark { 1. } else { 0. };
        let history_owner = self.browser.active_user_profile_id();
        let material_backdrop = bar.material_backdrop.clone();
        let material_alive = bar.material_alive.clone();
        let page_parent = self.native_view.map(|view| view as usize);
        let panel_fill = self.command_bar_panel_fill(material_backdrop.get());
        let row_selection = self.command_bar_selection();
        let page_selection = self.command_bar_page_selection();
        let chatgpt_keyword = matches!(bar.target, Target::NewTab | Target::Tab(_)) && bar.site_search.is_none()
            && self.profile.handle.read_settings(|b| b.settings.max.enabled(echo_ai::MaxFeature::ChatGptCommandBar))
            && bar.query(cx).trim().eq_ignore_ascii_case("chatgpt");
        let scope_color = if bar.site_search == Some(omnibox::SiteSearch::ChatGpt) { 0x64ab9c } else { 0xffa000 };
        let accent = self.accent();
        // Arc's split draft uses the ordinary window-wide Command Bar, even
        // when its empty pane is only 220 px wide.
        let panel_width = COMMAND_BAR_WIDTH.min((width - 32.).max(0.));
        let archive = matches!(bar.target, Target::Archive(_));
        // Arc anchors the top to the center of a nominal 338px panel. Result
        // changes grow down from that point instead of moving the input.
        let default_top = ((height - 338.) * 0.5).max(16.);
        let rows_height = (height - default_top - if archive { 150. } else { 90. } - 16.).max(40.);
        let top = default_top;
        let mut previous_date = None;
        let suggestions = self.suggestions(cx);
        let atc_selected = matches!(suggestions.get(bar.selected), Some(Suggestion::Action { command: Command::AirTrafficControl, .. }));
        let preview_url = match suggestions.get(bar.selected) {
            Some(Suggestion::SwitchToTab { id, url, .. }) => Some((url.clone(), Some(*id))),
            Some(Suggestion::History { url, .. }) => Some((url.clone(), None)),
            Some(Suggestion::Load(Resolved::Url(url))) => Some((url.clone(), match bar.target {
                Target::Tab(id) if self.browser.all_tabs().any(|tab| tab.id == id && tab.url == *url) => Some(id),
                _ => None,
            })),
            _ => None,
        };
        let rows: Vec<_> = suggestions.into_iter()
            .enumerate()
            .map(|(index, suggestion)| {
                let selected = index == bar.selected;
                let date = match &suggestion {
                    Suggestion::ArchivedTab { date, .. } if previous_date.as_ref() != Some(date) => { previous_date = Some(date.clone()); Some(date.clone()) }
                    _ => None,
                };
                let current_url_tab = match (&suggestion, bar.target) {
                    (Suggestion::Load(Resolved::Url(url)), Target::Tab(id)) => self.browser.all_tabs().find(|tab| tab.id == id && tab.url == *url),
                    _ => None,
                };
                let page_result = matches!(&suggestion, Suggestion::SwitchToTab { .. } | Suggestion::History { .. } | Suggestion::Load(Resolved::Url(_)));
                let page = match &suggestion {
                    Suggestion::Load(Resolved::Url(url)) => Some(self.render_command_page_icon(url, current_url_tab.map(|tab| tab.id), dark && !selected)),
                    Suggestion::SwitchToTab { id, url, .. } => Some(self.render_command_page_icon(url, Some(*id), dark && !selected)),
                    Suggestion::SplitTab { id, .. } => self.browser.all_tabs().find(|tab| tab.id == *id).map(|tab| self.render_favicon_on(tab, accent, !selected)),
                    _ => None,
                };
                let archived_id = match &suggestion { Suggestion::ArchivedTab { id, .. } => Some(*id), _ => None };
                let history_url = match &suggestion { Suggestion::History { url, .. } => Some(url.clone()), _ => None };
                let history_result = history_url.is_some();
                let row_group = SharedString::from(format!("command-result-{index}"));
                let ask_chatgpt = matches!(&suggestion, Suggestion::AskChatGpt { .. });
                let direct_search = matches!(&suggestion, Suggestion::Load(Resolved::Search { .. }));
                let search_row = direct_search || matches!(&suggestion, Suggestion::Calculator { .. });
                let (icon, label, detail): (crate::icons::Icon, String, String) = match suggestion {
                    Suggestion::AskChatGpt { query } => (crate::icons::Icon::Search, query, "Ask ChatGPT".into()),
                    Suggestion::Load(Resolved::Search { query, .. }) => {
                        (crate::icons::Icon::Search, query, String::new())
                    }
                    Suggestion::Calculator { answer } => (crate::icons::Icon::Calculator, format!("= {answer}"), "Copy".into()),
                    Suggestion::Load(Resolved::Url(url)) => {
                        let display = display_url(&url);
                        let label = current_url_tab.map(|tab| self.browser.item_title(tab.id)).unwrap_or_else(||
                            std::iter::once(&("chatgpt.com", "ChatGPT")).chain(omnibox::TOP_DOMAINS.iter()).find(|(domain, _)| display.trim_end_matches('/') == *domain)
                                .map_or(display.clone(), |(_, name)| format!("{name} — {display}")));
                        (crate::icons::Icon::External, label, String::new())
                    }
                    Suggestion::SplitTab { title, space, .. } => (crate::icons::Icon::Enter, title, space),
                    Suggestion::History { title, url } => (crate::icons::Icon::History, format!("{title} — {}", display_url(&url)), String::new()),
                    Suggestion::SwitchToTab { title, space, .. } => {
                        let hint = if space != self.browser.space().name { space }
                            else { "Switch to Tab".into() };
                        (crate::icons::Icon::Enter, title, hint)
                    }
                    Suggestion::Action { command: Command::AirTrafficControl, label } => (crate::icons::Icon::AirTraffic, label, String::new()),
                    Suggestion::Action { label, .. } => (crate::icons::Icon::Command, label, String::new()),
                    Suggestion::SaveFolderName { name } => (crate::icons::Icon::Folder, name, if matches!(bar.target, Target::RenameFolder(_)) { "Rename Folder".into() } else { "Create Folder".into() }),
                    Suggestion::FolderDestination { label, .. } => (crate::icons::Icon::Folder, label, "Move".into()),
                    Suggestion::SaveIcon { icon } => (crate::icons::Icon::Enter, icon.unwrap_or_else(|| "Reset Icon".into()), match bar.target { Target::Icon(crate::icons::IconTarget::Space(_)) => "Space Icon".into(), Target::Icon(crate::icons::IconTarget::Folder(_)) => "Folder Icon".into(), _ => "Tab Icon".into() }),
                    Suggestion::SaveTabName { name } => (crate::icons::Icon::Enter, name, "Rename Tab".into()),
                    Suggestion::SaveSpace { name } => (crate::icons::Icon::Plus, name, if bar.target == Target::NewSpace { "Create Space".into() } else { "Rename Space".into() }),
                    Suggestion::ArchivedTab { title, detail, .. } => (crate::icons::Icon::Restore, title, detail),
                    Suggestion::MoreArchive { remaining } => (crate::icons::Icon::Down, "Show More".into(), format!("{remaining} more archived {}", if remaining == 1 { "tab" } else { "tabs" })),
                    Suggestion::ConfirmClearArchive => (crate::icons::Icon::Close, "Clear Archive".into(), "Permanently delete".into()),
                };
                // Pages show their favicon; commands a glyph on a faint tile.
                // The selected row's tile turns white, as in the reference.
                let tile = div().size(px(24.0)).flex_none().flex().items_center().justify_center().rounded(px(6.0))
                    .when(!history_result && !search_row && !ask_chatgpt && selected && page.is_none(), |this| this.bg(gpui::white()).text_color(hsla(0.0, 0.0, 0.12, 1.0)))
                    .when(!history_result && !search_row && !ask_chatgpt && !selected && page.is_none(), |this| this.bg(hsla(0.0, 0.0, 1.0, 0.07)).text_color(hsla(0.0, 0.0, 1.0, 0.7)))
                    .when(history_result, |tile| tile.text_color(hsla(0., 0., if selected { 1. } else { ink }, 0.7)))
                    .when(selected && matches!(icon, crate::icons::Icon::AirTraffic), |this| this.text_color(gpui::rgb(0x6e74ff)))
                    .child(page.unwrap_or_else(|| crate::icons::control_sized(icon, 14.0)));
                let label_len = label.len();
                let highlights = history_url.as_ref().map(|url| (
                    label_len - display_url(url).len() - " — ".len()..label_len,
                    gpui::HighlightStyle {
                        fade_out: Some(if selected { 0.35 } else { 0.6 }),
                        font_weight: Some(gpui::FontWeight::MEDIUM),
                        ..Default::default()
                    },
                ));
                let label = gpui::StyledText::new(label).with_highlights(highlights);
                let row = div()
                    .id(("suggestion", index))
                    .group(row_group.clone())
                    .debug_selector(|| self.sidebar_debug_name((move || format!("suggestion-{index}"))()))
                    .when(ask_chatgpt, |this| this.debug_selector(|| "ask-chatgpt".into()))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .h(px(46.0)).flex_none()
                    .pl(px(10.0)).pr(px(11.0))
                    .mx_2()
                    .rounded(px(8.0))
                    .text_color(hsla(0.0, 0.0, if selected { 1. } else { ink }, if selected { 1.0 } else { 0.82 }))
                    .when(selected, |this| this.bg(if bar.site_search.is_some() { gpui::rgb(scope_color).into() } else if ask_chatgpt { gpui::rgb(0x64ab9c).into() } else if page_result || direct_search || (chatgpt_keyword && index == 0) { page_selection } else { row_selection }))
                    .when(!selected, |this| this.hover(|s| s.bg(hsla(0.0, 0.0, ink, if direct_search || ask_chatgpt { 0.05 } else { 0.04 }))))
                    .on_click(cx.listener(move |this, _, _, cx| this.confirm(index, cx)))
                    .child(tile)
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(260.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(hsla(0.0, 0.0, if selected { 1. } else { ink }, if selected { 0.95 } else { 0.4 }))
                            .child(detail),
                    )
                    .when_some(archived_id, |this, id| this.child(div()
                        .id(("delete-archived", id.get() as usize))
                        .debug_selector(|| self.sidebar_debug_name((move || format!("delete-archived-{}", id.get()))()))
                        .px_2().rounded_md().hover(|s| s.bg(hsla(0.0, 0.0, ink, 0.12)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_archive_entry(id, cx);
                        })).child(crate::icons::control(crate::icons::Icon::Close))))
                    .when_some(history_url, |this, url| this.child(div()
                        .id(("delete-suggestion", index))
                        .debug_selector(|| self.sidebar_debug_name((move || format!("delete-suggestion-{index}"))()))
                        .px_2().rounded_md().invisible().group_hover(row_group.clone(), |s| s.visible())
                        .hover(|s| s.bg(hsla(0.0, 0.0, ink, 0.12)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_command_history_entry(history_owner, &url, cx);
                        })).child(crate::icons::control(crate::icons::Icon::Close))))
                    .when(!search_row && current_url_tab.is_none(), |this| this.child(div().size(px(24.0)).flex_none().flex().items_center().justify_center().rounded(px(5.0))
                        .when(history_result && !selected, |arrow| arrow.invisible().group_hover(row_group, |s| s.visible()))
                        .bg(if selected { gpui::white() } else { hsla(0.0, 0.0, ink, 0.06) })
                        .text_color(if selected && ask_chatgpt { gpui::rgb(0x64ab9c).into() } else if selected && page_result { page_selection } else if selected { hsla(0.0, 0.0, 0.12, 1.0) } else { hsla(0.0, 0.0, ink, 0.6) })
                        .child(crate::icons::control_sized(crate::icons::Icon::Arrow, 15.0))));
                div().flex().flex_col().flex_none()
                    .children(date.map(|date| div().debug_selector(|| self.sidebar_debug_name((move || format!("archive-date-{index}"))())).h(px(24.)).flex_none().px_4().text_xs().text_color(hsla(0., 0., 1., 0.5)).child(date)))
                    .child(row).into_any_element()
            })
            .collect();
        let has_rows = !rows.is_empty();

        // Full-window backdrop; clicking outside the panel dismisses it.
        div()
            .id("command-bar-backdrop")
            .debug_selector(|| self.sidebar_debug_name((|| "command-bar-backdrop".into())()))
            .size_full()
            // The overlay window is square; match the browser window's corners.
            .rounded(px(WINDOW_CORNER_RADIUS))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(top))
            .bg(gpui::transparent_black())
            .on_click(cx.listener(|this, _, _, cx| {this.boost_creation_pending=false;this.close_command_bar(cx);}))
            .child(
                div()
                    .id("command-bar")
                    .debug_selector(|| self.sidebar_debug_name((|| "command-bar".into())()))
                    .key_context(COMMAND_BAR_CONTEXT)
                    .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "tab" && this.scope_site_search(cx) { cx.stop_propagation(); }
                        if event.keystroke.key == "backspace" {
                            if let Some(bar) = &mut this.command_bar {
                                if bar.site_search.is_some() && bar.input.read(cx).text().is_empty() {
                                    bar.site_search = None;
                                    bar.input.update(cx, |input, cx| input.set_placeholder("Search or Enter URL…", cx));
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }
                        }
                    }))
                    .on_action(cx.listener(|this, _: &OpenCommandBar, _, cx| { this.toggle_command_target(Target::NewTab, "", cx); }))
                    .on_action(cx.listener(|this, _: &EditCurrentUrl, _, cx| {
                        let (target, text) = this.browser.active_tab().map(|tab| (Target::Tab(tab.id), tab.url.clone())).unwrap_or((Target::NewTab, String::new()));
                        this.toggle_command_target(target, &text, cx);
                    }))
                    .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select(1, cx)))
                    .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.select(-1, cx)))
                    .on_action(cx.listener(|this, _: &ConfirmBackground, _, cx| {
                        let index = this.command_bar.as_ref().map_or(0, |bar| bar.selected);
                        this.confirm_with_background(index, true, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ConfirmInstant, _, cx| {
                        if !this.start_instant_links(cx) {
                            let index = this.command_bar.as_ref().map_or(0, |bar| bar.selected);
                            this.confirm(index, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                        let index = this.command_bar.as_ref().map_or(0, |bar| bar.selected);
                        this.confirm(index, cx)
                    }))
                    .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                        this.boost_creation_pending=false;
                        this.close_command_bar(cx);
                        if this.transient.is_some() && this.browser.active_tab().is_none() { this.close_window(cx); }
                    }))
                    // Keep clicks inside the panel from reaching the backdrop.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .w(px(panel_width))
                    .relative()
                    .flex()
                    .flex_col()
                    .rounded(px(COMMAND_BAR_RADIUS))
                    .bg(panel_fill)
                    .child(div().absolute().inset_0().child(self.command_bar_shadow()))
                    .border_1()
                    .border_color(if dark { hsla(0., 0., 0., 0.9) } else { hsla(0., 0., 1., 0.8) })
                    .child(div().absolute().inset(px(1.)).rounded(px(COMMAND_BAR_RADIUS - 1.))
                        .border_1().border_color(hsla(0., 0., ink, if dark { 0.2 } else { 0.08 })))
                    .text_color(hsla(0.0, 0.0, ink, 0.92))
                    .font_family(".SystemUIFont")
                    .child(gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                        // A queued child frame must not recreate a material
                        // after its owner closes or changes to the icon picker.
                        if !material_alive.get() { return; }
                        let installed = page_parent.is_some_and(|view| {
                            use crate::material::*;
                            crate::native::set_content_backdrop(view as *mut std::ffi::c_void, "command-bar-page", Some([
                                f32::from(bounds.origin.x) as f64, f32::from(bounds.origin.y) as f64,
                                f32::from(bounds.size.width) as f64, f32::from(bounds.size.height) as f64,
                            ]), COMMAND_BAR_RADIUS as f64, BackdropConfig {
                                appearance: if dark { EffectAppearance::Dark } else { EffectAppearance::Light }, state: EffectState::Active,
                                blending: EffectBlending::WithinWindow, emphasized: false,
                                effect_alpha: 1., material: EffectMaterial::Menu,
                            }) == BackdropInstallation::Installed
                        });
                        if material_backdrop.replace(installed) != installed { window.refresh(); }
                    }).absolute().inset_0())
                    .child(
                        div()
                            .h(px(COMMAND_BAR_HEADER - 1.)).flex_none()
                            .flex().items_center().gap(px(13.0))
                            .pl(px(22.0)).pr(px(21.0))
                            .child(div().text_color(hsla(0.0, 0.0, ink, 0.75)).child(
                                if atc_selected { crate::icons::control_sized(crate::icons::Icon::AirTraffic, 16.0) } else if let Some((url, tab)) = &preview_url {
                                    self.render_command_page_icon(url, *tab, dark)
                                } else if let Target::Tab(id) = bar.target {
                                    self.browser.all_tabs().find(|tab| tab.id == id)
                                        .map(|tab| self.render_favicon_on(tab, accent, dark))
                                        .unwrap_or_else(|| crate::icons::control_sized(crate::icons::Icon::Search, 16.0))
                                } else { crate::icons::control_sized(crate::icons::Icon::Search, 16.0) }))
                            .when_some(bar.site_search, |this, site| this.child(div()
                                .debug_selector(|| "site-search-pill".into())
                                .rounded_full().px(px(10.)).h(px(24.)).flex().items_center()
                                .bg(gpui::rgb(scope_color)).text_color(gpui::white())
                                .text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD).child(site.name(&self.browser.active_user_profile().settings.search).unwrap_or_default().to_owned())))
                            .child(div()
                                .when(bar.site_search == Some(omnibox::SiteSearch::ChatGpt), |this| this.ml(px(-3.)))
                                .debug_selector(|| self.sidebar_debug_name((|| "command-bar-input".into())()))
                                .flex_1().min_w_0()
                                .text_size(px(18.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .line_height(px(28.0))
                                .child(bar.input.clone()))
                            .when(chatgpt_keyword, |this| this.child(div().debug_selector(|| "chatgpt-tab-hint".into())
                                .flex().items_center().gap(px(9.)).text_size(px(13.)).text_color(hsla(0.,0.,ink,0.4))
                                .child("Ask ChatGPT").child(div().px(px(4.)).py(px(2.)).rounded(px(3.)).bg(hsla(0.,0.,ink,0.08)).child("Tab"))))
                            .when(matches!(bar.target, Target::NewTab | Target::Tab(_)) && bar.site_search.is_none() && !chatgpt_keyword, |this| this.child(div()
                                .id("suggestions-info").debug_selector(|| "suggestions-info".into())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(bar) = &mut this.command_bar { bar.suggestions_privacy = !bar.suggestions_privacy; }
                                    cx.notify();
                                }))
                                .size(px(24.0)).flex_none().flex().items_center().justify_center().rounded_full()
                                .bg(hsla(0.0, 0.0, 1. - ink, 0.8)).text_color(hsla(0.0, 0.0, ink, 0.55))
                                .child(crate::icons::control_sized(crate::icons::Icon::Info, 17.0)))),
                    )
                    .when(matches!(bar.target, Target::Archive(_)), |this| this.child(self.render_archive_controls(cx)))
                    .when(bar.target == Target::ConfirmClearArchive, |this| this.child(div().px_4().pb_2().text_sm().child("This action is permanent. Archived tabs from all Spaces will be deleted.")))
                    .when(!has_rows && matches!(bar.target, Target::Archive(_)), |this| this.child(div().px_4().py_3().text_sm().child("No archived tabs match your search.")))
                    .when(has_rows, |this| {
                        this.child(div().h(px(1.0)).bg(hsla(0.0, 0.0, ink, 0.1)))
                            .text_size(px(if archive { 14.0 } else { 13.0 }))
                            .child(div().id("command-suggestions").debug_selector(|| self.sidebar_debug_name((|| "command-suggestions".into())()))
                                .flex().flex_col().gap_1().pt(px(6.0)).pb(px(10.0)).max_h(px(rows_height + 16.)).overflow_y_scroll().track_scroll(&bar.rows_scroll).children(rows))
                    })
                    .when(bar.suggestions_privacy, |this| this.child(div()
                        .debug_selector(|| "suggestions-privacy".into())
                        .flex().items_center().gap_3().px_4().py_3()
                        .bg(gpui::black()).text_color(gpui::white()).rounded_b(px(COMMAND_BAR_RADIUS))
                        .child(crate::icons::control_sized(crate::icons::Icon::Info, 16.))
                        .child(div().flex_1().text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM)
                            .child("We talk to your search engine to help us suggest results."))
                        .child(div().id("dismiss-suggestions-privacy").debug_selector(|| "dismiss-suggestions-privacy".into())
                            .rounded(px(6.)).px_3().py_2().text_size(px(13.))
                            .bg(hsla(accent, 0.3, 0.3, 1.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit_browser(|browser| browser.active_user_profile_mut().settings.search.notice_dismissed = true);
                                if let Some(bar) = &mut this.command_bar { bar.suggestions_privacy = false; }
                                cx.notify();
                            })).child("Dismiss")))),
            )
    }
}

impl Render for BrowserWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(pages)=&self.pages { pages.borrow_mut().sync_shortcuts(crate::shortcuts::page(cx)); }
        if window.is_window_active() {
            let sidebar = self.command_bar.is_none() && self.tab_rename.is_none()
                && !self.browser.active_tab().is_some_and(|tab| self.pages.as_ref().is_some_and(|pages| pages.borrow().state(tab.id).editable));
            let labels = if let Some(owner) = self.peek_owner.as_ref().and_then(|state| state.owner.upgrade()) {
                let owner = owner.read(cx); (owner.browser.sidebar_undo_action(), owner.browser.sidebar_redo_action())
            } else { (self.browser.sidebar_undo_action(), self.browser.sidebar_redo_action()) };
            crate::menus::sync_pinned(self.browser.active_tab().is_some_and(|tab| tab.pinned_url.is_some()), cx);
            crate::menus::sync_developer_mode(self.developer_mode_enabled(), cx);
            crate::menus::sync_appearance(self.theme_appearance(), cx);
            crate::menus::sync_sidebar_undo(if sidebar { labels.0 } else { None }, if sidebar { labels.1 } else { None }, self.sidebar_selection_items().len() > 1, cx);
        }
        if crate::quit::is_quitting() {
            return div().size_full().p_8().bg(gpui::rgb(0x202126)).text_color(gpui::rgb(0xf2f2f2))
                .child(crate::quit::diagnostic(cx)).into_any_element();
        }
        self.refresh_profile_projection();
        self.reconcile_managed_search(cx);
        self.sync_search_settings();
        self.start_max_title_jobs(cx);
        self.sync_site_settings(window,cx);
        if self.peek_owner.is_some() { return self.render_peek(window, cx); }
        self.sync_library_geometry(window.viewport_size(), cx);
        let theme_blurred = crate::native::view_of(window).is_some_and(|view| {
            use crate::material::*;
            crate::native::set_backdrop_configured(view, "space-theme", Some([0., 0., f32::from(window.viewport_size().width) as f64, f32::from(window.viewport_size().height) as f64]), WINDOW_CORNER_RADIUS as f64,
                BackdropConfig { appearance: if self.theme_is_dark() { EffectAppearance::Dark } else { EffectAppearance::Light }, state: EffectState::FollowWindow, blending: EffectBlending::BehindWindow, emphasized: false, effect_alpha: 1., material: EffectMaterial::WindowBackground }) == BackdropInstallation::Installed
        });
        if self.theme_background_blurred != Some(theme_blurred) {
            window.set_background_appearance(if theme_blurred { WindowBackgroundAppearance::Transparent } else { WindowBackgroundAppearance::Opaque });
            self.theme_background_blurred = Some(theme_blurred);
        }
        if self.tab_rename.as_ref().is_some_and(|editor| !self.can_rename_tab_inline_for(editor.space, editor.profile, editor.tab)) {
            self.finish_tab_rename(false, cx);
        }
        self.validate_password_prompt(cx);
        self.poll_download_ui(cx);
        self.sync_library_downloads(window, cx);
        self.remember_library_preview_focus(window, cx);
        let open: std::collections::HashSet<_> = self.browser.all_tabs().map(|tab| tab.id).collect();
        self.tab_drop_edges.get_mut().retain(|id, _| open.contains(id));
        for space in &self.browser.spaces { self.sidebar_scrolls.entry(space.id).or_default(); }
        if self.page_content_visible() && self.setup.is_none() && self.user_profile_dialog.is_none() { self.sync_history_focus(window, cx); }
        if self.tab_menu.as_ref().is_some_and(|menu| !self.menu_is_current(menu)) {
            self.close_tab_menu(cx);
        }
        if self.find_bar.as_ref().is_some_and(|bar| Some(bar.tab) != self.browser.space().active_tab) {
            self.close_find(cx);
        }
        self.sync_page_viewport();
        if let Some(pages) = &self.pages {
            match pages.try_borrow_mut() {
                Ok(mut pages) => {
                    pages.preview_escape=self.max.preview.open.is_some();
                    pages.set_top_toolbar(self.page_toolbar_shown() && self.browser.active_split().is_none());
                    pages.set_top_toolbar_height(self.page_toolbar_height());
                    pages.sync(&self.browser, self.page_content_visible());
                    pages.sync_color_schemes(self.theme_appearance());
                },
                // Re-entered from engine work; sync on the next frame.
                Err(_) => cx.on_next_frame(window, |_, _, cx| cx.notify()),
            }
        }
        if self.split_drop.is_some() && !cx.has_active_drag() { self.close_split_drop(cx); }
        self.sync_max_previews(window,cx);
        self.sync_link_status(window, cx);
        self.sync_page_zoom(window, cx);
        self.sync_page_swipe(window,cx);
        self.sync_page_loading(window, cx);
        let sidebar_amount = self.sidebar_amount();
        let sidebar_offset = self.sidebar_width() * (sidebar_amount - 1.);
        let window_controls_offset = self.window_controls_offset();
        if let Some(view) = crate::native::view_of(window) {
            crate::native::position_traffic_lights(view, if self.transient.is_some() { 10. } else { TRAFFIC_LIGHT_X + window_controls_offset });
        }

        let library_open = !self.page_content_visible() && self.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Library;
        let sidebar = (!library_open && sidebar_amount > 0.).then(|| self.render_sidebar(cx));
        let library = library_open.then(|| self.render_library_host(cx));
        let compact = self.transient.is_some();
        let content = if compact { self.render_transient_content(cx) } else if library_open { div().into_any_element() } else { self.render_content(window, cx).into_any_element() };
        div()
            .key_context(if self.sidebar_resize.is_some() { "BrowserWindow SidebarResizing" } else { WINDOW_CONTEXT })
            .when(library_open, |this| this.on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_sidebar_scroll(event, cx))))
            .track_focus(&self.focus_handle)
            .on_mouse_down(gpui::MouseButton::Left,cx.listener(|this,_,_,cx|this.close_max_preview(cx)))
            .on_drag_move::<crate::tab_drag::TabDrag>(cx.listener(|this, event: &gpui::DragMoveEvent<crate::tab_drag::TabDrag>, window, cx| {
                let drag = event.drag(cx).clone();
                this.update_split_drop(&drag, event.event.position, window, cx);
            }))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| { if !this.capture_max_preview_key(event,cx) {this.capture_sidebar_resize_key(event, window, cx);} }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| { this.end_sidebar_resize(f32::from(event.position.x), window, cx); }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.sidebar_resize.is_some() { return; }
                this.close_link_status(cx);
                this.on_floating_sidebar_pointer(f32::from(event.position.x), f32::from(event.position.y), cx);
                if !this.sidebar_visible && event.position.x < px(SIDEBAR_REVEAL_EDGE) {
                    this.reveal_sidebar(window, cx);
                }
            }))
            .on_action(cx.listener(Self::on_open_command_bar))
            .on_action(cx.listener(|this, _: &crate::menus::BlankWindow, _, cx| this.blank_window(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::NewWindow, _, cx| this.new_window(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::Settings, _, cx| this.open_settings(cx)))
            .on_modifiers_changed(cx.listener(|this, event: &gpui::ModifiersChangedEvent, _, cx| {
                if this.tab_switcher.is_some() && !event.modifiers.control { this.finish_recent_items(true,PopupFocus::Restore,cx); }
            }))
            .on_action(cx.listener(|this, _: &crate::tab_switcher::NextRecentItem, window, cx| this.show_recent_items(false,window,cx)))
            .on_action(cx.listener(|this, _: &crate::tab_switcher::PreviousRecentItem, window, cx| this.show_recent_items(true,window,cx)))
            .on_action(cx.listener(|this, _: &crate::menus::About, _, cx| this.open_menu_utility(crate::about::open, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::CopyBrowserInfo, _, cx| this.copy_browser_info(cx)))
            .on_action(cx.listener(|this, action: &crate::standard_edit::Perform, window, cx| this.perform_standard_edit(action.0, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::RevealData, _, cx| this.open_menu_utility(crate::diagnostics::reveal_data, cx)))
            .on_action(cx.listener(|this, _: &crate::task_manager::OpenTaskManager, _, cx| this.open_menu_utility(crate::task_manager::open, cx)))
            .on_action(cx.listener(|this, _: &crate::essential_shortcuts::EssentialShortcuts, _, cx| this.open_menu_utility(crate::essential_shortcuts::open, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ImportFromBrowser, _, cx| this.open_setup(false,cx)))
            .on_action(cx.listener(|this, _: &crate::menus::SetDefaultBrowser, _, cx| this.request_default_browser(cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::NewProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_creation(crate::user_profile_dialog::CreationSource::File(space), cx); }))
                .on_action(cx.listener(|this, _: &crate::menus::ChangeProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_switch(space, cx); })))
            .on_action(cx.listener(|this, _: &crate::menus::EditTheme, window, cx| { let space = this.browser.space().id; this.open_theme_editor(space, window, cx); }))
            .on_action(cx.listener(|this, _: &crate::menus::CloseWindow, _, cx| this.close_window(cx)))
            .when(self.tab_rename.is_none(), |this| this
                .on_action(cx.listener(|this, _: &crate::text_input::Undo, window, cx| this.edit_undo(false, window, cx)))
                .on_action(cx.listener(|this, _: &crate::text_input::Redo, window, cx| this.edit_undo(true, window, cx))))
            .on_action(cx.listener(|this, _: &crate::history::ViewHistory, window, cx| this.open_history(window, cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &ViewLibrary, _, cx| this.run_command(Command::OpenLibrary, cx))).on_action(cx.listener(|this, _: &ViewDownloads, _, cx| this.run_command(Command::ViewDownloads, cx))).on_action(cx.listener(|this, _: &crate::menus::ManageSpaces, _, cx| this.run_command(Command::ManageSpaces, cx))))
            .on_action(cx.listener(|this, _: &crate::text_input::PasteAndMatchStyle, _, cx| this.paste_and_match_style(cx)))
            .when(self.save_page_available(), |this| this.on_action(cx.listener(|this, _: &SavePage, _, cx| this.save_page(cx))))
            .when(self.can_share_page(), |this| this.on_action(cx.listener(|this, _: &SharePage, _, cx| this.share_page(cx))))
            .when(self.print_available(), |this| this.on_action(cx.listener(|this, _: &PrintPage, _, cx| this.print_page(cx))))
            .when(self.current_source_url().is_some(), |this| this.on_action(cx.listener(|this, _: &ViewSource, _, cx| this.view_source(cx))))
            .when(self.devtools_available(), |this| this
                .on_action(cx.listener(|this, _: &DeveloperTools, _, cx| this.run_devtools(crate::devtools::Command::Toggle, cx)))
                .on_action(cx.listener(|this, _: &InspectElements, _, cx| this.run_devtools(crate::devtools::Command::Elements, cx)))
                .on_action(cx.listener(|this, _: &JavaScriptConsole, _, cx| this.run_devtools(crate::devtools::Command::Console, cx)))
                .on_action(cx.listener(|this, _: &NetworkInspector, _, cx| this.run_devtools(crate::devtools::Command::Network, cx))))
            .on_action(cx.listener(|this, _: &AddSplit, window, cx| this.open_command_action(Command::AddSplit(None), window, cx)))
            .on_action(cx.listener(|this, _: &CloseSplitPane, _, cx| this.run_command(Command::CloseSplitPane, cx)))
            .on_action(cx.listener(|this, _: &ExpandCurrentSplit, _, cx| this.run_command(Command::ExpandCurrentSplit, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::Minimize, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Minimize, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ZoomWindow, _, cx| this.perform_window_operation(crate::menus::WindowOperation::Zoom, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ToggleFullscreen, _, cx| this.perform_window_operation(crate::menus::WindowOperation::ToggleFullscreen, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::RenameSpace, window, cx| this.open_command_action(Command::RenameSpace, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::RenameTab, window, cx| this.open_command_action(Command::RenameTab, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ViewArchive, window, cx| this.open_command_action(Command::ViewArchive, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ClearArchive, window, cx| this.open_command_action(Command::RequestClearArchive, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ResetTabName, _, cx| this.run_command(Command::ResetTabName, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ResetPinnedTab, _, cx| this.run_command(Command::ResetPinnedTab, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ReplacePinnedUrl, _, cx| this.run_command(Command::ReplacePinnedUrl, cx)))
            .when(self.browser.space().pinned.iter().any(|tab| tab.pinned_url.is_some()), |view| view.on_action(cx.listener(|this, _: &crate::menus::ResetSpace, _, cx| this.run_command(Command::ResetSpace, cx))))
            .on_action(cx.listener(|this, _: &NewSpace, window, cx| this.begin_space_creation(this.browser.space().id, cx)))
            .on_action(cx.listener(Self::on_edit_current_url))
            .on_action(cx.listener(|this, _: &CopyUrl, _, cx| this.copy_url(false, cx)))
            .on_action(cx.listener(|this, _: &CopyUrlAsMarkdown, _, cx| this.copy_url(true, cx)))
            .on_action(cx.listener(|this, _: &CopyUrlAsQuote, _, cx| this.copy_url_as_quote(cx)))
            .on_action(cx.listener(|this, _: &CapturePage, _, cx| this.capture_page(cx)))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(|this, _: &crate::peek::ExitPeek, _, cx| this.exit_peek(cx)))
            .on_action(cx.listener(|this, _: &crate::transient::NewLittleWindow, _, cx| this.new_little_window(cx)))
            .on_action(cx.listener(|this, _: &crate::transient::OpenInSpace, _, cx| this.promote_transient(None, cx)))
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
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.change_page_zoom(1, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.change_page_zoom(-1, cx)))
            .on_action(cx.listener(|this, _: &ResetZoom, _, cx| this.change_page_zoom(0, cx)))
            .on_action(cx.listener(|this, _: &NewBoost, _, cx|this.new_boost(cx)))
            .when(!self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &NewFolder, window, cx| this.open_command_action(Command::NewFolder, window, cx))))
            .on_action(cx.listener(|this, _: &CollapseAllFolders, _, cx| this.run_command(Command::CollapseAllFolders, cx)))
            .on_action(cx.listener(|this, _: &ExpandAllFolders, _, cx| this.run_command(Command::ExpandAllFolders, cx)))
            .on_action(cx.listener(|this, _: &CustomizeTabIcon, window, cx| this.open_command_action(Command::CustomizeTabIcon, window, cx)))
            .on_action(cx.listener(|this, _: &CustomizeSpaceIcon, window, cx| this.open_command_action(Command::CustomizeSpaceIcon, window, cx)))
            .on_action(cx.listener(|this, _: &ResetNameAndIcon, _, cx| this.run_command(Command::ResetNameAndIcon, cx)))
            .on_action(cx.listener(|this, _: &ToggleSpaceHeader, _, cx| this.run_command(Command::ToggleSpaceHeader, cx)))
            .on_action(cx.listener(|this, _: &ToggleFavoritesOrdering, _, cx| this.run_command(Command::ToggleFavoritesOrdering, cx)))
            .on_action(cx.listener(|this, _: &RevealTab, _, cx| this.reveal_active_tab(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::AutomaticAppearance, _, cx| this.set_website_appearance(browser_core::themes::ThemeAppearance::Automatic, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::LightAppearance, _, cx| this.set_website_appearance(browser_core::themes::ThemeAppearance::Light, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::DarkAppearance, _, cx| this.set_website_appearance(browser_core::themes::ThemeAppearance::Dark, cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::ViewMedia, _, cx| this.open_library_media(cx)))
                .on_action(cx.listener(|this, _: &crate::menus::ViewBoosts, _, cx| this.open_library_boosts(cx))))
            .when(self.can_clear_active_site(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::ClearCookiesAndRefresh, _, cx| this.clear_active_site(engine::ClearSiteData::Cookies, cx)))
                .on_action(cx.listener(|this, _: &crate::menus::ClearCacheAndRefresh, _, cx| this.clear_active_site(engine::ClearSiteData::Cache, cx))))
            .on_action(cx.listener(|this, _: &DuplicateTab, _, cx| this.run_command(Command::DuplicateTab, cx)))
            .when(!self.browser.is_blank(), |this| this.on_action(cx.listener(Self::on_toggle_pin)))
            .on_action(cx.listener(|this, _: &ToggleFavorite, _, cx| this.run_command(Command::ToggleFavorite, cx)))
            .on_action(cx.listener(|this, _: &ToggleMute, _, cx| this.run_command(Command::ToggleMute, cx)))
            .on_action(cx.listener(|this, _: &MuteAllTabs, _, cx| this.run_command(Command::MuteAllTabs, cx)))
            .on_action(cx.listener(|this, _: &UnmuteAllTabs, _, cx| this.run_command(Command::UnmuteAllTabs, cx)))
            .on_action(cx.listener(|this, _: &TogglePinnedSection, _, cx| this.toggle_pinned_section(this.browser.active_space, cx)))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(|this, _: &ToggleToolbar, _, cx| this.toggle_toolbar(cx)))
            .on_action(cx.listener(|this, _: &ToggleDeveloperMode, _, cx| this.toggle_developer_mode(cx)))
            .on_action(cx.listener(|this, _: &NextSpace, _, cx| this.cycle_space(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousSpace, _, cx| this.cycle_space(-1, cx)))
            .on_action(cx.listener(|this, action: &SwitchToSpace, _, cx| this.switch_space(action.0, cx)))
            .on_action(cx.listener(|this, action: &GoToItem, _, cx| {
                this.edit_browser(|browser| browser.activate_item(action.0, SystemTime::now()));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearToday, _, cx| {
                this.clear_today_tabs(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RestoreTab, _, cx| {
                if this.edit_browser(|browser| browser.restore_last_archived(SystemTime::now())).is_some() {
                    this.settle_pager(cx);
                }
                cx.notify();
            }))
            .relative()
            .flex()
            .size_full()
            .font_family(".SystemUIFont")

            .when(self.browser.is_private(), |this| this.bg(hsla(210. / 360., 0.06, 0.05, 1.)))
            .child(self.theme_surface(theme_blurred))
            .when(!compact && !library_open, |this| this.child(div().debug_selector(|| self.sidebar_debug_name((|| "sidebar-clip".into())()))
                .relative().flex_none().h_full().w(px(self.sidebar_width() * sidebar_amount)).overflow_hidden()
                .when_some(sidebar, |this, sidebar| this.child(div().absolute().left(px(sidebar_offset)).top_0().bottom_0().child(sidebar)))))
            .child(self.render_sidebar_resize_tracking(cx))
            .child(div().id("native-action-scope").absolute().size_0().track_focus(&self.native_action_focus))
            .child(content)
            .when(!library_open && sidebar_amount > 0., |this| this.child(self.render_sidebar_resize_handle(false, cx)))
            .when_some(library, |this, library| this.child(library))
            .when(self.native_page_content_visible(), |this| this.children(self.render_window_snapshots()))
            .when(!compact, |this| this.child(div().absolute().left(px(window_controls_offset)).top_0().w(px(74.)).h(px(SIDEBAR_TOOLBAR_HEIGHT)).child(self.render_traffic_lights(cx))))
            .child(self.render_page_loading(cx))
            .child(release_hover_on_exit())
            .into_any_element()

    }
}

/// Arc leaves unknown sites as a quiet, neutral page tile. The placeholder
/// carries no title initial or Space accent; a cached site icon replaces it.
fn favicon_placeholder(dark: bool) -> impl IntoElement {
    div()
        .debug_selector(|| "favicon-placeholder".into())
        .flex_none()
        .size(px(14.))
        .child(canvas(|_, _, _| (), move |bounds, _, window, _| {
            let foreground = if dark { gpui::white() } else { window.text_style().color };
            window.paint_quad(gpui::quad(bounds, px(3.), foreground.opacity(0.25), px(0.), gpui::transparent_black(), gpui::BorderStyle::Solid));
        }).size_full())
}

pub(crate) fn tint(hue: f32, lightness: f32) -> Hsla {
    hsla(hue, 0.45, lightness, 1.0)
}

/// The sidebar address field shows only the site, as in the reference.
pub fn address_host(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return display_url(url); };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") { return display_url(url); }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') { return authority.split_once(']').map_or(authority, |(host, _)| &authority[..host.len()+1]).to_owned(); }
    let host = authority.rsplit_once(':').filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit())).map_or(authority, |(host, _)| host);
    host.strip_prefix("www.").unwrap_or(host).to_owned()
}

pub fn display_url(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_string()
}

/// Native pages cover the window, so moving onto one only reports that the
/// pointer exited. The toolkit keeps the last position then, leaving hover
/// states stuck; replay the exit as a move so they are released.
fn release_hover_on_exit() -> impl IntoElement {
    canvas(|_, _, _| (), |_, _, window, _| {
        window.on_mouse_event(|event: &gpui::MouseExitEvent, phase, window, cx| {
            let _ = (event, cx);
            if phase != gpui::DispatchPhase::Bubble { return; }
            if let Some(view) = crate::native::view_of(window) { crate::native::replay_pointer(view); }
        });
    }).absolute().size_0()
}

const PRESS_BOUNCE: Duration = Duration::from_millis(440);
const PLUS_TURN: Duration = Duration::from_millis(300);
const COPY_FEEDBACK: Duration = Duration::from_millis(1300);

/// Runs `animate` with progress measured from `started`, not from when the
/// element mounted: controls that reappear (e.g. on hover) must not replay
/// feedback that already finished.
fn clocked<E: IntoElement + 'static>(element: E, id: String, started: Instant, duration: Duration, animate: impl Fn(E, f32) -> E + 'static) -> AnyElement {
    let duration = if crate::native::prefers_reduced_motion() { Duration::ZERO } else { duration };
    let elapsed = started.elapsed();
    if elapsed >= duration { return animate(element, 1.).into_any_element(); }
    element.with_animation(SharedString::from(id), Animation::new(duration - elapsed), move |element, delta| {
        let t = if delta >= 1. { 1. } else { (started.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.) };
        animate(element, t)
    }).into_any_element()
}

/// Ease-out that overshoots by an amount set by `overshoot`, then settles.
fn back_out(t: f32, overshoot: f32) -> f32 {
    let u = t - 1.;
    1. + (overshoot + 1.) * u * u * u + overshoot * u * u
}

const ICON_DIP: f32 = 0.08;
const HIGHLIGHT_DIP: f32 = 0.1;

/// A soft press: ease down by `dip`, then spring back just past full size.
fn press_scale(t: f32, dip: f32) -> f32 {
    if t < 0.3 {
        let p = t / 0.3;
        1. - dip * p * (2. - p)
    } else {
        1. - dip + dip * back_out((t - 0.3) / 0.7, 0.9)
    }
}

/// Hover-only controls fade like the reference instead of popping in.
const HOVER_FADE: Duration = Duration::from_millis(160);

fn hover_animation() -> Animation {
    let duration = if crate::native::prefers_reduced_motion() { Duration::from_millis(1) } else { HOVER_FADE };
    Animation::new(duration).with_easing(gpui::ease_in_out)
}

/// Fades `element` in when `shown` turns on and out when it turns off.
fn hover_fade<E: IntoElement + Styled + 'static>(element: E, id: impl Into<SharedString>, shown: bool) -> AnyElement {
    let id: SharedString = id.into();
    element.with_animation(SharedString::from(format!("{id}-{shown}")), hover_animation(), move |element, delta| {
        element.opacity(if shown { delta } else { 1. - delta })
    }).into_any_element()
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

#[cfg(test)]
#[path = "sidebar_navigation_pager_tests.rs"]
mod sidebar_navigation_pager_tests;

#[path = "sidebar_resize.rs"]
mod sidebar_resize;
#[cfg(test)]
#[path = "sidebar_resize_tests.rs"]
mod sidebar_resize_tests;

#[path = "printing.rs"]
mod printing;
#[path = "paste_style.rs"]
mod paste_style;

#[path = "page_share.rs"]
mod page_share;
#[path = "save_page.rs"]
mod save_page;

/// Arc's toolbar follows the document canvas, independently of chrome mode.
pub(crate) fn toolbar_colors(color: Option<[u8; 3]>, dark_fallback: bool) -> (gpui::Hsla, f32) {
    let color = color.unwrap_or(if dark_fallback { [34; 3] } else { [255; 3] });
    let light = (color[0] as f32 * 0.2126 + color[1] as f32 * 0.7152 + color[2] as f32 * 0.0722) > 127.5;
    let fill = if light { 0xffffff } else { (color[0] as u32) << 16 | (color[1] as u32) << 8 | color[2] as u32 };
    (gpui::rgb(fill).into(), if light { 0. } else { 1. })
}
