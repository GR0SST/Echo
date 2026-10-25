//! Settings' observed one-item footer menu. The extension editor is a separate action.
use super::*;
use gpui::{Render, WindowBackgroundAppearance, WindowHandle, WindowKind};
#[cfg(test)]
use gpui::AnyWindowHandle;

gpui::actions!(settings_shortcuts_menu, [ExtensionShortcuts]);

pub(super) struct FooterMenu {
    parent: WindowHandle<SettingsWindow>,
    focus: FocusHandle,
    colors: appearance::Colors,
    closing: bool,
}
impl SettingsWindow {
    pub(super) fn close_shortcuts_menu(&mut self, cx: &mut App) {
        self.shortcuts_menu_pending = false;
        if let Some(handle) = self.shortcuts_menu.take() {
            let _ = handle.update(cx, |menu, window, _| {
                menu.closing = true;
                window.remove_window();
            });
        }
    }
    pub(super) fn toggle_shortcuts_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcuts_menu.is_some() || self.shortcuts_menu_pending {
            self.close_shortcuts_menu(cx);
            return;
        }
        if crate::quit::is_quitting() { return; }
        self.shortcuts_menu_pending = true;
        let owner = cx.entity().downgrade();
        let parent = window.window_handle().downcast::<Self>().expect("Settings owns its footer");
        let native_parent = crate::native::view_of(window);
        let colors = self.colors;
        let mut bounds = window.bounds();
        // The native reference menu extends beyond the Settings window's right edge.
        bounds.size.width += px(80.);
        cx.defer(move |cx| {
            if crate::quit::is_quitting() || !owner.read_with(cx, |s, _| s.shortcuts_menu_pending && s.tab == Tab::Shortcuts).unwrap_or(false) { return; }
            let menu = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::Normal,
                focus: false,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            }, move |window, cx| {
                if let (Some(parent), Some(child)) = (native_parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window);
                    cx.observe_window_activation(window, |menu: &mut FooterMenu, window, cx| {
                        if !window.is_window_active() { menu.dismiss(window, cx, false); }
                    }).detach();
                    FooterMenu { parent, focus, colors, closing: false }
                })
            }).ok();
            let attached = owner.update(cx, |s, cx| {
                let open = s.shortcuts_menu_pending && s.tab == Tab::Shortcuts && !crate::quit::is_quitting();
                s.shortcuts_menu_pending = false;
                if open { s.shortcuts_menu = menu; cx.notify(); }
                open
            }).unwrap_or(false);
            if !attached {
                if let Some(menu) = menu {
                    let _ = menu.update(cx, |menu, window, _| { menu.closing = true; window.remove_window(); });
                }
                return;
            }
            if let Some(menu) = menu {
                let _ = menu.update(cx, |_, window, _| window.activate_window());
            }
        });
    }
    #[cfg(test)]
    pub(crate) fn shortcut_footer_window(&self) -> Option<AnyWindowHandle> {
        self.shortcuts_menu.map(Into::into)
    }
}
impl FooterMenu {
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>, restore: bool) {
        if self.closing { return; }
        self.closing = true;
        let _ = self.parent.update(cx, |parent, window, cx| {
            parent.shortcuts_menu = None;
            if restore { window.activate_window(); parent.focus.focus(window); }
            cx.notify();
        });
        window.remove_window();
    }
    fn invoke(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.is_action_available(&ExtensionShortcuts) { return; }
        self.dismiss(window, cx, true);
        cx.dispatch_action(&ExtensionShortcuts);
    }
}
impl Render for FooterMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = cx.is_action_available(&ExtensionShortcuts);
        let colors = self.colors;
        div().id("shortcut-footer-backdrop").debug_selector(|| "shortcut-footer-backdrop".into()).size_full().track_focus(&self.focus)
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|s, _, w, cx| s.dismiss(w, cx, true)))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|s, _, w, cx| s.dismiss(w, cx, true)))
            .on_key_down(cx.listener(|s, event: &gpui::KeyDownEvent, w, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => s.dismiss(w, cx, true),
                    "enter" => s.invoke(w, cx),
                    "w" if event.keystroke.modifiers.platform => {
                        let _ = s.parent.update(cx, |parent, w, _| { parent.shortcuts_menu = None; w.remove_window(); });
                        s.closing = true; w.remove_window();
                    }
                    _ => {}
                }
                cx.stop_propagation();
            }))
            .child(div().id("shortcut-footer-menu").debug_selector(|| "shortcut-footer-menu".into())
                .absolute().left(px(575.)).top(px(606.)).w(px(166.)).h(px(34.))
                .rounded(px(6.)).border_1().border_color(colors.rgb(0x636565))
                .bg(colors.rgb(0x343636)).p(px(4.)).shadow_md()
                .font_family(".SystemUIFont").text_size(px(13.)).line_height(px(16.))
                .text_color(colors.rgb(0xc4c5c5))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().id("extension-shortcuts").debug_selector(|| "extension-shortcuts".into())
                    .h(px(24.)).w_full().rounded(px(4.)).px(px(12.)).flex().items_center()
                    .when(enabled, |row| row.bg(rgb(0x087dff)).text_color(rgb(0xffffff)).cursor_pointer())
                    .when(!enabled, |row| row.opacity(0.45))
                    .on_click(cx.listener(|s, _, w, cx| s.invoke(w, cx)))
                    .child("Extension Shortcuts…")))
    }
}
