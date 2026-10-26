//! The selected/recording/conflict states inspected in Arc's Shortcuts tab.
use super::*;
use crate::shortcuts::{self as runtime, CATALOG};
pub(super) struct State {
    pub selected: Option<String>,
    pub armed: bool,
    extension: Option<(String,String,browser_core::UserProfileId)>,
    extension_error: Option<String>,
    pub priority_subject: Option<(String, Option<String>)>,
    pub focus: FocusHandle,
    _interceptor: Subscription,
}
impl State {
    pub fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let weak = cx.entity().downgrade();
        let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            let Some(view) = weak.upgrade() else {
                return;
            };
            if !view.read(cx).shortcuts.armed || !view.read(cx).shortcuts.focus.is_focused(window) {
                return;
            }
            cx.stop_propagation();
            view.update(cx, |this, cx| {
                this.record_shortcut(&event.keystroke, window, cx)
            });
        });
        Self {
            selected: None,
            armed: false,
            extension: None,
            extension_error: None,
            priority_subject: None,
            focus: cx.focus_handle(),
            _interceptor: interceptor,
        }
    }
}
impl SettingsWindow {
    pub(crate) fn recording_shortcut(&self) -> bool { self.shortcuts.armed }
    fn shortcut_entries(&self, cx: &App) -> Vec<(String, String)> {
        let mut entries = CATALOG.clone();
        self.profile.read_settings(|b| {
            entries.extend(b.spaces.iter().take(9).enumerate().map(|(index, space)| {
                (format!("Go to “{}”", space.name), format!("⌃{}", index + 1))
            }))
        });
        let settings = self.profile.settings_snapshot().settings.shortcuts;
        entries.sort_by_key(|(id, _)| (!settings.bindings.contains_key(id), id.to_lowercase()));
        let query = self.shortcut_search.read(cx).text().trim().to_lowercase();
        entries
            .into_iter()
            .filter(|(id, key)| {
                query.is_empty()
                    || format!(
                        "{id} {key} {}",
                        runtime::display(
                            settings.effective(id, runtime::default_key(key).as_deref())
                        )
                    )
                    .to_lowercase()
                    .contains(&query)
            })
            .collect()
    }
    fn shortcut_value(&self, id: &str, default: &str) -> Option<String> {
        let default = runtime::default_key(default);
        self.profile
            .settings_snapshot()
            .settings
            .shortcuts
            .effective(id, default.as_deref())
            .map(str::to_owned)
    }
    pub(super) fn shortcut_conflicts(&self, id: &str, value: Option<&str>) -> Vec<String> {
        let Some(value) = value else { return Vec::new(); };
        let mut entries = CATALOG.clone();
        self.profile.read_settings(|browser| {
            entries.extend(browser.spaces.iter().take(9).enumerate().map(|(index, space)| {
                (format!("Go to “{}”", space.name), format!("⌃{}", index + 1))
            }));
        });
        entries.iter().filter(|(other, default)| {
            other != id && self.shortcut_value(other, default).as_deref() == Some(value)
        }).map(|(id, _)| id.clone()).collect()
    }
    fn shortcut_conflict(&self, id: &str, value: Option<&str>) -> Option<String> {
        self.shortcut_conflicts(id, value).into_iter().next()
    }
    pub(super) fn shortcut_menu_subject(&self) -> Option<(String, Option<String>)> {
        let id = self.shortcuts.selected.as_ref()?;
        let (_, default) = CATALOG.iter().find(|(label, _)| label == id)?;
        Some((id.clone(), self.shortcut_value(id, default)))
    }
    pub(super) fn shortcut_supported(&self, id: &str, cx: &App) -> bool {
        CATALOG.iter().find(|(label, _)| label == id).is_some_and(|(_, default)| {
            runtime::supported(id, runtime::default_key(default).as_deref(), cx)
        })
    }
    pub(super) fn remove_shortcut_conflicts(
        &mut self, subject: &(String, Option<String>), expected: &[String], cx: &mut Context<Self>,
    ) -> bool {
        let conflicts = self.shortcut_conflicts(&subject.0, subject.1.as_deref());
        if expected.is_empty() || self.shortcut_menu_subject().as_ref() != Some(subject)
            || conflicts != expected || !self.shortcut_supported(&subject.0, cx)
            || !conflicts.iter().all(|id| self.shortcut_supported(id, cx)) {
            return false;
        }
        self.profile.edit_settings(|b| {
            for id in &conflicts { b.settings.shortcuts.set(id.clone(), None); }
        });
        runtime::apply(&self.profile.settings_snapshot().settings.shortcuts, cx);
        self.shortcuts.armed = false;
        cx.notify();
        true
    }
    pub(super) fn shortcut_priority_options(&self) -> Vec<(String, bool)> {
        let Some((id, _)) = self.shortcuts.priority_subject.as_ref() else {
            return Vec::new();
        };
        let priority = self
            .profile
            .settings_snapshot()
            .settings
            .shortcuts
            .priority(id, runtime::default_priority(id));
        priority_choices()
            .into_iter()
            .map(|(value, label, _)| (label.into(), value == priority))
            .collect()
    }
    pub(super) fn choose_shortcut_priority(&mut self, index: usize, cx: &mut Context<Self>) {
        let subject = self.shortcuts.priority_subject.take();
        let Some((priority, _, _)) = priority_choices().get(index).copied() else {
            return;
        };
        let Some(subject) = subject.filter(|subject| {
            self.shortcut_menu_subject().as_ref() == Some(subject)
                && subject.1.is_some()
                && self.shortcut_supported(&subject.0, cx)
        }) else {
            return;
        };
        self.profile
            .edit_settings(|browser| browser.settings.shortcuts.set_priority(subject.0, priority));
        runtime::apply(&self.profile.settings_snapshot().settings.shortcuts, cx);
        cx.notify();
    }
    fn ordering_profile(&self,cx:&App)->Option<UserProfileId> {
        let id=self.owner.as_ref().and_then(|owner|owner.upgrade()).map(|owner|owner.read(cx).browser.active_user_profile_id()).unwrap_or(self.selected_profile);
        self.profile.read_settings(|browser|browser.user_profile(id).map(|_|id))
    }
    fn ordering_checkbox(&self,cx:&mut Context<Self>)->AnyElement {
        let Some(profile)=self.ordering_profile(cx) else{return div().into_any_element();};
        let included=self.profile.read_settings(|browser|browser.user_profile(profile).unwrap().include_favorites_in_ordering);
        let colors=self.colors;
        div().id("shortcut-include-favorites").debug_selector(||"shortcut-include-favorites".into())
            .mt(px(8.)).h(px(20.)).flex().items_center().gap(px(6.)).text_size(px(13.)).cursor_pointer()
            .child(div().size(px(16.)).flex_none().rounded(px(4.)).bg(colors.rgb(if included {0x1684ff}else{0x555968})).text_color(rgb(0xffffff))
                .when(included,|check|check.child(icons::control_sized(Icon::Check,16.))))
            .child("Include Favorites in ordering.")
            .on_click(cx.listener(move |this,_,_,cx| {
                if this.ordering_profile(cx)!=Some(profile) {return;}
                this.profile.edit_settings(|browser| {
                    if let Some(target)=browser.user_profile_mut(profile).filter(|p|p.include_favorites_in_ordering==included) {target.include_favorites_in_ordering=!included;}
                });
                cx.notify();
            })).into_any_element()
    }
    fn shortcut_priority_panel(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let priority = self
            .profile
            .settings_snapshot()
            .settings
            .shortcuts
            .priority(id, runtime::default_priority(id));
        let (_, label, description) = priority_choices()
            .into_iter()
            .find(|(value, _, _)| *value == priority)
            .unwrap();
        div()
            .absolute()
            .top(px(168.))
            .left_0()
            .w(px(200.))
            .border_t_1()
            .border_color(self.colors.rgb(0x3e3f41))
            .pt(px(16.))
            .child(
                self.settings_button(label)
                    .id("shortcut-priority")
                    .debug_selector(|| "shortcut-priority".into())
                    .w(px(144.))
                    .px(px(12.))
                    .justify_between()
                    .child(popup_chevrons())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_shortcuts_menu(cx);
                        this.shortcuts.armed = false;
                        if this.popup == Some(Popup::ShortcutPriority) {
                            this.popup = None;
                        } else {
                            this.shortcuts.priority_subject = this.shortcut_menu_subject();
                            this.popup = Some(Popup::ShortcutPriority);
                            this.popup_selection = this
                                .shortcut_priority_options()
                                .iter()
                                .position(|(_, selected)| *selected)
                                .unwrap_or(0);
                            this.popup_keyboard = false;
                        }
                        this.focus.focus(window);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(11.))
                    .line_height(px(14.))
                    .text_color(self.colors.rgb(0x929494))
                    .child(description),
            )
            .when(id.starts_with("Go to Sidebar Item ") || id=="Go to last Sidebar Item",|panel|panel.child(self.ordering_checkbox(cx)))
            .into_any_element()
    }
    pub(super) fn edit_shortcut(&mut self, id: &str, value: Option<String>, cx: &mut Context<Self>) {
        self.profile
            .edit_settings(|browser| browser.settings.shortcuts.set(id, value));
        runtime::apply(&self.profile.settings_snapshot().settings.shortcuts, cx);
        self.shortcuts.armed = false;
        cx.notify();
    }
    pub(super) fn reset_shortcut(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        self.profile.edit_settings(|b| {
            if let Some(id) = id {
                b.settings.shortcuts.reset(id)
            } else {
                b.settings.shortcuts.bindings.clear()
            }
        });
        runtime::apply(&self.profile.settings_snapshot().settings.shortcuts, cx);
        self.shortcuts.armed = false;
        cx.notify();
    }
    #[cfg(test)]
    pub(crate) fn shortcut_query(&self, cx: &App) -> String {
        self.shortcut_search.read(cx).text().to_owned()
    }
    #[cfg(test)]
    pub(crate) fn shortcut_has_conflict(&self, id: &str) -> bool {
        let default = CATALOG.iter().find(|(label, _)| label == id).unwrap();
        self.shortcut_conflict(id, self.shortcut_value(id, &default.1).as_deref())
            .is_some()
    }
    fn record_shortcut(
        &mut self,
        key: &gpui::Keystroke,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if key.key == "escape" {
            self.shortcuts.armed = false;
            cx.notify();
            return;
        }
        if self.shortcuts.extension.is_some() {
            if matches!(key.key.as_str(),"shift"|"control"|"alt"|"cmd"|"super"|"fn"|"meta") {return;}
            let clear=matches!(key.key.as_str(),"backspace"|"delete")&&key.modifiers==gpui::Modifiers::default();
            if !clear && key.modifiers.alt && (key.modifiers.platform || key.modifiers.control) {
                self.shortcuts.extension_error=Some(if cfg!(target_os="macos") {"Option cannot be combined with Command or Control for extension shortcuts."}else{"Alt cannot be combined with Control for extension shortcuts."}.into());
                cx.notify();return;
            }
            let mut parts=Vec::new();
            if key.modifiers.platform {parts.push(if cfg!(target_os="macos"){"Command"}else{"Ctrl"}.to_owned());}
            if key.modifiers.control && !parts.iter().any(|part|part=="Ctrl") {parts.push("Ctrl".to_owned());}
            if key.modifiers.alt {parts.push("Alt".into());}
            if key.modifiers.shift {parts.push("Shift".into());}
            let token=match key.key.as_str() {
                "left"=>"Left", "right"=>"Right", "up"=>"Up", "down"=>"Down",
                "home"=>"Home", "end"=>"End", "pageup"=>"PageUp", "pagedown"=>"PageDown",
                "insert"=>"Insert", "delete"=>"Delete", "space"=>"Space", "tab"=>"Tab", ","=>"Comma", "."=>"Period", _=>"",
            };
            parts.push(if token.is_empty(){key.key.to_uppercase()}else{token.to_owned()});
            let value=if clear{String::new()}else{parts.join("+")};
            let conflict=if clear{None}else{self.shortcut_conflict("__extension",Some(&key.unparse()))};
            if let Some(conflict)=conflict{self.shortcuts.extension_error=Some(format!("Already in use by “{conflict}”"));}
            else {self.set_extension_shortcut(&value,cx);}
            cx.notify();return;
        }
        let Some(id) = self.shortcuts.selected.clone() else {
            return;
        };
        if matches!(key.key.as_str(), "backspace" | "delete")
            && key.modifiers == gpui::Modifiers::default()
        {
            self.edit_shortcut(&id, None, cx);
            return;
        }
        if matches!(
            key.key.as_str(),
            "shift" | "control" | "alt" | "cmd" | "super" | "fn" | "meta"
        ) {
            return;
        }
        let modified = key.modifiers.platform || key.modifiers.control || key.modifiers.alt;
        let function = key
            .key
            .strip_prefix('f')
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
        if modified || function {
            self.edit_shortcut(&id, Some(key.unparse()), cx);
        }
    }
    fn extension_shortcuts(&self,cx:&App)->Vec<(String,String,String,String,String,browser_core::UserProfileId)> {
        self.owner.as_ref().and_then(|owner|owner.read_with(cx,|owner,_|{
            if owner.extensions.profile!=Some(self.selected_profile){return Vec::new();}
            owner.extensions.entries.iter().flat_map(|extension|extension.commands.iter().map(|command|(
                extension.id.clone(),command.name.clone(),extension.name.clone(),command.display_description(&extension.manifest),command.shortcut.clone(),self.selected_profile))).collect()
        }).ok()).unwrap_or_default()
    }
    fn set_extension_shortcut(&mut self,value:&str,cx:&mut Context<Self>) {
        let Some((id,command,profile))=self.shortcuts.extension.clone()else{return;};
        if profile!=self.selected_profile{return;}
        let result=self.owner.as_ref().ok_or("Extension window closed".to_owned()).and_then(|owner|owner.update(cx,|owner,_|{
            if owner.extensions.profile!=Some(profile)||owner.browser.active_user_profile_id()!=profile{return Err("Extension profile changed".into());}
            owner.extensions.service.as_ref().ok_or("Extensions are unavailable".to_owned())?.set_shortcut(&id,&command,value,3)
        }).map_err(|e|e.to_string())?);
        self.shortcuts.extension_error=result.err();self.shortcuts.armed=false;cx.notify();
    }
    fn render_extension_shortcuts(&self,cx:&mut Context<Self>)->AnyElement {
        let query=self.shortcut_search.read(cx).text().trim().to_lowercase();
        let entries=self.extension_shortcuts(cx).into_iter().filter(|(_,_,name,description,key,_)|query.is_empty()||format!("Extension {name} {description} {key}").to_lowercase().contains(&query)).collect::<Vec<_>>();
        let colors=self.colors;
        div().flex().flex_col().when(!entries.is_empty(),|list|list.child(div().h(px(38.)).pt(px(12.)).text_size(px(11.)).text_color(colors.rgb(0x929494)).child("Extensions")))
            .children(entries.into_iter().map(|(id,command,name,description,key,profile)|{
                let selector=format!("extension-shortcut-{id}-{command}");
                div().id(gpui::SharedString::from(selector.clone())).debug_selector(move||selector.clone()).h(px(48.)).border_b_1().border_color(colors.rgb(0x3e3f41)).flex().items_center().gap(px(8.)).px(px(8.)).cursor_pointer()
                    .child(div().flex_1().min_w_0().flex().flex_col().child(name).child(div().text_size(px(11.)).child(description)))
                    .child(div().w(px(64.)).h(px(24.)).flex_none().rounded(px(5.)).bg(colors.rgb(0x3e3f41)).text_color(colors.rgb(0xd5d5d5)).flex().items_center().justify_center().text_size(px(12.)).child(extension_shortcut_display(&key)))
                    .on_click(cx.listener(move|this,_,window,cx|{this.shortcuts.extension=Some((id.clone(),command.clone(),profile));this.shortcuts.selected=None;this.shortcuts.extension_error=None;this.shortcuts.armed=true;this.shortcuts.focus.focus(window);cx.notify();}))
            })).into_any_element()
    }
    pub(super) fn shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let entries = self.shortcut_entries(cx);
        let settings = self.profile.settings_snapshot().settings.shortcuts;
        let selected = self
            .shortcuts
            .selected
            .as_ref()
            .and_then(|id| CATALOG.iter().find(|(label, _)| label == id))
            .cloned();
        let details = if let Some((id,command,profile)) = &self.shortcuts.extension {
            let label=self.extension_shortcuts(cx).into_iter().find(|(extension,name,_,_,_,owner)|extension==id&&name==command&&owner==profile);
            div().w(px(200.)).flex().flex_col().gap(px(14.))
                .child(div().text_size(px(24.)).child(label.as_ref().map(|(_,_,_,_,key,_)|if key.is_empty(){"Not Set".into()}else{extension_shortcut_display(key)}).unwrap_or_default()))
                .child(label.map(|(_,_,name,description,_,_)|format!("{name} — {description}")).unwrap_or_default())
                .when(self.shortcuts.armed,|panel|panel.child("Press a new shortcut"))
                .when_some(self.shortcuts.extension_error.clone(),|panel,error|panel.child(error))
                .child(self.settings_button("Clear Shortcut").id("extension-shortcut-clear").debug_selector(||"extension-shortcut-clear".into()).on_click(cx.listener(|this,_,_,cx|this.set_extension_shortcut("",cx))))
                .into_any_element()
        } else if let Some((id, default)) = selected {
            let value = self.shortcut_value(&id, &default);
            let text = runtime::display(value.as_deref());
            let conflict = self.shortcut_conflict(&id, value.as_deref());
            let conflict_tip = conflict
                .as_ref()
                .map(|other| format!("{text} is already in use by “{other}”"));
            let supported = runtime::supported(&id, runtime::default_key(&default).as_deref(), cx);
            let reset_id = id.clone();
            div()
                .relative()
                .flex()
                .flex_col()
                .w(px(200.))
                .min_w_0()
                .h_full()
                .pb(px(1.))
                .child(
                    div()
                        .id("shortcut-keycaps")
                        .debug_selector(|| "shortcut-keycaps".into())
                        .when_some(conflict_tip, |keys, tip| {
                            keys.tooltip(move |_, cx| {
                                cx.new(|_| ShortcutTooltip(tip.clone())).into()
                            })
                        })
                        .flex()
                        .h(px(64.))
                        .gap(px(4.))
                        .children(text.chars().map(|key| keycap(key, conflict.is_some()))),
                )
                .child(
                    div()
                        .mt(px(24.))
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(id.clone()),
                )
                .when(id == "Go to Sidebar Item 1", |panel|panel.child(div().mt(px(6.)).text_size(px(11.)).line_height(px(14.)).child("Jumps to the first item in your Sidebar, starting with your Favorites at the top.")))
                .when(id == "Blank Window", |panel| {
                    panel.child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .child("Creates a new blank window."),
                    )
                })
                .when(id == "Duplicate", |panel| {
                    panel.child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .child("Duplicates selected tabs."),
                    )
                })
                .when(id == "Add Split View", |panel| {
                    panel.child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .child("Adds a new page to your current Split View."),
                    )
                })
                .when(supported && value.is_some(), |panel| panel.child(self.shortcut_priority_panel(&id, cx)))
                .child(div().flex_1())
                .child(div().h(px(24.)).w(px(200.)).flex().gap(px(4.))
                    .child(self.settings_button("Reset Shortcut to Default")
                        .id("reset-one-shortcut")
                        .debug_selector(|| "reset-one-shortcut".into())
                        .w(px(172.))
                        .when(!supported, |b| b.opacity(0.5))
                        .when(supported, |b| {
                            b.cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.reset_shortcut(Some(&reset_id), cx)
                                }))
                        }))
                    .child(self.shortcut_more_button(cx)))
                .into_any_element()
        } else {
            div().w(px(200.)).min_w_0().flex().flex_col().gap(px(10.)).h_full()
                .child(div().flex().gap(px(4.)).text_color(rgb(0x1686ff)).child(div().size(px(64.)).rounded(px(12.)).bg(rgb(0x75acff)).border_2().border_color(rgb(0x0788ff)).flex().items_center().justify_center().child(icons::control_sized(Icon::Command,40.))).child(div().size(px(64.)).rounded(px(12.)).bg(rgb(0x75acff)).border_2().border_color(rgb(0x0788ff)).flex().items_center().justify_center().child(icons::ionicon("happy-outline",42.,1.))))
                .child(div().mt(px(12.)).font_weight(gpui::FontWeight::BOLD).child("Custom Shortcuts"))
                .child(div().text_size(px(11.)).line_height(px(14.)).text_color(colors.rgb(0x929494)).child("Change shortcuts for your favorite actions, and even choose whether Echo or a website takes priority."))
                .child(div().flex_1())
                .child(div().w(px(168.)).h(px(24.)).mb(px(1.)).flex().gap(px(4.))
                    .child(self.settings_button("Reset All Shortcuts").id("reset-all-shortcuts").debug_selector(|| "reset-all-shortcuts".into())
                        .w(px(140.)).px(px(8.)).cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.reset_shortcut(None, cx))))
                    .child(self.shortcut_more_button(cx)))
                .into_any_element()
        };
        let mut last_group = None;
        let rows = entries
            .into_iter()
            .map(|(id, default)| {
                let custom = settings.bindings.contains_key(&id);
                let heading = if last_group != Some(custom) {
                    last_group = Some(custom);
                    Some(if custom {
                        "Customized Shortcuts"
                    } else {
                        "Default Shortcuts"
                    })
                } else {
                    None
                };
                let value = self.shortcut_value(&id, &default);
                let supported =
                    runtime::supported(&id, runtime::default_key(&default).as_deref(), cx);
                let conflict = self.shortcut_conflict(&id, value.as_deref());
                let selected = self.shortcuts.selected.as_ref() == Some(&id);
                let select = id.clone();
                let record = id.clone();
                let selector = format!("shortcut-record-{id}");
                let recording = selected && self.shortcuts.armed;
                let row_tip = conflict.as_ref().map(|other| {
                    format!(
                        "{} is already in use by “{other}”",
                        runtime::display(value.as_deref())
                    )
                });
                div()
                    .flex_none()
                    .when_some(heading, |row, text| {
                        row.child(
                            div()
                                .h(px(28.))
                                .mb(px(10.))
                                .flex()
                                .items_center()
                                .text_size(px(11.))
                                .text_color(colors.rgb(0x929494))
                                .child(text),
                        )
                    })
                    .child(
                        div()
                            .h(px(48.))
                            .flex_none()
                            .border_b_1()
                            .border_color(colors.rgb(0x3e3f41))
                            .relative()
                            .child(
                                div()
                                    .absolute()
                                    .top(px(4.))
                                    .left_0()
                                    .right_0()
                                    .h(px(40.))
                                    .rounded(px(8.))
                                    .when(selected, |row| {
                                        row.bg(if conflict.is_some() {
                                            rgb(if colors.light { 0xfbc6c6 } else { 0x553133 })
                                        } else {
                                            colors.rgb(0x263342)
                                        })
                                    }),
                            )
                            .child(
                                div()
                                    .relative()
                                    .size_full()
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .id(gpui::ElementId::Name(
                                                format!("shortcut-select-{id}").into(),
                                            ))
                                            .debug_selector(move || {
                                                format!("shortcut-select-{select}")
                                            })
                                            .flex_1()
                                            .min_w_0()
                                            .cursor_pointer()
                                            .on_click(cx.listener({
                                                let id = id.clone();
                                                move |this, _, _, cx| {
                                                    this.shortcuts.extension=None;
                                                    this.shortcuts.selected = Some(id.clone());
                                                    this.shortcuts.armed = false;
                                                    cx.notify();
                                                }
                                            }))
                                            .child(id),
                                    )
                                    .when(conflict.is_some(), |row| row.child(conflict_marker()))
                                    .child(
                                        div()
                                            .id(gpui::ElementId::Name(selector.clone().into()))
                                            .debug_selector(move || selector.clone())
                                            .w(px(64.))
                                            .h(px(24.))
                                            .rounded(px(5.))
                                            .border_1()
                                            .border_color(rgb(0xbdbdbd))
                                            .bg(gpui::linear_gradient(
                                                180.,
                                                gpui::linear_color_stop(rgb(0xf7f7f7), 0.),
                                                gpui::linear_color_stop(rgb(0xffffff), 1.),
                                            ))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.))
                                            .text_color(rgb(0x747474))
                                            .child(runtime::display(value.as_deref()))
                                            .when_some(row_tip, |key, tip| {
                                                key.tooltip(move |_, cx| {
                                                    cx.new(|_| ShortcutTooltip(tip.clone())).into()
                                                })
                                            })
                                            .when(selected, |key| {
                                                key.track_focus(&self.shortcuts.focus)
                                            })
                                            .when(recording, |key| {
                                                key.border_2().border_color(rgb(0x1686ff))
                                            })
                                            .when(!supported, |key| key.opacity(0.5))
                                            .when(supported, |key| {
                                                key.cursor_pointer().on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.shortcuts.selected =
                                                            Some(record.clone());
                                                        this.shortcuts.armed = true;
                                                        this.shortcuts.focus.focus(window);
                                                        cx.notify();
                                                    },
                                                ))
                                            }),
                                    ),
                            ),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .w(px(556.))
            .flex()
            .flex_col()
            .gap(px(30.))
            .child(
                div().flex().justify_center().child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .p(px(6.))
                        .rounded(px(6.))
                        .bg(colors.rgb(0x5b575f))
                        .when(colors.light, |callout| {
                            callout.border_1().border_color(rgb(0xe7e7e7)).shadow_sm()
                        })
                        .child("Start with the basics")
                        .child(
                            self.settings_button("Learn the Essential Shortcuts")
                                .id("learn-essential-shortcuts")
                                .debug_selector(|| "learn-essential-shortcuts".into())
                                .bg(rgb(0x147bff))
                                .text_color(rgb(0xffffff))
                                .cursor_pointer()
                                .on_click(|_, _, cx| cx.defer(crate::essential_shortcuts::open)),
                        ),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .h(px(450.))
                    .child(
                        self.settings_group()
                            .when(colors.light, |list| list.bg(rgb(0xffffff)))
                            .w(px(340.))
                            .flex_none()
                            .px_0()
                            .child(
                                div()
                                    .id("shortcut-search")
                                    .debug_selector(|| "shortcut-search".into())
                                    .h(px(40.))
                                    .flex_none()
                                    .px(px(16.))
                                    .bg(colors.rgb(0x3b3e3e))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_color(colors.rgb(0xa6a8a8))
                                    .child(icons::control_sized(Icon::Search, 14.))
                                    .child(self.shortcut_search.clone()),
                            )
                            .child(
                                div()
                                    .id("settings-shortcuts-list")
                                    .min_h_0()
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .px(px(15.))
                                    .children(rows)
                                    .child(self.render_extension_shortcuts(cx)),
                            ),
                    )
                    .child(details),
            )
            .into_any_element()
    }
}

