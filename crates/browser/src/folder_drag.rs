//! Folder drag payloads keep a complete subtree behind one stable identity.
use crate::window::{BrowserWindow, Target};
use browser_core::folders::{Folder, FolderId};
use gpui::{div, hsla, prelude::*, px, Context, Window};

#[derive(Clone)]
pub(crate) struct FolderDrag {
    pub id: FolderId,
    name: String,
    icon: Option<String>,
}

impl FolderDrag {
    pub(crate) fn new(folder: &Folder) -> Self {
        Self {
            id: folder.id,
            name: folder.name.clone(),
            icon: folder.custom_icon.clone(),
        }
    }
}

impl BrowserWindow {
    pub(crate) fn drop_folder_in_folder(
        &mut self,
        folder: FolderId,
        parent: FolderId,
        cx: &mut Context<Self>,
    ) {
        if let Some(space) = self.browser.folder_space(parent) {
            self.confirm_folder_destination(Target::MoveFolder(folder), space, Some(parent), cx);
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
            .child(crate::icons::glyph(self.icon.as_deref().unwrap_or("📁")))
            .child(div().min_w_0().truncate().child(self.name.clone()))
    }
}
