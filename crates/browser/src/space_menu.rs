//! Space context actions refer to the clicked Space, even when it is inactive.
use crate::{
    icons::IconTarget,
    tab_menu::{Command, MenuTarget},
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
        self.open_sidebar_menu(MenuTarget::Space(space), position, window, cx);
    }

    pub(crate) fn space_menu_commands(&self, id: SpaceId) -> Vec<(&'static str, Command)> {
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return Vec::new();
        };
        vec![
            ("Rename…", Command::SpaceRename),
            ("Change Space Icon…", Command::SpaceIcon),
            (
                if space.show_header {
                    "Hide Space Header"
                } else {
                    "Show Space Header"
                },
                Command::SpaceHeader,
            ),
            ("New Folder…", Command::SpaceNewFolder),
        ]
    }

    pub(crate) fn confirm_space_menu(
        &mut self,
        id: SpaceId,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return;
        };
        match command {
            Command::SpaceRename => {
                let name = space.name.clone();
                self.open_command_bar(Target::RenameSpace(id), &name, window, cx);
            }
            Command::SpaceIcon => {
                let icon = space.custom_icon.clone().unwrap_or_default();
                self.open_command_bar(Target::Icon(IconTarget::Space(id)), &icon, window, cx);
            }
            Command::SpaceHeader => {
                self.browser.set_space_header(id, !space.show_header);
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
