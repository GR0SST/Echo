//! The shortcut footer's context-sensitive native-reference menu.
use super::*;
#[cfg(test)]
use gpui::AnyWindowHandle;
use gpui::{Pixels, Point, Render, WindowBackgroundAppearance, WindowHandle, WindowKind};
use std::{cell::Cell, rc::Rc};

gpui::actions!(settings_shortcuts_menu, [ExtensionShortcuts]);

#[derive(Clone, Copy)]
enum MenuAction {
    Remove,
    RemoveConflicts,
    ResetAll,
    Extensions,
}
struct Entry {
    action: MenuAction,
    title: String,
    enabled: bool,
    separator: bool,
}
impl Entry {
    fn selector(&self) -> &'static str {
        match self.action {
            MenuAction::Remove => "shortcut-menu-remove",
            MenuAction::RemoveConflicts => "shortcut-menu-remove-conflicts",
            MenuAction::ResetAll => "shortcut-menu-reset-all",
            MenuAction::Extensions => "extension-shortcuts",
        }
    }
}
pub(super) struct FooterMenu {
    parent: WindowHandle<SettingsWindow>,
    focus: FocusHandle,
    colors: appearance::Colors,
    closing: bool,
    subject: Option<(String, Option<String>)>,
    conflicts: Vec<String>,
    entries: Vec<Entry>,
    selected: Option<usize>,
    position: Point<Pixels>,
    width: f32,
    height: f32,
    native: Option<*mut std::ffi::c_void>,
    alive: Rc<Cell<bool>>,
    material: Rc<Cell<bool>>,
}
impl Drop for FooterMenu {
    fn drop(&mut self) {
        self.alive.set(false);
        if let Some(view) = self.native {
            crate::native::forget_overlay(view);
        }
    }
}
impl SettingsWindow {
    pub(super) fn shortcut_more_button(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let colors = self.colors;
        div()
            .id("shortcut-footer-more")
            .debug_selector(|| "shortcut-footer-more".into())
            .size(px(24.))
            .flex_none()
            .rounded_full()
            .bg(colors.rgb(0x393b3d))
            .flex()
            .items_center()
            .justify_center()
            .hover(move |s| s.bg(colors.rgb(0x484a4a)))
            .cursor_pointer()
            .child(icons::more())
            .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                this.toggle_shortcuts_menu(
                    event.position(),
                    event.mouse_position().is_some(),
                    window,
                    cx,
                )
            }))
    }
    pub(super) fn close_shortcuts_menu(&mut self, cx: &mut App) {
        self.shortcuts_menu_pending = false;
        if let Some(handle) = self.shortcuts_menu.take() {
            let _ = handle.update(cx, |menu, window, _| {
                menu.closing = true;
                menu.alive.set(false);
                window.remove_window();
            });
        }
    }
    pub(super) fn toggle_shortcuts_menu(
        &mut self,
        at: Point<Pixels>,
        pointer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shortcuts_menu.is_some() || self.shortcuts_menu_pending {
            self.close_shortcuts_menu(cx);
            return;
        }
        if crate::quit::is_quitting() {
            return;
        }
        let subject = self.shortcut_menu_subject();
        let conflicts = subject
            .as_ref()
            .map(|(id, value)| self.shortcut_conflicts(id, value.as_deref()))
            .unwrap_or_default();
        let mut entries = Vec::new();
        if let Some((id, value)) = &subject {
            let supported = self.shortcut_supported(id, cx);
            entries.push(Entry {
                action: MenuAction::Remove,
                title: "Remove Shortcut".into(),
                enabled: supported && value.is_some(),
                separator: false,
            });
            if !conflicts.is_empty() {
                entries.push(Entry {
                    action: MenuAction::RemoveConflicts,
                    title: if conflicts.len() == 1 {
                        "Remove Conflicting Shortcut"
                    } else {
                        "Remove Conflicting Shortcuts"
                    }
                    .into(),
                    enabled: supported
                        && conflicts
                            .iter()
                            .all(|other| self.shortcut_supported(other, cx)),
                    separator: true,
                });
            }
            entries.push(Entry {
                action: MenuAction::ResetAll,
                title: "Reset All Shortcuts".into(),
                enabled: true,
                separator: conflicts.is_empty(),
            });
        }
        entries.push(Entry {
            action: MenuAction::Extensions,
            title: "Extension Shortcuts…".into(),
            enabled: cx.is_action_available(&ExtensionShortcuts),
            separator: false,
        });
        let width = if conflicts.len() > 1 {
            212.
        } else if !conflicts.is_empty() {
            206.
        } else if subject.is_some() {
            164.
        } else {
            166.
        };
        let height = if subject.is_none() {
            34.
        } else {
            10. + entries.len() as f32 * 24. + 11.
        };
        let selected = (pointer && entries.first().is_some_and(|entry| entry.enabled)).then_some(0);
        let position = gpui::point(at.x, (at.y - px(5.)).max(px(0.)));
        self.shortcuts.armed = false;
        self.shortcuts_menu_pending = true;
        let owner = cx.entity().downgrade();
        let parent = window
            .window_handle()
            .downcast::<Self>()
            .expect("Settings owns its footer");
        let native_parent = crate::native::view_of(window);
        let colors = self.colors;
        let mut bounds = window.bounds();
        bounds.size.width = bounds.size.width.max(position.x + px(width + 2.));
        bounds.size.height = bounds.size.height.max(position.y + px(height + 2.));
        cx.defer(move |cx| {
            if crate::quit::is_quitting()
                || !owner
                    .read_with(cx, |s, _| {
                        s.shortcuts_menu_pending && s.tab == Tab::Shortcuts
                    })
                    .unwrap_or(false)
            {
                return;
            }
            let menu = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: None,
                        kind: WindowKind::PopUp,
                        focus: false,
                        is_movable: false,
                        is_resizable: false,
                        is_minimizable: false,
                        window_background: WindowBackgroundAppearance::Transparent,
                        ..Default::default()
                    },
                    move |window, cx| {
                        let native = crate::native::view_of(window);
                        if let (Some(parent), Some(child)) = (native_parent, native) {
                            crate::native::attach_overlay(parent, child, false);
                        }
                        cx.new(|cx| {
                            let focus = cx.focus_handle();
                            focus.focus(window);
                            cx.observe_window_activation(
                                window,
                                |menu: &mut FooterMenu, window, cx| {
                                    if !window.is_window_active() {
                                        menu.dismiss(window, cx, false);
                                    }
                                },
                            )
                            .detach();
                            FooterMenu {
                                parent,
                                focus,
                                colors,
                                closing: false,
                                subject,
                                conflicts,
                                entries,
                                selected,
                                position,
                                width,
                                height,
                                native,
                                alive: Rc::new(Cell::new(true)),
                                material: Rc::new(Cell::new(false)),
                            }
                        })
                    },
                )
                .ok();
            let attached = owner
                .update(cx, |s, cx| {
                    let open = s.shortcuts_menu_pending
                        && s.tab == Tab::Shortcuts
                        && !crate::quit::is_quitting();
                    s.shortcuts_menu_pending = false;
                    if open {
                        s.shortcuts_menu = menu;
                        cx.notify();
                    }
                    open
                })
                .unwrap_or(false);
            if !attached {
                if let Some(menu) = menu {
                    let _ = menu.update(cx, |menu, window, _| {
                        menu.closing = true;
                        menu.alive.set(false);
                        window.remove_window();
                    });
                }
                return;
            }
            if let Some(menu) = menu {
                let _ = menu.update(cx, |view, window, cx| {
                    window.activate_window();
                    view.focus.focus(window);
                    cx.on_next_frame(window, |view: &mut FooterMenu, window, _| {
                        window.activate_window();
                        view.focus.focus(window);
                    });
                });
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
        if self.closing {
            return;
        }
        self.closing = true;
        self.alive.set(false);
        let _ = self.parent.update(cx, |parent, window, cx| {
            parent.shortcuts_menu = None;
            if restore {
                window.activate_window();
                parent.focus.focus(window);
            }
            cx.notify();
        });
        window.remove_window();
    }
    fn hover_at(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        let mut selected = None;
        if point.x >= self.position.x && point.x < self.position.x + px(self.width) {
            let mut y = self.position.y + px(5.);
            for (index, entry) in self.entries.iter().enumerate() {
                if entry.separator {
                    y += px(11.);
                }
                if point.y >= y && point.y < y + px(24.) && entry.enabled {
                    selected = Some(index);
                    break;
                }
                y += px(24.);
            }
        }
        if self.selected != selected {
            self.selected = selected;
            cx.notify();
        }
    }
    fn step(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let count = self.entries.len();
        let start = self
            .selected
            .unwrap_or(if backwards { 0 } else { count - 1 });
        self.selected = (1..=count)
            .map(|n| {
                if backwards {
                    (start + count - n) % count
                } else {
                    (start + n) % count
                }
            })
            .find(|i| self.entries[*i].enabled);
        cx.notify();
    }
    fn invoke(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(index).filter(|entry| entry.enabled) else {
            return;
        };
        let action = entry.action;
        let subject = self.subject.clone();
        let conflicts = self.conflicts.clone();
        self.dismiss(window, cx, true);
        if matches!(action, MenuAction::Extensions) {
            cx.dispatch_action(&ExtensionShortcuts);
            return;
        }
        let _ = self.parent.update(cx, |parent, _, cx| match action {
            MenuAction::Remove => {
                if subject.is_some() && parent.shortcut_menu_subject() == subject {
                    parent.edit_shortcut(&subject.as_ref().unwrap().0, None, cx);
                }
            }
            MenuAction::RemoveConflicts => {
                if let Some(subject) = subject {
                    parent.remove_shortcut_conflicts(&subject, &conflicts, cx);
                }
            }
            MenuAction::ResetAll => parent.reset_shortcut(None, cx),
            MenuAction::Extensions => unreachable!(),
        });
    }
}
impl Render for FooterMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let selected_menu = self.subject.is_some();
        let menu = if selected_menu {
            crate::panel::menu_shell_at_appearance(
                "shortcut-footer-menu",
                "shortcut-menu",
                250. / 360.,
                self.material.clone(),
                None,
                self.alive.clone(),
                !colors.light,
            )
            .py(px(5.))
            .px(px(5.))
            .child(
                div()
                    .absolute()
                    .inset(px(-1.))
                    .rounded(px(13.))
                    .border_1()
                    .border_color(gpui::rgba(0x000000cc)),
            )
        } else {
            div()
                .id("shortcut-footer-menu")
                .rounded(px(6.))
                .border_1()
                .border_color(colors.rgb(0x636565))
                .bg(colors.rgb(0x343636))
                .p(px(4.))
                .shadow_md()
        };
        let entries = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let selector = entry.selector();
                let enabled = entry.enabled;
                let highlighted = self.selected == Some(index);
                div()
                    .flex_none()
                    .when(entry.separator, |row| {
                        row.child(div().h(px(11.)).px(px(12.)).flex().items_center().child(
                            div().w_full().h(px(1.)).bg(gpui::hsla(
                                0.,
                                0.,
                                if colors.light { 0. } else { 1. },
                                0.18,
                            )),
                        ))
                    })
                    .child(
                        div()
                            .id(selector)
                            .debug_selector(move || selector.into())
                            .h(px(24.))
                            .w_full()
                            .rounded(px(if selected_menu { 6. } else { 4. }))
                            .px(px(if selected_menu { 11. } else { 12. }))
                            .flex()
                            .items_center()
                            .when(!enabled, |row| row.opacity(0.45))
                            .when(highlighted && enabled, |row| {
                                row.bg(rgb(0x087dff)).text_color(rgb(0xffffff))
                            })
                            .when(enabled, |row| row.cursor_pointer())
                            .on_hover(cx.listener(move |menu, over: &bool, _, cx| {
                                if *over && enabled {
                                    menu.selected = Some(index);
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |menu, _, window, cx| {
                                menu.invoke(index, window, cx)
                            }))
                            .child(entry.title.clone()),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("shortcut-footer-backdrop")
            .debug_selector(|| "shortcut-footer-backdrop".into())
            .size_full()
            .track_focus(&self.focus)
            .on_mouse_move(cx.listener(|menu, event: &gpui::MouseMoveEvent, _, cx| {
                menu.hover_at(event.position, cx)
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|s, _, w, cx| s.dismiss(w, cx, true)),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|s, _, w, cx| s.dismiss(w, cx, true)),
            )
            .on_key_down(cx.listener(|s, event: &gpui::KeyDownEvent, w, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => s.dismiss(w, cx, true),
                    "down" => s.step(false, cx),
                    "up" => s.step(true, cx),
                    "enter" => {
                        if let Some(index) = s.selected {
                            s.invoke(index, w, cx);
                        }
                    }
                    "w" if event.keystroke.modifiers.platform => {
                        let _ = s.parent.update(cx, |parent, w, _| {
                            parent.shortcuts_menu = None;
                            w.remove_window();
                        });
                        s.closing = true;
                        s.alive.set(false);
                        w.remove_window();
                    }
                    _ => {}
                }
                cx.stop_propagation();
            }))
            .child(
                menu.debug_selector(|| "shortcut-footer-menu".into())
                    .absolute()
                    .left(self.position.x)
                    .top(self.position.y)
                    .w(px(self.width))
                    .h(px(self.height))
                    .font_family(".SystemUIFont")
                    .font_weight(if selected_menu {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_size(px(13.))
                    .line_height(px(16.))
                    .text_color(colors.rgb(if selected_menu { 0xdfdfdf } else { 0xc4c5c5 }))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(entries),
            )
    }
}
