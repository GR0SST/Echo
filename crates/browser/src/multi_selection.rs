//! Window-local sidebar selection; modifier clicks never navigate the page.
use crate::{tab_menu::{self, item, Command, Entry}, window::BrowserWindow};
use browser_core::{SpaceId, TabId, folders::PinnedItem, splits::Placement};
use gpui::{ClipboardItem, Context, Modifiers, Window};

#[derive(Default)]
pub(crate) struct SidebarSelection {
    space: Option<SpaceId>,
    anchor: Option<TabId>,
    ids: Vec<TabId>,
    pub(crate) folder_items: Vec<TabId>,
    pub(crate) new_space_items: Vec<TabId>,
}
impl BrowserWindow {
    pub(crate) fn extension_highlight_supported(&self, id: TabId, highlighted: bool) -> bool {
        if self.browser.active_tab().is_some_and(|tab|tab.id == id) { return highlighted; }
        !highlighted || self.selectable_sidebar_items().contains(&id)
    }
    pub(crate) fn set_extension_highlight(&mut self, id: TabId, highlighted: bool, cx: &mut Context<Self>) {
        if self.browser.active_tab().is_some_and(|tab|tab.id == id) { return; }
        if self.sidebar_item_selected(id) != highlighted {
            self.select_sidebar_item(id,gpui::Modifiers {platform:true,..Default::default()},cx);
        }
    }
    fn selectable_sidebar_items(&self) -> Vec<TabId> {
        self.browser.pinned_rows(self.browser.space().id, false).into_iter()
            .filter_map(|row| self.browser.pinned_item_tab(self.browser.space().id, row.item).filter(|_| !matches!(row.item, PinnedItem::Folder(_))))
            .chain(self.browser.today_rows(self.browser.space().id).into_iter().filter_map(|row|match row {browser_core::max::TodayRow::Tab(id)=>Some(id),browser_core::max::TodayRow::Group(_)=>None})).collect()
    }
    pub(crate) fn sidebar_selection_items(&self) -> Vec<TabId> {
        if self.sidebar_selection.space != Some(self.browser.space().id) { return vec![]; }
        self.selectable_sidebar_items().into_iter().filter(|id| self.sidebar_selection.ids.contains(id)).collect()
    }
    pub(crate) fn sidebar_item_selected(&self, id: TabId) -> bool {
        self.sidebar_selection.space == Some(self.browser.space().id) && self.sidebar_selection.ids.contains(&self.browser.item_anchor(id))
    }
    /// Returns true when this click is consumed as a selection operation.
    pub(crate) fn select_sidebar_item(&mut self, id: TabId, modifiers: Modifiers, cx: &mut Context<Self>) -> bool {
        let id = self.browser.item_anchor(id);
        if !self.selectable_sidebar_items().contains(&id) { return false; }
        if self.sidebar_selection.space != Some(self.browser.space().id) {
            self.sidebar_selection = SidebarSelection { space: Some(self.browser.space().id), ..Default::default() };
        }
        if modifiers.shift {
            let order = self.selectable_sidebar_items();
            let anchor = self.sidebar_selection.anchor.or(self.browser.space().active_tab.map(|id| self.browser.item_anchor(id))).unwrap_or(id);
            let a = order.iter().position(|entry| *entry == anchor).unwrap_or(0);
            let b = order.iter().position(|entry| *entry == id).unwrap();
            self.sidebar_selection.ids = order[a.min(b)..=a.max(b)].to_vec();
        } else if modifiers.platform {
            if self.sidebar_selection.ids.is_empty() {
                if let Some(active) = self.browser.space().active_tab.map(|id| self.browser.item_anchor(id)).filter(|id| self.selectable_sidebar_items().contains(id)) { self.sidebar_selection.ids.push(active); }
            }
            if let Some(index) = self.sidebar_selection.ids.iter().position(|entry| *entry == id) { self.sidebar_selection.ids.remove(index); }
            else { self.sidebar_selection.ids.push(id); }
            if self.sidebar_selection.anchor.is_none() { self.sidebar_selection.anchor = Some(id); }
        } else {
            self.sidebar_selection.ids.clear();
            self.sidebar_selection.anchor = Some(id);
            return false;
        }
        cx.notify();
        true
    }
    pub(crate) fn selection_menu_commands(&self) -> Vec<Entry> {
        let ids = self.sidebar_selection_items();
        if ids.len() < 2 { return vec![]; }
        let muted = ids.iter().flat_map(|id| self.browser.related_tabs(*id)).all(|id| self.browser.all_tabs().find(|tab| tab.id == id).is_some_and(|tab| tab.muted));
        let today_only = ids.iter().all(|id| self.browser.space().today.iter().any(|tab| tab.id == *id));
        let alternate = today_only && self.tab_menu.as_ref().is_some_and(|menu| menu.alternate);
        let mut rows = vec![
            item("Copy Links", Command::Copy).into(),
            Entry::Separator,
            item("Change Icon…", Command::Icon).enabled(false).into(),
            item(if muted { "Unmute" } else { "Mute" }, Command::Mute).enabled(ids.iter().any(|id| self.browser.all_tabs().find(|tab| tab.id == *id).is_some_and(|tab| self.show_tab_audio(tab)))).into(),
            Entry::Separator,
            item("Open in Split View", Command::SelectionSplit).enabled(ids.len() <= browser_core::splits::MAX_SPLIT_PANES && ids.iter().all(|id| self.browser.split_for(*id).is_none())).into(),
            item("Duplicate", Command::Duplicate).shortcut("⌥D").into(),
            tab_menu::pending("Move to").submenu(self.selection_move_entries(None)).into(),
            item(format!("New Folder with {} Items", ids.len()), Command::NewFolderWithTab).enabled(!self.browser.is_blank()).into(),
            Entry::Separator,
            item(if alternate { "Archive Other Tabs" } else { "Archive Tabs" }, if alternate { Command::SelectionArchiveOthers } else { Command::Close }).into(),
        ];
        if today_only {
            // Arc changes the alternate row's title/action but retains the
            // ordinary Below row's validation (arc-multi-option.png).
            rows.push(item(if alternate { "Archive Tabs Above" } else { "Archive Tabs Below" }, if alternate { Command::SelectionArchiveAbove } else { Command::SelectionArchiveBelow }).enabled(!self.selection_tabs_below().is_empty()).into());
        }
        rows
    }
    pub(crate) fn selection_move_entries(&self, item: Option<TabId>) -> Vec<Entry> {
        let mut entries: Vec<_> = self.browser.spaces.iter()
            .map(|space| tab_menu::item(space.name.clone(), Command::SelectionMove(space.id)).space_destination(space.id).checked(space.id == self.browser.space().id).enabled(!item.is_some_and(|id| self.browser.is_favorite(id)) || space.user_profile_id == self.browser.active_user_profile_id()).into()).collect();
        entries.push(Entry::Separator);
        entries.push(tab_menu::item("New Space…", Command::SelectionNewSpace).enabled(!self.browser.is_blank() && !self.browser.is_private()).into());
        entries
    }
    fn selection_tabs_below(&self) -> Vec<TabId> {
        let ids = self.sidebar_selection_items();
        let order: Vec<_> = self.browser.space().today.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).map(|tab| tab.id).collect();
        order.iter().rposition(|id| ids.contains(id)).map_or(vec![], |last| order[last+1..].to_vec())
    }
    fn selection_tabs_above(&self) -> Vec<TabId> {
        let ids = self.sidebar_selection_items();
        let order: Vec<_> = self.browser.space().today.iter().filter(|tab| !self.browser.is_split_secondary(tab.id)).map(|tab| tab.id).collect();
        order.iter().position(|id| ids.contains(id)).map_or(vec![], |first| order[..first].to_vec())
    }
    pub(crate) fn confirm_selection_menu(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.sidebar_selection_items();
        if ids.len() < 2 { return; }
        match command {
            Command::Copy | Command::CopyMarkdown => { self.copy_sidebar_selection(matches!(command, Command::CopyMarkdown), cx); }
            Command::Share => {
                let urls = ids.iter().flat_map(|id| self.browser.related_tabs(*id)).filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == id)).map(|tab| tab.url.clone()).collect();
                self.share_context_links(urls, window, cx);
            }
            Command::Close | Command::SelectionArchiveBelow | Command::SelectionArchiveAbove | Command::SelectionArchiveOthers => {
                let targets = match command {
                    Command::SelectionArchiveBelow => self.selection_tabs_below(),
                    Command::SelectionArchiveAbove => self.selection_tabs_above(),
                    Command::SelectionArchiveOthers => self.browser.space().today.iter().filter(|tab| !self.browser.is_split_secondary(tab.id) && !ids.contains(&tab.id)).map(|tab| tab.id).collect(),
                    _ => ids,
                };
                if self.edit_browser(|browser| browser.archive_sidebar_items(&targets, std::time::SystemTime::now())) > 0 {
                    self.sidebar_selection.ids.clear();
                    self.after_tab_removal(cx);
                }
            }
            Command::Duplicate => { for id in ids { self.duplicate_tab(id, cx); } }
            Command::Mute => {
                let muted = ids.iter().flat_map(|id| self.browser.related_tabs(*id)).any(|id| self.browser.all_tabs().find(|tab| tab.id == id).is_some_and(|tab| !tab.muted));
                for id in ids.into_iter().flat_map(|id| self.browser.related_tabs(id)).collect::<Vec<_>>() { self.set_tab_muted(id, muted, cx); }
            }
            Command::SelectionSplit => {
                if ids.len() > browser_core::splits::MAX_SPLIT_PANES || ids.iter().any(|id| self.browser.split_for(*id).is_some()) { return; }
                let first = self.browser.space().active_tab.filter(|id| ids.contains(id)).unwrap_or(ids[0]);
                let peers: Vec<_> = ids.iter().copied().filter(|id| *id != first).collect();
                self.edit_browser(|browser| {
                    for id in peers.iter().rev() { let _ = browser.add_split(first, *id, Placement::Right, std::time::SystemTime::now()); }
                    if let Some(focus) = peers.last() { browser.activate_tab(*focus, std::time::SystemTime::now()); }
                });
                self.sidebar_selection.ids.clear();
            }
            Command::SelectionMove(space) => { self.move_sidebar_context_items(&ids, space, cx); }
            Command::SelectionNewSpace => { self.begin_space_creation(self.browser.space().id, cx); self.sidebar_selection.new_space_items = ids; }
            Command::NewFolderWithTab => { self.open_folder_creator(None, Some(ids[0]), window, cx); self.sidebar_selection.folder_items = ids; }
            _ => {}
        }
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }
    pub(crate) fn move_sidebar_context_items(&mut self, ids: &[TabId], space: SpaceId, _cx: &mut Context<Self>) {
        if !self.browser.spaces.iter().any(|entry| entry.id == space) { return; }
        self.edit_browser(|browser| {
            let (pins, today): (Vec<_>, Vec<_>) = ids.iter().copied().partition(|id| browser.all_tabs().find(|tab| tab.id == *id).is_some_and(|tab| tab.pinned_url.is_some()));
            for id in pins { browser.move_sidebar_item_to_space(id, space); }
            for id in today.into_iter().rev() { browser.move_sidebar_item_to_space(id, space); }
        });
        if let Some(pages) = &self.pages { pages.borrow_mut().reconcile_inventory(&self.browser); }
        self.sidebar_selection.ids.clear();
    }
    pub(crate) fn copy_sidebar_selection(&mut self, markdown: bool, cx: &mut Context<Self>) -> bool {
        self.pending_context_image=None;
        let ids = self.sidebar_selection_items();
        if ids.len() < 2 { return false; }
        let tabs: Vec<_> = ids.iter().flat_map(|id| self.browser.related_tabs(*id)).filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == id)).collect();
        let urls: Vec<_> = tabs.iter().map(|tab| browser_core::links::clean_url(&tab.url)).collect();
        let text = tabs.iter().map(|tab| if markdown { format!("- {}", browser_core::links::markdown_link(&self.browser.item_title(tab.id), &tab.url)) } else { browser_core::links::clean_url(&tab.url) }).collect::<Vec<_>>().join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        // Existing Arc copy-link feedback, shared by multi-item copies.
        let clean = tabs.iter().any(|tab| browser_core::links::clean_url(&tab.url) != tab.url);
        self.show_link_toast(if clean { "Copied clean links without trackers" } else { "Copied item links" }.into(), urls, cx);
        true
    }
    pub(crate) fn share_context_links(&mut self, urls: Vec<String>, window: &Window, cx: &mut Context<Self>) {
        let anchor = window.mouse_position();
        let main = self.main_window;
        cx.spawn(async move |owner, cx| {
            let Ok(view) = main.update(cx, |_, window, _| crate::native::view_of(window)) else { return; };
            let picker = crate::native::sharing::show(view, [f32::from(anchor.x), f32::from(anchor.y), 1., 1.], &urls);
            let _ = owner.update(cx, |this, _| this.share_picker = picker);
        }).detach();
    }
}
