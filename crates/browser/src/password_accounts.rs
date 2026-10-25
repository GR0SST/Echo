//! Origin/profile-bound saved accounts, positioned under the requesting field.
use crate::{
    pages::PageIdentity,
    password_flow,
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use engine::{LoginCredential, PasswordToken};
use gpui::{
    AnyElement, Bounds, Context, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, div, point, prelude::*, px, rgb, size,
};

pub(crate) struct Accounts {
    pub identity: PageIdentity,
    pub token: PasswordToken,
    pub field: u32,
    pub origin: String,
    pub rows: Vec<LoginCredential>,
    pub selected: Option<usize>,
    pub revision: u64,
    pub overlay: Option<WindowHandle<Overlay>>,
    pub generation: u64,
    pub third_party: bool,
}
fn popup_bounds(
    field: engine::Bounds,
    viewport: gpui::Size<gpui::Pixels>,
    count: usize,
    third_party: bool,
) -> Bounds<gpui::Pixels> {
    let width = if third_party { 233. } else { 225. };
    let height = 8. + 56. * count.min(6) as f32 + 45. + 6.;
    let x = (field.x + field.width / 2. - 25.5) as f32;
    let below = (field.y + field.height + 1.) as f32;
    Bounds::new(
        point(
            px(x.clamp(0., (f32::from(viewport.width) - width).max(0.))),
            px(if below + height <= f32::from(viewport.height) {
                below
            } else {
                (field.y as f32 - height).max(0.)
            }),
        ),
        size(px(width), px(height)),
    )
}
impl BrowserWindow {
    pub(crate) fn password_accounts_activation(&self, active: bool, cx: &mut Context<Self>) {
        if active {
            return;
        }
        let Some(child) = self.passwords.accounts.as_ref().and_then(|a| a.overlay) else {
            return;
        };
        let main = self.main_window;
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let parent_active = main
                .update(cx, |_, w, _| w.is_window_active())
                .unwrap_or(false);
            let picker_active = child
                .update(cx, |_, w, _| w.is_window_active())
                .unwrap_or(false);
            if !parent_active && !picker_active {
                let _ = owner.update(cx, |this, cx| this.close_password_accounts(cx));
            }
        });
    }
    pub(crate) fn close_password_accounts(&mut self, cx: &mut Context<Self>) {
        self.close_password_accounts_with_focus(PopupFocus::OnActivation, cx);
    }
    fn close_password_accounts_with_focus(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        self.passwords.lookup_generation = self.passwords.lookup_generation.wrapping_add(1);
        if let Some(accounts) = self.passwords.accounts.take() {
            if let Some(pages) = &self.pages {
                pages
                    .borrow_mut()
                    .password_selection_active(accounts.identity, false);
            }
            self.close_popup_overlay(accounts.overlay, focus, false, cx);
            cx.notify();
        }
    }
    pub(crate) fn show_password_accounts(
        &mut self,
        identity: PageIdentity,
        token: PasswordToken,
        origin: String,
        rows: Vec<LoginCredential>,
        anchor: [f64; 4],
        field_request: u32,
        cx: &mut Context<Self>,
    ) {
        if rows.is_empty() {
            self.close_password_accounts(cx);
            return;
        }
        if !self.password_owner(identity)
            || !self.password_document(identity, token, &origin)
            || self.passwords.prompt.is_some()
            || self.command_bar.is_some()
            || self.setup.is_some()
            || self.user_profile_dialog.is_some()
        {
            return;
        }
        let Some(field) = self
            .pages
            .as_ref()
            .and_then(|p| p.borrow().password_anchor(identity, anchor))
        else {
            return;
        };
        self.close_password_accounts(cx);
        self.passwords.serial = self.passwords.serial.wrapping_add(1);
        let generation = self.passwords.serial;
        let revision = password_flow::revision(&password_flow::profile_key(identity), cx);
        let third_party = self.password_cross_origin(identity, &origin);
        self.passwords.accounts = Some(Accounts {
            identity,
            token,
            field: field_request,
            origin,
            rows,
            selected: None,
            revision,
            overlay: None,
            generation,
            third_party,
        });
        let owner = cx.weak_entity();
        let main = self.main_window;
        let parent = self.native_view;
        cx.defer(move |cx| {
            let Ok((root, active)) = main.update(cx, |_, w, _| (w.bounds(), w.is_window_active()))
            else {
                return;
            };
            if !active {
                let _ = owner.update(cx, |this, cx| this.close_password_accounts(cx));
                return;
            }
            let count = owner
                .read_with(cx, |this, _| {
                    this.passwords
                        .accounts
                        .as_ref()
                        .filter(|a| a.generation == generation)
                        .map(|a| (a.rows.len(), a.third_party))
                })
                .ok()
                .flatten();
            let Some((count, third_party)) = count else {
                return;
            };
            let region = popup_bounds(field, root.size, count, third_party);
            let bounds = Bounds::new(root.origin + region.origin, region.size);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            let Ok(child) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    crate::native::prepare_toast_window(child);
                }
                cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::PasswordAccounts, window, cx))
            }) else {
                let _ = owner.update(cx, |this, cx| this.close_password_accounts(cx));
                return;
            };
            let attached = owner
                .update(cx, |this, _| {
                    if let Some(a) = this
                        .passwords
                        .accounts
                        .as_mut()
                        .filter(|a| a.generation == generation)
                    {
                        a.overlay = Some(child);
                        if let Some(pages) = &this.pages {
                            pages.borrow_mut().password_selection_active(identity, true);
                        }
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if !attached {
                let _ = child.update(cx, |_, w, _| w.remove_window());
            }
        });
        cx.notify();
    }
    pub(crate) fn password_accounts_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(a) = self.passwords.accounts.as_mut() else {
            return false;
        };
        match key {
            "escape" => self.close_password_accounts(cx),
            "down" | "up" => {
                let n = a.rows.len() + 1;
                a.selected = Some(match (a.selected, key) {
                    (None, "up") => n - 1,
                    (None, _) => 0,
                    (Some(i), "up") => (i + n - 1) % n,
                    (Some(i), _) => (i + 1) % n,
                });
                cx.notify();
            }
            "enter" => {
                let i = a.selected.unwrap_or(0);
                let generation = a.generation;
                self.choose_password_account(i, generation, window, cx);
            }
            _ => return false,
        }
        true
    }
    fn choose_password_account(
        &mut self,
        index: usize,
        generation: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.passwords.accounts.as_ref() else {
            return;
        };
        if a.generation != generation {
            return;
        }
        let valid = self.password_owner(a.identity)
            && self.password_document(a.identity, a.token, &a.origin)
            && password_flow::revision(&password_flow::profile_key(a.identity), cx) == a.revision;
        let identity = a.identity;
        let token = a.token;
        let field = a.field;
        let origin = a.origin.clone();
        let credential = a.rows.get(index).cloned();
        let manage = index == a.rows.len();
        self.close_password_accounts_with_focus(PopupFocus::Restore, cx);
        if !valid {
            return;
        }
        if let Some(credential) = credential {
            if let Some(pages) = &self.pages {
                pages
                    .borrow_mut()
                    .select_password(identity, token, &origin, &credential, field);
            }
        } else if manage {
            let main = self.main_window;
            let owner = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = main.update(cx, |_, w, cx| {
                    owner.update(cx, |this, cx| {
                        this.open_password_manager(identity.profile, w, cx)
                    })
                });
            });
        }
    }
    pub(crate) fn render_password_accounts(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(a) = self.passwords.accounts.as_ref() else {
            return div().into_any_element();
        };
        let generation = a.generation;
        let arrow = gpui::canvas(
            |_, _, _| (),
            |bounds, _, window, _| {
                let mut p = gpui::PathBuilder::fill();
                p.move_to(bounds.origin + point(px(0.), px(8.)));
                p.line_to(bounds.origin + point(px(10.), px(0.)));
                p.line_to(bounds.origin + point(px(20.), px(8.)));
                p.close();
                if let Ok(p) = p.build() {
                    window.paint_path(p, rgb(0x202020));
                }
            },
        )
        .w(px(20.))
        .h(px(8.));
        let row = |index: usize| {
            div()
                .id(("password-account", index))
                .debug_selector(move || format!("password-account-{index}").into())
                .h(px(52.))
                .flex_none()
                .px(px(10.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .gap(px(14.))
                .cursor_pointer()
                .bg(rgb(if a.selected == Some(index) {
                    0x353535
                } else {
                    0x202020
                }))
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    if *hovered {
                        if let Some(a) = this
                            .passwords
                            .accounts
                            .as_mut()
                            .filter(|a| a.generation == generation)
                        {
                            a.selected = Some(index);
                        }
                        cx.notify();
                    }
                }))
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.prevent_default()
                })
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.choose_password_account(index, generation, w, cx)
                }))
        };
        div()
            .size_full()
            .relative()
            .text_size(px(13.))
            .line_height(px(18.))
            .text_color(rgb(0xcccccc))
            .capture_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, w, cx| {
                if this.password_accounts_key(&e.keystroke.key, w, cx) {
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .child(div().absolute().left(px(14.)).top_0().child(arrow))
            .child(
                div()
                    .id("password-accounts")
                    .debug_selector(|| "password-accounts".into())
                    .absolute()
                    .left(px(4.))
                    .top(px(8.))
                    .w(px(if a.third_party { 225. } else { 217. }))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(rgb(0x202020))
                    .shadow_md()
                    .child(
                        div()
                            .max_h(px(336.))
                            .id("password-account-list")
                            .overflow_y_scroll()
                            .children(a.rows.iter().enumerate().map(|(i, r)| {
                                div().h(px(56.)).flex_none().px(px(4.)).py(px(2.)).child(
                                    row(i)
                                        .child(crate::icons::control_sized(
                                            crate::icons::Icon::Globe,
                                            16.,
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(12.))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .truncate()
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                )
                                                                .text_color(rgb(0xe8e8e8))
                                                                .child(
                                                                    r.username()
                                                                        .chars()
                                                                        .take(128)
                                                                        .map(|c| {
                                                                            if c.is_control() {
                                                                                ' '
                                                                            } else {
                                                                                c
                                                                            }
                                                                        })
                                                                        .collect::<String>(),
                                                                ),
                                                        )
                                                        .when(a.third_party, |line| {
                                                            line.child(
                                                                div()
                                                                    .flex_none()
                                                                    .font_weight(
                                                                        gpui::FontWeight::NORMAL,
                                                                    )
                                                                    .child(
                                                                        url::Url::parse(&a.origin)
                                                                            .ok()
                                                                            .and_then(|u| {
                                                                                u.host_str().map(
                                                                                    str::to_owned,
                                                                                )
                                                                            })
                                                                            .unwrap_or_default(),
                                                                    ),
                                                            )
                                                        }),
                                                )
                                                .child("••••••••••••••••"),
                                        ),
                                )
                            })),
                    )
                    .child(
                        div()
                            .h(px(45.))
                            .id("password-accounts-manage")
                            .debug_selector(|| "password-accounts-manage".into())
                            .border_t_1()
                            .border_color(rgb(0x5f5f5f))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x353535)))
                            .on_click(cx.listener(move |this, _, w, cx| {
                                let n =
                                    this.passwords.accounts.as_ref().map_or(0, |a| a.rows.len());
                                this.choose_password_account(n, generation, w, cx);
                            }))
                            .child(crate::icons::control_sized(crate::icons::Icon::Globe, 16.))
                            .child("Manage Passwords...")
                            .child(crate::icons::ionicon("key", 16., 1.)),
                    ),
            )
            .into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_popup_stays_inside_the_page_window_and_uses_arcs_measured_size() {
        let field = engine::Bounds {
            x: 296.,
            y: 191.,
            width: 165.,
            height: 35.,
        };
        let bounds = popup_bounds(field, size(px(1000.), px(760.)), 2, false);
        assert_eq!(bounds.origin, point(px(353.), px(227.)));
        assert_eq!(bounds.size, size(px(225.), px(171.)));
        let edge = popup_bounds(
            engine::Bounds {
                x: 950.,
                y: 710.,
                ..field
            },
            size(px(1000.), px(760.)),
            2,
            false,
        );
        assert_eq!(edge.origin.x, px(775.));
        assert!(edge.bottom() <= px(760.));
    }
}
