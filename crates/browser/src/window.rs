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
    WindowBounds, WindowHandle, WindowKind, WindowOptions, hsla, linear_color_stop, linear_gradient, prelude::*, px, App, Context, Entity,
    FocusHandle, Focusable, Hsla, KeyBinding, Keystroke, Modifiers, ScrollDelta, ScrollWheelEvent,
    SharedString, Subscription, TouchPhase, Window,
};

use crate::pages::{Pages, WebEngine, CONTENT_CORNER_RADIUS, CONTENT_INSET, WINDOW_CORNER_RADIUS};
use crate::text_input::{self, TextInput};

#[path = "window_close.rs"]
pub(crate) mod window_close;
#[path = "download_probe.rs"]
pub(crate) mod download_probe;

pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;
/// Sidebar metrics measured from the reference at 1x.
const SIDEBAR_INSET: f32 = 8.0;
const SIDEBAR_TOOLBAR_HEIGHT: f32 = 47.0;
pub(crate) const SIDEBAR_ROW_HEIGHT: f32 = 38.0;
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
const SPACE_SWIPE_COMMIT: f32 = 0.25;
/// Finger speed (px/s) that changes space regardless of distance (a flick).
const SPACE_FLICK_SPEED: f32 = 400.0;
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
        ExpandCurrentSplit,
        ViewSource,
        ViewLibrary,
        ViewDownloads,
        PrintPage,
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
    "cmd-shift-c" => CopyUrl,
    "cmd-alt-shift-c" => CopyUrlAsMarkdown,
    "cmd-w" => CloseTab,
    "cmd-shift-w" => crate::menus::CloseWindow,
    "ctrl-cmd-n" => crate::menus::BlankWindow,
    "cmd-n" => crate::menus::NewWindow,
    "cmd-shift-n" => crate::menus::NewIncognitoWindow,
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
    AddSplit(Option<browser_core::splits::Placement>),
    CloseSplitPane,
    SplitAxis(browser_core::splits::Axis),
    SeparateSplit,
    SeparatePane,
    ExpandCurrentSplit,
    ViewSource,
    DevTools(crate::devtools::Command),
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
    ManageSpaces,
    PrintPage,
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
    pub(crate) site_search: Option<omnibox::SiteSearch>,
    pub(crate) remote: Vec<String>,
    pub(crate) remote_generation: u64,
    pub(crate) remote_task: Option<Task<()>>,
    pub(crate) suggestions_privacy: bool,
    pub(crate) archive_reason: Option<browser_core::ArchiveReason>,
    pub(crate) archive_limit: usize,
    pub(crate) rows_scroll: gpui::ScrollHandle,
    /// The bar is drawn in its own transparent window above the browser
    /// window, since pages are native views that cover anything drawn below.
    pub overlay: Option<WindowHandle<Overlay>>,
    _changed: Subscription,
    _empty_backspace: Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    CommandBar,
    Find,
    Toast,
    LinkStatus,
    /// The hidden sidebar, shown floating over the page.
    Sidebar,
    TabMenu,
    SiteControls,
    NewMenu,
    UserProfileDialog,
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
                    OverlayKind::CommandBar => browser.render_command_bar_overlay(f32::from(window.viewport_size().width), f32::from(window.viewport_size().height), cx),
                    OverlayKind::Find => browser.render_find_bar(cx),
                    OverlayKind::Toast => browser.render_toast(),
                    OverlayKind::LinkStatus => browser.render_link_status(window),
                    OverlayKind::Sidebar => browser.render_floating_sidebar(cx),
                    OverlayKind::TabMenu => browser.render_tab_menu(cx),
                    OverlayKind::SiteControls => browser.render_site_controls(cx),
                    OverlayKind::NewMenu => browser.render_new_menu(cx),
                    OverlayKind::UserProfileDialog => browser.render_user_profile_dialog(),
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
    height: f32,
    _pointer_task: Option<Task<()>>,
}


#[derive(Default)]
struct SidebarScrolls { pinned: gpui::ScrollHandle, today: gpui::ScrollHandle, pending: Rc<Cell<Option<(Section, usize)>>> }

pub struct BrowserWindow {
    pub browser: browser_core::windows::WindowProjection,
    pub(crate) profile: crate::profile::ProfileBinding,
    pub(crate) sidebar_navigation: crate::sidebar_navigation::SidebarNavigation,
    pub(crate) sidebar_debug_namespace: Option<String>,
    pub(crate) sidebar_page_interactive: bool,
    pub sidebar_visible: bool,
    sidebar_width: f32,
    sidebar_resize: Option<sidebar_resize::SidebarResizeGesture>,
    sidebar_resize_generation: u64,
    sidebar_resize_focus_on_activation: Option<(crate::pages::ResizePageFocus, u64)>,
    pub(crate) sidebar_render_width: Option<f32>,
    sidebar_reveal: Option<PinnedReveal>,
    sidebar_scrolls: HashMap<SpaceId, SidebarScrolls>,
    pinned_reveals: HashMap<SpaceId, PinnedReveal>,
    tab_drop_edges: RefCell<HashMap<TabId, Rc<Cell<bool>>>>,
    pub(crate) tab_drag_active: bool,
    space_drag_hover: Option<(SpaceId, Task<()>)>,
    pub(crate) split_draft: Option<crate::split::SplitDraft>,
    pub(crate) hovered_row: Option<TabId>,
    /// Layout that changes on sidebar hover (Today actions) needs state:
    /// hover styles alone repaint without a new layout.
    pub(crate) sidebar_hovered: bool,
    pub(crate) traffic_lights_hovered: bool,
    favicon_styles: RefCell<crate::favicon_style::FaviconStyles>,
    pub(crate) snapshot_images: RefCell<crate::window_snapshots::SnapshotImages>,
    pub command_bar: Option<CommandBar>,
    pub(crate) user_profile_dialog: Option<crate::user_profiles::ProfileDialogHost>,
    pub(crate) tab_rename: Option<crate::inline_rename::TabRename>,
    pub(crate) tab_rename_pending: Option<TabId>,
    pub(crate) tab_rename_generation: u64,
    pub(crate) find_bar: Option<crate::find::FindBar>,
    pub(crate) toast: Option<crate::toast::Toast>,
    pub(crate) toast_generation: u64,
    pub(crate) link_status: Option<crate::link_status::LinkStatus>,
    pub(crate) link_status_generation: u64,
    pub(crate) history_page: crate::history::HistoryPage,
    pub(crate) tab_menu: Option<crate::tab_menu::TabMenu>,
    pub(crate) site_controls: Option<crate::site_controls::SiteControls>,
    pub(crate) site_controls_anchor: crate::site_controls::Anchor,
    pub(crate) new_menu: Option<crate::new_menu::NewMenu>,
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
    native_action_focus: FocusHandle,
    print_generation: u64,
    popup_focus_generation: u64,
    popup_focus_pending: usize,
    popup_focus_on_activation: bool,
}

impl BrowserWindow {
    pub(crate) fn window_controls_offset(&self) -> f32 {
        if !self.page_content_visible() { 0. } else { self.sidebar_width() * (self.sidebar_amount() - 1.) }
    }

    pub(crate) fn sidebar_debug_name(&self, name: String) -> String {
        self.sidebar_debug_namespace.as_ref().map_or_else(|| name.clone(), |namespace| format!("{namespace}/{name}"))
    }

    pub(crate) fn reset_profile_pager(&mut self, index: usize) {
        self.pager.settle = None;
        self.pager.drag = None;
        self.pager.position = if self.browser.is_blank() || self.browser.is_private() {
            index
        } else {
            self.sidebar_page_index()
        } as f32;
    }

