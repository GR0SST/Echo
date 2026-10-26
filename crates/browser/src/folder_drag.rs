//! Folder drag payloads keep a complete subtree behind one stable identity.
use crate::window::{BrowserWindow, Target};
use browser_core::folders::{Folder, FolderId};
use browser_core::{SpaceId, TabId};
use gpui::{div, hsla, prelude::*, px, Context, Window};
use std::time::SystemTime;

#[derive(Clone)]
pub(crate) struct FolderDrag {
    pub id: FolderId,
    workspace: crate::profile::ProfileHandle,
    profile: browser_core::UserProfileId,
    focus: Option<TabId>,
    name: String,
    icon: Option<String>,
}

impl FolderDrag {
    pub(crate) fn start(&self, owner: gpui::WeakEntity<BrowserWindow>, cx: &mut gpui::App) -> gpui::Entity<Self> {
        cx.new(|cx| {
            cx.on_release(move |_, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.cancel_sidebar_drag_scroll();
                    this.cancel_space_drag_hover();
                    cx.notify();
                });
            }).detach();
            self.clone()
        })
    }

    pub(crate) fn new(folder: &Folder, focus: Option<TabId>, workspace: crate::profile::ProfileHandle, profile: browser_core::UserProfileId) -> Self {
        Self {
            id: folder.id,
            workspace,
            profile,
            focus,
            name: folder.name.clone(),
            icon: folder.custom_icon.clone(),
        }
    }
}

impl BrowserWindow {
    pub(crate) fn accepts_folder_drag(&self, drag: &FolderDrag, space: SpaceId) -> bool {
        self.profile.handle.same_workspace(&drag.workspace)
            && self.browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == drag.profile)
            && self.browser.folder_space(drag.id)
                .and_then(|id| self.browser.spaces.iter().find(|space| space.id == id))
                .is_some_and(|space| space.user_profile_id == drag.profile)
    }

    pub(crate) fn drop_folder_on_space(
        &mut self,
        drag: &FolderDrag,
        space: SpaceId,
        cx: &mut Context<Self>,
    ) {
        self.cancel_space_drag_hover();
        self.drop_folder_to(drag, space, None, cx);
    }

    pub(crate) fn drop_folder_in_folder(
        &mut self,
        drag: &FolderDrag,
        parent: FolderId,
        cx: &mut Context<Self>,
    ) {
        if let Some(space) = self.browser.folder_space(parent) {
            self.drop_folder_to(drag, space, Some(parent), cx);
        }
    }

    pub(crate) fn drop_folder_to(
        &mut self,
        drag: &FolderDrag,
        space: SpaceId,
        parent: Option<FolderId>,
        cx: &mut Context<Self>,
    ) {
        self.cancel_sidebar_drag_scroll();
        if !self.accepts_folder_drag(drag, space) { return; }
        if self.confirm_folder_destination(Target::MoveFolder(drag.id), space, parent, cx) {
            if let Some(focus) = drag
                .focus
                .filter(|id| self.browser.folder_tabs(drag.id).contains(id))
            {
                self.edit_browser(|browser| browser.activate_tab(focus, SystemTime::now()));
                cx.notify();
            }
        }
    }
}

impl Render for FolderDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(34.))
            .max_w(px(240.))
            .px_2()
            .rounded_lg()
            .shadow_md()
            .bg(hsla(0., 0., 0.98, 0.96))
            .text_color(hsla(0., 0., 0.1, 1.))
            .text_sm()
            .child(crate::icons::folder(self.icon.as_deref()))
            .child(div().min_w_0().truncate().child(self.name.clone()))
    }
}
