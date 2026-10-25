//! Application menus share the same GPUI actions as shortcuts and the Command Bar.
use crate::{find, history, native, text_input, window, Quit};
use gpui::{actions, App, KeyBinding, Menu, MenuItem, OsAction};

actions!(
    menu,
    [
        About,
        CopyBrowserInfo,
        AppearanceHeading,
        AutomaticAppearance,
        LightAppearance,
        DarkAppearance,
        ViewMedia,
        ViewBoosts,
        ClearCookiesAndRefresh,
        ClearCacheAndRefresh,
        PrivacyPolicy,
        Settings,
        ImportFromBrowser,
        SetDefaultBrowser,
        HideApp,
        HideOthers,
        ShowAllApps,
        BringAllToFront,
        Minimize,
        ZoomWindow,
        ToggleFullscreen,
        CloseWindow,
        NewWindow,
        NewProfile,
        ManageSpaces,
        EditTheme,
        ChangeProfile,
        BlankWindow,
        NewIncognitoWindow,
        RenameSpace,
        RenameTab,
        ResetTabName,
        ResetPinnedTab,
        ReplacePinnedUrl,
        ResetSpace,
        ViewArchive,
        ClearArchive,
        AllowJavaScriptFromAppleEvents
    ]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-n", NewWindow, None), KeyBinding::new("cmd-,", Settings, None), KeyBinding::new("cmd-alt-n", crate::transient::NewLittleWindow, None)]);
    cx.on_action(|_: &crate::transient::NewLittleWindow, cx| { cx.defer(|cx| crate::transient::open(None, cx)); });
    cx.bind_keys([KeyBinding::new("ctrl-cmd-n", BlankWindow, None)]);
    cx.on_action(|_: &BlankWindow, cx| { cx.defer(crate::blank_window::open); });
    cx.on_action(|_: &NewWindow, cx| { cx.defer(|cx| crate::new_window::open(true, cx)); });
    cx.on_action(|_: &NewIncognitoWindow, cx| { cx.defer(crate::open_incognito_window); });
    cx.on_action(|_: &Quit, cx| crate::quit::request(cx));
    cx.on_action(|_: &AllowJavaScriptFromAppleEvents,cx|crate::scripting::toggle_javascript(cx));
    cx.on_action(|_: &HideApp, cx| native::set_app_visibility(native::AppVisibility::Hide, cx));
    cx.on_action(|_: &HideOthers, cx| {
        native::set_app_visibility(native::AppVisibility::HideOthers, cx)
    });
    cx.on_action(|_: &ShowAllApps, cx| {
        native::set_app_visibility(native::AppVisibility::ShowAll, cx)
    });
    cx.on_action(|_: &BringAllToFront, cx| {
        cx.defer(|cx| {
            cx.activate(true);
            for handle in cx.windows() {
                if handle.downcast::<window::BrowserWindow>().is_some() {
                    let _ = handle.update(cx, |_, window, _| window.activate_window());
                }
            }
        });
    });
    if native::supports_app_visibility() {
        cx.bind_keys([
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
        ]);
    }
    cx.set_menus(build());
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct SidebarUndoLabels(Option<&'static str>, Option<&'static str>, bool);
impl gpui::Global for SidebarUndoLabels {}

pub(crate) fn sync_sidebar_undo(undo: Option<&'static str>, redo: Option<&'static str>, multiple: bool, cx: &mut App) {
    let labels = SidebarUndoLabels(undo, redo, multiple);
    if cx.try_global::<SidebarUndoLabels>() == Some(&labels) { return; }
    cx.set_global(labels);
    cx.set_menus(build_with_sidebar_undo(undo, redo, multiple));
    sync_native_appearance(cx);
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MenuAppearance(browser_core::themes::ThemeAppearance);
impl gpui::Global for MenuAppearance {}
pub(crate) fn sync_appearance(mode: browser_core::themes::ThemeAppearance, cx: &mut App) {
    if cx.try_global::<MenuAppearance>().is_some_and(|state| state.0 == mode) { return; }
    cx.set_global(MenuAppearance(mode));
    sync_native_appearance(cx);
}
fn sync_native_appearance(cx: &mut App) {
    if let Some(state) = cx.try_global::<MenuAppearance>() {
        let index = match state.0 {
            browser_core::themes::ThemeAppearance::Automatic => 0,
            browser_core::themes::ThemeAppearance::Light => 1,
            browser_core::themes::ThemeAppearance::Dark => 2,
        };
        native::set_appearance_menu_selection(index);
    }
}

pub(crate) fn build() -> Vec<Menu> { build_with_sidebar_undo(None, None, false) }
fn build_with_sidebar_undo(undo: Option<&'static str>, redo: Option<&'static str>, multiple: bool) -> Vec<Menu> {
    let mut app = vec![MenuItem::action("About Echo", About), MenuItem::separator(), MenuItem::action("Preferences…", Settings),
        MenuItem::action("Set as Default Browser", SetDefaultBrowser), MenuItem::separator(),
        MenuItem::action("Import from Another Browser…", ImportFromBrowser), MenuItem::separator()];
    app.extend([MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
        MenuItem::action("Privacy Policy", PrivacyPolicy), MenuItem::separator()]);
    if native::supports_app_visibility() {
        app.extend([
            MenuItem::action("Hide Echo", HideApp),
            MenuItem::action("Hide Other Windows", HideOthers),
            MenuItem::action("Show All", ShowAllApps),
            MenuItem::separator(),
        ]);
    }
    app.push(MenuItem::action("Quit Echo", Quit));
    vec![
        Menu {
            name: "Echo".into(),
            items: app,
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Tab…", window::OpenCommandBar),
                MenuItem::action("New Window", NewWindow),
                MenuItem::action("Blank Window", BlankWindow),
                MenuItem::action("New Incognito Window", NewIncognitoWindow),
                MenuItem::action("New Little Echo Window", crate::transient::NewLittleWindow),
                MenuItem::action("Restore Last Closed Tab", window::RestoreTab),
                MenuItem::separator(),
                MenuItem::action("Open Command Bar", window::EditCurrentUrl),
                MenuItem::separator(),
                MenuItem::action("New Profile", NewProfile),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
                MenuItem::action("Archive Tab", window::CloseTab),
                MenuItem::separator(),
                MenuItem::action("Share…", window::SharePage),
                MenuItem::action("Save Page As", window::SavePage),
                MenuItem::action("Print", window::PrintPage),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action(undo.map(|label| format!("Undo {label}")).unwrap_or_else(|| "Undo".into()), text_input::Undo, OsAction::Undo),
                MenuItem::os_action(redo.map(|label| format!("Redo {label}")).unwrap_or_else(|| "Redo".into()), text_input::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", text_input::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", text_input::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", text_input::Paste, OsAction::Paste),
                MenuItem::action("Paste and Match Style", text_input::PasteAndMatchStyle),
                MenuItem::os_action("Select All", text_input::SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action(if multiple { "Copy URLs" } else { "Copy URL" }, window::CopyUrl),
                MenuItem::action(if multiple { "Copy URLs as Markdown" } else { "Copy URL as Markdown" }, window::CopyUrlAsMarkdown),
                MenuItem::action("Copy URL as Quote", window::CopyUrlAsQuote),
                MenuItem::separator(),
                MenuItem::action("Find…", find::FindPage),
                MenuItem::action("Find Next", find::NextMatch),
                MenuItem::action("Find Previous", find::PreviousMatch),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::submenu(Menu {
                    name: "Appearance".into(),
                    items: vec![
                        MenuItem::action("Websites, Easels, and Notes will use:", AppearanceHeading),
                        MenuItem::action("Automatic", AutomaticAppearance),
                        MenuItem::action("Light", LightAppearance),
                        MenuItem::action("Dark", DarkAppearance),
                    ],
                }),
                MenuItem::separator(),
                MenuItem::action("Add Split View…", window::AddSplit),
                MenuItem::action("Close this Split Pane", window::CloseSplitPane),
                MenuItem::action("Expand Current Split", window::ExpandCurrentSplit),
                MenuItem::separator(),
                MenuItem::action("Show/Hide Sidebar", window::ToggleSidebar),
                MenuItem::action("Show/Hide Toolbar", window::ToggleToolbar),
                MenuItem::action(
                    "Expand/Collapse Pinned Section",
                    window::TogglePinnedSection,
                ),
                MenuItem::separator(),
                MenuItem::action("Stop Loading", window::StopLoading),
                MenuItem::action("Refresh the Page", window::Reload),
                MenuItem::action("Force Refresh", window::ForceReload),
                MenuItem::action("Clear Cookies and Refresh", ClearCookiesAndRefresh),
                MenuItem::action("Clear Cache and Refresh", ClearCacheAndRefresh),
                MenuItem::separator(),
                MenuItem::action("Zoom to Actual Size", window::ResetZoom),
                MenuItem::action("Zoom In", window::ZoomIn),
                MenuItem::action("Zoom Out", window::ZoomOut),
                MenuItem::separator(),
                MenuItem::Submenu(Menu {
                    name: "Developer".into(),
                    items: developer_items(),
                }),
                MenuItem::action("Enter/Exit Full Screen", ToggleFullscreen),
            ],
        },
        Menu {
            name: "Spaces".into(),
            items: vec![
                MenuItem::action("New Space…", window::NewSpace),
                MenuItem::action("Edit Theme…", EditTheme),
                MenuItem::action("New Folder…", window::NewFolder),
                MenuItem::action("New Boost", window::NewBoost),
                MenuItem::action("Rename Space…", RenameSpace),
                MenuItem::action("Change Profile", ChangeProfile),
                MenuItem::action("Change Space Icon…", window::CustomizeSpaceIcon),
                MenuItem::action("Show/Hide Space Header", window::ToggleSpaceHeader),
                MenuItem::separator(),
                MenuItem::action("Next Space", window::NextSpace),
                MenuItem::action("Previous Space", window::PreviousSpace),
                MenuItem::separator(),
                MenuItem::action("Manage Spaces…", ManageSpaces),
                MenuItem::action("Reset Space", ResetSpace),
            ],
        },
        Menu {
            name: "Tabs".into(),
            items: vec![
                MenuItem::action("Include/Exclude Favorites in Ordering", window::ToggleFavoritesOrdering),
                MenuItem::action("Collapse All Folders", window::CollapseAllFolders),
                MenuItem::action("Expand All Folders", window::ExpandAllFolders),
                MenuItem::action("Reveal Tab in Sidebar", window::RevealTab),
                MenuItem::action("Duplicate Current Tab", window::DuplicateTab),
                MenuItem::action("Pin/Unpin Tab", window::TogglePin),
                MenuItem::action("Favorite/Unfavorite Tab", window::ToggleFavorite),
                MenuItem::action("Mute/Unmute Tab", window::ToggleMute),
                MenuItem::action("Mute All Tabs", window::MuteAllTabs),
                MenuItem::action("Unmute All Tabs", window::UnmuteAllTabs),
                MenuItem::action("Rename Current Tab…", RenameTab),
                MenuItem::action("Customize Tab Icon…", window::CustomizeTabIcon),
                MenuItem::action("Reset Name and Icon", window::ResetNameAndIcon),
                MenuItem::action("Reset Tab Name", ResetTabName),
                MenuItem::action("Reset Tab to Original URL", ResetPinnedTab),
                MenuItem::action("Replace Pinned URL with Current", ReplacePinnedUrl),
                MenuItem::separator(),
                MenuItem::action("Next Tab", window::NextTab),
                MenuItem::action("Previous Tab", window::PreviousTab),
                MenuItem::action("Clear Today", window::ClearToday),
            ],
        },
        Menu {
            name: "Archive".into(),
            items: vec![
                MenuItem::action("Go Back", window::GoBack),
                MenuItem::action("Go Forward", window::GoForward),
                MenuItem::separator(),
                MenuItem::action("View History", history::ViewHistory),
            MenuItem::action("View Archive", ViewArchive),
                MenuItem::action("Clear Archive…", ClearArchive),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize Window", Minimize),
                MenuItem::action("Zoom", ZoomWindow),
                MenuItem::separator(),
                MenuItem::action("Open Library…", window::ViewLibrary),
                MenuItem::action("View Downloads…", window::ViewDownloads),
                MenuItem::action("View Media…", ViewMedia),
                MenuItem::action("View Boosts…", ViewBoosts),
                MenuItem::separator(),
                MenuItem::action("Bring All to Front", BringAllToFront),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("Essential Keyboard Shortcuts", crate::essential_shortcuts::EssentialShortcuts), MenuItem::separator(), MenuItem::Submenu(Menu { name: "Troubleshooting".into(), items: vec![MenuItem::action("Open Task Manager", crate::task_manager::OpenTaskManager), MenuItem::separator(), MenuItem::action("Copy Echo Info", CopyBrowserInfo)] })],
        },
    ]
}

#[derive(Clone, Copy)]
pub(crate) enum WindowOperation {
    Minimize,
    Zoom,
    ToggleFullscreen,
    Fullscreen(bool),
}

impl window::BrowserWindow {
    pub(crate) fn open_menu_utility(&mut self, open: fn(&mut App), cx: &mut gpui::Context<Self>) {
        self.suspend_popup_focus_for_utility();
        self.finish_recent_items(false, crate::window::PopupFocus::OnActivation, cx);
        self.close_command_bar_with_focus(crate::window::PopupFocus::OnActivation, cx);
        cx.defer(open);
    }

    pub(crate) fn perform_window_operation(
        &mut self,
        operation: WindowOperation,
        cx: &mut gpui::Context<Self>,
    ) {
        self.close_popups_for_window_operation(matches!(operation, WindowOperation::Minimize), cx);
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, _| match operation {
                WindowOperation::Minimize => window.minimize_window(),
                WindowOperation::Zoom => window.zoom_window(),
                WindowOperation::ToggleFullscreen => window.toggle_fullscreen(),
                WindowOperation::Fullscreen(wanted) => {
                    if window.is_fullscreen() != wanted {
                        window.toggle_fullscreen();
                    }
                }
            });
        });
    }
}

fn developer_items()->Vec<MenuItem>{
    let mut items=vec![MenuItem::action("View Source",window::ViewSource),MenuItem::action("Developer Tools",window::DeveloperTools),MenuItem::action("Inspect Elements",window::InspectElements),MenuItem::action("JavaScript Console",window::JavaScriptConsole),MenuItem::action("Network Inspector",window::NetworkInspector)];
    #[cfg(target_os="macos")]
    items.push(MenuItem::action("Allow JavaScript from Apple Events",AllowJavaScriptFromAppleEvents));
    items
}
