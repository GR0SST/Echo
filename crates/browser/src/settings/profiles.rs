//! Profile management sheets captured read-only in Arc Settings → Profiles.
use super::*;
use crate::text_input::{Changed, TextInput};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Rename,
    Delete,
}
pub(super) struct Editor {
    id: UserProfileId,
    name: String,
    operation: Operation,
    input: Entity<TextInput>,
    cancel: FocusHandle,
    accept: FocusHandle,
    _changed: Subscription,
}
gpui::actions!(profile_settings, [Accept, Cancel, Next, Previous]);
pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", Accept, Some("ProfileSettingsEditor")),
        gpui::KeyBinding::new("escape", Cancel, Some("ProfileSettingsEditor")),
        gpui::KeyBinding::new("tab", Next, Some("ProfileSettingsEditor")),
        gpui::KeyBinding::new("shift-tab", Previous, Some("ProfileSettingsEditor")),
        gpui::KeyBinding::new("space", Accept, Some("ProfileSettingsButton")),
    ]);
}
impl SettingsWindow {
    #[cfg(test)]
    pub(crate) fn profile_editor_open(&self) -> bool {
        self.profile_editor.is_some()
    }
    #[cfg(test)]
    pub(crate) fn editor_debug(&self, window: &Window, cx: &App) -> (String, bool, bool) {
        let e = self.profile_editor.as_ref().unwrap();
        (
            e.input.read(cx).text().to_owned(),
            e.input.read(cx).focus_handle(cx).is_focused(window),
            e.cancel.is_focused(window),
        )
    }

