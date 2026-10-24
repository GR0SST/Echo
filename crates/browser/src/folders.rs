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

impl BrowserWindow {
    pub(crate) fn open_folder_creator(
        &mut self,
        parent: Option<FolderId>,
        tab: Option<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() { return; }
        let space = parent
            .and_then(|id| self.browser.folder_space(id))
            .unwrap_or(self.browser.space().id);
        self.open_command_bar(Target::NewFolder { space, parent, tab }, "", window, cx);
    }

    pub(crate) fn edit_new_folder(
        &mut self,
        parent: Option<FolderId>,
        tab: Option<TabId>,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() { return; }
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
        if self.browser.is_blank() { return; }
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
            Target::RenameFolder(id) => self.edit_browser(|browser| browser.rename_folder(id, name)),
            Target::NewFolder { space, parent, tab } => {
                if tab.is_some_and(|id| !self.browser.all_tabs().any(|tab| tab.id == id)) {
                    self.show_toast("Tab is no longer open".into(), cx);
                    return false;
                }
                let active = self.browser.space().active_tab;
                let was_active = tab.is_some_and(|tab| {
                    active.is_some_and(|active| self.browser.related_tabs(tab).contains(&active))
                });
                let Some(folder) = self.edit_browser(|browser| browser.create_folder(space, parent, name)) else {
                    self.show_toast("Cannot create a folder here".into(), cx);
                    return false;
                };
                if let Some(tab) = tab {
                    self.edit_browser(|browser| browser.move_item_to_folder(tab, folder));
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
                true
            }
            _ => false,
        }
    }

