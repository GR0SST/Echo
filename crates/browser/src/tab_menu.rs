//! Sidebar context menus target the clicked item without activating it.
//! They are drawn like the reference's native menus: translucent, with
//! separators, section headers, checkmarks, shortcuts and submenus.
use crate::window::{BrowserWindow, Overlay, OverlayKind, Target};
use browser_core::{SpaceId, TabId, folders::FolderId, splits::{Axis, Placement}};
use gpui::{
    actions, canvas, div, hsla, point, prelude::*, px, AnyElement, App, Bounds, Context, FocusHandle,
    KeyBinding, Pixels, Point, SharedString, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
};
use std::{cell::Cell, rc::Rc};

const CONTEXT: &str = "TabContextMenu";
const ROW_HEIGHT: f32 = 24.0;
const SEPARATOR_HEIGHT: f32 = 11.0;
const PADDING: f32 = 5.0;
/// Text starts here; a checkmark column moves it right.
const TEXT_INSET: f32 = 16.0;
const CHECK_INSET: f32 = 31.0;
/// Native menus overlap their parent frame without covering its row text.
const SUBMENU_OVERLAP: f32 = 5.0;
const MIN_WIDTH: f32 = 180.0;
actions!(tab_menu, [Dismiss, Next, Previous, Confirm, OpenSubmenu, CloseSubmenu]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("right", OpenSubmenu, Some(CONTEXT)),
        KeyBinding::new("left", CloseSubmenu, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuTarget { Tab(TabId), Folder(FolderId), Space(SpaceId), Address, Download(browser_core::downloads::CompletedDownloadId, crate::library_downloads_host::DownloadsOwner) }

pub(crate) struct TabMenu {
    pub target: MenuTarget,
    /// Highlighted row of the top level.
    pub selected: Option<usize>,
    /// Top-level row whose submenu is open, and the highlighted row in it.
    pub submenu: Option<usize>,
    pub sub_selected: Option<usize>,
    max_height: Pixels,
    viewport: gpui::Size<Pixels>,
    width: Pixels,
    scroll: gpui::ScrollHandle,
    pane: bool,
    backdrop: [Rc<Cell<bool>>; 2],
    position: Point<Pixels>,
    focus: FocusHandle,
    pub overlay: Option<WindowHandle<Overlay>>,
    /// Where the top level was painted, to place its submenu.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

#[derive(Clone, Copy)]
pub(crate) enum Command { ToggleToolbar, CustomizePage, NewFolderWithTab, MoveToFolder, FolderIntoSpace, FolderDuplicate, FolderRename, FolderNew, FolderMove, FolderIcon, FolderCollapse, FolderDelete, FolderCopy, FolderCopyMarkdown, Icon, ResetCustomization, Duplicate, Copy, CopyMarkdown, Pin, Favorite, Rename, Reset, Replace, Mute, Close,
    Download(crate::download_files::FileAction),
    SpaceTheme, SpaceRename, SpaceIcon, SpaceNewFolder, SpaceProfile,
    Add(Placement), Axis(Axis), SeparateAll, SeparatePane, Move(isize), ResetName, ClosePane, ArchiveRelative(browser_core::bulk_archive::Relative) }

impl Command {
    fn selector(self) -> &'static str {
        match self {
            Self::ToggleToolbar => "toggle-toolbar",
            Self::Download(action) => action.selector(),
            Self::CustomizePage => "customize-page",
            Self::SpaceTheme => "edit-theme-color",
            Self::SpaceProfile => "set-profile", Self::SpaceRename => "space-rename", Self::SpaceIcon => "space-icon", Self::SpaceNewFolder => "space-new-folder",
            Self::FolderDuplicate => "folder-duplicate",
            Self::FolderIntoSpace => "folder-into-space",
            Self::NewFolderWithTab => "new-folder-with-tab", Self::MoveToFolder => "move-to-folder",
            Self::FolderRename => "folder-rename", Self::FolderNew => "folder-new", Self::FolderMove => "folder-move", Self::FolderIcon => "folder-icon", Self::FolderCollapse => "folder-collapse", Self::FolderDelete => "folder-delete", Self::FolderCopy => "folder-copy", Self::FolderCopyMarkdown => "folder-copy-markdown",
            Self::Icon => "icon", Self::ResetCustomization => "reset-customization", Self::Duplicate => "duplicate", Self::Copy => "copy", Self::CopyMarkdown => "copy-markdown", Self::Pin => "pin", Self::Favorite => "favorite",
            Self::Rename => "rename", Self::Reset => "reset", Self::Replace => "replace", Self::Mute => "mute", Self::Close => "close",
            Self::ArchiveRelative(browser_core::bulk_archive::Relative::Other) => "archive-other",
            Self::ArchiveRelative(browser_core::bulk_archive::Relative::Above) => "archive-above",
            Self::ArchiveRelative(browser_core::bulk_archive::Relative::Below) => "archive-below",
            Self::Add(Placement::Left) => "add-left", Self::Add(Placement::Right) => "add-right",
            Self::Add(Placement::Above) => "add-top", Self::Add(Placement::Below) => "add-bottom",
            Self::Axis(_) => "orientation", Self::SeparateAll => "separate-all", Self::SeparatePane => "separate-pane",
            Self::Move(delta) => if delta < 0 { "move-backward" } else { "move-forward" }, Self::ResetName => "reset-name", Self::ClosePane => "close-pane",
        }
    }
}

#[derive(Clone)]
pub(crate) enum Entry { Item(Item), Separator, Header(&'static str) }

#[derive(Clone)]
pub(crate) struct Item {
    pub label: SharedString,
    /// `None` for rows kept for parity whose action is not built yet.
    pub command: Option<Command>,
    pub shortcut: Option<&'static str>,
    pub checked: bool,
    pub enabled: bool,
    /// A tab whose favicon leads the row.
    pub icon: Option<TabId>,
    /// The split pane the command acts on, instead of the menu's target.
    pub pane: Option<TabId>,
    pub submenu: Vec<Entry>,
    submenu_width: Option<f32>,
}

pub(crate) fn item(label: impl Into<SharedString>, command: Command) -> Item {
    Item { label: label.into(), command: Some(command), shortcut: None, checked: false, enabled: true, icon: None, pane: None, submenu: Vec::new(), submenu_width: None }
}

/// A row the reference has whose action is not built yet.
pub(crate) fn pending(label: impl Into<SharedString>) -> Item {
    Item { command: None, ..item(label, Command::Copy) }
}

impl Item {
    pub(crate) fn shortcut(mut self, shortcut: &'static str) -> Self { self.shortcut = Some(shortcut); self }
    pub(crate) fn checked(mut self, checked: bool) -> Self { self.checked = checked; self }
    pub(crate) fn enabled(mut self, enabled: bool) -> Self { self.enabled = enabled; self }
    pub(crate) fn icon(mut self, tab: TabId) -> Self { self.icon = Some(tab); self }
    pub(crate) fn pane(mut self, tab: TabId) -> Self { self.pane = Some(tab); self }
    pub(crate) fn submenu(mut self, entries: Vec<Entry>) -> Self { self.submenu = entries; self }
    fn submenu_width(mut self, width: f32) -> Self { self.submenu_width = Some(width); self }
    fn selector(&self) -> String {
        match (self.command, self.submenu.is_empty()) {
            (Some(command), true) => format!("tab-menu-{}", command.selector()),
            _ => format!("tab-menu-{}", slug(&self.label)),
        }
    }
}

impl From<Item> for Entry { fn from(item: Item) -> Self { Entry::Item(item) } }

fn slug(label: &str) -> String {
    let mut slug = String::new();
    for c in label.chars() {
        if c.is_ascii_alphanumeric() { slug.push(c.to_ascii_lowercase()); }
        else if !slug.ends_with('-') && !slug.is_empty() { slug.push('-'); }
    }
    slug.trim_end_matches('-').to_string()
}

fn selectable(entry: &Entry) -> bool { matches!(entry, Entry::Item(item) if item.enabled) }

fn entry_height(entry: &Entry) -> f32 { if matches!(entry, Entry::Separator) { SEPARATOR_HEIGHT } else { ROW_HEIGHT } }

fn entries_height(entries: &[Entry]) -> f32 { PADDING * 2. + entries.iter().map(entry_height).sum::<f32>() }

/// Removes separators at the ends and doubled ones left by conditional rows.
pub(crate) fn tidy(entries: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for entry in entries {
        if matches!(entry, Entry::Separator) && out.last().is_none_or(|last| matches!(last, Entry::Separator)) { continue; }
        out.push(entry);
    }
    while out.last().is_some_and(|last| matches!(last, Entry::Separator)) { out.pop(); }
    out
}

/// Moves `from` by `delta` over selectable rows, wrapping.
fn step(entries: &[Entry], from: Option<usize>, delta: isize) -> Option<usize> {
    let count = entries.len() as isize;
    if count == 0 { return None; }
    let mut index = from.map_or(if delta > 0 { -1 } else { count }, |index| index as isize);
    for _ in 0..count {
        index = (index + delta).rem_euclid(count);
        if selectable(&entries[index as usize]) { return Some(index as usize); }
    }
    None
}

/// Shortens long page titles the way the reference's menus do.
pub(crate) fn short_title(title: &str) -> String {
    const MAX: usize = 30;
    if title.chars().count() <= MAX { title.to_string() } else { format!("{}…", title.chars().take(MAX).collect::<String>().trim_end()) }
}

impl BrowserWindow {
    pub(crate) fn open_tab_menu(&mut self, tab: TabId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_sidebar_menu(MenuTarget::Tab(tab), position, window, cx);
    }

    pub(crate) fn open_folder_menu(&mut self, folder: FolderId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_sidebar_menu(MenuTarget::Folder(folder), position, window, cx);
    }

    pub(crate) fn menu_target_exists(&self, target: MenuTarget) -> bool {
        match target {
            MenuTarget::Tab(tab) => self.browser.active_user_profile().favorites.iter().chain(&self.browser.space().pinned).chain(&self.browser.space().today).any(|entry| entry.id == tab),
            MenuTarget::Folder(id) => self.browser.folder_space(id) == Some(self.browser.space().id),
            MenuTarget::Space(id) => self.browser.spaces.iter().any(|space| space.id == id),
            MenuTarget::Address => self.browser.active_tab().is_some(),
            MenuTarget::Download(id, owner) => self.download_menu_record(id,owner).is_some(),
        }
    }

    pub(crate) fn open_sidebar_menu(&mut self, target: MenuTarget, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_target_exists(target) { return; }
        let position = if matches!(target,MenuTarget::Download(..)) { point(position.x,position.y-px(5.)) } else { position };
        self.close_tab_menu(cx);
        self.close_user_profile_dialog(cx);
        self.close_command_bar(cx);
        let focus = cx.focus_handle();
        self.tab_menu = Some(TabMenu { target, selected: matches!(target,MenuTarget::Download(..)).then_some(0), submenu: None, sub_selected: None, max_height: (window.bounds().size.height - px(16.)).max(px(32.)), viewport: window.bounds().size, width: px(MIN_WIDTH), scroll: gpui::ScrollHandle::new(), pane: false, backdrop: Default::default(), position, focus: focus.clone(), overlay: None, bounds: Default::default() });
        let width = self.menu_level_width(&self.menu_entries(), false, window);
        if let Some(menu) = &mut self.tab_menu { menu.width = width; }
        let origin = window.bounds().origin;
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.notify();
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.tab_menu.as_ref().is_some_and(|menu| menu.focus == focus)).unwrap_or(false) { return; }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            let _ = browser.update(cx, |this, _| {
                let height = px(entries_height(&this.menu_entries())).min((bounds.size.height - px(16.)).max(px(32.)));
                if let Some(menu) = &mut this.tab_menu {
                    menu.max_height = (bounds.size.height - px(16.)).max(px(32.));
                    menu.viewport = bounds.size;
                    let at = position + point(px(1.), px(-6.)) + (origin - bounds.origin);
                    menu.position = point(at.x.clamp(px(8.), (bounds.size.width - menu.width - px(8.)).max(px(8.))), at.y.clamp(px(8.), (bounds.size.height - height - px(8.)).max(px(8.))));
                }
            });
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                focus.focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) { crate::native::attach_overlay(parent, child, false); }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::TabMenu, cx))
            }) {
                let _ = browser.update(cx, |this, _| { if let Some(menu) = &mut this.tab_menu { menu.overlay = Some(overlay); } });
            }
        });
    }

    pub(crate) fn open_pane_menu(&mut self, tab: TabId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.browser.split_for(tab).is_none() { return; }
        self.open_tab_menu(tab, position, window, cx);
        if let Some(menu) = &mut self.tab_menu { menu.pane = true; }
        let width = self.menu_level_width(&self.menu_entries(), false, window);
        if let Some(menu) = &mut self.tab_menu { menu.width = width; }
        cx.notify();
    }

    pub(crate) fn close_tab_menu(&mut self, cx: &mut Context<Self>) {
        self.close_tab_menu_with_focus(crate::window::PopupFocus::Restore, cx);
    }

    pub(crate) fn close_tab_menu_with_focus(&mut self, focus: crate::window::PopupFocus, cx: &mut Context<Self>) {
        let Some(menu) = self.tab_menu.take() else { return; };
        self.close_popup_overlay(menu.overlay, focus, false, cx);
        cx.notify();
    }


    pub(crate) fn menu_entries(&self) -> Vec<Entry> {
        let Some(menu) = &self.tab_menu else { return Vec::new(); };
        tidy(match menu.target {
            MenuTarget::Space(id) => self.space_menu_commands(id),
            MenuTarget::Download(id, owner) => self.download_menu_commands(id,owner),
            MenuTarget::Folder(id) => self.folder_menu_commands(id),
            MenuTarget::Address => vec![
                item("Copy URL", Command::Copy).into(),
                pending("Share").submenu(vec![item("Copy URL as Markdown", Command::CopyMarkdown).into()]).into(),
                pending("Capture…").into(),
                pending("Capture Full Page").into(),
                Entry::Separator,
                item(if self.toolbar_shown() { "Hide Toolbar" } else { "Show as Toolbar" }, Command::ToggleToolbar).into(),
                pending("Disable Shared Quotes").into(),
            ],
            MenuTarget::Tab(id) => self.tab_entries(id, menu.pane),
        })
    }

    fn move_to_entries(&self) -> Vec<Entry> {
        vec![
            item("New Folder…", Command::NewFolderWithTab).enabled(!self.browser.is_blank()).into(),
            item("Folder…", Command::MoveToFolder).enabled(!self.browser.is_blank()).into(),
        ]
    }

    fn tab_entries(&self, id: TabId, pane_menu: bool) -> Vec<Entry> {
        let Some(tab) = self.browser.active_user_profile().favorites.iter().chain(&self.browser.space().pinned).chain(&self.browser.space().today).find(|tab| tab.id == id) else { return Vec::new(); };
        let split = self.browser.split_for(tab.id);
        let internal = browser_core::omnibox::is_internal_url(&tab.url);
        let orientation = if split.is_some_and(|split| split.axis == Axis::Horizontal) {
            item("Convert to Vertical Split View", Command::Axis(Axis::Vertical))
        } else { item("Convert to Horizontal Split View", Command::Axis(Axis::Horizontal)) };
        let archive_relative = |rows: &mut Vec<Entry>| {
            for (relative, label) in [(browser_core::bulk_archive::Relative::Other, "Archive Other Tabs"), (browser_core::bulk_archive::Relative::Above, "Archive Tabs Above"), (browser_core::bulk_archive::Relative::Below, "Archive Tabs Below")] {
                if !self.browser.tabs_to_archive(tab.id, relative).is_empty() { rows.push(item(label, Command::ArchiveRelative(relative)).into()); }
            }
        };
        if pane_menu {
            let Some(split) = split else { return Vec::new(); };
            let horizontal = split.axis == Axis::Horizontal;
            let index = split.panes.iter().position(|id| *id == tab.id).unwrap();
            let can_add = split.panes.len() < browser_core::splits::MAX_SPLIT_PANES;
            return vec![
                item(if horizontal { "Move Left" } else { "Move Up" }, Command::Move(-1)).enabled(index > 0).into(),
                item(if horizontal { "Move Right" } else { "Move Down" }, Command::Move(1)).enabled(index + 1 < split.panes.len()).into(),
                Entry::Separator,
                item(if horizontal { "Add Right Split" } else { "Add Top Split" }, Command::Add(if horizontal { Placement::Right } else { Placement::Above })).enabled(can_add).into(),
                item(if horizontal { "Add Left Split" } else { "Add Bottom Split" }, Command::Add(if horizontal { Placement::Left } else { Placement::Below })).enabled(can_add).into(),
                Entry::Separator,
                item("Separate Tab from Split", Command::SeparatePane).into(),
                orientation.into(),
                Entry::Separator,
                pending("Share Split View…").into(),
            ];
        }
        if let Some(split) = split {
            let mut rows: Vec<Entry> = vec![pending("Share Split View…").into(), Entry::Separator,
                item("Separate All Tabs", Command::SeparateAll).into(), item("Add Tab…", Command::Add(Placement::Right)).into(), orientation.into(),
                Entry::Separator, Entry::Header("Tab Options")];
            for pane in split.panes.iter().filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == *id)) {
                let pane_internal = browser_core::omnibox::is_internal_url(&pane.url);
                rows.push(Item { label: short_title(&pane.title).into(), ..pending("") }.icon(pane.id).submenu(vec![
                    item("Separate from Split View", Command::SeparatePane).pane(pane.id).into(),
                    Entry::Separator,
                    pending("Share").submenu(vec![item("Copy Link as Markdown", Command::CopyMarkdown).pane(pane.id).into()]).into(),
                    item(if pane.muted { "Unmute" } else { "Mute" }, Command::Mute).pane(pane.id).enabled(!pane_internal && self.show_tab_audio(pane)).into(),
                    item("Copy Link", Command::Copy).shortcut("⇧⌘C").pane(pane.id).into(),
                    item("Copy link as Markdown", Command::CopyMarkdown).pane(pane.id).into(),
                    Entry::Separator,
                    item("Archive Item", Command::ClosePane).pane(pane.id).into(),
                ]).into());
            }
            rows.extend([Entry::Separator, item("Change Icon…", Command::Icon).into(), item("Rename…", Command::Rename).into()]);
            if self.browser.item_has_custom_name(tab.id) { rows.push(item("Reset Name", Command::ResetName).into()); }
            rows.extend([Entry::Separator, item("Duplicate", Command::Duplicate).shortcut("⌥D").into(),
                pending("Move to").submenu(self.move_to_entries()).into(), Entry::Separator,
                item(if tab.pinned_url.is_some() { "Close Tab" } else { "Archive Item" }, Command::Close).into()]);
            archive_relative(&mut rows);
            return rows;
        }
        let favorite = self.browser.is_favorite(tab.id);
        let pinned = tab.pinned_url.is_some();
        let mut rows: Vec<Entry> = vec![
            Item { shortcut: (!favorite).then_some("⇧⌘C"), ..item("Copy Link", Command::Copy) }.into(),
            pending("Share").submenu(vec![item("Copy Link as Markdown", Command::CopyMarkdown).into()]).into(),
            Entry::Separator,
        ];
        if favorite { rows.extend([pending("Show Unread Badge").checked(true).into(), Entry::Separator]); }
        rows.extend([item("Change Icon…", Command::Icon).into(), item("Rename…", Command::Rename).into()]);
        if self.browser.item_has_custom_name(tab.id) || self.browser.item_icon(tab.id).is_some() { rows.push(item("Reset Name and Icon", Command::ResetCustomization).into()); }
        rows.push(item(if tab.muted { "Unmute" } else { "Mute" }, Command::Mute).enabled(!internal && self.show_tab_audio(tab)).into());
        if !favorite { rows.push(item("Customize Page…",Command::CustomizePage).into()); }
        if pinned {
            let moved = tab.pinned_url.as_ref().is_some_and(|url| *url != tab.url);
            rows.push(pending("Edit Pinned Page").submenu(vec![
                item("Replace Pinned URL with Current", Command::Replace).enabled(moved).into(),
                pending("Edit…").into(),
            ]).submenu_width(232.).into());
        }
        rows.push(Entry::Separator);
        if !internal { rows.push(item(if self.browser.space().active_tab == Some(id) { "Add Split View" } else { "Open in Split View" }, Command::Add(Placement::Right)).into()); }
        rows.extend([item("Duplicate", Command::Duplicate).shortcut("⌥D").into(), pending("Move to").submenu(self.move_to_entries()).into(), Entry::Separator]);
        if favorite {
            rows.push(item("Remove Favorite", Command::Favorite).into());
        } else if pinned {
            rows.push(item("Remove Pin", Command::Pin).into());
        } else {
            rows.push(item("Archive Tab", Command::Close).into());
            let below = browser_core::bulk_archive::Relative::Below;
            rows.push(item("Archive Tabs Below", Command::ArchiveRelative(below))
                .enabled(!self.browser.tabs_to_archive(tab.id, below).is_empty()).into());
        }
        rows
    }

    fn current_level(&self) -> (Vec<Entry>, bool) {
        let entries = self.menu_entries();
        match self.tab_menu.as_ref().and_then(|menu| menu.submenu.filter(|_| menu.sub_selected.is_some())) {
            Some(index) => (match entries.get(index) { Some(Entry::Item(item)) => item.submenu.clone(), _ => Vec::new() }, true),
            None => (entries, false),
        }
    }

    fn select_tab_menu(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (entries, nested) = self.current_level();
        if let Some(menu) = &mut self.tab_menu {
            if nested { menu.sub_selected = step(&entries, menu.sub_selected, delta); }
            else {
                menu.selected = step(&entries, menu.selected, delta);
                menu.submenu = None;
                if let Some(index) = menu.selected { menu.scroll.scroll_to_item(index); }
            }
        }
        cx.notify();
    }

    fn open_tab_submenu(&mut self, cx: &mut Context<Self>) {
        let entries = self.menu_entries();
        if let Some(menu) = &mut self.tab_menu {
            if let Some(index) = menu.selected.filter(|index| matches!(entries.get(*index), Some(Entry::Item(item)) if !item.submenu.is_empty())) {
                let Some(Entry::Item(item)) = entries.get(index) else { return; };
                menu.submenu = Some(index);
                menu.sub_selected = step(&item.submenu, None, 1);
            }
        }
        cx.notify();
    }

    fn close_tab_submenu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.tab_menu { menu.submenu = None; menu.sub_selected = None; }
        cx.notify();
    }

    fn confirm_selected_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (entries, nested) = self.current_level();
        let selected = self.tab_menu.as_ref().and_then(|menu| if nested { menu.sub_selected } else { menu.selected });
        match selected.and_then(|index| entries.get(index)) {
            Some(Entry::Item(item)) if !item.submenu.is_empty() => self.open_tab_submenu(cx),
            Some(Entry::Item(item)) => self.confirm_menu_item(item.clone(), window, cx),
            _ => {}
        }
    }

    fn confirm_menu_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        if !item.enabled { return; }
        if !item.submenu.is_empty() { return; }
        let Some((target, pane_menu)) = self.tab_menu.as_ref().map(|menu| (menu.target, menu.pane)) else { return; };
        self.close_tab_menu(cx);
        // Items without a command are shown for parity but not built yet.
        let Some(command) = item.command else { return; };
        if self.browser.is_blank() && matches!(command, Command::Pin | Command::Favorite | Command::NewFolderWithTab | Command::MoveToFolder) { return; }
        let pane_menu = pane_menu || item.pane.is_some();
        let id = match target {
            MenuTarget::Download(id, owner) => {
                if let Command::Download(action) = command { self.confirm_download_menu(id,owner,action,window,cx); }
                return;
            }
            MenuTarget::Space(id) => { self.confirm_space_menu(id, command, window, cx); return; }
            MenuTarget::Folder(id) => { self.confirm_folder_menu(id, command, window, cx); return; }
            MenuTarget::Address => {
                match command { Command::ToggleToolbar => self.toggle_toolbar(cx), Command::Copy => self.copy_url(false, cx), Command::CopyMarkdown => self.copy_url(true, cx), _ => {} }
                return;
            }
            MenuTarget::Tab(id) => item.pane.unwrap_or(id),
        };
        match command {
            Command::ToggleToolbar => self.toggle_toolbar(cx),
            Command::CustomizePage => { self.edit_browser(|browser|browser.activate_tab(id,std::time::SystemTime::now()));self.customize_page(cx); }
            Command::NewFolderWithTab => self.open_folder_creator(None, Some(id), window, cx),
            Command::MoveToFolder => { self.open_command_bar(Target::MoveToFolder(id), "", window, cx); self.edit_folder_destination(Target::MoveToFolder(id), cx); }
            Command::Add(mut placement) => {
                if !self.can_add_split(id) { self.show_toast("Cannot Add New Pane".into(), cx); }
                else {
                    if !pane_menu && self.browser.split_for(id).is_some_and(|split| split.axis == Axis::Vertical) { placement = Placement::Below; }
                    self.open_command_bar(Target::NewTab, "", window, cx);
                    self.begin_split(id, placement, cx);
                }
            }
            Command::Axis(axis) => { self.edit_browser(|browser| browser.set_split_axis(id, axis)); }
            Command::SeparateAll => { if !self.edit_browser(|browser| browser.separate_split(id)) { self.show_toast("You can have up to 12 Favorites".into(), cx); } }
            Command::SeparatePane => {
                if self.edit_browser(|browser| browser.separate_split_pane(id)) { self.edit_browser(|browser| browser.activate_tab(id, std::time::SystemTime::now())); }
                else { self.show_toast("You can have up to 12 Favorites".into(), cx); }
            }
            Command::Move(delta) => { self.edit_browser(|browser| browser.move_split_pane(id, delta)); }
            Command::ResetName => { self.edit_browser(|browser| browser.reset_tab_name(id)); }
            Command::ClosePane => self.close_split_pane(id, cx),
            Command::Icon => self.open_command_bar(Target::Icon(crate::icons::IconTarget::Tab(id)), &self.browser.item_icon(id).unwrap_or_default().to_string(), window, cx),
            Command::ResetCustomization => { self.edit_browser(|browser| browser.reset_item_customization(id)); }
            Command::ArchiveRelative(relative) => self.archive_relative(id, relative, cx),
            Command::Duplicate => self.duplicate_tab(id, cx),
            Command::Copy => self.copy_tab_url(id, false, cx),
            Command::CopyMarkdown => self.copy_tab_url(id, true, cx),
            Command::Pin => self.remove_pin(id, cx),
            Command::Favorite => {
                let active = self.browser.space().active_tab;
                if self.browser.is_favorite(id) { self.edit_browser(|browser| browser.unfavorite_tab(id)); }
                else if !self.edit_browser(|browser| browser.favorite_tab(id)) { self.show_toast("You can have up to 12 Favorites".into(), cx); }
                self.edit_browser(|browser| browser.spaces[browser.active_space].active_tab = active);
            }
            Command::Rename => {
                self.open_tab_rename(id, window, cx);
            }
            Command::Reset => {
                if self.edit_browser(|browser| browser.reset_pinned_tab(id)) {
                    if let Some(pages) = &self.pages { let mut pages = pages.borrow_mut(); for id in self.browser.related_tabs(id) { pages.discard(id); } }
                }
            }
            Command::Replace => { self.edit_browser(|browser| browser.replace_pinned_url(id)); }
            Command::Mute => {
                let ids = if pane_menu { vec![id] } else { self.browser.related_tabs(id) };
                let muted = !ids.iter().all(|id| self.browser.all_tabs().any(|tab| tab.id == *id && tab.muted));
                for id in ids { self.set_tab_muted(id, muted, cx); }
            }
            Command::Close => self.close_tab(id, cx),
            _ => {},
        }
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }


    pub(crate) fn render_tab_menu(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.tab_menu else { return div().into_any_element(); };
        let entries = self.menu_entries();
        let top = self.render_menu_level(&entries, menu.selected, menu.submenu, false, window, cx)
            .absolute().left(menu.position.x).top(menu.position.y).max_h(menu.max_height)
            .overflow_y_scroll().track_scroll(&menu.scroll);
        let bounds = menu.bounds.clone();
        let submenu = menu.submenu.and_then(|index| match entries.get(index) { Some(Entry::Item(item)) if !item.submenu.is_empty() => Some((index, item.submenu.clone())), _ => None });
        let sub = submenu.as_ref().and_then(|(index, sub_entries)| {
            let parent = menu.bounds.get()?;
            let row_top: f32 = PADDING + entries[..*index].iter().map(entry_height).sum::<f32>();
            let width = self.menu_level_width(sub_entries, true, window);
            let right = parent.right() - px(SUBMENU_OVERLAP);
            let left = if right + width > menu.viewport.width - px(8.) { parent.left() - width + px(SUBMENU_OVERLAP) } else { right };
            let height = px(entries_height(sub_entries));
            let top = parent.top() + px(row_top - PADDING) + menu.scroll.offset().y;
            Some(self.render_menu_level(sub_entries, menu.sub_selected, None, true, window, cx)
                .absolute().left(left.max(px(8.))).top(top.clamp(px(8.), (menu.viewport.height - height - px(8.)).max(px(8.)))))
        });
        let has_sub = sub.is_some();
        div().id("tab-menu-backdrop").debug_selector(|| "tab-menu-backdrop".into()).size_full().key_context(CONTEXT).track_focus(&menu.focus)
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, _, _, cx| this.close_tab_menu(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_tab_menu(cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.select_tab_menu(1, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.select_tab_menu(-1, cx)))
            .on_action(cx.listener(|this, _: &OpenSubmenu, _, cx| this.open_tab_submenu(cx)))
            .on_action(cx.listener(|this, _: &CloseSubmenu, _, cx| this.close_tab_submenu(cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| this.confirm_selected_tab_menu(window, cx)))
            .on_click(cx.listener(|this, _, _, cx| this.close_tab_menu(cx)))
            .child(top.on_click(|_, _, cx| cx.stop_propagation())
                .child(canvas(|_, _, _| (), move |painted, _, window, _| {
                    bounds.set(Some(painted));
                    if !has_sub { crate::panel::backdrop(window, "submenu", None, 0., crate::native::Material::Menu); }
                }).absolute().inset_0()))
            .children(sub.map(|sub| sub.on_click(|_, _, cx| cx.stop_propagation())))
            .into_any_element()
    }

    fn menu_level_width(&self, entries: &[Entry], nested: bool, window: &Window) -> Pixels {
        if let Some(width) = nested.then(|| self.tab_menu.as_ref()?.submenu
            .and_then(|index| match self.menu_entries().get(index) {
                Some(Entry::Item(item)) => item.submenu_width,
                _ => None,
            })).flatten() { return px(width); }
        let minimum = if nested { 0. } else {
            self.tab_menu.as_ref().and_then(|menu| match menu.target {
                MenuTarget::Tab(id) if !menu.pane && self.browser.split_for(id).is_none() && !self.browser.is_favorite(id) =>
                    self.browser.all_tabs().find(|tab| tab.id == id).map(|tab| if tab.pinned_url.is_some() { 200. } else { 208. }),
                MenuTarget::Download(..) => Some(120.),
                MenuTarget::Address => Some(204.),
                _ => None,
            }).unwrap_or(MIN_WIDTH)
        };
        let checks = entries.iter().any(|entry| matches!(entry, Entry::Item(item) if item.checked));
        let inset = if checks { CHECK_INSET } else { TEXT_INSET };
        let text_width = |label: &str| f32::from(window.text_system().shape_line(label.to_owned().into(), px(13.),
            &[gpui::TextRun { len: label.len(), font: gpui::font(".SystemUIFont"), color: gpui::white(),
                background_color: None, underline: None, strikethrough: None }], None).width);
        let pane_controls = !nested && self.tab_menu.as_ref().is_some_and(|menu| menu.pane);
        px(entries.iter().filter_map(|entry| match entry {
            Entry::Item(item) => Some(inset + 17. + text_width(&item.label)
                + if item.icon.is_some() { 22. } else { 0. }
                + if pane_controls { 22. } else { 0. }
                + item.shortcut.map_or(0., |shortcut| 30. + text_width(shortcut))
                + if item.submenu.is_empty() { 0. } else { 38. }),
            Entry::Header(label) => Some(inset + 17. + text_width(label)),
            Entry::Separator => None,
        }).fold(minimum, f32::max).ceil())
    }

    /// One menu panel: rows, separators and headers; `nested` is a submenu.
    fn render_menu_level(&self, entries: &[Entry], selected: Option<usize>, open: Option<usize>, nested: bool, window: &Window, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let pane_controls = !nested && self.tab_menu.as_ref().is_some_and(|menu| menu.pane);
        let horizontal = self.browser.active_split().is_some_and(|split| split.axis == Axis::Horizontal);
        let checks = entries.iter().any(|entry| matches!(entry, Entry::Item(item) if item.checked));
        let inset = if checks { CHECK_INSET } else { TEXT_INSET };
        let rows = entries.iter().enumerate().map(|(index, entry)| match entry {
            Entry::Separator => div().h(px(SEPARATOR_HEIGHT)).flex().items_center().pl(px(inset)).pr(px(17.))
                .child(div().w_full().h(px(1.)).bg(hsla(0., 0., 1., 0.14))).into_any_element(),
            Entry::Header(label) => div().h(px(ROW_HEIGHT)).flex().items_center().pl(px(inset)).pr(px(17.))
                .text_color(hsla(0., 0., 1., 0.35)).child(*label).into_any_element(),
            Entry::Item(item) => {
                let highlighted = selected == Some(index) || open == Some(index);
                let enabled = item.enabled;
                let has_submenu = !item.submenu.is_empty();
                let selector = item.selector();
                let item_for_click = item.clone();
                let id = SharedString::from(format!("{}-row-{index}", if nested { "submenu" } else { "menu" }));
                div().id(id).h(px(ROW_HEIGHT)).mx(px(PADDING)).rounded(px(6.)).flex().items_center()
                    .pl(px(inset - PADDING)).pr(px(12.)).gap(px(6.))
                    .when(highlighted && enabled, |row| row.bg(hsla(211. / 360., 1., 0.5, 1.)).text_color(gpui::white()))
                    .when(!enabled, |row| row.text_color(hsla(0., 0., 1., 0.3)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        let Some(menu) = &mut this.tab_menu else { return; };
                        if nested {
                            if *hovered { menu.sub_selected = enabled.then_some(index); }
                            else if menu.sub_selected == Some(index) { menu.sub_selected = None; }
                        } else if *hovered {
                            menu.selected = enabled.then_some(index);
                            menu.submenu = (has_submenu && enabled).then_some(index);
                            menu.sub_selected = None;
                        } else if menu.selected == Some(index) && menu.submenu != Some(index) {
                            menu.selected = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if has_submenu {
                            if let Some(menu) = &mut this.tab_menu { menu.selected = Some(index); menu.submenu = Some(index); }
                            cx.notify();
                        } else { this.confirm_menu_item(item_for_click.clone(), window, cx); }
                    }))
                    .when(checks, |row| row.child(div().absolute().left(px(PADDING + 4.)).top(px(3.))
                        .when(item.checked, |slot| slot.child(crate::icons::check(0., 0.8)))))
                    .when(pane_controls, |row| row.child(pane_control_icon(item.command, horizontal)))
                    .when_some(item.icon.and_then(|id| self.browser.all_tabs().find(|tab| tab.id == id)), |row, tab| row.child(div().size(px(16.)).flex_none().flex().items_center().justify_center()
                        .child(self.render_favicon(tab, self.accent()))))
                    .child(div().debug_selector(move || selector.clone()).flex_1().whitespace_nowrap().child(item.label.clone()))
                    .when_some(item.shortcut, |row, shortcut| row.child(div().ml(px(24.)).whitespace_nowrap()
                        .text_color(if highlighted && enabled { hsla(0., 0., 1., 0.85) } else { hsla(0., 0., 1., 0.4) }).child(shortcut)))
                    .when(has_submenu, |row| row.child(div().ml(px(24.)).child(submenu_chevron())))
                    .relative()
                    .into_any_element()
            }
        });
        let id = if nested { "tab-context-submenu" } else { "tab-context-menu" };
        let width = self.menu_level_width(entries, nested, window);
        let backdrop = self.tab_menu.as_ref().map(|menu| menu.backdrop[usize::from(nested)].clone()).unwrap_or_default();
        crate::panel::menu_shell(id, if nested { "submenu" } else { "menu" }, self.accent(), backdrop).debug_selector(move || id.into())
            .w(width).py(px(PADDING)).flex().flex_col().children(rows)
    }
}

/// The submenu arrow: a fixed vector chevron, never a font glyph.
fn submenu_chevron() -> AnyElement {
    canvas(|_, _, _| (), |bounds, _, window, _| {
        let color = window.text_style().color;
        let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
        let mut path = gpui::PathBuilder::stroke(px(1.5));
        path.move_to(at(2.5, 2.));
        path.line_to(at(6.5, 6.));
        path.line_to(at(2.5, 10.));
        if let Ok(path) = path.build() { window.paint_path(path, color); }
    }).w(px(9.)).h(px(12.)).flex_none().into_any_element()
}

/// Pane controls use the reference's small pane-and-arrow diagrams, rather
/// than font glyphs or the unrelated sidebar action icons.
fn pane_control_icon(command: Option<Command>, horizontal: bool) -> AnyElement {
    canvas(|_, _, _| (), move |bounds, _, window, _| {
        let foreground = window.text_style().color;
        let frame = hsla(0., 0., 1., 0.32);
        let draw = |points: &[(f32, f32)], color, rotate: bool, window: &mut Window| {
            let mut path = gpui::PathBuilder::stroke(px(1.3));
            for (index, &(x, y)) in points.iter().enumerate() {
                let (x, y) = if rotate { (y, x) } else { (x, y) };
                let point = bounds.origin + point(px(x), px(y));
                if index == 0 { path.move_to(point); } else { path.line_to(point); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, color); }
        };
        match command {
            Some(Command::Move(delta)) => {
                let mirrored = delta > 0;
                let map = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| (if mirrored { 16. - x } else { x }, y)).collect::<Vec<_>>();
                draw(&map(&[(8., 5.), (8., 2.), (14., 2.), (14., 14.), (8., 14.), (8., 11.)]), frame, !horizontal, window);
                draw(&map(&[(11., 8.), (3., 8.), (6.5, 4.5)]), foreground, !horizontal, window);
                draw(&map(&[(3., 8.), (6.5, 11.5)]), foreground, !horizontal, window);
            }
            Some(Command::Add(placement)) => {
                let mirrored = matches!(placement, Placement::Left | Placement::Above);
                let rotate = matches!(placement, Placement::Above | Placement::Below);
                let map = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| (if mirrored { 16. - x } else { x }, y)).collect::<Vec<_>>();
                draw(&map(&[(14., 4.), (14., 2.), (2., 2.), (2., 14.), (14., 14.), (14., 12.)]), frame, rotate, window);
                draw(&map(&[(8., 2.), (8., 14.)]), frame, rotate, window);
                draw(&map(&[(11., 8.), (17., 8.)]), foreground, rotate, window);
                draw(&map(&[(14., 5.), (14., 11.)]), foreground, rotate, window);
            }
            Some(Command::SeparatePane) => {
                draw(&[(2., 4.), (2., 2.), (14., 2.), (14., 14.), (2., 14.), (2., 12.)], frame, false, window);
                draw(&[(8., 2.), (8., 14.)], frame, false, window);
                draw(&[(7., 5.), (2., 10.), (2., 5.)], hsla(210. / 360., 0.55, 0.65, 1.), false, window);
                draw(&[(2., 10.), (7., 10.)], hsla(210. / 360., 0.55, 0.65, 1.), false, window);
            }
            Some(Command::Axis(axis)) => {
                draw(&[(1., 2.), (15., 2.), (15., 14.), (1., 14.), (1., 2.)], frame, false, window);
                draw(&[(1., 8.), (15., 8.)], frame, axis == Axis::Horizontal, window);
            }
            _ => {
                draw(&[(2., 6.), (2., 15.), (14., 15.), (14., 6.)], frame, false, window);
                draw(&[(8., 12.), (8., 1.), (4.5, 4.5)], foreground, false, window);
                draw(&[(8., 1.), (11.5, 4.5)], foreground, false, window);
            }
        }
    }).size(px(16.)).flex_none().into_any_element()
}
