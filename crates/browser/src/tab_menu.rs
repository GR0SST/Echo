//! Sidebar context menus target the clicked tab without activating it.
use crate::window::{BrowserWindow, Overlay, OverlayKind, Target};
use browser_core::TabId;
use gpui::{
    actions, div, hsla, point, prelude::*, px, AnyElement, App, Context, FocusHandle,
    KeyBinding, Pixels, Point, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions,
};

const CONTEXT: &str = "TabContextMenu";
const WIDTH: f32 = 260.0;
actions!(tab_menu, [Dismiss, Next, Previous, Confirm]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
    ]);
}

pub(crate) struct TabMenu {
    pub tab: TabId,
    pub selected: usize,
    position: Point<Pixels>,
    focus: FocusHandle,
    pub overlay: Option<WindowHandle<Overlay>>,
}

#[derive(Clone, Copy)]
enum Command { Copy, CopyMarkdown, Pin, Favorite, Rename, Reset, Replace, Mute, Close }

impl BrowserWindow {
    pub(crate) fn open_tab_menu(&mut self, tab: TabId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.browser.favorites.iter().chain(&self.browser.space().pinned).chain(&self.browser.space().today).any(|entry| entry.id == tab) { return; }
        self.close_tab_menu(cx);
        self.close_command_bar(cx);
        let focus = cx.focus_handle();
        self.tab_menu = Some(TabMenu { tab, selected: 0, position, focus: focus.clone(), overlay: None });
        let origin = window.bounds().origin;
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.notify();
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.tab_menu.as_ref().is_some_and(|menu| menu.focus == focus)).unwrap_or(false) { return; }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            let _ = browser.update(cx, |this, _| {
                let height = px(this.tab_menu_commands().len() as f32 * 32.0 + 8.0);
                if let Some(menu) = &mut this.tab_menu {
                    let at = position + (origin - bounds.origin);
                    menu.position = point(at.x.clamp(px(8.), (bounds.size.width - px(WIDTH + 8.)).max(px(8.))), at.y.clamp(px(8.), (bounds.size.height - height - px(8.)).max(px(8.))));
                }
            });
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                focus.focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) { crate::native::attach_overlay(parent, child, false); }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::TabMenu, cx))
            }) {
                let _ = browser.update(cx, |this, _| { if let Some(menu) = &mut this.tab_menu { menu.overlay = Some(overlay); } });
            }
        });
    }

    pub(crate) fn close_tab_menu(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.tab_menu.take() else { return; };
        let main = self.main_window;
        let focus = self.focus_handle.clone();
        let pages = self.pages.clone();
        cx.defer(move |cx| {
            if let Some(overlay) = menu.overlay { let _ = overlay.update(cx, |_, window, _| window.remove_window()); }
            let _ = main.update(cx, |_, window, _| { window.activate_window(); focus.focus(window); });
            if let Some(pages) = pages { pages.borrow_mut().focus_shown(); }
        });
        cx.notify();
    }

    fn tab_menu_commands(&self) -> Vec<(&'static str, Command)> {
        let Some(menu) = &self.tab_menu else { return Vec::new(); };
        let Some(tab) = self.browser.favorites.iter().chain(&self.browser.space().pinned).chain(&self.browser.space().today).find(|tab| tab.id == menu.tab) else { return Vec::new(); };
        let favorite = self.browser.is_favorite(tab.id);
        let pinned = tab.pinned_url.is_some();
        let mut rows = vec![("Copy Link", Command::Copy), ("Copy Link as Markdown", Command::CopyMarkdown), ("Rename…", Command::Rename)];
        if !favorite { rows.push((if pinned { "Unpin Tab" } else { "Pin Tab" }, Command::Pin)); }
        rows.push((if favorite { "Unfavorite Tab" } else { "Favorite Tab" }, Command::Favorite));
        if tab.pinned_url.as_ref().is_some_and(|url| *url != tab.url) {
            rows.push(("Reset Tab to Original URL", Command::Reset));
            rows.push(("Replace Pinned URL with Current", Command::Replace));
        }
        if !browser_core::omnibox::is_internal_url(&tab.url) { rows.push((if tab.muted { "Unmute Tab" } else { "Mute Tab" }, Command::Mute)); }
        rows.push((if pinned { "Close Tab" } else { "Archive Tab" }, Command::Close));
        rows
    }

    fn select_tab_menu(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.tab_menu_commands().len();
        if let Some(menu) = &mut self.tab_menu {
            if count > 0 { menu.selected = (menu.selected as isize + delta).rem_euclid(count as isize) as usize; }
        }
        cx.notify();
    }

    fn confirm_tab_menu(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.tab_menu.as_ref().map(|menu| menu.tab) else { return; };
        let Some((_, command)) = self.tab_menu_commands().get(index).copied() else { self.close_tab_menu(cx); return; };
        self.close_tab_menu(cx);
        match command {
            Command::Copy => self.copy_tab_url(id, false, cx),
            Command::CopyMarkdown => self.copy_tab_url(id, true, cx),
            Command::Pin => self.browser.toggle_pin(id),
            Command::Favorite => {
                let active = self.browser.space().active_tab;
                if self.browser.is_favorite(id) { self.browser.unfavorite_tab(id); }
                else if !self.browser.favorite_tab(id) { self.show_toast("You can have up to 12 Favorites".into(), cx); }
                self.browser.spaces[self.browser.active_space].active_tab = active;
            }
            Command::Rename => {
                let name = self.browser.item_title(id);
                self.open_command_bar(Target::RenameTab(id), &name, window, cx);
            }
            Command::Reset => {
                if self.browser.reset_pinned_tab(id) {
                    if let Some(pages) = &self.pages { let mut pages = pages.borrow_mut(); for id in self.browser.related_tabs(id) { pages.discard(id); } }
                }
            }
            Command::Replace => { self.browser.replace_pinned_url(id); }
            Command::Mute => {
                let muted = self.browser.all_tabs().find(|tab| tab.id == id).map(|tab| !tab.muted);
                if let Some(muted) = muted { self.set_tab_muted(id, muted, cx); }
            }
            Command::Close => self.close_tab(id, cx),
        }
        cx.notify();
    }

    pub(crate) fn render_tab_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.tab_menu else { return div().into_any_element(); };
        let rows = self.tab_menu_commands().into_iter().enumerate().map(|(index, (label, _))| {
            div().id(("tab-menu-row", index)).debug_selector(move || format!("tab-menu-row-{index}"))
                .h(px(32.)).px_3().flex().items_center().rounded_md()
                .when(menu.selected == index, |this| this.bg(hsla(0.6, 0.7, 0.5, 1.)).text_color(gpui::white()))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered { if let Some(menu) = &mut this.tab_menu { menu.selected = index; } cx.notify(); }
                }))
                .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); this.confirm_tab_menu(index, window, cx); }))
                .child(label)
        });
        div().id("tab-menu-backdrop").debug_selector(|| "tab-menu-backdrop".into()).size_full().key_context(CONTEXT).track_focus(&menu.focus)
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, _, _, cx| this.close_tab_menu(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_tab_menu(cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.select_tab_menu(1, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.select_tab_menu(-1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                let index = this.tab_menu.as_ref().map_or(0, |menu| menu.selected);
                this.confirm_tab_menu(index, window, cx);
            }))
            .on_click(cx.listener(|this, _, _, cx| this.close_tab_menu(cx)))
            .child(div().id("tab-context-menu").debug_selector(|| "tab-context-menu".into())
                .absolute().left(menu.position.x).top(menu.position.y).w(px(WIDTH)).p_1()
                .rounded_lg().bg(gpui::white()).shadow_lg().border_1().border_color(hsla(0., 0., 0., 0.15))
                .flex().flex_col().text_sm().text_color(gpui::black()).children(rows)
                .on_click(|_, _, cx| cx.stop_propagation()))
            .into_any_element()
    }
}