    pub(crate) fn edit_folder_destination(&mut self, target: Target, cx: &mut Context<Self>) {
        if self.browser.is_blank() { return; }
        if let Target::MoveToFolder(tab) = target {
            if self
                .browser
                .spaces
                .iter()
                .all(|space| space.folders.is_empty())
            {
                self.edit_new_folder(None, Some(tab), cx);
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
        let mut rows: Vec<Entry> = vec![
            item("Rename…", MenuCommand::FolderRename).into(),
            item("Change Icon…", MenuCommand::FolderIcon).into(),
            item("New Nested Folder…", MenuCommand::FolderNew).into(),
            item(if folder.collapsed { "Expand Folder" } else { "Collapse Folder" }, MenuCommand::FolderCollapse).into(),
            Entry::Separator,
            item("Move to…", MenuCommand::FolderMove).into(),
            item("Duplicate", MenuCommand::FolderDuplicate).into(),
            item("Turn into Space", MenuCommand::FolderIntoSpace).into(),
            Entry::Separator,
        ];
        if !self.browser.folder_tabs(id).is_empty() {
            rows.extend([
                item("Copy All Links", MenuCommand::FolderCopy).into(),
                item("Copy All Links as Markdown", MenuCommand::FolderCopyMarkdown).into(),
                Entry::Separator,
            ]);
        }
        if folder.items.is_empty() {
            rows.push(item("Delete Empty Folder", MenuCommand::FolderDelete).into());
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
            MenuCommand::FolderRename => {
                let name = self
                    .browser
                    .folder(id)
                    .map(|folder| folder.name.clone())
                    .unwrap_or_default();
                self.open_command_bar(Target::RenameFolder(id), &name, window, cx);
            }
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
            MenuCommand::FolderCollapse => {
                if let Some(collapsed) = self.browser.folder(id).map(|folder| !folder.collapsed) {
                    self.edit_browser(|browser| browser.set_folder_collapsed(id, collapsed));
                }
            }
            MenuCommand::FolderDelete => {
                self.edit_browser(|browser| browser.delete_empty_folder(id));
            }
            MenuCommand::FolderCopy | MenuCommand::FolderCopyMarkdown => {
                let markdown = matches!(command, MenuCommand::FolderCopyMarkdown);
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
                self.show_toast("Copied folder links".into(), cx);
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
        self.browser.pinned_rows(space, false).into_iter().filter_map(|row| {
            let indent = (row.depth as f32 * 12.).min(96.);
            match row.item {
                PinnedItem::Folder(id) => {
                    let folder = self.browser.folder(id)?;
                    let browser = cx.entity().downgrade();
                    let key = id.get() as usize;
                    let focus = self.browser.spaces.iter().find(|entry| entry.id == space).and_then(|space| space.active_tab).filter(|tab| self.browser.folder_ancestors(*tab).contains(&id));
                    let active = focus.is_some();
                    let group = gpui::SharedString::from(format!("folder-{key}"));
                    Some(div().pl(px(indent)).flex_none().min_w_0().child(div().id(("folder", key)).debug_selector(|| self.sidebar_debug_name((move || format!("folder-{key}"))()))
                        .relative().child(self.track_icon_anchor(IconTarget::Folder(id)))
                        .group(group.clone()).flex().items_center().h(px(crate::window::SIDEBAR_ROW_HEIGHT)).pl(px(9.)).pr(px(6.)).gap(px(9.)).rounded(px(crate::window::SIDEBAR_ROW_RADIUS))
                        .text_size(px(14.)).font_weight(gpui::FontWeight::SEMIBOLD)
                        .when(active, |this| this.bg(hsla(0., 0., 1., 0.25))).hover(|this| this.bg(hsla(0., 0., 1., 0.35)))
                        .on_click(cx.listener(move |this, _, _, cx| { if !painted_profile.is_some_and(|profile| this.prepare_sidebar_target(space, profile)) { return; } if let Some(collapsed) = this.browser.folder(id).map(|folder| !folder.collapsed) { this.edit_browser(|browser| browser.set_folder_collapsed(id, collapsed)); } cx.notify(); }))
                        .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| { cx.stop_propagation(); if !painted_profile.is_some_and(|profile| this.prepare_sidebar_target(space, profile)) { return; } this.open_folder_menu(id, event.position, window, cx); }))
                        .on_drag(crate::folder_drag::FolderDrag::new(folder, focus, self.profile.handle.clone(), self.browser.spaces.iter().find(|entry| entry.id == space).expect("folder Space").user_profile_id), |drag, _, _, cx| cx.new(|_| drag.clone()))
                        .can_drop(move |value, _, cx| {
                            if let Some(drag) = value.downcast_ref::<crate::folder_drag::FolderDrag>() {
                                browser.read_with(cx, |this, _| this.browser.can_move_folder(drag.id, space, Some(id))).unwrap_or(false)
                            } else { value.is::<crate::tab_drag::TabDrag>() }
                        })
                        .drag_over::<crate::folder_drag::FolderDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.65)))
                        .on_drop(cx.listener(move |this, drag: &crate::folder_drag::FolderDrag, _, cx| this.drop_folder_in_folder(drag, id, cx)))
                        .drag_over::<crate::tab_drag::TabDrag>(|style, _, _, _| style.bg(hsla(0., 0., 1., 0.65)))
                        .on_drop(cx.listener(move |this, drag: &crate::tab_drag::TabDrag, _, cx| this.drop_tab_in_folder(drag, id, cx)))
                        .child(div().relative().size(px(18.)).flex_none()
                            .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("folder-icon-{key}"))())).size_full().group_hover(group.clone(), |style| style.invisible())
                                .child(crate::icons::folder_state(folder.custom_icon.as_deref(), !folder.collapsed)))
                            .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("folder-chevron-{key}"))())).absolute().top_0().left_0().size_full().invisible().group_hover(group.clone(), |style| style.visible())
                                .child(crate::icons::disclosure(folder.collapsed))))
                        .child(div().debug_selector(|| self.sidebar_debug_name((move || format!("folder-name-{key}"))())).flex_1().min_w_0().truncate().child(folder.name.clone()))
                        .child(div().id(("folder-new", key)).debug_selector(|| self.sidebar_debug_name((move || format!("folder-new-{key}"))())).invisible().group_hover(group.clone(), |style| style.visible()).px_1().rounded_sm().hover(|this| this.bg(hsla(0., 0., 0., 0.08)))
                            .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); if !painted_profile.is_some_and(|profile| this.prepare_sidebar_target(space, profile)) { return; } this.open_folder_creator(Some(id), None, window, cx); })).child(crate::icons::control(crate::icons::Icon::Plus)))
                        .child(div().id(("folder-menu", key)).debug_selector(|| self.sidebar_debug_name((move || format!("folder-menu-{key}"))())).invisible().group_hover(group, |style| style.visible()).px_1().rounded_sm().hover(|this| this.bg(hsla(0., 0., 0., 0.08)))
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| { cx.stop_propagation(); if !painted_profile.is_some_and(|profile| this.prepare_sidebar_target(space, profile)) { return; } this.open_folder_menu(id, event.position(), window, cx); })).child(crate::icons::more())))
                        .into_any_element())
                }
                item => {
                    let id = self.browser.pinned_item_tab(space, item)?;
                    let tab = self.browser.all_tabs().find(|tab| tab.id == id)?;
                    let active = self.browser.spaces.iter().find(|entry| entry.id == space).and_then(|space| space.active_tab).is_some_and(|active| self.browser.item_anchor(active) == id);
                    // Indent the row itself: a wrapper makes ellipsized titles measure too narrow.
                    Some(self.render_tab(tab, Section::Pinned, active, hue, cx).ml(px(indent)).into_any_element())
                }
            }
        }).collect()
    }
}
