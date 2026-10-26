//! Group creation immediately edits the new sidebar folder, as Arc does.
use crate::{text_input::TextInput, window::BrowserWindow};
use browser_core::{folders::FolderId, SpaceId, UserProfileId};
use gpui::{
    actions, div, prelude::*, AnyElement, Context, Entity, Focusable, MouseButton, Subscription,
};
actions!(inline_folder_rename, [CommitFolderName, CancelFolderName]);
const CONTEXT: &str = "InlineFolderRename";
pub(crate) struct FolderRename {
    pub id: FolderId,
    space: SpaceId,
    profile: UserProfileId,
    input: Entity<TextInput>,
    _blur: Subscription,
}
impl BrowserWindow {
    pub(crate) fn folder_rename_is_current(&self) -> bool {
        self.folder_rename.as_ref().is_none_or(|e| {
            self.browser.space().id == e.space
                && self.browser.active_user_profile_id() == e.profile
                && self.browser.folder_space(e.id) == Some(e.space)
        })
    }
    pub(crate) fn rename_folder_inline(&mut self, id: FolderId, cx: &mut Context<Self>) {
        self.finish_tab_rename(true, cx);
        let Some(space) = self.browser.folder_space(id) else {
            return;
        };
        if space != self.browser.space().id {
            return;
        }
        let profile = self.browser.active_user_profile_id();
        self.close_tab_menu(cx);
        self.scroll_to_pinned_item(browser_core::folders::PinnedItem::Folder(id), cx);
        self.folder_rename_generation += 1;
        let generation = self.folder_rename_generation;
        self.folder_rename_pending = Some(id);
        // Selection-menu teardown and the following page sync run before the
        // deferred root editor. Keep their native responder out of that gap.
        if let Some(pages) = &self.pages {
            pages.borrow_mut().focus_ui();
        }
        let main = self.main_window;
        let owner = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.refresh_profile_projection();
                    if this.folder_rename_generation != generation {
                        return;
                    }
                    this.folder_rename_pending = None;
                    if this.browser.space().id != space
                        || this.browser.active_user_profile_id() != profile
                        || this.browser.folder_space(id) != Some(space)
                    {
                        return;
                    }
                    let name = this.browser.folder(id).unwrap().name.clone();
                    let input = cx.new(|cx| {
                        let mut input = TextInput::new("", cx);
                        input.set_text(name, cx);
                        input.select_all_text(cx);
                        input
                    });
                    let focus = input.read(cx).focus_handle(cx);
                    let blur = cx.on_blur(&focus, window, |this, _, cx| {
                        this.finish_folder_rename(true, cx)
                    });
                    this.folder_rename = Some(FolderRename {
                        id,
                        space,
                        profile,
                        input,
                        _blur: blur,
                    });
                    if let Some(pages) = &this.pages {
                        pages.borrow_mut().focus_ui();
                    }
                    window.activate_window();
                    focus.focus(window);
                    cx.notify();
                });
            });
        });
    }
    pub(crate) fn finish_folder_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        self.folder_rename_generation += 1;
        self.folder_rename_pending = None;
        let Some(editor) = self.folder_rename.take() else {
            return;
        };
        let name = editor.input.read(cx).text().trim().to_owned();
        if save && !name.is_empty() {
            self.edit_profile(|browser| {
                if browser.folder_space(editor.id) != Some(editor.space)
                    || !browser
                        .spaces
                        .iter()
                        .any(|s| s.id == editor.space && s.user_profile_id == editor.profile)
                {
                    return false;
                }
                browser.rename_folder_with_undo(editor.id, &name)
            });
        }
        cx.notify();
    }
    pub(crate) fn render_inline_folder_name(
        &self,
        id: FolderId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.folder_rename.as_ref().filter(|e| {
            e.id == id && self.folder_rename_is_current() && self.sidebar_page_interactive
        })?;
        Some(
            div()
                .id("inline-folder-rename")
                .debug_selector(|| self.sidebar_debug_name("inline-folder-rename".into()))
                .flex_1()
                .min_w_0()
                .key_context(CONTEXT)
                .on_action(cx.listener(|this, _: &CommitFolderName, window, cx| {
                    this.finish_folder_rename(true, cx);
                    this.focus_handle.focus(window);
                }))
                .on_action(cx.listener(|this, _: &CancelFolderName, window, cx| {
                    this.finish_folder_rename(false, cx);
                    this.focus_handle.focus(window);
                }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(
                    cx.listener(|this, _, _, cx| this.finish_folder_rename(true, cx)),
                )
                .child(editor.input.clone())
                .into_any_element(),
        )
    }
}
pub(crate) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", CommitFolderName, Some(CONTEXT)),
        gpui::KeyBinding::new("escape", CancelFolderName, Some(CONTEXT)),
    ]);
}