    pub(crate) fn receive_profile_projection(&mut self, projection: browser_core::windows::WindowProjection) {
        let selected = self.sidebar_navigation.controller.selected();
        let previous_index = self.sidebar_navigation.controller.pages(&self.browser).iter().position(|page| *page == selected);
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
    pub(crate) fn reset_sidebar_motion(&mut self) { self.pager.drag = None; self.pager.settle = None; }

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
            if let Some(parent) = crate::native::view_of(window) { pages.set_parent(parent); }
            Rc::new(RefCell::new(pages))
        });
        let closing_root = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            // AppKit must not destroy the parent before the outside-App
            // preparation boundary. Red close and Cmd+Shift+W share one path.
            let _ = closing_root.update(cx, |this, cx| this.close_window(cx));
            false
        });
        cx.observe_window_activation(window, |this, window, cx| {
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
        Self {
            pager: Pager {
                position,
                ..Default::default()
            },
            browser,
            profile,
            sidebar_navigation,
            sidebar_debug_namespace: None,
            sidebar_page_interactive: true,
            sidebar_visible: true,
            sidebar_width: SIDEBAR_WIDTH,
            sidebar_resize: None,
            sidebar_resize_generation: 0,
            sidebar_resize_focus_on_activation: None,
            sidebar_render_width: None,
            sidebar_reveal: None,
            sidebar_scrolls: HashMap::new(),
            pinned_reveals: HashMap::new(),
            tab_drop_edges: RefCell::new(HashMap::new()),
            tab_drag_active: false,
            space_drag_hover: None,
            split_draft: None,
            hovered_row: None,
            sidebar_hovered: false,
            traffic_lights_hovered: false,
            favicon_styles: Default::default(),
            snapshot_images: Default::default(),
            command_bar: None,
            user_profile_dialog: None,
            tab_rename: None,
            tab_rename_pending: None,
            tab_rename_generation: 0,
            find_bar: None,
            toast: None,
            toast_generation: 0,
            link_status: None,
            link_status_generation: 0,
            history_page: crate::history::HistoryPage::new(cx),
            pages,
            pending_engine_events: Default::default(),
            engine_shortcut_pending: false,
            floating_sidebar: None,
            tab_menu: None,
            site_controls: None,
            site_controls_anchor: Default::default(),
            new_menu: None,
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
        self.profile.handle.flush_for_quit(cx)
    }

    pub(crate) fn detach_for_quit(&mut self, cx: &mut Context<Self>) {
        // Parent windows stay alive until all engine-tracked children are closed.
        self.remove_child_windows(cx);
        self.pages.take();
        self.pending_engine_events.clear();
        cx.notify();
    }

    /// Closing a window keeps its tabs in the session, unlike Archive Tab.
    pub(crate) fn close_window(&mut self, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() || crate::quit::last_window_close(cx) { return; }
        if !window_close::claim(self.main_window, cx) { return; }
        self.cancel_split_draft();
        // Parent remains alive. release_host only queues retirement; the
        // engine's native handoff must run in the task, outside this update.
        if let Some(pages) = &self.pages { pages.borrow_mut().release_host(); }
        if let Some(downloads) = cx.try_global::<crate::downloads::AppDownloads>().cloned() { downloads.wake(cx); }
        self.remove_child_windows(cx);
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
        self.discard_sidebar_resize();
        let profile_child = self.take_user_profile_child(cx);
        let children = [
            profile_child.map(|handle| handle.into()),
            self.command_bar.take().and_then(|bar| bar.overlay).map(|handle| handle.into()),
            self.find_bar.take().and_then(|bar| bar.overlay).map(|handle| handle.into()),
            self.tab_menu.take().and_then(|menu| menu.overlay).map(|handle| handle.into()),
            self.site_controls.take().and_then(|menu| menu.overlay).map(|handle| handle.into()),
            self.new_menu.take().and_then(|menu| menu.overlay).map(|handle| handle.into()),
            self.toast.take().and_then(|toast| toast.overlay).map(|handle| handle.into()),
            self.link_status.take().and_then(|status| status.overlay).map(|handle| handle.into()),
            self.floating_sidebar.take().and_then(|sidebar| sidebar.overlay).map(|handle| handle.into()),
        ];
        for child in children.into_iter().flatten() {
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
            if matches!(&event, Event::Focused { .. }) && (!self.native_page_content_visible() || self.user_profile_dialog.is_some() || self.command_bar.is_some() || self.find_bar.is_some() || self.tab_menu.is_some() || self.site_controls.is_some() || self.new_menu.is_some()) { continue; }
            pages.borrow_mut().record(&event);
            match event {
                Event::Focused { view } => {
                    self.finish_tab_rename(true, cx);
                    let tab = pages.borrow().tab_for(view).filter(|tab| pages.borrow().is_visible(*tab));
                    if let Some(tab) = tab {
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
                Event::StatusChanged { view, text } => self.update_link_status(view, text, window, cx),
                Event::EditFocusChanged { .. } => {},
                Event::OpenInNewTab { url, .. } => {
                    let Some(source) = source_space.and_then(|id| self.browser.spaces.iter().position(|space| space.id == id && Some(space.user_profile_id) == page_profile)) else { continue; };
                    let id = self.edit_browser(|browser| {
                        let mut selection = browser.window_selection();
                        let background = browser.active_space != source;
                        browser.switch_space(source);
                        let id = browser.new_tab(url.clone(), SystemTime::now());
                        if background { browser.edit_in_window(&mut selection, |_| {}); }
                        id
                    });
                    self.edit_profile(|browser| browser.set_title(id, display_url(&url)));
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
                Event::LoadingChanged { .. } => {}
            }
        }
        }
        // CEF can initialize its audio state after create_view returned. Reapply
        // persisted intent after engine events even when the window is occluded.
        pages.borrow_mut().sync_audio(&self.browser);
        cx.notify();
    }

    fn defer_engine_shortcut(&mut self, shortcut: engine::Shortcut, source: Option<crate::pages::InputOrigin>, window: &mut Window, cx: &mut Context<Self>) {
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

    fn edit_undo(&mut self, redo: bool, cx: &mut Context<Self>) {
        let before = self.browser.all_tabs().count();
        let editable = self.browser.active_tab().is_some_and(|tab|
            self.pages.as_ref().is_some_and(|pages| pages.borrow().state(tab.id).editable));
        if !editable && if redo { self.edit_browser(|browser| browser.redo_sidebar(SystemTime::now())) } else { self.edit_browser(|browser| browser.undo_sidebar(SystemTime::now())) } {
            self.settle_pager(cx);
            if self.browser.all_tabs().count() < before { self.after_tab_removal(cx); }
            cx.notify();
        } else {
            self.with_active_page(|pages, tab| if redo { pages.redo(tab) } else { pages.undo(tab) });
        }
    }

    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    pub(crate) fn accent(&self) -> f32 {
        self.browser.space().hue / 360.0
    }

    /// Background hue at the pager position, blending neighbouring pages.
    fn pager_hue(&self) -> f32 {
        use crate::sidebar_pages::SidebarPage;
        let hues: Vec<f32> = if self.browser.is_blank() || self.browser.is_private() {
            self.browser.spaces.iter().map(|space| space.hue).collect()
        } else {
            self.sidebar_navigation.controller.pages(&self.browser).into_iter().map(|page| match page {
                SidebarPage::Space(id) => self.browser.spaces.iter()
                    .find(|space| space.id == id).map_or(self.browser.space().hue, |space| space.hue),
                SidebarPage::CreateSpace => self.sidebar_navigation.controller.draft()
                    .map_or(self.browser.space().hue, |draft| draft.hue),
                SidebarPage::Library => self.browser.space().hue,
            }).collect()
        };
        let position = self.pager.position.clamp(0.0, hues.len().saturating_sub(1) as f32);
        let (a, b) = (position.floor() as usize, position.ceil() as usize);
        let (from, to) = (hues[a], hues[b]);
        // Shortest way around the color wheel.
        let diff = ((to - from + 540.0) % 360.0) - 180.0;
        (from + diff * position.fract()).rem_euclid(360.0) / 360.0
    }

    /// The exact field used by the root and floating sidebar. Region previews
    /// share it, including the current Space-swipe hue and private appearance.
    pub(crate) fn theme_background(&self) -> gpui::Background {
        if self.browser.is_private() {
            return hsla(210. / 360., 0.06, 0.05, 1.).into();
        }
        let hue = self.pager_hue();
        linear_gradient(160., linear_color_stop(tint(hue, 0.82), 0.), linear_color_stop(tint(hue, 0.72), 1.))
    }

    /// Content of an already positioned region in a passive overlay. Bounds
    /// are window-relative; the caller owns rounded clipping and the contour.
    /// Translate the full viewport field instead of stretching it to the pane.
    pub(crate) fn theme_region(&self, bounds: Bounds<gpui::Pixels>, viewport: gpui::Size<gpui::Pixels>) -> AnyElement {
        div().relative().w(bounds.size.width).h(bounds.size.height).overflow_hidden()
            .debug_selector(|| "theme-region".into())
            .child(div().absolute().left(-bounds.origin.x).top(-bounds.origin.y)
                .debug_selector(|| "theme-region-field".into())
                .w(viewport.width).h(viewport.height).bg(self.theme_background()))
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
        if target == Target::NewSpace { self.begin_space_creation(self.browser.space().id, cx); return; }
        if self.browser.is_blank() && !matches!(target, Target::NewTab | Target::Tab(_) | Target::Split { .. } | Target::RenameTab(_) | Target::Archive(_) | Target::ConfirmClearArchive | Target::Icon(crate::icons::IconTarget::Tab(_))) { return; }
        if crate::quit::is_quitting() { return; }
        self.remember_library_popup_focus(window, cx);
        self.close_user_profile_dialog(cx);
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
                Target::Icon(_) => "Enter one symbol or emoji…",
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
            target,
            selected: 0,
            site_search: None,
            remote: Vec::new(),
            remote_generation: 0,
            remote_task: None,
            suggestions_privacy: false,
            archive_reason: None,
            archive_limit: 8,
            rows_scroll: gpui::ScrollHandle::new(),
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

    pub(crate) fn close_command_bar_with_focus(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        self.cancel_split_draft();
        let Some(bar) = self.command_bar.take() else { return; };
        self.close_popup_overlay(bar.overlay, focus, true, cx);
        cx.notify();
    }

    pub(crate) fn popup_owns_focus(&self) -> bool {
        !self.page_content_visible() || self.user_profile_dialog.is_some() || self.site_controls.is_some() || self.new_menu.is_some() || self.command_bar.is_some() || self.tab_menu.is_some() || self.find_bar.is_some()
            || self.tab_rename.is_some() || self.tab_rename_pending.is_some()
    }

    /// Native activation may be asynchronous (GPUI's macOS backend queues
    /// makeKeyAndOrderFront). Minimizing must never enqueue that activation,
    /// including from an earlier dismissal in the same command dispatch.
    pub(crate) fn close_popups_for_window_operation(&mut self, minimize: bool, cx: &mut Context<Self>) {
        if minimize { self.abandon_sidebar_resize(); self.queue_floating_resize(cx); }
        let focus = if minimize {
            // A root with no popup already owns its responder intent (for
            // example History search or docked DevTools). Only replace it when
            // this operation actually takes over a popup's restoration.
            self.popup_focus_on_activation |= self.user_profile_dialog.is_some() || self.site_controls.is_some() || self.new_menu.is_some() || self.command_bar.is_some()
                || self.find_bar.is_some() || self.tab_menu.is_some()
                || self.popup_focus_pending > 0;
            self.popup_focus_generation = self.popup_focus_generation.wrapping_add(1);
            self.popup_focus_pending = 0;
            self.invalidate_navigation_focus();
            PopupFocus::OnActivation
        } else {
            PopupFocus::Restore
        };
        self.close_user_profile_dialog_with_focus(focus, cx);
        self.close_site_controls_with_focus(focus, cx);
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
        if let Some(site) = bar.site_search {
            return if query.is_empty() { Vec::new() } else { vec![Suggestion::Load(site.search(text.trim()))] };
        }
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
                (Command::ViewDownloads, "View Downloads", "view downloads|downloads", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ViewArchive, "View Archive", "view archive|open archive|search archived tabs|archive", true),
                (Command::RequestClearArchive, "Clear Archive…", "clear archive|delete archived tabs", !self.browser.archive.is_empty()),
                (Command::CopyUrl, "Copy URL", "copy url|copy link|copy clean link", active.is_some()),
                (Command::CopyUrlAsMarkdown, "Copy URL as Markdown", "copy url as markdown|copy link as markdown", active.is_some()),
                (Command::Settings, "Settings…", "settings|preferences|search settings", true),
                (Command::FindPage, "Find in Page…", "find in page|search page", active.is_some()),
                (Command::NewProfile(self.browser.space().id), "New Profile", "new profile|create profile|add profile", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::ChangeProfile(self.browser.space().id), "Change Profile", "change profile|set profile|switch profile", !self.browser.is_private() && !self.browser.is_blank()),
                (Command::NewSpace, "New Space…", "new space|create space|add space", true),
                (Command::BlankWindow, "Blank Window", "blank window|new blank window", true),
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
                (Command::Reload, "Refresh the Page", "refresh page|reload tab", active.is_some()),
                (Command::ForceReload, "Force Refresh the Page", "force refresh page|reload without cache|hard reload", active.is_some()),
                (Command::StopLoading, "Stop Loading", "stop loading|stop page", active.is_some()),
                (Command::ViewSource, "View Source", "view source|page source|show source", self.current_source_url().is_some()),
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
        let engine = self.browser.active_user_profile().settings.search.engine;
        suggestions.extend(omnibox::resolve_with_engine(text, engine).map(Suggestion::Load));
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
                .filter(|entry| !open.contains(entry.url.as_str()))
                .filter(|entry| entry.title.to_lowercase().contains(&query) || entry.url.to_lowercase().contains(&query))
                .take(MAX_HISTORY_SUGGESTIONS)
                .map(|entry| Suggestion::History { title: entry.title.clone(), url: entry.url.clone() }));
        }
        if self.browser.active_user_profile().settings.search.suggestions && !self.browser.is_private() {
            suggestions.extend(bar.remote.iter().map(|query| Suggestion::Load(engine.search(query))));
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
                    Target::NewTab => self.edit_browser(|browser| browser.new_tab(resolved.url(), now)),
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
                    Target::NewTab => self.edit_browser(|browser| browser.new_tab(&url, now)),
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
                    self.edit_browser(|browser| browser.switch_space(index));
                    self.edit_browser(|browser| browser.activate_tab(id, now));
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
                    self.show_toast(format!("Tab restored to {}", self.browser.space().name), cx);
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
    }

    pub(crate) fn run_command(&mut self, command: Command, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() { return; }
        if !self.blank_allows_command(command) { return; }
        match command {
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
            Command::OpenLibrary => self.select_sidebar_page(crate::sidebar_pages::SidebarPage::Library, cx),
            Command::ManageSpaces => self.open_library_spaces(cx),
            Command::ViewDownloads => self.open_library_downloads(cx),
            Command::ViewArchive => self.edit_archive_view(None, cx),
            Command::RequestClearArchive => self.confirm_clear_archive(cx),
            Command::CopyUrl => self.copy_url(false, cx),
            Command::CopyUrlAsMarkdown => self.copy_url(true, cx),
            Command::Settings => self.open_settings(cx),
            Command::FindPage => self.find_from_command(cx),
            Command::NewSpace => self.begin_space_creation(self.browser.space().id, cx),
            Command::NewProfile(space) => self.open_user_profile_creation(crate::user_profile_dialog::CreationSource::File(space), cx),
            Command::ChangeProfile(space) => self.open_user_profile_switch(space, cx),
            Command::BlankWindow => self.blank_window(cx),
            Command::NewWindow => self.new_window(cx),
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
                if let Some(id) = self.browser.space().active_tab { self.edit_browser(|browser| browser.toggle_pin(id)); }
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
                    if !self.browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == self.browser.active_user_profile_id()) { return; }
                    if self.edit_browser(|browser| browser.move_tab(id, space, section)) {
                        if let Some(index) = self.browser.spaces.iter().position(|candidate| candidate.id == space) {
                            self.switch_space(index, cx);
                            self.edit_browser(|browser| browser.activate_tab(id, SystemTime::now()));
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

    fn render_overlay_shell(&self, content: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        div().size_full().key_context(if self.sidebar_resize.is_some() { "BrowserWindow SidebarResizing" } else { WINDOW_CONTEXT })
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| { this.capture_sidebar_resize_key(event, window, cx); }))
            .on_action(cx.listener(|this, _: &crate::menus::BlankWindow, _, cx| this.blank_window(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::NewWindow, _, cx| this.new_window(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::Settings, _, cx| this.open_settings(cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::NewProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_creation(crate::user_profile_dialog::CreationSource::File(space), cx); }))
                .on_action(cx.listener(|this, _: &crate::menus::ChangeProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_switch(space, cx); })))
            .on_action(cx.listener(|this, _: &crate::menus::CloseWindow, _, cx| this.close_window(cx)))
            .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
                if this.user_profile_dialog.is_some() { this.close_user_profile_dialog(cx); }
                else if this.command_bar.is_some() { this.close_command_bar(cx); }
                else { this.close_focused_tab(cx); }
            }))
            .when(!self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &NewFolder, window, cx| this.open_command_action(Command::NewFolder, window, cx))))
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
            .on_action(cx.listener(|this, _: &ExpandCurrentSplit, _, cx| this.run_command(Command::ExpandCurrentSplit, cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &ViewLibrary, _, cx| this.run_command(Command::OpenLibrary, cx))).on_action(cx.listener(|this, _: &ViewDownloads, _, cx| this.run_command(Command::ViewDownloads, cx))).on_action(cx.listener(|this, _: &crate::menus::ManageSpaces, _, cx| this.run_command(Command::ManageSpaces, cx))))
            .on_action(cx.listener(|this, _: &crate::text_input::PasteAndMatchStyle, _, cx| this.paste_and_match_style(cx)))
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

    fn view_source(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.current_source_url() else { return; };
        self.close_command_bar(cx);
        self.close_find(cx);
        let id = self.edit_browser(|browser| browser.new_tab(&url, SystemTime::now()));
        self.edit_browser(|browser| browser.set_title(id, url));
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }

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

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
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
        self.finish_tab_rename(true, cx);
        let related = self.browser.related_tabs(id);
        if self.edit_browser(|browser| browser.close_tab(id, SystemTime::now())).is_some() {
            if let Some(pages) = &self.pages {
                let mut pages = pages.borrow_mut();
                for id in related { pages.discard(id); }
                pages.sync(&self.browser, self.page_content_visible());
            }
            self.after_tab_removal(cx);
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
            self.edit_browser(|browser| browser.toggle_pin(id));
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

    pub(crate) fn scroll_to_today_item(&mut self, tab: TabId, cx: &mut Context<Self>) {
        let anchor = self.browser.item_anchor(tab);
        if let Some(index) = self.browser.space().today.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).position(|tab| tab.id == anchor) {
            self.sidebar_scrolls.entry(self.browser.space().id).or_default().pending.set(Some((Section::Today, index)));
        }
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
            if section == Section::Pinned { self.edit_browser(|browser| browser.spaces[browser.active_space].pinned_collapsed = false); }
        }
        self.settle_pager(cx);
        cx.notify();
    }

    pub(crate) fn sidebar_amount(&self) -> f32 {
        self.sidebar_reveal.as_ref().filter(|reveal| reveal.collapsed == !self.sidebar_visible)
            .map_or(if self.sidebar_visible { 1. } else { 0. }, |reveal| reveal.amount)
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
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
        if self.sidebar_visible || self.floating_sidebar.is_some() {
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
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        if self.sidebar_resize.is_some() { return; }
        let sidebar_width = self.sidebar_width();
        let page_count = if self.browser.is_private() {
            self.browser.spaces.len()
        } else {
            self.sidebar_navigation.controller.pages(&self.browser).len()
        };
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
                let raw = drag.start - drag.travel / sidebar_width.max(1.);
                self.pager.position = rubber_band(raw, page_count);
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
                    self.settle_pager(cx);
                }
            }
        }
    }

    /// Eases the pager to the selected page in the current derived order.
    pub(crate) fn settle_pager(&mut self, cx: &mut Context<Self>) {
        let target = if self.browser.is_blank() || self.browser.is_private() {
            self.browser.active_space
        } else {
            self.sidebar_page_index()
        } as f32;
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
        let private = self.browser.is_private();
        let segmented = self.browser.split_for(id).is_some_and(|split| split.title.is_none());
        let renaming = self.tab_rename.as_ref().is_some_and(|editor| editor.tab == id);
        let reserve_close = !segmented && !tab.muted && !renaming;
        let group = SharedString::from(format!("tab-{key}"));
        // Keep the current edge across view refreshes caused by drag hover.
        let edge = self.tab_drop_edges.borrow_mut().entry(id).or_default().clone();
        let drag_moves_active = self.sidebar_page_interactive;
        let move_edge = edge.clone();
        let paint_edge = edge.clone();
        let drag_owner = cx.entity().downgrade();

        div()
            .id(("tab", key))
            .debug_selector(|| self.sidebar_debug_name((|| format!("tab-{key}"))()))
            .group(group.clone())
            .on_drag(self.tab_drag(tab), move |drag, _, _, cx| crate::tab_drag::TabDrag::start(drag, drag_owner.clone(), cx))
            .can_drop(move |value, _, _| value.downcast_ref::<crate::tab_drag::TabDrag>().is_some_and(|drag| drag.id != id))
            .on_drag_move::<crate::tab_drag::TabDrag>(move |event, _, _| { if drag_moves_active { move_edge.set(event.event.position.y > event.bounds.center().y); } })
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
            // hover. A mute control keeps its place left of it.
            // Hover alone only repaints, so the padding follows hover state.
            .pr(px(if tab.muted || (reserve_close && self.hovered_row == Some(id)) { 34. } else { 6. }))
            .text_size(px(13.))
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered { this.hovered_row = Some(id); }
                else if this.hovered_row == Some(id) { this.hovered_row = None; }
                cx.notify();
            }))
            .when(segmented, |this| this.px_1().gap_0())
            .rounded(px(SIDEBAR_ROW_RADIUS))
            .when(active, |this| this.bg(hsla(0.0, 0.0, 1.0, 0.85)).shadow_sm())
            .when(self.browser.is_private(), |this| this.text_color(hsla(0., 0., 1., 0.85))
                .when(active, |this| this.bg(hsla(0., 0., 1., 0.16))))
            .when(!active, |this| this.hover(move |style| style.bg(hsla(0.0, 0.0, 1.0, if private { 0.1 } else { 0.3 }))))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                this.open_tab_menu(id, event.position, window, cx);
            }))
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                if this.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Library { this.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(painted_space), cx); }
                if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                this.edit_browser(|browser| browser.activate_sidebar_item(id, SystemTime::now()));
                if event.click_count() == 2 {
                    this.open_tab_rename(id, window, cx);
                }
                cx.notify();
            }))
            .child(self.render_sidebar_item_contents(tab, hue, cx))
            .when(tab.muted, |this| this.child(self.render_mute_control(tab, false, cx)))
            .child(
                div()
                    .id(("close", key))
                    .debug_selector(|| self.sidebar_debug_name((|| format!("close-{key}"))()))
                    .absolute().top(px(10.)).right(px(if segmented { 4. } else { 6. }))
                    .flex_none()
                    .px_1()
                    .rounded_sm()
                    .text_sm()
                    .text_color(hsla(0.0, 0.0, 0.0, 0.5))
                    .when(private, |this| this.text_color(hsla(0., 0., 1., 0.6)))
                    .invisible()
                    .when(!renaming, |this| this.group_hover(group, |s| s.visible()))
                    .hover(|s| s.bg(hsla(0.0, 0.0, 0.0, 0.08)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                        this.close_tab(id, cx);
                    }))
                    .child(crate::icons::control(match section {
                        Section::Pinned => crate::icons::Icon::Minus,
                        Section::Today => crate::icons::Icon::Close,
                    })),
            )
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
                if let Some(pages) = &this.pages { pages.borrow_mut().request_page_focus(); }
                this.edit_browser(|browser| browser.activate_sidebar_item(id, SystemTime::now()));
                if event.click_count() == 2 {
                    let title = this.browser.item_title(id);
                    this.open_command_bar(Target::RenameTab(id), &title, window, cx);
                }
                cx.notify();
            }))
            .child(self.render_item_icon(tab, self.accent()))
            .when(tab.muted, |this| this.child(self.render_mute_control(tab, true, cx)))
    }

    fn render_mute_control(&self, tab: &Tab, favorite: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let id = tab.id;
        let painted_space = self.browser.space().id;
        let painted_profile = self.browser.active_user_profile_id();
        let key = id.get() as usize;
        let title = tab.title.clone();
        div().id(("unmute", key)).debug_selector(|| self.sidebar_debug_name((move || format!("unmute-{key}"))()))
            .flex_none().px_1().rounded_sm().text_sm()
            .when(favorite, |this| this.absolute().bottom(px(1.)).left(px(1.)))
            .hover(|s| s.bg(hsla(0., 0., 0., 0.1)))
            .tooltip(move |_, cx| cx.new(|_| TabTooltip { title: "Unmute Tab".into(), url: title.clone() }).into())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !this.prepare_sidebar_target(painted_space, painted_profile) { return; }
                this.set_tab_muted(id, false, cx);
            })).child(crate::icons::control(crate::icons::Icon::Mute))
    }

    pub(crate) fn render_favicon(&self, tab: &Tab, hue: f32) -> AnyElement { self.render_favicon_on(tab, hue, self.browser.is_private()) }

    /// `tab`'s favicon for a light surface, or a dark one when `dark`.
    pub(crate) fn render_favicon_on(&self, tab: &Tab, _hue: f32, dark: bool) -> AnyElement {
        if let Some(icon) = &tab.custom_icon { return crate::icons::glyph(icon); }
        let icon = match self.browser.favicon(self.browser.active_user_profile_id(), &tab.url) {
            Some(png) => {
                let style = self.favicon_styles.borrow_mut().get(png);
                gpui::img(if dark { style.on_dark } else { style.image })
                    .size(px(16.0)).flex_none().object_fit(gpui::ObjectFit::Contain)
                    .with_fallback(move || favicon_placeholder(dark).into_any_element())
                    .into_any_element()
            }
            None => favicon_placeholder(dark).into_any_element(),
        };
        // Favicons are 16 px, centered in the 18 px slot shared with symbols.
        div().size(px(18.)).flex_none().flex().items_center().justify_center().child(icon).into_any_element()
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
            .unwrap_or_else(|| if self.browser.is_private() { "Search or Enter URL…".into() } else { String::new() });
        let private = self.browser.is_private();
        let blank = self.browser.is_blank();
        let (pages, offset) = pager_pages.unwrap_or_else(|| (vec![if blank { self.render_blank_tabs(cx).into_any_element() } else if private { self.render_private_tabs(cx).into_any_element() } else { self.render_space_page(self.browser.active_space, cx).into_any_element() }], 0.));
        let browser = cx.entity().downgrade();
        let state = self.browser.space().active_tab.and_then(|id| self.pages.as_ref()?.try_borrow().ok().map(|pages| pages.state(id))).unwrap_or_default();

        let loading = state.loading;
        let control = |id: &'static str| div().id(id).debug_selector(|| self.sidebar_debug_name((move || id.into())())).flex_none().size(px(28.)).rounded_md()
            .flex().items_center().justify_center().text_color(hsla(0., 0., if private { 1. } else { 0. }, 0.6)).hover(|style| style.bg(hsla(0., 0., 1., 0.25)));
        let controls_open = self.site_controls.is_some();
        let anchor = if self.sidebar_debug_namespace.is_none() { self.site_controls_anchor.clone() } else { Default::default() };
        let address_action = |id: &'static str, active: bool| div().id(id).debug_selector(|| self.sidebar_debug_name((move || id.into())()))
            .group(id).relative().flex_none().size(px(24.)).flex().items_center().justify_center()
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
            .pt(px(SIDEBAR_TOOLBAR_HEIGHT - 1.))
            .gap(px(8.))
            .text_color(hsla(0.0, 0.0, 0.1, 1.0))
            .child(div().absolute().top_0().left_0().right(px(8.)).h(px(SIDEBAR_TOOLBAR_HEIGHT)).flex().items_center().gap(px(6.))
                .child(div().flex_none().w(px(74.)).h_full())
                .child(control("toggle-sidebar").ml(px(-1.))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
                    .child(crate::icons::control(crate::icons::Icon::Sidebar)))
                .child(div().flex_1())
                .child(control("go-back")
                    .text_color(hsla(0., 0., if private { 1. } else { 0. }, if state.can_go_back { 0.6 } else { 0.25 }))
                    .on_click(cx.listener(|this, _, _, _| { this.with_active_page(|pages, tab| pages.go_back(tab)); }))
                    .child(crate::icons::control(crate::icons::Icon::Back)))
                .child(control("go-forward")
                    .text_color(hsla(0., 0., if private { 1. } else { 0. }, if state.can_go_forward { 0.6 } else { 0.25 }))
                    .on_click(cx.listener(|this, _, _, _| { this.with_active_page(|pages, tab| pages.go_forward(tab)); }))
                    .child(crate::icons::control(crate::icons::Icon::Forward)))
                .child(control("reload-page")
                    .on_click(cx.listener(move |this, _, _, _| { this.with_active_page(|pages, tab| { if loading { pages.stop_loading(tab); } else { pages.reload(tab); } }); }))
                    .child(crate::icons::control(if state.loading { crate::icons::Icon::Close } else { crate::icons::Icon::Reload }))))
            .child(
                div().px(px(SIDEBAR_INSET)).child(
                    div()
                        .id("address")
                        .debug_selector(|| self.sidebar_debug_name((|| "address".into())()))
                        .h(px(36.0))
                        .pl(px(13.))
                        .pr(px(5.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .rounded(px(SIDEBAR_ROW_RADIUS))
                        .bg(hsla(0.0, 0.0, 1.0, 0.35))
                        .hover(move |s| s.bg(hsla(0.0, 0.0, 1.0, if private { 0.2 } else { 0.5 })))
                        .text_size(px(13.))
                        .text_color(hsla(0.0, 0.0, 0.0, 0.6))
                        .when(private, |this| this.bg(hsla(0., 0., 1., 0.14)).text_color(hsla(0., 0., 1., 0.65)))
                        .whitespace_nowrap()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.on_edit_current_url(&EditCurrentUrl, window, cx)
                        }))
                        .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_sidebar_menu(crate::tab_menu::MenuTarget::Address, event.position, window, cx);
                        }))
                        .child(div().flex_1().min_w_0().overflow_hidden().child(url))
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
            )
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
        div().id("empty-favorites").debug_selector(|| self.sidebar_debug_name((|| "empty-favorites".into())()))
            .relative().w_full().h(px(95.)).flex_none()
            .rounded(px(12.)).border_1().border_dashed()
            .border_color(hsla(0., 0., 1., 0.26))
            .flex().flex_col().items_center().pt(px(11.))
            .text_color(hsla(0., 0., 1., 0.78))
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
                .text_color(hsla(0., 0., 1., 0.45))
                .hover(|style| style.text_color(hsla(0., 0., 1., 0.8)))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.edit_browser(|browser| browser.active_user_profile_mut().empty_favorites_dismissed = true);
                    cx.notify();
                }))
                .child(crate::icons::control_sized(crate::icons::Icon::Close, 12.)))
    }

    /// Window controls stay native for their behavior, but idle as neutral
    /// dots and show their colors only while the pointer is over them.
    fn render_traffic_lights(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let reveal = self.pinned_reveals.get(&space.id)
            .filter(|reveal| reveal.collapsed == space.pinned_collapsed)
            .map_or(if space.pinned_collapsed { 0. } else { 1. }, |reveal| reveal.amount);
        let pinned_height = pinned.len() as f32 * (SIDEBAR_ROW_HEIGHT + SIDEBAR_ROW_GAP) - SIDEBAR_ROW_GAP;
        let header_group = SharedString::from(format!("space-header-{index}"));
        let today_actions = !space.today.is_empty() && self.sidebar_hovered;
        let line_end = TODAY_ACTIONS_WIDTH + 11.;
        let header_menu_open = self.tab_menu.as_ref().is_some_and(|menu| menu.target == crate::tab_menu::MenuTarget::Space(space_id));

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
                    .group(header_group.clone())
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
                    .text_color(hsla(0., 0., 0., 0.5))
                    .when(space.custom_icon.is_some(), |this| this.child(div().relative().size(px(18.)).flex_none()
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("space-header-icon-{index}"))()))
                            .size_full().flex().items_center().justify_center()
                            .when(space.pinned_collapsed || header_menu_open, |this| this.invisible())
                            .group_hover(header_group.clone(), |style| style.invisible())
                            .when_some(space.custom_icon.as_deref(), |this, icon| this.child(crate::icons::glyph(icon))))
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
                .relative().flex_none().h(px(22.)).rounded_md()
                .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.4)))
                .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_on_section(drag, space_id, Section::Today, cx)))
                .child(div().absolute().left(px(10.)).top(px(11.)).h(px(1.)).bg(hsla(0.0, 0.0, 0.0, 0.12))
                    .with_animation(SharedString::from(format!("today-line-{index}-{today_actions}")), hover_animation(), move |line, delta| {
                        let shown = if today_actions { delta } else { 1. - delta };
                        line.right(px(10. + line_end * shown))
                    }))
                .when(!space.today.is_empty(), |this| this.child(hover_fade(div().absolute().right(px(10.)).top_0().h_full().w(px(TODAY_ACTIONS_WIDTH))
                    .flex().items_center().justify_end()
                    .text_size(px(11.)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(hsla(0.0, 0.0, 0.0, 0.35))
                    .child(div().id(("tidy-today", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "tidy-today".into())())))
                        .flex().items_center().gap(px(3.)).hover(|s| s.text_color(hsla(0.0, 0.0, 0.0, 0.6)))
                        .child(crate::icons::control_sized(crate::icons::Icon::Tidy, 12.)).child("Tidy"))
                    .child(div().mx(px(8.)).w(px(1.)).h(px(10.)).bg(hsla(0.0, 0.0, 0.0, 0.25)))
                    .child(div().id(("clear-today", index)).when(current, |this| this.debug_selector(|| self.sidebar_debug_name((|| "clear-today".into())())))
                        .flex().items_center().gap(px(2.)).hover(|s| s.text_color(hsla(0.0, 0.0, 0.0, 0.6)))
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
                    .text_color(hsla(0.0, 0.0, 0.0, 0.45))
                    .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)))
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
        let drag_moves_active = self.sidebar_page_interactive;
        let new_menu_anchor = if self.sidebar_debug_namespace.is_none() { self.new_menu_anchor.clone() } else { Default::default() };
        let normal = !self.browser.is_blank() && !self.browser.is_private();
        let pages = self.sidebar_navigation.controller.pages(&self.browser);
        let selected = self.sidebar_navigation.controller.selected();
        let corner = |id: &'static str| div().id(id).debug_selector(|| self.sidebar_debug_name((move || id.into())()))
            .flex_none().size(px(32.)).rounded_md().flex().items_center().justify_center()
            .text_color(hsla(0.0, 0.0, 0.0, 0.5)).hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.3)));
        div()
            .flex_none()
            .h(px(SPACE_BAR_HEIGHT))
            .px(px(SIDEBAR_INSET))
            .flex()
            .items_center()
            .child(corner("library")
                .when(normal && selected == crate::sidebar_pages::SidebarPage::Library, |this| this.bg(hsla(0., 0., 1., 0.3)))
                .when(normal, |this| this.on_click(cx.listener(|this, _, _, cx| {
                    this.select_sidebar_page(crate::sidebar_pages::SidebarPage::Library, cx);
                    this.settle_pager(cx);
                })))
                .child(crate::icons::control(crate::icons::Icon::Library)))
            .child(div().flex_1().min_w_0().flex().items_center().justify_center().gap(px(4.))
            .children(self.browser.spaces.iter().enumerate().flat_map(|(index, space)| {
                let space_id = space.id;
                let page_index = if normal {
                    pages.iter().position(|page| *page == crate::sidebar_pages::SidebarPage::Space(space_id)).unwrap_or(index + 1)
                } else { index };
                let active = page_index == self.pager.position.round() as usize;
                let drag = crate::space_drag::SpaceDrag { id: space_id, workspace: self.profile.handle.clone(), name: space.name.clone(), icon: space.custom_icon.clone(), hue: space.hue };
                // Space marks are monochrome; only user emoji keep their colors.
                let mark = hsla(0., 0., 0.1, if active { 0.8 } else { 0.3 });
                let real_mark = div()
                    .id(("space", index))
                    .debug_selector(|| self.sidebar_debug_name((move || format!("space-{index}"))()))
                    .group(SharedString::from(format!("space-mark-{index}")))
                    .flex_none().size(px(22.)).rounded_md().flex().items_center().justify_center()
                    .hover(|s| s.bg(hsla(0., 0., 1., 0.3)))
                    .when(space.custom_icon.is_none(), |this| this.child(div().size(px(if active { 8. } else { 7. })).rounded_full().bg(mark)
                        .group_hover(SharedString::from(format!("space-mark-{index}")), |s| s.bg(hsla(0., 0., 0.1, if active { 0.8 } else { 0.5 })))))
                    .when_some(space.custom_icon.clone(), |this, icon| this.text_color(mark)
                        .when(!active, |this| this.opacity(0.5))
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("space-icon-{index}"))())).child(crate::icons::glyph(&icon))))
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
                        .text_color(hsla(0., 0., 0.1, 0.6))
                        .when(selected == crate::sidebar_pages::SidebarPage::CreateSpace, |mark| mark.bg(hsla(0., 0., 1., 0.3)))
                        .on_click(cx.listener(move |this, _, _, cx| this.begin_space_creation(space_id, cx)))
                        .child(crate::icons::control_sized(crate::icons::Icon::NewSpace, 16.)).into_any_element());
                }
                marks
            })))
            .child(div().id("new-space").debug_selector(|| self.sidebar_debug_name((|| "new-space".into())())).group("new-space")
                .relative().flex_none().size(px(32.)).flex().items_center().justify_center()
                .text_color(hsla(0.0, 0.0, 0.0, 0.5))
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
                            move |bounds, _, cx| {
                                let bounds = engine::Bounds {
                                    x: f64::from(f32::from(bounds.origin.x)),
                                    y: f64::from(f32::from(bounds.origin.y)),
                                    width: f64::from(f32::from(bounds.size.width)),
                                    height: f64::from(f32::from(bounds.size.height)),
                                };
                                let surfaces = match pages.try_borrow_mut() {
                                    Ok(mut pages) => { if pages.set_viewport_bounds(generation, bounds) { Some(pages.surface_bounds()) } else { None } }
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
                                        let surfaces = { let mut pages = pages.borrow_mut(); if !pages.set_viewport_bounds(generation, bounds) { return; } pages.surface_bounds() };
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
                        .left_0()
                        .size_full()
                        .into_any_element())
        })
    }

    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let virtual_page = !self.page_content_visible();
        let split_open = !virtual_page && self.browser.active_split().is_some();
        let has_page = !virtual_page && self.browser.active_tab().is_some();
        let history_open = !virtual_page && self.history_is_open();
        let internal = self.browser.active_tab().is_some_and(|tab| omnibox::is_internal_url(&tab.url));
        // The page view covers the card; text only shows when there is no page.
        let placeholder = if virtual_page || split_open || history_open || (self.browser.is_private() && self.browser.active_tab().is_none()) { None } else if internal { Some(("Page not found".into(), self.browser.active_tab().unwrap().url.clone())) } else { match (self.browser.active_tab(), &self.pages) {
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
            .py(px(CONTENT_INSET))
            .pr(px(CONTENT_INSET))
            .pl(px(CONTENT_INSET * (1. - self.sidebar_amount())))
            .child(
                div()
                    .debug_selector(|| "page-surface".into())
                    .left(px(self.library_page_offset()))
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .when(!split_open, |this| this.px_8())
                    .overflow_hidden()
                    .when(has_page && !split_open, |this| this.rounded(px(CONTENT_CORNER_RADIUS)).bg(gpui::white()).shadow_md())
                    .when(self.browser.is_private() && has_page && !split_open, |this| this.bg(hsla(210. / 360., 0.035, 0.15, 1.)))
                    .relative()
                    .when(!has_page, |this| this.child(div().debug_selector(|| "empty-space-canvas".into()).absolute().inset_0()))
                    .when(split_open, |this| this.child(self.render_split(cx)))
                    .when(history_open, |this| this.child(self.render_history(cx)))
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
                    .children(self.render_page_viewport_canvas()),
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
            .bg(self.theme_background())
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
        let count = self.suggestions(cx).len();
        if let Some(bar) = &mut self.command_bar { bar.selected = bar.selected.min(count.saturating_sub(1)); }
        cx.notify();
    }

    fn render_command_bar_overlay(&self, width: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        match &self.command_bar {
            Some(bar) => self.render_command_bar(bar, width, height, cx).into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn render_command_bar(&self, bar: &CommandBar, width: f32, height: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let history_owner = self.browser.active_user_profile_id();
        let accent = self.accent();
        let region = self.split_draft.as_ref().and_then(|draft| {
            let split = self.browser.split_for(draft.pane)?;
            let left = self.sidebar_width() * self.sidebar_amount() + CONTENT_INSET * (1. - self.sidebar_amount());
            let card = engine::Bounds { x: left.into(), y: CONTENT_INSET.into(), width: (width - left - CONTENT_INSET).max(0.).into(), height: (height - 2. * CONTENT_INSET).max(0.).into() };
            let index = split.panes.iter().position(|id| *id == draft.pane)?;
            crate::split_layout::layout(card, split.axis, &split.weights, true).get(index).map(|bounds| bounds.page)
        });
        let panel_width = region.map_or(COMMAND_BAR_WIDTH.min((width - 32.).max(0.)), |bounds| COMMAND_BAR_WIDTH.min((bounds.width as f32 - 32.).max(0.)));
        let archive = matches!(bar.target, Target::Archive(_));
        // Arc anchors the top to the center of a nominal 338px panel. Result
        // changes grow down from that point instead of moving the input.
        let default_top = ((height - 338.) * 0.5).max(16.);
        let rows_height = region.map_or((height - default_top - if archive { 150. } else { 90. } - 16.).max(40.), |bounds| (bounds.height as f32 - 96.).max(40.));
        let count = self.suggestions(cx).len() as f32;
        let panel_height = COMMAND_BAR_HEADER + if count > 0. { 18. + (count * 50. - 4.).min(rows_height) } else { 0. };
        let top = region.map_or(default_top, |bounds| bounds.y as f32 + ((bounds.height as f32 - panel_height) * 0.5).max(8.));
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
                let current_url_tab = match (&suggestion, bar.target) {
                    (Suggestion::Load(Resolved::Url(url)), Target::Tab(id)) => self.browser.all_tabs().find(|tab| tab.id == id && tab.url == *url),
                    _ => None,
                };
                let page = match &suggestion {
                    Suggestion::Load(Resolved::Url(_)) if current_url_tab.is_some() => current_url_tab.map(|tab| self.render_favicon_on(tab, accent, !selected)),
                    Suggestion::SwitchToTab { id, .. } | Suggestion::SplitTab { id, .. } => self.browser.all_tabs().find(|tab| tab.id == *id).map(|tab| self.render_favicon_on(tab, accent, !selected)),
                    Suggestion::History { url, .. } => self.browser.favicon(self.browser.active_user_profile_id(), url).map(|png| self.favicon_styles.borrow_mut().get(png)).map(|style| gpui::img(if selected { style.image } else { style.on_dark })
                        .size(px(16.0)).flex_none().object_fit(gpui::ObjectFit::Contain).into_any_element()),
                    _ => None,
                };
                let archived_id = match &suggestion { Suggestion::ArchivedTab { id, .. } => Some(*id), _ => None };
                let history_url = match &suggestion { Suggestion::History { url, .. } => Some(url.clone()), _ => None };
                let search_row = matches!(&suggestion, Suggestion::Load(Resolved::Search { .. }) | Suggestion::Calculator { .. });
                let (icon, label, detail): (crate::icons::Icon, String, String) = match suggestion {
                    Suggestion::Load(Resolved::Search { query, .. }) => {
                        (crate::icons::Icon::Search, query, String::new())
                    }
                    Suggestion::Calculator { answer } => (crate::icons::Icon::Calculator, format!("= {answer}"), "Copy".into()),
                    Suggestion::Load(Resolved::Url(url)) => {
                        let display = display_url(&url);
                        let label = current_url_tab.map(|tab| self.browser.item_title(tab.id)).unwrap_or_else(||
                            omnibox::TOP_DOMAINS.iter().find(|(domain, _)| display.trim_end_matches('/') == *domain)
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
                    .when(!search_row && selected && page.is_none(), |this| this.bg(gpui::white()).text_color(hsla(0.0, 0.0, 0.12, 1.0)))
                    .when(!search_row && !selected && page.is_none(), |this| this.bg(hsla(0.0, 0.0, 1.0, 0.07)).text_color(hsla(0.0, 0.0, 1.0, 0.7)))
                    .child(page.unwrap_or_else(|| crate::icons::control_sized(icon, 14.0)));
                let row = div()
                    .id(("suggestion", index))
                    .debug_selector(|| self.sidebar_debug_name((move || format!("suggestion-{index}"))()))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .h(px(46.0)).flex_none()
                    .pl(px(10.0)).pr(px(11.0))
                    .mx_2()
                    .rounded(px(8.0))
                    .text_color(hsla(0.0, 0.0, 1.0, if selected { 1.0 } else { 0.82 }))
                    .when(selected, |this| this.bg(if bar.site_search.is_some() { gpui::rgb(0xffa000).into() } else { hsla(accent, 0.67, 0.55, 1.0) }))
                    .when(!selected, |this| this.hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.04))))
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
                            .text_color(hsla(0.0, 0.0, 1.0, if selected { 0.95 } else { 0.4 }))
                            .child(detail),
                    )
                    .when_some(archived_id, |this, id| this.child(div()
                        .id(("delete-archived", id.get() as usize))
                        .debug_selector(|| self.sidebar_debug_name((move || format!("delete-archived-{}", id.get()))()))
                        .px_2().rounded_md().hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.12)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_archive_entry(id, cx);
                        })).child(crate::icons::control(crate::icons::Icon::Close))))
                    .when_some(history_url, |this, url| this.child(div()
                        .id(("delete-suggestion", index))
                        .debug_selector(|| self.sidebar_debug_name((move || format!("delete-suggestion-{index}"))()))
                        .px_2().rounded_md().hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.12)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_command_history_entry(history_owner, &url, cx);
                        })).child(crate::icons::control(crate::icons::Icon::Close))))
                    .when(!search_row && current_url_tab.is_none(), |this| this.child(div().size(px(24.0)).flex_none().flex().items_center().justify_center().rounded(px(5.0))
                        .bg(if selected { gpui::white() } else { hsla(0.0, 0.0, 1.0, 0.06) })
                        .text_color(if selected { hsla(0.0, 0.0, 0.12, 1.0) } else { hsla(0.0, 0.0, 1.0, 0.6) })
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
            .when_some(region, |this, bounds| this.pl(px(bounds.x as f32)).pr(px((width - bounds.x as f32 - bounds.width as f32).max(0.))))
            .bg(hsla(0.0, 0.0, 0.0, if region.is_some() { 0.0 } else { 0.15 }))
            .on_click(cx.listener(|this, _, _, cx| this.close_command_bar(cx)))
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
                    .on_action(cx.listener(|this, _: &Confirm, _, cx| {
                        let index = this.command_bar.as_ref().map_or(0, |bar| bar.selected);
                        this.confirm(index, cx)
                    }))
                    .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                        this.close_command_bar(cx)
                    }))
                    // Keep clicks inside the panel from reaching the backdrop.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .w(px(panel_width))
                    .relative()
                    .flex()
                    .flex_col()
                    .rounded(px(COMMAND_BAR_RADIUS))
                    .bg(hsla(200. / 360., 0.05, 0.1, if crate::native::HAS_BACKDROP { 0.88 } else { 0.98 }))
                    .shadow_2xl()
                    .border_1()
                    .border_color(hsla(0.0, 0.0, 1.0, 0.2))
                    .text_color(hsla(0.0, 0.0, 1.0, 0.92))
                    .font_family(".SystemUIFont")
                    .child(gpui::canvas(|_, _, _| (), |bounds, _, window, _| crate::panel::backdrop(window, "command-bar", Some(bounds), COMMAND_BAR_RADIUS, crate::native::Material::Hud)).absolute().inset_0())
                    .child(
                        div()
                            .h(px(COMMAND_BAR_HEADER - 1.)).flex_none()
                            .flex().items_center().gap(px(13.0))
                            .pl(px(22.0)).pr(px(21.0))
                            .child(div().text_color(hsla(0.0, 0.0, 1.0, 0.75)).child(
                                if let Target::Tab(id) = bar.target {
                                    self.browser.all_tabs().find(|tab| tab.id == id)
                                        .map(|tab| self.render_favicon_on(tab, accent, true))
                                        .unwrap_or_else(|| crate::icons::control_sized(crate::icons::Icon::Search, 16.0))
                                } else { crate::icons::control_sized(crate::icons::Icon::Search, 16.0) }))
                            .when_some(bar.site_search, |this, site| this.child(div()
                                .debug_selector(|| "site-search-pill".into())
                                .rounded_full().px(px(10.)).h(px(24.)).flex().items_center()
                                .bg(gpui::rgb(0xffa000)).text_color(gpui::white())
                                .text_size(px(13.)).font_weight(gpui::FontWeight::SEMIBOLD).child(site.name())))
                            .child(div()
                                .debug_selector(|| self.sidebar_debug_name((|| "command-bar-input".into())()))
                                .flex_1().min_w_0()
                                .text_size(px(18.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .line_height(px(28.0))
                                .child(bar.input.clone()))
                            .when(matches!(bar.target, Target::NewTab | Target::Tab(_)) && bar.site_search.is_none(), |this| this.child(div()
                                .id("suggestions-info").debug_selector(|| "suggestions-info".into())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(bar) = &mut this.command_bar { bar.suggestions_privacy = !bar.suggestions_privacy; }
                                    cx.notify();
                                }))
                                .size(px(24.0)).flex_none().flex().items_center().justify_center().rounded_full()
                                .bg(hsla(0.0, 0.0, 0.0, 0.8)).text_color(hsla(0.0, 0.0, 1.0, 0.55))
                                .child(crate::icons::control_sized(crate::icons::Icon::Info, 17.0)))),
                    )
                    .when(matches!(bar.target, Target::Icon(_)), |this| this.child(div().px_4().pb_2().text_xs().child("Enter one symbol or emoji. Leave blank to restore the default icon.")))
                    .when(matches!(bar.target, Target::Archive(_)), |this| this.child(self.render_archive_controls(cx)))
                    .when(bar.target == Target::ConfirmClearArchive, |this| this.child(div().px_4().pb_2().text_sm().child("This action is permanent. Archived tabs from all Spaces will be deleted.")))
                    .when(!has_rows && matches!(bar.target, Target::Archive(_)), |this| this.child(div().px_4().py_3().text_sm().child("No archived tabs match your search.")))
                    .when(has_rows, |this| {
                        this.child(div().h(px(1.0)).bg(hsla(0.0, 0.0, 1.0, 0.1)))
                            .text_size(px(14.0))
                            .child(div().id("command-suggestions").debug_selector(|| self.sidebar_debug_name((|| "command-suggestions".into())()))
                                .flex().flex_col().gap_1().pt(px(6.0)).pb(px(10.0)).max_h(px(rows_height + 16.)).overflow_y_scroll().track_scroll(&bar.rows_scroll).children(rows))
                    })
                    .when(bar.suggestions_privacy, |this| this.child(div()
                        .debug_selector(|| "suggestions-privacy".into())
                        .flex().items_center().gap_3().px_4().py_3()
                        .bg(gpui::black()).rounded_b(px(COMMAND_BAR_RADIUS))
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
        if crate::quit::is_quitting() {
            return div().size_full().p_8().bg(gpui::rgb(0x202126)).text_color(gpui::rgb(0xf2f2f2))
                .child(crate::quit::diagnostic(cx)).into_any_element();
        }
        self.refresh_profile_projection();
        self.sync_library_geometry(window.viewport_size(), cx);
        if self.tab_rename.as_ref().is_some_and(|editor| !self.can_rename_tab_inline_for(editor.space, editor.profile, editor.tab)) {
            self.finish_tab_rename(false, cx);
        }
        self.sync_library_downloads(window, cx);
        self.remember_library_preview_focus(window, cx);
        let open: std::collections::HashSet<_> = self.browser.all_tabs().map(|tab| tab.id).collect();
        self.tab_drop_edges.get_mut().retain(|id, _| open.contains(id));
        for space in &self.browser.spaces { self.sidebar_scrolls.entry(space.id).or_default(); }
        if self.page_content_visible() && self.user_profile_dialog.is_none() { self.sync_history_focus(window, cx); }
        if self.tab_menu.as_ref().is_some_and(|menu| !self.menu_target_exists(menu.target)) {
            self.close_tab_menu(cx);
        }
        if self.find_bar.as_ref().is_some_and(|bar| Some(bar.tab) != self.browser.space().active_tab) {
            self.close_find(cx);
        }
        self.sync_page_viewport();
        if let Some(pages) = &self.pages {
            match pages.try_borrow_mut() {
                Ok(mut pages) => pages.sync(&self.browser, self.page_content_visible()),
                // Re-entered from engine work; sync on the next frame.
                Err(_) => cx.on_next_frame(window, |_, _, cx| cx.notify()),
            }
        }
        self.sync_link_status(window, cx);
        let sidebar_amount = self.sidebar_amount();
        let sidebar_offset = self.sidebar_width() * (sidebar_amount - 1.);
        let window_controls_offset = self.window_controls_offset();
        if let Some(view) = self.native_view {
            crate::native::position_traffic_lights(view, TRAFFIC_LIGHT_X + window_controls_offset);
        }

        let library_open = !self.page_content_visible() && self.sidebar_navigation.controller.selected() == crate::sidebar_pages::SidebarPage::Library;
        let sidebar = (!library_open && sidebar_amount > 0.).then(|| self.render_sidebar(cx));
        let library = library_open.then(|| self.render_library_host(cx));
        let content = if library_open { div().into_any_element() } else { self.render_content(cx).into_any_element() };
        div()
            .key_context(if self.sidebar_resize.is_some() { "BrowserWindow SidebarResizing" } else { WINDOW_CONTEXT })
            .when(library_open, |this| this.on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_sidebar_scroll(event, cx))))
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| { this.capture_sidebar_resize_key(event, window, cx); }))
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
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this
                .on_action(cx.listener(|this, _: &crate::menus::NewProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_creation(crate::user_profile_dialog::CreationSource::File(space), cx); }))
                .on_action(cx.listener(|this, _: &crate::menus::ChangeProfile, _, cx| { let space = this.browser.space().id; this.open_user_profile_switch(space, cx); })))
            .on_action(cx.listener(|this, _: &crate::menus::CloseWindow, _, cx| this.close_window(cx)))
            .when(self.tab_rename.is_none(), |this| this
                .on_action(cx.listener(|this, _: &crate::text_input::Undo, _, cx| this.edit_undo(false, cx)))
                .on_action(cx.listener(|this, _: &crate::text_input::Redo, _, cx| this.edit_undo(true, cx))))
            .on_action(cx.listener(|this, _: &crate::history::ViewHistory, window, cx| this.open_history(window, cx)))
            .when(!self.browser.is_private() && !self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &ViewLibrary, _, cx| this.run_command(Command::OpenLibrary, cx))).on_action(cx.listener(|this, _: &ViewDownloads, _, cx| this.run_command(Command::ViewDownloads, cx))).on_action(cx.listener(|this, _: &crate::menus::ManageSpaces, _, cx| this.run_command(Command::ManageSpaces, cx))))
            .on_action(cx.listener(|this, _: &crate::text_input::PasteAndMatchStyle, _, cx| this.paste_and_match_style(cx)))
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
            .on_action(cx.listener(|this, _: &crate::menus::ResetSpace, _, cx| this.run_command(Command::ResetSpace, cx)))
            .on_action(cx.listener(|this, _: &NewSpace, window, cx| this.begin_space_creation(this.browser.space().id, cx)))
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
            .when(!self.browser.is_blank(), |this| this.on_action(cx.listener(|this, _: &NewFolder, window, cx| this.open_command_action(Command::NewFolder, window, cx))))
            .on_action(cx.listener(|this, _: &CollapseAllFolders, _, cx| this.run_command(Command::CollapseAllFolders, cx)))
            .on_action(cx.listener(|this, _: &ExpandAllFolders, _, cx| this.run_command(Command::ExpandAllFolders, cx)))
            .on_action(cx.listener(|this, _: &CustomizeTabIcon, window, cx| this.open_command_action(Command::CustomizeTabIcon, window, cx)))
            .on_action(cx.listener(|this, _: &CustomizeSpaceIcon, window, cx| this.open_command_action(Command::CustomizeSpaceIcon, window, cx)))
            .on_action(cx.listener(|this, _: &ResetNameAndIcon, _, cx| this.run_command(Command::ResetNameAndIcon, cx)))
            .on_action(cx.listener(|this, _: &ToggleSpaceHeader, _, cx| this.run_command(Command::ToggleSpaceHeader, cx)))
            .on_action(cx.listener(|this, _: &ToggleFavoritesOrdering, _, cx| this.run_command(Command::ToggleFavoritesOrdering, cx)))
            .on_action(cx.listener(|this, _: &RevealTab, _, cx| this.reveal_active_tab(cx)))
            .on_action(cx.listener(|this, _: &DuplicateTab, _, cx| this.run_command(Command::DuplicateTab, cx)))
            .when(!self.browser.is_blank(), |this| this.on_action(cx.listener(Self::on_toggle_pin)))
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
            .bg(self.theme_background())
            .when(!library_open, |this| this.child(div().debug_selector(|| self.sidebar_debug_name((|| "sidebar-clip".into())()))
                .relative().flex_none().h_full().w(px(self.sidebar_width() * sidebar_amount)).overflow_hidden()
                .when_some(sidebar, |this, sidebar| this.child(div().absolute().left(px(sidebar_offset)).top_0().bottom_0().child(sidebar)))))
            .child(self.render_sidebar_resize_tracking(cx))
            .child(div().id("native-action-scope").absolute().size_0().track_focus(&self.native_action_focus))
            .child(content)
            .when(!library_open && sidebar_amount > 0., |this| this.child(self.render_sidebar_resize_handle(false, cx)))
            .when_some(library, |this, library| this.child(library))
            .when(self.native_page_content_visible(), |this| this.children(self.render_window_snapshots()))
            .child(div().absolute().left(px(window_controls_offset)).top_0().w(px(74.)).h(px(SIDEBAR_TOOLBAR_HEIGHT)).child(self.render_traffic_lights(cx)))
            .child(release_hover_on_exit())
            .into_any_element()

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
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else { return display_url(url); };
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    host.strip_prefix("www.").unwrap_or(host).to_string()
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