struct ShortcutTooltip(String);
impl Render for ShortcutTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(28.))
            .px(px(10.))
            .rounded(px(14.))
            .bg(rgb(0x303030))
            .border_1()
            .border_color(rgb(0x545454))
            .flex()
            .items_center()
            .text_color(gpui::white())
            .text_size(px(11.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(self.0.clone())
    }
}

fn keycap(key: char, conflict: bool) -> gpui::Div {
    let (outer_top, outer_bottom, inner_top, inner_bottom, ink, edge) = if conflict {
        (0xde4043, 0xcb3a3c, 0xfad7d7, 0xf3c7c7, 0xfe6263, 0xfdecec)
    } else {
        (0x0f6fdf, 0x0d65ca, 0xb1ccf5, 0x92b4e7, 0x0077ff, 0xd1e0fd)
    };
    div()
        .relative()
        .w(px(64.))
        .h(px(64.))
        .min_w_0()
        .flex_shrink()
        .rounded(px(12.))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(rgb(outer_top), 0.),
            gpui::linear_color_stop(rgb(outer_bottom), 1.),
        ))
        .child(
            div()
                .absolute()
                .left(px(4.))
                .right(px(4.))
                .top(px(4.))
                .h(px(51.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(edge))
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(rgb(inner_top), 0.),
                    gpui::linear_color_stop(rgb(inner_bottom), 1.),
                ))
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(ink))
                .font_family(".AppleSystemUIFontRounded")
                .text_size(px(32.))
                .line_height(px(38.))
                .font_weight(gpui::FontWeight::BOLD)
                .child(key.to_string()),
        )
}

