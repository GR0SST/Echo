//! Space context actions refer to the clicked Space, even when it is inactive.
use crate::{
    icons::IconTarget,
    tab_menu::{Command, Entry, MenuTarget, item, pending},
    window::{BrowserWindow, Target},
};
use browser_core::SpaceId;
use gpui::{AnyElement, Context, Pixels, Point, Window, div, prelude::*, px, rgb};

impl BrowserWindow {
    pub(crate) fn open_space_menu(
        &mut self,
        space: SpaceId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() {
            return;
        }
        self.open_sidebar_menu(MenuTarget::Space(space), position, window, cx);
    }

    pub(crate) fn space_menu_commands(&self, id: SpaceId) -> Vec<Entry> {
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return Vec::new();
        };
        let mut profiles: Vec<_> = self.browser.user_profiles().collect();
        profiles.sort_by_key(|profile| profile.name().to_lowercase());
        let mut choices: Vec<Entry> = profiles
            .into_iter()
            .map(|profile| {
                item(
                    profile.name().to_owned(),
                    Command::SpaceSetProfile(profile.id()),
                )
                .checked(profile.id() == space.user_profile_id)
                .into()
            })
            .collect();
        choices.extend([
            Entry::Separator,
            item("New Profile…", Command::SpaceNewProfile).into(),
        ]);
        let others: Vec<Entry> = self
            .browser
            .spaces
            .iter()
            .map(|other| {
                item(other.name.clone(), Command::SpaceExport(other.id))
                    .space_icon(other.id)
                    .enabled(other.id != id)
                    .into()
            })
            .collect();
        vec![
            item("Change Space Icon…", Command::SpaceIcon).into(),
            item("Rename Space…", Command::SpaceRename).into(),
            item("Edit Theme Color…", Command::SpaceTheme).into(),
            pending("Set Profile").submenu(choices).into(),
            Entry::Separator,
            item("New Folder", Command::SpaceNewFolder).into(),
            pending("Live Folders")
                .submenu(vec![pending("GitHub").into()])
                .into(),
            Entry::Separator,
            pending(format!("Export “{}” into Folder", space.name))
                .submenu(others)
                .into(),
            Entry::Separator,
            item("Manage Spaces…", Command::SpaceManage).into(),
            Entry::Separator,
            item("Delete Space", Command::SpaceDelete)
                .enabled(self.browser.spaces.len() > 1 && !self.browser.is_private())
                .into(),
        ]
    }

    pub(crate) fn confirm_space_menu(
        &mut self,
        id: SpaceId,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_blank() {
            return;
        }
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return;
        };
        match command {
            Command::SpaceSetProfile(to) => {
                if to != space.user_profile_id {
                    self.open_user_profile_switch_to(id, to, cx);
                }
            }
            Command::SpaceNewProfile => self.open_user_profile_creation(
                crate::user_profile_dialog::CreationSource::File(id),
                cx,
            ),
            Command::SpaceManage => self.open_library_spaces(cx),
            Command::SpaceDelete => self.open_sidebar_menu(
                MenuTarget::SpaceDelete(id),
                gpui::point(px(0.), px(0.)),
                window,
                cx,
            ),
            Command::SpaceExport(target) => {
                if let Some(folder) =
                    self.edit_browser(|browser| browser.export_space_into_folder(id, target))
                {
                    if let Some(pages) = &self.pages {
                        pages.borrow_mut().reconcile_inventory(&self.browser);
                    }
                    self.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(target), cx);
                    self.scroll_to_pinned_item(
                        browser_core::folders::PinnedItem::Folder(folder),
                        cx,
                    );
                    self.settle_sidebar_pager(cx);
                }
            }
            Command::SpaceTheme => self.open_theme_editor(id, window, cx),
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

    pub(crate) fn confirm_space_delete(&mut self, id: SpaceId, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if self.tab_menu.as_ref().is_none_or(|menu| menu.target != MenuTarget::SpaceDelete(id) || !self.menu_is_current(menu)) { self.close_tab_menu(cx); return; }
        self.close_tab_menu(cx);
        if self.edit_browser(|browser| browser.delete_space(id, std::time::SystemTime::now())) {
            if let Some(pages) = &self.pages {
                pages.borrow_mut().reconcile_inventory(&self.browser);
            }
            self.select_sidebar_page(
                crate::sidebar_pages::SidebarPage::Space(self.browser.space().id),
                cx,
            );
            self.settle_sidebar_pager(cx);
        }
        cx.notify();
    }
    pub(crate) fn render_space_delete(&self, id: SpaceId, cx: &mut Context<Self>) -> AnyElement {
        let Some(space) = self.browser.spaces.iter().find(|space| space.id == id) else {
            return div().into_any_element();
        };
        let Some(menu) = &self.tab_menu else {
            return div().into_any_element();
        };
        let button = |name: &'static str, label: &'static str, primary: bool| {
            div()
                .id(name)
                .debug_selector(move || name.into())
                .w(px(if primary { 99. } else { 110. }))
                .h(px(36.))
                .rounded(px(6.))
                .border_1()
                .border_color(rgb(if primary { 0xff614b } else { 0x615a9d }))
                .bg(rgb(if primary { 0xff3514 } else { 0x302c63 }))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(gpui::white())
                .child(label)
                .child(
                    div()
                        .h(px(20.))
                        .px(px(4.))
                        .rounded(px(3.))
                        .bg(gpui::hsla(0., 0., 1., 0.18))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(if primary {
                            crate::icons::control_sized(crate::icons::Icon::Enter, 14.)
                        } else {
                            div().child("ESC").into_any_element()
                        }),
                )
        };
        div()
            .size_full()
            .bg(gpui::hsla(0., 0., 0., 0.55))
            .flex()
            .items_center()
            .justify_center()
            .key_context("TabContextMenu")
            .track_focus(&menu.focus)
            .on_action(
                cx.listener(|this, _: &crate::tab_menu::Dismiss, _, cx| this.close_tab_menu(cx)),
            )
            .on_action(
                cx.listener(move |this, _: &crate::tab_menu::Confirm, _, cx| {
                    this.confirm_space_delete(id, cx)
                }),
            )
            .child(
                div()
                    .id("delete-space-dialog")
                    .debug_selector(|| "delete-space-dialog".into())
                    .relative()
                    .w(px(450.))
                    .h(px(274.))
                    .rounded(px(18.))
                    .border_1()
                    .border_color(rgb(0x020202))
                    .bg(rgb(0x141c31))
                    .shadow_lg()
                    .font_family(".SystemUIFont")
                    .text_color(gpui::white())
                    .child(
                        gpui::canvas(
                            |_, _, _| (),
                            |bounds, _, window, _| {
                                window.paint_quad(
                                    gpui::fill(bounds, rgb(0x2c3345)).corner_radii(px(17.)),
                                );
                                window.paint_quad(
                                    gpui::fill(bounds.inset(px(1.)), rgb(0x141c31))
                                        .corner_radii(px(16.)),
                                );
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(
                        gpui::img(std::sync::Arc::new(gpui::Image::from_bytes(
                            gpui::ImageFormat::Png,
                            include_bytes!("../assets/echo-icon.png").to_vec(),
                        )))
                        .absolute()
                        .left(px(38.))
                        .top(px(42.))
                        .size(px(52.)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(38.))
                            .right(px(28.))
                            .top(px(116.))
                            .text_size(px(18.))
                            .line_height(px(22.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(format!("Delete your “{}” Space?", space.name)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(38.))
                            .right(px(28.))
                            .top(px(152.))
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .text_color(rgb(0xb8b8c0))
                            .child("This will archive all the tabs and folders inside it."),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(28.))
                            .bottom(px(28.))
                            .flex()
                            .gap(px(8.))
                            .child(
                                button("delete-space-cancel", "Cancel", false).on_click(
                                    cx.listener(|this, _, _, cx| this.close_tab_menu(cx)),
                                ),
                            )
                            .child(button("delete-space-confirm", "Delete", true).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.confirm_space_delete(id, cx)
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }
}
