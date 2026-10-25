//! Application menus share the same GPUI actions as shortcuts and the Command Bar.
use crate::{find, history, native, text_input, window, Quit};
use gpui::{actions, App, KeyBinding, Menu, MenuItem, OsAction};

actions!(
    menu,
    [
        About,
        CopyBrowserInfo,
        RevealData,
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
    cx.on_action(|_: &RevealData, cx| cx.defer(crate::diagnostics::reveal_data));
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
    install_menu_model(build(), cx);
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct SidebarUndoLabels(Option<&'static str>, Option<&'static str>, bool);
impl gpui::Global for SidebarUndoLabels {}

pub(crate) fn sync_sidebar_undo(undo: Option<&'static str>, redo: Option<&'static str>, multiple: bool, cx: &mut App) {
    let labels = SidebarUndoLabels(undo, redo, multiple);
    if cx.try_global::<SidebarUndoLabels>() == Some(&labels) { return; }
    cx.set_global(labels);
    install_menu_model(build_with_sidebar_undo(undo, redo, multiple), cx);
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PinnedMenu(bool);
impl gpui::Global for PinnedMenu {}
pub(crate) fn sync_pinned(pinned: bool, cx: &mut App) {
    if cx.try_global::<PinnedMenu>().is_some_and(|state| state.0 == pinned) { return; }
    cx.set_global(PinnedMenu(pinned));
    refresh_shortcuts(cx);
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct DeveloperModeMenu(bool);
impl gpui::Global for DeveloperModeMenu {}
pub(crate) fn sync_developer_mode(enabled: bool, cx: &mut App) {
    if cx.try_global::<DeveloperModeMenu>().is_some_and(|mode|mode.0==enabled) { return; }
    cx.set_global(DeveloperModeMenu(enabled));
    refresh_shortcuts(cx);
}

pub(crate) fn refresh_shortcuts(cx: &mut App) {
    let labels = cx.try_global::<SidebarUndoLabels>().copied().unwrap_or_default();
    install_menu_model(build_with_sidebar_undo(labels.0, labels.1, labels.2), cx);
}

// Replacing GPUI menus replaces their native NSMenu objects as well. Reapply
// responder bindings and appearance checks at every installation boundary.
fn install_menu_model(mut menus: Vec<Menu>, cx: &mut App) {
    fn label(menus: &mut [Menu], enabled: bool) {
        for menu in menus { for item in &mut menu.items {
            match item {
                MenuItem::Submenu(menu) => label(std::slice::from_mut(menu), enabled),
                MenuItem::Action { name, .. } if name.as_ref() == "Toggle Developer Mode for this site" => {
                    *name = if enabled { "Turn off Developer Mode for this site" } else { "Turn on Developer Mode for this site" }.into();
                }
                _ => {}
            }
        }}
    }
    label(&mut menus, cx.try_global::<DeveloperModeMenu>().is_some_and(|mode|mode.0));
    if cx.try_global::<PinnedMenu>().is_some_and(|state| state.0) {
        for menu in &mut menus {
            for item in &mut menu.items {
                if let MenuItem::Action { name, .. } = item {
                    if name.as_ref() == "Pin Tab" { *name = "Unpin Tab".into(); }
                }
            }
        }
    }
    cx.set_menus(menus);
    native::configure_standard_edit_menu();
    native::set_menu_minimum_width("Tabs", 256.);
    sync_native_appearance(cx);
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
                MenuItem::action("Capture…", window::CapturePage),
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
            ].into_iter().chain({
                let mut items = Vec::new();
                crate::standard_edit::append_submenus(&mut items);
                items
            }).collect(),
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
                MenuItem::action("New Tab…", window::OpenCommandBar),
                MenuItem::action("Pin Tab", window::TogglePin),
                MenuItem::action("New Folder…", window::NewFolder),
                MenuItem::separator(),
                MenuItem::action("Next Tab", window::NextTab),
                MenuItem::action("Previous Tab", window::PreviousTab),
                MenuItem::action("Reveal Tab in Sidebar", window::RevealTab),
                MenuItem::separator(),
                MenuItem::action("Clear Today", window::ClearToday),
                MenuItem::action("Reset all tabs in this Space", ResetSpace),
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
            items: vec![MenuItem::action("Essential Keyboard Shortcuts", crate::essential_shortcuts::EssentialShortcuts), MenuItem::separator(), MenuItem::Submenu(Menu { name: "Troubleshooting".into(), items: vec![MenuItem::action("Open Task Manager", crate::task_manager::OpenTaskManager), MenuItem::separator(), MenuItem::action("Reveal Echo Data", RevealData), MenuItem::action("Copy Echo Info", CopyBrowserInfo)] })],
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
    items.push(MenuItem::action("Toggle Developer Mode for this site",window::ToggleDeveloperMode));
    items
}
