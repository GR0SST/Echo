//! Pinned folder rows and focused editors share the browser's native overlays.
use crate::{
    icons::IconTarget,
    tab_menu::Command as MenuCommand,
    window::{BrowserWindow, Suggestion, Target},
};
use browser_core::{
    folders::{FolderId, PinnedItem},
    Section, SpaceId, TabId,
};
use gpui::{div, hsla, prelude::*, px, AnyElement, ClipboardItem, Context, Window};
use std::time::SystemTime;

/// Same ease-out family as the Pinned section's reveal.
const FOLDER_REVEAL_SECONDS: f32 = 0.22;
// GPUI tests click folder contents without advancing the clock; motion tests opt in.
#[cfg(test)]
thread_local! { pub(crate) static ANIMATE_FOLDER_REVEAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
fn folder_reveal_animates() -> bool {
    #[cfg(test)] { ANIMATE_FOLDER_REVEAL.with(|animate| animate.get()) }
    #[cfg(not(test))] { true }
}

impl BrowserWindow {
    pub(crate) fn open_folder_creator(
        &mut self,
        parent: Option<FolderId>,
        tab: Option<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        self.create_folder_inline(parent, tab, cx);
    }

    /// Arc adds an "Untitled" folder and edits its name in the sidebar row.
    pub(crate) fn create_folder_inline(&mut self, parent: Option<FolderId>, tab: Option<TabId>, cx: &mut Context<Self>) {
        let space = parent
            .and_then(|id| self.browser.folder_space(id))
            .unwrap_or(self.browser.space().id);
        self.create_folder_inline_in(space, parent, tab, cx);
    }
    pub(crate) fn create_folder_inline_in(&mut self, space: SpaceId, parent: Option<FolderId>, tab: Option<TabId>, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() { return; }
        self.close_command_bar(cx);
        if let Some(parent) = parent { self.edit_browser(|browser| browser.set_folder_collapsed(parent, false)); }
        if let Some(folder) = self.create_folder_at(space, parent, tab, "Untitled", cx) {
            self.rename_folder_inline(folder, cx);
        }
    }

    pub(crate) fn edit_new_folder(
        &mut self,
        parent: Option<FolderId>,
        tab: Option<TabId>,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_private() || self.browser.is_blank() { return; }
        let space = parent
            .and_then(|id| self.browser.folder_space(id))
            .unwrap_or(self.browser.space().id);
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::NewFolder { space, parent, tab };
            bar.selected = 0;
            bar.input.update(cx, |input, cx| {
                input.set_placeholder("Name your Folder…", cx);
                input.set_text("", cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn edit_folder_name(&mut self, id: FolderId, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() { return; }
        let Some(name) = self.browser.folder(id).map(|folder| folder.name.clone()) else {
            return;
        };
        if let Some(bar) = &mut self.command_bar {
            bar.target = Target::RenameFolder(id);
            bar.selected = 0;
            bar.input.update(cx, |input, cx| {
                input.set_placeholder("Rename Folder…", cx);
                input.set_text(name, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn save_folder_name(
        &mut self,
        target: Target,
        name: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        match target {
            Target::RenameFolder(id) => self.edit_browser(|browser| browser.rename_folder_with_undo(id, name)),
            Target::NewFolder { space, parent, tab } => self.create_folder_at(space, parent, tab, name, cx).is_some(),
            _ => false,
        }
    }

    fn create_folder_at(&mut self, space: SpaceId, parent: Option<FolderId>, tab: Option<TabId>, name: &str, cx: &mut Context<Self>) -> Option<FolderId> {
        {
            {
                if tab.is_some_and(|id| !self.browser.all_tabs().any(|tab| tab.id == id)) {
                    self.show_toast("Tab is no longer open".into(), cx);
                    return None;
                }
                let active = self.browser.space().active_tab;
                let was_active = tab.is_some_and(|tab| {
                    active.is_some_and(|active| self.browser.related_tabs(tab).contains(&active))
                });
                let Some(folder) = self.edit_browser(|browser| browser.create_folder(space, parent, name)) else {
                    self.show_toast("Cannot create a folder here".into(), cx);
                    return None;
                };
                if let Some(tab) = tab {
                    self.edit_browser(|browser| browser.move_item_to_folder(tab, folder));
                    self.show_folder_pin(folder, cx);
                }
                if let Some(index) = self
                    .browser
                    .spaces
                    .iter()
                    .position(|entry| entry.id == space)
                {
                    self.switch_space(index, cx);
                }
                if was_active {
                    if let Some(active) = active {
                        self.edit_browser(|browser| browser.activate_tab(active, SystemTime::now()));
                    }
                }
                self.scroll_to_pinned_item(PinnedItem::Folder(folder), cx);
                Some(folder)
            }
        }
    }

    pub(crate) fn edit_folder_destination(&mut self, target: Target, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() { return; }
        if let Target::MoveToFolder(tab) = target {
            if self
                .browser
                .spaces
                .iter()
                .all(|space| space.folders.is_empty())
            {
                self.create_folder_inline(None, Some(tab), cx);
                return;
            }
        }
        if let Some(bar) = &mut self.command_bar {
            bar.target = target;
            bar.selected = 0;
            bar.input.update(cx, |input, cx| {
                input.set_placeholder("Choose a destination folder…", cx);
                input.set_text("", cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn folder_destinations(&self, target: Target, text: &str) -> Vec<Suggestion> {
        let query = text.trim().to_lowercase();
        let mut rows = Vec::new();
        let spaces = std::iter::once(self.browser.space()).chain(
            self.browser
                .spaces
                .iter()
                .filter(|space| space.id != self.browser.space().id),
        );
        for space in spaces {
            if space.user_profile_id != self.browser.active_user_profile_id() { continue; }
            for folder in &space.folders {
                let eligible = match target {
                    Target::MoveFolder(id) => {
                        self.browser.can_move_folder(id, space.id, Some(folder.id))
                    }
                    Target::MoveToFolder(tab) => {
                        self.browser.folder_ancestors(tab).last() != Some(&folder.id)
                    }
                    _ => false,
                };
                let label = format!("{} · {}", space.name, self.browser.folder_path(folder.id));
                if eligible && (query.is_empty() || label.to_lowercase().contains(&query)) {
                    rows.push(Suggestion::FolderDestination {
                        space: space.id,
                        parent: Some(folder.id),
                        label,
                    });
                }
            }
            let root = match target {
                Target::MoveFolder(id) => self.browser.can_move_folder(id, space.id, None),
                Target::MoveToFolder(tab) => !self.browser.folder_ancestors(tab).is_empty(),
                _ => false,
            };
            let label = format!("Pinned in {}", space.name);
            if root && (query.is_empty() || label.to_lowercase().contains(&query)) {
                rows.push(Suggestion::FolderDestination {
                    space: space.id,
                    parent: None,
                    label,
                });
            }
        }
        rows.truncate(12);
        rows
    }

    pub(crate) fn confirm_folder_destination(
        &mut self,
        target: Target,
        space: SpaceId,
        parent: Option<FolderId>,
        cx: &mut Context<Self>,
    ) -> bool {
        let source_profile = match target {
            Target::MoveToFolder(tab) => self.browser.user_profile_id_for_tab(tab),
            Target::MoveFolder(folder) => self.browser.folder_space(folder).and_then(|id| self.browser.spaces.iter().find(|space| space.id == id)).map(|space| space.user_profile_id),
            _ => None,
        };
        if source_profile.is_none() || !self.browser.spaces.iter().any(|entry| entry.id == space && Some(entry.user_profile_id) == source_profile) { return false; }
        let active = self.browser.space().active_tab;
        let moved = match target {
            Target::MoveToFolder(tab) => self.browser.related_tabs(tab),
            Target::MoveFolder(id) => self.browser.folder_tabs(id),
            _ => return false,
        };
        let changed = match target {
            Target::MoveToFolder(tab) => match parent {
                Some(parent) => self.edit_browser(|browser| browser.move_item_to_folder(tab, parent)),
                None => self.edit_browser(|browser| browser.move_item_to_pinned_root(tab, space)),
            },
            Target::MoveFolder(id) => self.edit_browser(|browser| browser.move_folder(id, space, parent)),
            _ => false,
        };
        if !changed {
            self.show_toast("Cannot move this item here".into(), cx);
            return false;
        }
        if let Some(index) = self
            .browser
            .spaces
            .iter()
            .position(|entry| entry.id == space)
        {
            self.switch_space(index, cx);
        }
        if let Some(active) = active.filter(|id| moved.contains(id)) {
            self.edit_browser(|browser| browser.activate_tab(active, SystemTime::now()));
        } else if let Target::MoveToFolder(tab) = target {
            self.edit_browser(|browser| browser.activate_sidebar_item(tab, SystemTime::now()));
        }
        let item = match target {
            Target::MoveFolder(id) => PinnedItem::Folder(id),
            Target::MoveToFolder(tab) => self.browser.pinned_item(tab),
            _ => unreachable!(),
        };
        self.scroll_to_pinned_item(item, cx);
        match (target, parent) {
            (Target::MoveToFolder(_), Some(folder)) => self.show_folder_pin(folder, cx),
            (Target::MoveToFolder(tab), None) => self.show_tab_move(tab, cx),
            (Target::MoveFolder(_), _) => {
                if let Some(tab) = moved.first() { self.show_tab_move(*tab, cx); }
            }
            _ => {}
        }
        true
    }

    pub(crate) fn expand_folder(&mut self, id: FolderId, cx: &mut Context<Self>) {
        let Some(space) = self.browser.folder_space(id) else {
            return;
        };
        if let Some(index) = self
            .browser
            .spaces
            .iter()
            .position(|entry| entry.id == space)
        {
            self.switch_space(index, cx);
        }
        let mut current = Some(id);
        while let Some(id) = current {
            let parent = self
                .browser
                .space()
                .folders
                .iter()
                .find(|folder| folder.items.contains(&PinnedItem::Folder(id)))
                .map(|folder| folder.id);
            self.edit_browser(|browser| browser.set_folder_collapsed(id, false));
            current = parent;
        }
        self.edit_browser(|browser| browser.spaces[browser.active_space].pinned_collapsed = false);
        self.scroll_to_pinned_item(PinnedItem::Folder(id), cx);
    }

    pub(crate) fn folder_menu_commands(&self, id: FolderId) -> Vec<crate::tab_menu::Entry> {
        use crate::tab_menu::{item, Entry};
        let Some(folder) = self.browser.folder(id) else {
            return Vec::new();
        };
        if folder.live.is_some() { return self.live_folder_menu(id); }
        let mut rows: Vec<Entry> = vec![
            item("New Nested Folder…", MenuCommand::FolderNew).into(),
            item(format!("Turn “{}” into Space", folder.name), MenuCommand::FolderIntoSpace).into(),
            Entry::Separator,
            item("Change Icon…", MenuCommand::FolderIcon).into(),
            item("Rename…", MenuCommand::FolderRename).into(),
            Entry::Separator,
            item("Duplicate", MenuCommand::FolderDuplicate).into(),
            item("Move to", MenuCommand::FolderMove).into(),
            Entry::Separator,
        ];
        if folder.items.is_empty() {
            rows.push(item("Delete", MenuCommand::FolderDelete).into());
        } else {
            rows.push(item("Archive Folder", MenuCommand::FolderArchive).into());
        }
        rows
    }

    pub(crate) fn confirm_folder_menu(
        &mut self,
        id: FolderId,
        command: MenuCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            MenuCommand::LiveConnect => self.open_live_connection(id,cx),
            MenuCommand::LiveMove(space) => {self.edit_browser(|b|b.move_folder(id,space,None));cx.notify();},
            MenuCommand::LiveRefresh => self.refresh_live_folder(id,cx),
            MenuCommand::LiveFilter(filter) => self.toggle_live_filter(id,filter,cx),
            MenuCommand::FolderIntoSpace => {
                if self.edit_browser(|browser| browser.folder_into_space(id, SystemTime::now()))
                    .is_some()
                {
                    self.switch_space(self.browser.active_space, cx);
                }
            }
            MenuCommand::FolderDuplicate => {
                if let Some(copy) = self.edit_browser(|browser| browser.duplicate_folder(id, SystemTime::now())) {
                    self.scroll_to_pinned_item(PinnedItem::Folder(copy), cx);
                }
            }
            MenuCommand::FolderRename => self.rename_folder_inline(id, cx),
            MenuCommand::FolderNew => self.open_folder_creator(Some(id), None, window, cx),
            MenuCommand::FolderMove => {
                self.open_command_bar(Target::MoveFolder(id), "", window, cx)
            }
            MenuCommand::FolderIcon => {
                let icon = self
                    .browser
                    .folder(id)
                    .and_then(|folder| folder.custom_icon.clone())
                    .unwrap_or_default();
                self.open_command_bar(Target::Icon(IconTarget::Folder(id)), &icon, window, cx);
            }
            MenuCommand::FolderCollapse => self.toggle_folder(id, cx),
            MenuCommand::FolderArchive => {
                let name = self.browser.folder(id).map(|f|f.name.clone()).unwrap_or_default();
                let removed = self.browser.folder_tabs(id);
                if self.edit_browser(|browser| browser.archive_folder(id,SystemTime::now())).is_some() {
                    if let Some(pages)=&self.pages { let mut pages=pages.borrow_mut();for tab in removed {pages.discard(tab);} pages.sync(&self.browser,self.page_content_visible()); }
                    self.after_tab_removal(cx);
                    self.show_archive_undo(&name,cx);
                }
            }
            MenuCommand::FolderDelete => {
                if self.browser.folder(id).is_some_and(|f|f.live.is_some()){self.open_live_delete(id,cx);return;}
                self.edit_browser(|browser| browser.delete_empty_folder(id));
            }
            MenuCommand::FolderCopy | MenuCommand::FolderCopyMarkdown => {
                let markdown = matches!(command, MenuCommand::FolderCopyMarkdown);
                let urls: Vec<_> = self.browser.folder_tabs(id).into_iter()
                    .filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == id))
                    .map(|tab| browser_core::links::clean_url(&tab.url)).collect();
                let links: Vec<_> = self
                    .browser
                    .folder_tabs(id)
                    .into_iter()
                    .filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == id))
                    .map(|tab| {
                        if markdown {
                            browser_core::links::markdown_link(&tab.title, &tab.url)
                        } else {
                            browser_core::links::clean_url(&tab.url)
                        }
                    })
                    .collect();
                cx.write_to_clipboard(ClipboardItem::new_string(links.join("\n")));
                self.show_link_toast("Copied item links".into(), urls, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn render_folder_rows(
        &self,
        space: SpaceId,
        hue: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let painted_profile = self.browser.spaces.iter().find(|entry| entry.id == space).map(|entry| entry.user_profile_id);
        self.folder_row_weights(space).into_iter().filter_map(|(row, weight)| {
            let indent = (row.depth as f32 * 16.).min(96.);
            // A row inside an opening or closing folder shows only its part,
            // pulling the rows below along instead of jumping.
            let reveal = move |row: AnyElement| if weight >= 1. { row } else {
                div().flex_none().h(px(weight * crate::window::SIDEBAR_ROW_HEIGHT))
                    .mt(px(-(1. - weight) * crate::window::SIDEBAR_ROW_GAP))
                    .overflow_hidden().child(row).into_any_element()
            };
            let row: Option<AnyElement> = match row.item {
                PinnedItem::Folder(id) => {
                    let folder = self.browser.folder(id)?;
                    let live_folder = folder.live.is_some();
                    let browser = cx.entity().downgrade();
                    let key = id.get() as usize;
                    let focus = self.browser.spaces.iter().find(|entry| entry.id == space).and_then(|space| space.active_tab).filter(|tab| self.browser.folder_ancestors(*tab).contains(&id));
                    let active = focus.is_some();
                    let dark = self.theme_is_dark_for(space);
                    let folder_pigment = self.theme_primary_for(space);
                    Some(div().pl(px(indent)).flex_none().min_w_0().flex().flex_col().gap(px(crate::window::SIDEBAR_ROW_GAP)).child(div().id(("folder", key)).debug_selector(|| self.sidebar_debug_name((move || format!("folder-{key}"))()))
                        .relative().child(self.track_icon_anchor(IconTarget::Folder(id)))
                        // Resting on a folder previews its pages after a short dwell.
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| { if *hovered && this.sidebar_page_interactive { this.hover_folder(id, cx); } }))
                        .flex().items_center().h(px(crate::window::SIDEBAR_ROW_HEIGHT)).pl(px(9.)).pr(px(6.)).gap(px(9.)).rounded(px(crate::window::SIDEBAR_ROW_RADIUS))
                        .text_size(px(14.)).font_weight(gpui::FontWeight::SEMIBOLD)
                        .when(active || self.sidebar_folder_selected(id), |this| this.bg(hsla(0., 0., 1., 0.25))).hover(move |this| this.bg(hsla(0., 0., 1., if dark { 0.1 } else { 0.3 })))
                        .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| { if !painted_profile.is_some_and(|profile| this.prepare_sidebar_target(space, profile)) { return; } if this.select_sidebar_folder(id,event.modifiers(),cx) {return;} if this.browser.folder(id).and_then(|f|f.live.as_ref()).is_some_and(|l|l.credential_id.is_empty()) { this.open_live_connection(id,cx); } else { this.toggle_folder(id, cx); } cx.notify(); }))
                        .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| { cx.stop_propagation(); if !painted_profile.is_some_and(|profile| this.prepare_sidebar_target(space, profile)) { return; } this.open_folder_menu(id, event.position, window, cx); }))
                        .on_drag(crate::folder_drag::FolderDrag::new(folder, focus, self.profile.handle.clone(), self.browser.spaces.iter().find(|entry| entry.id == space).expect("folder Space").user_profile_id), { let owner = cx.entity().downgrade(); move |drag, _, _, cx| drag.start(owner.clone(), cx) })
                        .can_drop(move |value, _, cx| {
                            if live_folder { return false; }
                            if let Some(drag) = value.downcast_ref::<crate::folder_drag::FolderDrag>() {
                                browser.read_with(cx, |this, _| this.browser.can_move_folder(drag.id, space, Some(id))).unwrap_or(false)
                            } else { value.is::<crate::tab_drag::TabDrag>() }
                        })
                        .drag_over::<crate::folder_drag::FolderDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.65)))
                        .on_drop(cx.listener(move |this, drag: &crate::folder_drag::FolderDrag, _, cx| this.drop_folder_in_folder(drag, id, cx)))
                        .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.65)))
                        .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_in_folder(drag, id, cx)))
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("folder-icon-{key}"))())).size(px(18.)).flex_none()
                            .child(if folder.live.is_some() && folder.custom_icon.is_none() { crate::live_folders::folder_icon(!folder.collapsed) } else { crate::icons::folder_tinted(folder.custom_icon.as_deref(), self.folder_reveal(id, !folder.collapsed), folder_pigment) }))
                        .child(self.render_inline_folder_name(id,cx).unwrap_or_else(||div().debug_selector(|| self.sidebar_debug_name((move || format!("folder-name-{key}"))())).flex_1().min_w_0().truncate().child(folder.name.clone()).into_any_element()))
                        .when_some(folder.live.as_ref().filter(|l|l.unread_count()>0), |r,live|r.child(div().text_size(px(11.)).text_color(hsla(0.,0.,0.6,1.)).child(live.unread_count().to_string()))))
                        .children(self.render_live_items(id,cx))
                        .into_any_element())
                }
                item => {
                    let id = self.browser.pinned_item_tab(space, item)?;
                    let tab = self.browser.all_tabs().find(|tab| tab.id == id)?;
                    let active = self.browser.spaces.iter().find(|entry| entry.id == space).and_then(|space| space.active_tab).is_some_and(|active| self.browser.item_anchor(active) == id);
                    // Indent the row itself: a wrapper makes ellipsized titles measure too narrow.
                    let segmented = self.browser.split_for(id).is_some_and(|split| split.title.is_none());
                    let inset = if segmented { 0. } else { self.sidebar_press_inset(id, cx) };
                    Some(self.render_tab(tab, Section::Pinned, active, hue, cx).ml(px(indent + inset))
                        .into_any_element())
                }
            };
            row.map(reveal)
        }).collect()
    }

    /// Pinned rows with their visible fraction: 1 when shown, between 0 and 1
    /// while an ancestor folder opens or closes. Hidden rows are omitted.
    pub(crate) fn folder_row_weights(&self, space: SpaceId) -> Vec<(browser_core::folders::PinnedRow, f32)> {
        let shown: std::collections::HashSet<PinnedItem> = self.browser.pinned_rows(space, false).iter().map(|row| row.item).collect();
        // Per depth: the children's animated fraction and their settled openness.
        let mut fractions: Vec<f32> = Vec::new();
        let mut settled: Vec<bool> = Vec::new();
        let mut rows = Vec::new();
        for row in self.browser.pinned_rows(space, true) {
            fractions.truncate(row.depth);
            settled.truncate(row.depth);
            let parent = fractions.last().copied().unwrap_or(1.);
            let open = settled.last().copied().unwrap_or(true);
            // A closed folder still shows its active descendant.
            let weight = if !open && shown.contains(&row.item) { 1. } else { parent };
            if let PinnedItem::Folder(id) = row.item {
                let folder_open = self.browser.folder(id).is_some_and(|folder| !folder.collapsed);
                fractions.push(parent * self.folder_reveal(id, folder_open));
                settled.push(open && folder_open);
            }
            if weight > 0. { rows.push((row, weight)); }
        }
        rows
    }

    /// Folder icons take the Space theme's primary color; the default is blue.
    pub(crate) fn theme_primary_for(&self, space: SpaceId) -> gpui::Hsla {
        match self.browser.spaces.iter().find(|entry| entry.id == space).filter(|entry| entry.theme.is_some()) {
            Some(entry) => crate::theme_editor::color(entry.theme().colors[0]).into(),
            None => gpui::rgb(crate::icons::FOLDER_BLUE).into(),
        }
    }

    fn folder_reveal(&self, id: FolderId, open: bool) -> f32 {
        self.folder_reveals.get(&id).filter(|reveal| reveal.collapsed != open)
            .map_or(if open { 1. } else { 0. }, |reveal| reveal.amount)
    }

    /// Opens or closes a folder with the sidebar's reveal motion.
    pub(crate) fn toggle_folder(&mut self, id: FolderId, cx: &mut Context<Self>) {
        let Some(was_collapsed) = self.browser.folder(id).map(|folder| folder.collapsed) else { return; };
        let from = self.folder_reveals.remove(&id)
            .filter(|reveal| reveal.collapsed == was_collapsed)
            .map_or(if was_collapsed { 0. } else { 1. }, |reveal| reveal.amount);
        let collapsed = !was_collapsed;
        self.edit_browser(|browser| browser.set_folder_collapsed(id, collapsed));
        let to = if collapsed { 0. } else { 1. };
        if !crate::native::prefers_reduced_motion() && folder_reveal_animates() {
            let started = cx.background_executor().now();
            let task = cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
                let elapsed = cx.background_executor().now().duration_since(started).as_secs_f32();
                let progress = (elapsed / FOLDER_REVEAL_SECONDS).clamp(0., 1.);
                let amount = from + (to - from) * (1. - (1. - progress).powi(3));
                if this.update(cx, |this, cx| {
                    if progress >= 1. { this.folder_reveals.remove(&id); }
                    else if let Some(reveal) = this.folder_reveals.get_mut(&id) { reveal.amount = amount; }
                    cx.notify();
                }).is_err() || progress >= 1. { break; }
            });
            self.folder_reveals.insert(id, crate::window::PinnedReveal { amount: from, collapsed, _task: task });
        }
        cx.notify();
    }
}