fn conflict_marker() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let mut circle = gpui::fill(bounds, rgb(0xff454d));
            circle.corner_radii = px(6.).into();
            window.paint_quad(circle);
            for (x, y, w, h) in [(5.4, 2.5, 1.2, 4.5), (5.4, 8.2, 1.2, 1.2)] {
                let mut mark = gpui::fill(
                    gpui::Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h))),
                    rgb(0x553133),
                );
                mark.corner_radii = px(0.6).into();
                window.paint_quad(mark);
            }
        },
    )
    .size(px(12.))
    .into_any_element()
}

fn priority_choices() -> [(browser_core::shortcuts::ShortcutPriority, &'static str, &'static str);3] {
    use browser_core::shortcuts::ShortcutPriority::*;
    [
        (PreferWebsites,"Prefer Websites","When shortcuts conflict, the website wins."),
        (UseBoth,"Use Both","Press this shortcut once to use the website’s shortcut. Press it twice to use Echo’s shortcut."),
        (PreferBrowser,"Prefer Echo","When shortcuts conflict, Echo wins."),
    ]
}

fn extension_shortcut_display(value: &str) -> String {
    if value.is_empty() {return "---".into();}
    let binding=value.split('+').map(|part|match part {
        "Command"=>"cmd".into(), "Ctrl"|"MacCtrl"=>"ctrl".into(), "Alt"|"Option"=>"alt".into(), "Shift"=>"shift".into(),
        "Comma"=>",".into(), "Period"=>".".into(), "Space"=>"space".into(), "PageUp"=>"pageup".into(), "PageDown"=>"pagedown".into(),
        _=>part.to_lowercase(),
    }).collect::<Vec<String>>().join("-");
    runtime::display(Some(&binding))
}
