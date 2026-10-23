//! Space context actions refer to the clicked Space, even when it is inactive.
use crate::{
    icons::IconTarget,
    tab_menu::{item, pending, Command, Entry, MenuTarget},
    window::{BrowserWindow, Target},
};
use browser_core::SpaceId;
use gpui::{Context, Pixels, Point, Window};

impl BrowserWindow {
    pub(crate) fn open_space_menu(
        &mut self,
        space: SpaceId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() { return; }
        self.open_sidebar_menu(MenuTarget::Space(space), position, window, cx);
    }

    pub(crate) fn space_menu_commands(&self, id: SpaceId) -> Vec<Entry> {
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return Vec::new();
        };
        let others: Vec<Entry> = self.browser.spaces.iter().filter(|other| other.id != id)
            .map(|other| pending(other.name.clone()).into()).collect();
        vec![
            item("Change Space Icon…", Command::SpaceIcon).into(),
            item("Rename Space…", Command::SpaceRename).into(),
            pending("Edit Theme Color…").into(),
            item("Set Profile", Command::SpaceProfile).into(),
            Entry::Separator,
            item("New Folder", Command::SpaceNewFolder).into(),
            pending("Live Folders").submenu(vec![pending("GitHub Pull Requests").into(), pending("GitHub Issues").into()]).into(),
            Entry::Separator,
            pending("Share Space…").into(),
            pending(format!("Export \u{201c}{}\u{201d} into Folder", space.name)).submenu(others).into(),
            Entry::Separator,
            pending("Manage Spaces…").into(),
            Entry::Separator,
            pending("Delete Space").into(),
        ]
    }

    pub(crate) fn confirm_space_menu(
        &mut self,
        id: SpaceId,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() { return; }
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return;
        };
        match command {
            Command::SpaceProfile => self.open_user_profile_switch(id, cx),
            Command::SpaceRename => {
                let name = space.name.clone();
                self.open_command_bar(Target::RenameSpace(id), &name, window, cx);
            }
            Command::SpaceIcon => {
                let icon = space.custom_icon.clone().unwrap_or_default();
                self.open_command_bar(Target::Icon(IconTarget::Space(id)), &icon, window, cx);
            }
            Command::SpaceNewFolder => self.open_command_bar(
                Target::NewFolder {
                    space: id,
                    parent: None,
                    tab: None,
                },
                "",
                window,
                cx,
            ),
            _ => {}
        }
        cx.notify();
    }
}
