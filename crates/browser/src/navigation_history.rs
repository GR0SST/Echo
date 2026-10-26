//! Back/forward popovers select retained entries, never copied URLs.
use crate::{
    pages::PageIdentity,
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use gpui::{
    actions, canvas, div, point, prelude::*, px, AnyElement, App, Bounds, Context, FocusHandle,
    KeyBinding, Pixels, Point, Task, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowOptions,
};
use std::time::Duration;

const CONTEXT: &str = "NavigationHistory";
actions!(navigation_history, [Dismiss, Next, Previous, Confirm]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
    ]);
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scope {
    identity: PageIdentity,
    epoch: u64,
}
pub(crate) struct HistoryMenu {
    scope: Scope,
    history: engine::NavigationHistory,
    entries: Vec<engine::NavigationEntry>,
    selected: Option<usize>,
    position: Point<Pixels>,
    arrow_x: Pixels,
    max_height: Pixels,
    focus: FocusHandle,
    pub overlay: Option<WindowHandle<Overlay>>,
}
#[derive(Default)]
pub(crate) struct NavigationHistory {
    pub menu: Option<HistoryMenu>,
    pending: Option<(Scope, Task<()>)>,
    generation: u64,
}
impl BrowserWindow {
    fn history_scope(&self) -> Option<Scope> {
        if !self.native_page_content_visible() {
            return None;
        }
        let tab = self.browser.active_tab()?.id;
        let pages = self.pages.as_ref()?.try_borrow().ok()?;
        let identity = pages.identity_for_tab(tab)?;
        if identity.profile != self.browser.active_user_profile_id() {
            return None;
        }
        Some(Scope {
            identity,
            epoch: pages.page_document_epoch(tab)?,
        })
    }
    pub(crate) fn sync_navigation_history(&mut self, cx: &mut Context<Self>) {
        let scope = self.history_scope();
        if self
            .navigation_history
            .menu
            .as_ref()
            .is_some_and(|menu| Some(menu.scope) != scope)
            || self
                .navigation_history
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| Some(*pending) != scope)
        {
            self.close_navigation_history(PopupFocus::Restore, cx);
        }
    }
    pub(crate) fn close_navigation_history(&mut self, policy: PopupFocus, cx: &mut Context<Self>) {
        self.navigation_history.generation = self.navigation_history.generation.wrapping_add(1);
        self.navigation_history.pending = None;
        if let Some(menu) = self.navigation_history.menu.take() {
            self.close_popup_overlay(menu.overlay, policy, false, cx);
            cx.notify();
        }
    }
    /// Each painted button owns its anchor. A floating sidebar or a Split
    /// toolbar must not overwrite the other window's coordinate system.
    pub(crate) fn navigation_history_button(
        &self,
        button: gpui::Stateful<gpui::Div>,
        forward: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let anchor: crate::site_controls::Anchor = Default::default();
        let painted = anchor.clone();
        button
            .relative()
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |b, _, window, cx| {
                    b.open_navigation_history(forward, anchor.get(), window, cx)
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, _, _| painted.set(Some(bounds)),
                )
                .absolute()
                .inset_0(),
            )
    }
    pub(crate) fn open_navigation_history(
        &mut self,
        forward: bool,
        anchor: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_profile_projection();
        self.close_navigation_history(PopupFocus::Restore, cx);
        let Some(scope) = self.history_scope() else {
            return;
        };
        let Some(mut request) = self.pages.as_ref().and_then(|pages| {
            pages
                .borrow_mut()
                .request_navigation_history(scope.identity.tab)
        }) else {
            return;
        };
        let Some(anchor) = anchor else {
            return;
        };
        let anchor = anchor.center() + window.bounds().origin;
        let generation = self.navigation_history.generation;
        let task = cx.spawn(async move |this, cx| loop {
            if let Some(result) = request.poll() {
                let _ = this.update(cx, |this, cx| {
                    this.refresh_profile_projection();
                    if this.navigation_history.generation != generation
                        || this.history_scope() != Some(scope)
                    {
                        return;
                    }
                    let Ok(history) = result else {
                        return;
                    };
                    let entries = history.directional_entries(forward);
                    if entries.is_empty() {
                        return;
                    }
                    this.close_command_bar(cx);
                    this.close_site_controls(cx);
                    this.close_tab_menu(cx);
                    this.present_navigation_history(
                        scope, history, entries, anchor, generation, cx,
                    );
                });
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        });
        self.navigation_history.pending = Some((scope, task));
    }
    fn present_navigation_history(
        &mut self,
        scope: Scope,
        history: engine::NavigationHistory,
        entries: Vec<engine::NavigationEntry>,
        anchor: Point<Pixels>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        self.navigation_history.menu = Some(HistoryMenu {
            scope,
            history,
            entries,
            selected: None,
            position: Point::default(),
            arrow_x: px(110.),
            max_height: px(560.),
            focus: focus.clone(),
            overlay: None,
        });
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.weak_entity();
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |b, _| {
                    b.navigation_history.generation == generation
                        && b.history_scope() == Some(scope)
                        && b.navigation_history.menu.is_some()
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else {
                return;
            };
            let _ = browser.update(cx, |b, _| {
                if let Some(menu) = &mut b.navigation_history.menu {
                    let center = anchor - bounds.origin;
                    let x = (center.x - px(110.))
                        .clamp(px(6.), (bounds.size.width - px(226.)).max(px(6.)));
                    menu.position = point(x, center.y + px(31.));
                    menu.arrow_x = center.x - x;
                    menu.max_height =
                        (bounds.size.height - menu.position.y - px(8.)).clamp(px(28.), px(560.));
                }
            });
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                focus.focus(window);
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                }
                cx.new(|cx| {
                    Overlay::new(browser.clone(), OverlayKind::NavigationHistory, window, cx)
                })
            }) {
                let _ = browser.update(cx, |b, cx| {
                    if let Some(menu) = &mut b.navigation_history.menu {
                        menu.overlay = Some(overlay);
                    }
                    cx.notify();
                });
            }
        });
        cx.notify();
    }
    fn choose_navigation_history(&mut self, index: usize, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if let Some(menu) = &self.navigation_history.menu {
            if self.history_scope() == Some(menu.scope) {
                if let Some(entry) = menu.entries.get(index) {
                    if let Some(pages) = &self.pages {
                        pages.borrow_mut().navigate_history_entry(
                            menu.scope.identity.tab,
                            &menu.history,
                            entry.id,
                        );
                    }
                }
            }
        }
        self.close_navigation_history(PopupFocus::Restore, cx);
    }
    pub(crate) fn render_navigation_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.navigation_history.menu else {
            return div().into_any_element();
        };
        let dark = self.theme_is_dark();
        let ink = if dark { 1. } else { 0. };
        let arrow_x = menu.arrow_x;
        let rows = menu.entries.iter().enumerate().map(|(index, entry)| {
            let selected = menu.selected == Some(index);
            div()
                .id(("navigation-history-entry", index))
                .debug_selector(move || format!("navigation-history-entry-{index}"))
                .h(px(40.))
                .flex_none()
                .pl(px(11.))
                .pr(px(9.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(5.))
                .when(selected, |r| r.bg(gpui::hsla(0., 0., ink, 0.12)))
                .on_hover(cx.listener(move |b, hovered: &bool, _, cx| {
                    if *hovered {
                        if let Some(menu) = &mut b.navigation_history.menu {
                            menu.selected = Some(index);
                        }
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |b, _, _, cx| b.choose_navigation_history(index, cx)))
                .child(
                    self.cached_favicon_image(menu.scope.identity.profile, &entry.url, dark)
                        .map(|image| {
                            gpui::img(image)
                                .size(px(14.))
                                .flex_none()
                                .object_fit(gpui::ObjectFit::Contain)
                                .rounded(px(3.))
                                .with_fallback(move || history_favicon_placeholder(ink))
                                .into_any_element()
                        })
                        .unwrap_or_else(|| history_favicon_placeholder(ink)),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .child(if entry.title.is_empty() {
                            entry.url.clone()
                        } else {
                            entry.title.clone()
                        }),
                )
        });
        let height = (menu.entries.len() as f32 * 40. + 16.).min(560.);
        let fill = gpui::rgb(if dark { 0x050627 } else { 0xfdfdff });
        let card = div()
            .id("navigation-history-menu")
            .relative()
            .rounded(px(12.))
            .bg(fill)
            .shadow_lg()
            .text_color(gpui::hsla(0., 0., ink, 1.))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .debug_selector(|| "navigation-history-menu".into())
            .absolute()
            .left(menu.position.x)
            .top(menu.position.y)
            .w(px(220.))
            .max_h(px(height).min(menu.max_height))
            .py(px(8.))
            .px(px(6.))
            .flex()
            .flex_col()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let mut path = gpui::PathBuilder::fill();
                        let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
                        path.move_to(at(0., 7.));
                        path.line_to(at(7., 0.));
                        path.line_to(at(14., 7.));
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, fill);
                        }
                    },
                )
                .absolute()
                .left(arrow_x - px(7.))
                .top(px(-6.))
                .w(px(14.))
                .h(px(7.)),
            )
            .child(
                div()
                    .id("navigation-history-scroll")
                    .overflow_y_scroll()
                    .max_h((px(height).min(menu.max_height) - px(16.)).max(px(0.)))
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .children(rows),
            );
        div()
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&menu.focus)
            .on_action(cx.listener(|b, _: &Dismiss, _, cx| {
                b.close_navigation_history(PopupFocus::Restore, cx)
            }))
            .on_action(cx.listener(|b, _: &Next, _, cx| {
                if let Some(menu) = &mut b.navigation_history.menu {
                    menu.selected = Some(menu.selected.map_or(0, |i| (i + 1) % menu.entries.len()));
                }
                cx.notify();
            }))
            .on_action(cx.listener(|b, _: &Previous, _, cx| {
                if let Some(menu) = &mut b.navigation_history.menu {
                    menu.selected = Some(menu.selected.map_or(menu.entries.len() - 1, |i| {
                        (i + menu.entries.len() - 1) % menu.entries.len()
                    }));
                }
                cx.notify();
            }))
            .on_action(cx.listener(|b, _: &Confirm, _, cx| {
                if let Some(i) = b.navigation_history.menu.as_ref().and_then(|m| m.selected) {
                    b.choose_navigation_history(i, cx);
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|b, _, _, cx| b.close_navigation_history(PopupFocus::Restore, cx)),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|b, _, _, cx| b.close_navigation_history(PopupFocus::Restore, cx)),
            )
            .child(card)
            .into_any_element()
    }
}

fn history_favicon_placeholder(ink: f32) -> AnyElement {
    div()
        .size(px(14.))
        .flex_none()
        .rounded(px(3.))
        .bg(gpui::hsla(0., 0., ink, 0.25))
        .into_any_element()
}
