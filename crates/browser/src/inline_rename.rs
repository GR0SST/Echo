//! Tab names are edited in their sidebar rows, without covering the page.
use crate::{text_input::TextInput, window::{BrowserWindow, Target}};
use browser_core::{Browser, SpaceId, TabId, UserProfileId};
use gpui::{actions, div, prelude::*, App, AnyElement, Context, Entity, Focusable, KeyBinding, MouseButton, Subscription, Window};

actions!(inline_tab_rename, [CommitName, CancelName]);
const CONTEXT: &str = "InlineTabRename";

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", CommitName, Some(CONTEXT)),
        KeyBinding::new("escape", CancelName, Some(CONTEXT)),
    ]);
}

pub(crate) struct TabRename {
    pub tab: TabId,
    pub(crate) space: SpaceId,
    pub(crate) profile: UserProfileId,
    pub input: Entity<TextInput>,
    _blur: Subscription,
}

fn can_rename_inline(browser: &Browser, space: SpaceId, profile: UserProfileId, tab: TabId) -> bool {
    browser.user_profile_id_for_tab(tab) == Some(profile)
        && browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == profile
            && entry.pinned.iter().chain(&entry.today).any(|page| page.id == tab)
            && !entry.splits.iter().any(|split| split.panes.contains(&tab)))
}

impl BrowserWindow {
    pub(crate) fn can_rename_tab_inline(&self, tab: TabId) -> bool {
        self.can_rename_tab_inline_for(self.browser.space().id, self.browser.active_user_profile_id(), tab)
    }

    pub(crate) fn can_rename_tab_inline_for(&self, space: SpaceId, profile: UserProfileId, tab: TabId) -> bool {
        can_rename_inline(&self.browser, space, profile, tab)
    }

    pub(crate) fn open_tab_rename(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_rename_tab_inline(tab) { self.rename_tab_inline(tab, cx); }
        else {
            let title = self.browser.item_title(tab);
            self.open_command_bar(Target::RenameTab(tab), &title, window, cx);
        }
    }

    pub(crate) fn rename_tab_inline(&mut self, tab: TabId, cx: &mut Context<Self>) {
        self.close_user_profile_dialog(cx);
        self.finish_tab_rename(true, cx);
        if !self.can_rename_tab_inline(tab) { return; }
        let space = self.browser.space().id;
        let profile = self.browser.active_user_profile_id();
        self.reveal_tab_row(tab, cx);
        self.close_tab_menu(cx);
        self.tab_rename_generation += 1;
        let generation = self.tab_rename_generation;
        self.tab_rename_pending = Some(tab);
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        // A menu or Command Bar may be closing in a child window. Focus the
        // real sidebar after those windows release their native responders.
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = browser.update(cx, |this, cx| {
                    this.refresh_profile_projection();
                    if this.tab_rename_generation != generation { return; }
                    this.tab_rename_pending = None;
                    if !this.can_rename_tab_inline_for(space, profile, tab) { return; }
                    let title = this.browser.item_title(tab);
                    let placeholder = this.browser.all_tabs().find(|page| page.id == tab)
                        .map(|page| page.page_title.as_ref().unwrap_or(&page.title).clone()).unwrap_or_default();
                    let input = cx.new(|cx| {
                        let mut input = TextInput::new(placeholder, cx);
                        input.set_text(title, cx);
                        input
                    });
                    let focus = input.read(cx).focus_handle(cx);
                    let blur = cx.on_blur(&focus, window, |this, _, cx| this.finish_tab_rename(true, cx));
                    this.tab_rename = Some(TabRename { tab, space, profile, input, _blur: blur });
                    if let Some(pages) = &this.pages { pages.borrow_mut().focus_ui(); }
                    window.activate_window();
                    focus.focus(window);
                    cx.notify();
                });
            });
        });
    }

    pub(crate) fn finish_tab_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        self.finish_folder_rename(save,cx);
        self.tab_rename_generation += 1;
        self.tab_rename_pending = None;
        let Some(editor) = self.tab_rename.take() else { return; };
        if save {
            let name = editor.input.read(cx).text().trim().to_owned();
            // A peer can reassign the Space before our queued projection update.
            // Validate inside the authoritative metadata mutation, without
            // applying this window's stale selection before rejecting it.
            self.edit_profile(|browser| {
                if !can_rename_inline(browser, editor.space, editor.profile, editor.tab) { return false; }
                if name.is_empty() { browser.reset_tab_name(editor.tab) }
                else { browser.rename_tab(editor.tab, &name) }
            });
        }
        cx.notify();
    }

    pub(crate) fn render_inline_tab_name(&self, tab: TabId, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.sidebar_page_interactive { return None; }
        let editor = self.tab_rename.as_ref().filter(|editor| editor.tab == tab
            && editor.space == self.browser.space().id && editor.profile == self.browser.active_user_profile_id()
            && self.can_rename_tab_inline_for(editor.space, editor.profile, editor.tab))?;
        Some(div().id("inline-tab-rename").debug_selector(|| self.sidebar_debug_name((|| "inline-tab-rename".into())()))
            .flex_1().min_w_0().key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &CommitName, window, cx| {
                this.finish_tab_rename(true, cx);
                this.focus_handle.focus(window);
            }))
            .on_action(cx.listener(|this, _: &CancelName, window, cx| {
                this.finish_tab_rename(false, cx);
                this.focus_handle.focus(window);
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.finish_tab_rename(true, cx)))
            .child(editor.input.clone()).into_any_element())
    }
}