    pub(super) fn begin_profile_editor(
        &mut self,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let browser = self.profile.settings_snapshot();
        let Some(profile) = browser.user_profile(self.selected_profile) else {
            return;
        };
        if operation == Operation::Delete
            && (profile.id() == browser.default_user_profile_id()
                || browser
                    .spaces
                    .iter()
                    .any(|space| space.user_profile_id == profile.id()))
        {
            return;
        }
        let id = profile.id();
        let name = profile.name().to_owned();
        let input = cx.new(|cx| {
            TextInput::new(
                if operation == Operation::Delete {
                    name.clone()
                } else {
                    String::new()
                },
                cx,
            )
        });
        if operation == Operation::Rename {
            input.update(cx, |input, cx| {
                input.set_text(name.clone(), cx);
                input.select_all_text(cx);
            });
        }
        let changed = cx.subscribe(&input, |_, _, _: &Changed, cx| cx.notify());
        self.popup = None;
        self.previews = false;
        self.profile_editor = Some(Editor {
            id,
            name,
            operation,
            input: input.clone(),
            cancel: cx.focus_handle(),
            accept: cx.focus_handle(),
            _changed: changed,
        });
        input.read(cx).focus_handle(cx).focus(window);
        cx.on_next_frame(window, move |this, window, cx| {
            if this
                .profile_editor
                .as_ref()
                .is_some_and(|editor| editor.input == input)
            {
                input.read(cx).focus_handle(cx).focus(window);
                cx.notify();
            }
        });
        cx.notify();
    }
    pub(super) fn validate_profile_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let invalid = self.profile_editor.as_ref().is_some_and(|editor| {
            self.profile.read_settings(|browser| {
                !browser
                    .user_profile(editor.id)
                    .is_some_and(|profile| profile.name() == editor.name)
                    || (editor.operation == Operation::Delete
                        && (editor.id == browser.default_user_profile_id()
                            || browser
                                .spaces
                                .iter()
                                .any(|space| space.user_profile_id == editor.id)))
            })
        });
        if invalid {
            self.cancel_profile_editor(window, cx);
        }
    }
    fn editor_enabled(&self, cx: &App) -> bool {
        self.profile_editor
            .as_ref()
            .is_some_and(|editor| match editor.operation {
                Operation::Rename => !editor.input.read(cx).text().trim().is_empty(),
                Operation::Delete => editor.input.read(cx).text() == editor.name,
            })
    }
    pub(super) fn cancel_profile_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.profile_editor = None;
        self.focus.focus(window);
        cx.notify();
    }
    pub(super) fn accept_profile_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.validate_profile_editor(window, cx);
        let Some(editor) = &self.profile_editor else {
            return;
        };
        if editor.cancel.is_focused(window) {
            self.cancel_profile_editor(window, cx);
            return;
        }
        if !self.editor_enabled(cx) {
            return;
        }
        let (id, name, text, operation) = (
            editor.id,
            editor.name.clone(),
            editor.input.read(cx).text().to_owned(),
            editor.operation,
        );
        let applied = self.profile.edit_settings(|browser| {
            if !browser
                .user_profile(id)
                .is_some_and(|profile| profile.name() == name)
            {
                return false;
            }
            match operation {
                Operation::Rename => browser.rename_user_profile(id, &text).is_ok(),
                Operation::Delete => text == name && browser.delete_user_profile(id).is_ok(),
            }
        });
        if applied {
            if operation == Operation::Delete {
                self.selected_profile = self.profile.default_user_profile_id();
            }
            self.cancel_profile_editor(window, cx);
        }
    }
    pub(super) fn move_profile_editor_focus(&self, previous: bool, window: &mut Window, cx: &App) {
        let Some(editor) = &self.profile_editor else {
            return;
        };
        let mut stops = vec![
            editor.input.read(cx).focus_handle(cx),
            editor.cancel.clone(),
        ];
        if self.editor_enabled(cx) {
            stops.push(editor.accept.clone());
        }
        let index = stops
            .iter()
            .position(|focus| focus.is_focused(window))
            .unwrap_or(0);
        stops[if previous {
            (index + stops.len() - 1) % stops.len()
        } else {
            (index + 1) % stops.len()
        }]
        .focus(window);
    }
    fn editor_button(
        &self,
        primary: bool,
        enabled: bool,
        editor: &Editor,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let deleting = editor.operation == Operation::Delete;
        div()
            .id(if primary {
                "profile-settings-accept"
            } else {
                "profile-settings-cancel"
            })
            .debug_selector(move || {
                if primary {
                    "settings-profile-accept".into()
                } else {
                    "settings-profile-cancel".into()
                }
            })
            .key_context("ProfileSettingsButton")
            .track_focus(if primary {
                &editor.accept
            } else {
                &editor.cancel
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.prevent_default()
            })
            .w(px(if primary {
                if deleting {
                    99.
                } else {
                    89.
                }
            } else {
                110.
            }))
            .h(px(36.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if primary {
                if deleting {
                    0xb45539
                } else {
                    0x5d61fc
                }
            } else {
                0x676099
            }))
            .bg(rgb(if primary {
                if deleting {
                    0xc65f3f
                } else {
                    0x3339f1
                }
            } else {
                0x383456
            }))
            .text_color(rgb(0xffffff))
            .text_size(px(13.))
            .line_height(px(16.))
            .font_weight(gpui::FontWeight::MEDIUM)
            .when(!enabled, |s| s.opacity(0.35))
            .when(enabled, |s| s.cursor_pointer())
            .child(if primary {
                if deleting {
                    "Delete"
                } else {
                    "Save"
                }
            } else {
                "Cancel"
            })
            .child(
                div()
                    .w(px(if primary { 28. } else { 35. }))
                    .h(px(22.))
                    .rounded(px(4.))
                    .bg(gpui::rgba(0xffffff20))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(if primary {
                        icons::control(Icon::Enter)
                    } else {
                        div().child("ESC").into_any_element()
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if !enabled {
                    return;
                }
                if primary {
                    this.accept_profile_editor(window, cx);
                } else {
                    this.cancel_profile_editor(window, cx);
                }
            }))
    }
    pub(super) fn profile_editor_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = &self.profile_editor else {
            return div().into_any_element();
        };
        let deleting = editor.operation == Operation::Delete;
        let text = format!("Deleting {} will permanently remove any cookies, history, preferences, and logins that belong to this Profile.", editor.name);
        let warning = gpui::StyledText::new(text).with_highlights([(
            9..9 + editor.name.len(),
            gpui::HighlightStyle {
                font_weight: Some(gpui::FontWeight::BOLD),
                ..Default::default()
            },
        )]);
        div().absolute().inset_0().bg(gpui::rgba(0x00000046)).flex().items_center().justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().debug_selector(|| "settings-profile-sheet".into()).relative().flex_none()
                .w(px(if deleting { 450. } else { 496. })).h(px(if deleting { 350. } else { 256. }))
                .rounded(px(17.)).bg(rgb(0x161c2f)).text_color(rgb(0xffffff)).text_size(px(14.)).line_height(px(18.)).font_weight(gpui::FontWeight::NORMAL)
                .when(deleting, |s| s.child(div().absolute().left(px(38.)).top(px(38.)).size(px(64.)).flex().items_center().justify_center()
                    .child(div().size(px(56.)).rounded_full().bg(rgb(0xc43232)).flex().items_center().justify_center().child(icons::control_sized(Icon::Sidebar,36.)))
                    .child(div().absolute().right_0().bottom_0().size(px(18.)).rounded_full().bg(rgb(0xc43232)).flex().items_center().justify_center().child(icons::ionicon("trash-outline",12.,1.)))))
                .child(div().absolute().left(px(38.)).right(px(38.)).top(px(if deleting { 116. } else { 38. }))
                    .text_size(px(18.)).line_height(px(22.)).font_weight(gpui::FontWeight::BOLD).child(if deleting { "Delete this Profile" } else { "Rename your Profile" }))
                .child(div().absolute().left(px(38.)).right(px(38.)).top(px(if deleting { 146. } else { 72. })).text_color(rgb(0xb8b8c0))
                    .child(if deleting { warning.into_any_element() } else { div().child("This won’t affect the names of any of the Spaces associated with this Profile.").into_any_element() }))
                .when(deleting, |s| s.child(div().absolute().left(px(38.)).right(px(38.)).top(px(200.)).text_color(rgb(0xb8b8c0)).child("To continue, please type the name of this Profile:")))
                .child(div().id("profile-settings-input").debug_selector(|| "settings-profile-input".into()).absolute()
                    .left(px(38.)).right(px(38.)).top(px(if deleting { 224. } else { 124. })).h(px(if deleting { 34. } else { 36. }))
                    .px(px(8.)).flex().items_center().rounded(px(4.)).border_1()
                    .border_color(rgb(if deleting { 0x252c40 } else { 0x5b6070 })).bg(rgb(if deleting { 0x161c2f } else { 0x393e50 })).child(editor.input.clone()))
                .child(div().absolute().right(px(if deleting { 28. } else { 38. })).bottom(px(28.)).flex().gap(px(8.))
                    .child(self.editor_button(false,true,editor,cx)).child(self.editor_button(true,self.editor_enabled(cx),editor,cx))))
            .into_any_element()
    }
}

/// Fixed geometry for Arc's rectangle/pencil/ellipsis profile-name control.
pub(super) fn rename_control() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let at = |(x, y): (f32, f32)| bounds.origin + gpui::point(px(x), px(y));
            let mut path = gpui::PathBuilder::stroke(px(1.));
            path.move_to(at((13.5, 6.5)));
            path.line_to(at((13.5, 13.5)));
            path.line_to(at((1.5, 13.5)));
            path.line_to(at((1.5, 6.5)));
            path.line_to(at((10., 6.5)));
            path.move_to(at((11., 8.)));
            path.line_to(at((16.5, 2.5)));
            for x in [4., 7., 10.] {
                path.move_to(at((x, 10.)));
                path.line_to(at((x, 11.)));
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .size(px(18.))
    .into_any_element()
}
