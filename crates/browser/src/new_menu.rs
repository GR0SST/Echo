//! The New menu opened from the plus at the right of the Space bar.
use crate::icons::Icon;
use crate::window::{BrowserWindow, Command, Overlay, OverlayKind, Target};
use gpui::{
    actions, div, hsla, point, prelude::*, px, AnyElement, App, Context, FocusHandle, KeyBinding,
    Pixels, Point, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};

const CONTEXT: &str = "NewMenu";
pub(crate) const WIDTH: f32 = 222.0;
const ROW_HEIGHT: f32 = 39.0;
const SEPARATOR_HEIGHT: f32 = 17.0;
const PADDING: f32 = 6.0;
/// Space between the menu and the plus button below it.
const ANCHOR_GAP: f32 = 6.0;
actions!(new_menu, [Dismiss, Next, Previous, Confirm]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item { Space, Folder, Boost, Split, Tab }

/// Rows in order; `true` draws a separator above the row.
const ITEMS: [(Item, bool); 5] = [(Item::Space, false), (Item::Folder, false), (Item::Boost, true), (Item::Split, true), (Item::Tab, false)];

impl Item {
    fn label(self) -> &'static str {
        match self { Self::Space => "New Space", Self::Folder => "New Folder", Self::Boost => "New Boost", Self::Split => "New Split", Self::Tab => "New Tab" }
    }
    fn selector(self) -> &'static str {
        match self { Self::Space => "new-menu-space", Self::Folder => "new-menu-folder", Self::Boost => "new-menu-boost", Self::Split => "new-menu-split", Self::Tab => "new-menu-tab" }
    }
    fn icon(self) -> Icon {
        match self { Self::Space => Icon::NewSpace, Self::Folder => Icon::Folder, Self::Boost => Icon::Boost, Self::Split => Icon::Split, Self::Tab => Icon::NewTab }
    }
    fn shortcut(self) -> Option<&'static str> {
        match self { Self::Split => Some("⌃⇧="), Self::Tab => Some("⌘T"), _ => None }
    }
}

pub(crate) struct NewMenu {
    pub selected: Option<usize>,
    position: Point<Pixels>,
    focus: FocusHandle,
    pub overlay: Option<WindowHandle<Overlay>>,
}

fn height() -> f32 {
    PADDING * 2. + ROW_HEIGHT * ITEMS.len() as f32 + SEPARATOR_HEIGHT * ITEMS.iter().filter(|(_, separated)| *separated).count() as f32
}

impl BrowserWindow {
    pub(crate) fn toggle_new_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.new_menu.is_some() { self.close_new_menu(cx); return; }
        self.close_user_profile_dialog(cx);
        self.close_tab_menu(cx);
        self.close_site_controls(cx);
        self.close_command_bar(cx);
        let anchor = self.new_menu_anchor.get().unwrap_or_default();
        let focus = cx.focus_handle();
        let origin = window.bounds().origin;
        let at = point(anchor.right() - px(WIDTH), anchor.top() - px(ANCHOR_GAP + height()));
        self.mark_new_menu_toggled();
        self.new_menu = Some(NewMenu { selected: None, position: at, focus: focus.clone(), overlay: None });
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        cx.notify();
        cx.defer(move |cx| {
            if !browser.read_with(cx, |this, _| this.new_menu.as_ref().is_some_and(|menu| menu.focus == focus)).unwrap_or(false) { return; }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            let _ = browser.update(cx, |this, _| {
                if let Some(menu) = &mut this.new_menu {
                    let at = at + (origin - bounds.origin);
                    menu.position = point(at.x.clamp(px(8.), (bounds.size.width - px(WIDTH + 8.)).max(px(8.))), at.y.max(px(8.)));
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
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::NewMenu, cx))
            }) {
                let _ = browser.update(cx, |this, _| { if let Some(menu) = &mut this.new_menu { menu.overlay = Some(overlay); } });
            }
        });
    }

    pub(crate) fn close_new_menu(&mut self, cx: &mut Context<Self>) {
        self.close_new_menu_with_focus(crate::window::PopupFocus::Restore, cx);
    }

    pub(crate) fn close_new_menu_with_focus(&mut self, focus: crate::window::PopupFocus, cx: &mut Context<Self>) {
        let Some(menu) = self.new_menu.take() else { return; };
        self.mark_new_menu_toggled();
        self.close_popup_overlay(menu.overlay, focus, false, cx);
        cx.notify();
    }

    fn select_new_menu(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.new_menu {
            let count = ITEMS.len() as isize;
            let next = match menu.selected { Some(index) => index as isize + delta, None if delta > 0 => 0, None => count - 1 };
            menu.selected = Some(next.rem_euclid(count) as usize);
        }
        cx.notify();
    }

    pub(crate) fn confirm_new_menu(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        self.close_new_menu(cx);
        match item {
            Item::Space => self.begin_space_creation(self.browser.space().id, cx),
            Item::Folder => self.open_folder_creator(None, None, window, cx),
            // Boosts are not implemented yet.
            Item::Boost => {}
            Item::Split => self.open_command_action(Command::AddSplit(None), window, cx),
            Item::Tab => self.open_command_bar(Target::NewTab, "", window, cx),
        }
    }

    pub(crate) fn render_new_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.new_menu else { return div().into_any_element(); };
        let rows = ITEMS.iter().enumerate().map(|(index, (item, separated))| {
            let item = *item;
            div().flex().flex_col()
                .when(*separated, |this| this.child(div().h(px(SEPARATOR_HEIGHT)).flex().items_center().px(px(8.))
                    .child(div().w_full().h(px(1.)).bg(hsla(0., 0., 1., 0.12)))))
                .child(div().id(item.selector()).debug_selector(move || item.selector().into())
                    .h(px(ROW_HEIGHT)).px(px(10.)).flex().items_center().gap(px(10.)).rounded(px(8.))
                    .when(menu.selected == Some(index), |this| this.bg(hsla(0., 0., 1., 0.12)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if let Some(menu) = &mut this.new_menu {
                            if *hovered { menu.selected = Some(index); } else if menu.selected == Some(index) { menu.selected = None; }
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); this.confirm_new_menu(item, window, cx); }))
                    .child(crate::icons::control(item.icon()))
                    .child(div().flex_1().text_size(px(14.)).font_weight(gpui::FontWeight::SEMIBOLD).child(item.label()))
                    .when_some(item.shortcut(), |this, shortcut| this.child(div().text_size(px(13.)).text_color(hsla(0., 0., 1., 0.45)).child(shortcut))))
        });
        div().id("new-menu-backdrop").debug_selector(|| "new-menu-backdrop".into()).size_full()
            .key_context(CONTEXT).track_focus(&menu.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_new_menu(cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.select_new_menu(1, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.select_new_menu(-1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                match this.new_menu.as_ref().and_then(|menu| menu.selected) {
                    Some(index) => this.confirm_new_menu(ITEMS[index].0, window, cx),
                    None => this.close_new_menu(cx),
                }
            }))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| this.close_new_menu(cx)))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, _, _, cx| this.close_new_menu(cx)))
            .child(crate::panel::shell("new-menu")
                .absolute().left(menu.position.x).top(menu.position.y).w(px(WIDTH)).p(px(PADDING))
                .flex().flex_col()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(rows))
            .into_any_element()
    }
}
