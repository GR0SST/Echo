//! GPUI counterpart of Arc's Chromium search-engine management page.
use crate::{
    icons::{self, Icon},
    text_input::{Changed, TextInput},
    window::BrowserWindow,
};
use browser_core::{
    omnibox::{
        search_settings::{SearchProvider, ShortcutMode},
        CommandBarSearchPreferences,
    },
    UserProfileId,
};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, Context, Entity, Focusable, Subscription, Window,
};

pub(crate) const URL: &str = "browser://settings/searchEngines";
gpui::actions!(
    managed_search,
    [Accept, Cancel, Next, Previous, PageDown, PageUp]
);
pub(crate) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", Accept, Some("ManagedSearchEditor")),
        gpui::KeyBinding::new("space", Accept, Some("ManagedSearchButton")),
        gpui::KeyBinding::new("escape", Cancel, Some("ManagedSearchPage")),
        gpui::KeyBinding::new("escape", Cancel, Some("ManagedSearchEditor")),
        gpui::KeyBinding::new("tab", Next, Some("ManagedSearchEditor")),
        gpui::KeyBinding::new("shift-tab", Previous, Some("ManagedSearchEditor")),
        gpui::KeyBinding::new("pagedown", PageDown, Some("ManagedSearchPage")),
        gpui::KeyBinding::new("pageup", PageUp, Some("ManagedSearchPage")),
    ]);
}
pub(crate) fn open_from_settings(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    cx: &mut gpui::App,
) {
    cx.defer(move |cx| open_from_settings_now(profile, owner, selected, true, cx));
}
fn open_from_settings_now(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    create: bool,
    cx: &mut gpui::App,
) {
    if crate::quit::is_quitting() {
        return;
    }
    let preferred = owner
        .and_then(|owner| owner.upgrade())
        .map(|owner| owner.read(cx).main_window);
    let target = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<BrowserWindow>())
        .filter(|window| {
            window.read(cx).is_ok_and(|w| {
                !w.browser.is_private()
                    && !w.browser.is_blank()
                    && w.transient.is_none()
                    && w.peek_owner.is_none()
                    && w.profile.handle.same_workspace(&profile)
            })
        })
        .min_by_key(|window| {
            if preferred.is_some_and(|p| p.window_id() == window.window_id()) {
                0
            } else {
                1
            }
        });
    if let Some(target) = target {
        let _ = target.update(cx, |this, window, cx| {
            this.open_search_settings(selected, window, cx)
        });
    } else if create {
        crate::new_window::open(false, cx);
        cx.defer(move |cx| open_from_settings_now(profile, None, selected, false, cx));
    }
}
pub(crate) struct State {
    focus: gpui::FocusHandle,
    filter: Entity<TextInput>,
    _filter_changed: Subscription,
    editor: Option<Editor>,
    menu: Option<u64>,
    additional: bool,
    owner: Option<UserProfileId>,
    scroll: gpui::ScrollHandle,
}
struct Editor {
    owner: UserProfileId,
    id: Option<u64>,
    name: Entity<TextInput>,
    shortcut: Entity<TextInput>,
    template: Entity<TextInput>,
    readonly: bool,
    cancel: gpui::FocusHandle,
    save: gpui::FocusHandle,
    _changed: Vec<Subscription>,
}
impl State {
    pub fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let filter = cx.new(|cx| TextInput::new("Filter sites on page", cx));
        let changed = cx.subscribe(&filter, |_, _, _: &Changed, cx| cx.notify());
        Self {
            focus: cx.focus_handle(),
            filter,
            _filter_changed: changed,
            editor: None,
            menu: None,
            additional: false,
            owner: None,
            scroll: gpui::ScrollHandle::new(),
        }
    }
}
impl BrowserWindow {
    #[cfg(test)]
    pub(crate) fn search_editor_debug(&self, cx: &gpui::App) -> Option<Vec<String>> {
        self.search_settings.editor.as_ref().map(|e| {
            vec![
                e.name.read(cx).text().into(),
                e.shortcut.read(cx).text().into(),
                e.template.read(cx).text().into(),
            ]
        })
    }
    fn cancel_search_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_settings.editor.take().is_some() {
            self.search_settings.focus.focus(window);
        }
        self.search_settings.menu = None;
        cx.notify();
    }
    fn accept_search_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .search_settings
            .editor
            .as_ref()
            .is_some_and(|e| e.cancel.is_focused(window))
        {
            self.cancel_search_editor(window, cx);
        } else {
            self.save_search_editor(window, cx);
        }
    }
    fn move_search_editor_focus(&self, backwards: bool, window: &mut Window, cx: &Context<Self>) {
        let Some(e) = &self.search_settings.editor else {
            return;
        };
        let mut handles = vec![
            e.name.read(cx).focus_handle(cx),
            e.shortcut.read(cx).focus_handle(cx),
        ];
        if !e.readonly {
            handles.push(e.template.read(cx).focus_handle(cx));
        }
        handles.push(e.cancel.clone());
        if self.search_editor_value(cx).is_some() {
            handles.push(e.save.clone());
        }
        let index = handles
            .iter()
            .position(|f| f.is_focused(window))
            .unwrap_or(0);
        handles[if backwards {
            (index + handles.len() - 1) % handles.len()
        } else {
            (index + 1) % handles.len()
        }]
        .focus(window);
    }
    fn scroll_search_settings(&self, down: bool, window: &Window, cx: &mut Context<Self>) {
        let mut offset = self.search_settings.scroll.offset();
        offset.y += (window.viewport_size().height - px(100.)) * if down { -1. } else { 1. };
        self.search_settings.scroll.set_offset(offset);
        cx.notify();
    }
    pub(crate) fn search_settings_from_command(&mut self, cx: &mut Context<Self>) {
        self.close_command_bar_with_focus(crate::window::PopupFocus::OnActivation, cx);
        let selected = self
            .browser
            .blank_source_user_profile_id()
            .unwrap_or(self.browser.active_user_profile_id());
        let profile = if self.browser.is_private() || self.browser.is_blank() {
            let Ok(normal) = crate::new_window::normal_source(cx) else {
                return;
            };
            normal.profile
        } else {
            self.profile.handle.clone()
        };
        let owner = (!self.browser.is_private() && !self.browser.is_blank())
            .then(|| cx.entity().downgrade());
        open_from_settings(profile, owner, selected, cx);
    }
    pub(crate) fn search_settings_owner(&self) -> Option<UserProfileId> {
        if self.browser.active_split().is_some() {
            return None;
        }
        let url = url::Url::parse(&self.browser.active_tab()?.url).ok()?;
        if url.scheme() != "browser"
            || url.host_str() != Some("settings")
            || url.path() != "/searchEngines"
        {
            return None;
        }
        let owner = match url.query_pairs().find(|(key, _)| key == "profile") {
            Some((_, id)) => serde_json::from_str(&id).ok()?,
            None => self.browser.active_user_profile_id(),
        };
        self.profile
            .handle
            .read_settings(|b| b.user_profile(owner).map(|_| owner))
    }
    pub(crate) fn open_search_settings(
        &mut self,
        owner: UserProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .profile
            .handle
            .read_settings(|b| b.user_profile(owner).is_some())
        {
            return;
        }
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        let url = format!(
            "{URL}?profile={}",
            serde_json::to_string(&owner).expect("Profile ID serializes")
        );
        let existing = self
            .browser
            .space()
            .today
            .iter()
            .chain(&self.browser.space().pinned)
            .find(|t| t.url == url)
            .map(|t| t.id);
        let id = self.edit_browser(|b| match existing {
            Some(id) => {
                b.activate_tab(id, std::time::SystemTime::now());
                id
            }
            None => b.new_tab(url, std::time::SystemTime::now()),
        });
        self.edit_browser(|b| b.set_title(id, "Settings - Manage search engines and site search"));
        self.sidebar_navigation.controller.select(
            &self.browser,
            crate::sidebar_pages::SidebarPage::Space(self.browser.space().id),
        );
        self.search_settings.editor = None;
        self.search_settings.menu = None;
        self.search_settings.focus.focus(window);
        window.activate_window();
        cx.notify();
    }
    pub(crate) fn sync_search_settings(&mut self) {
        let owner = self.search_settings_owner();
        if self.search_settings.owner != owner {
            self.search_settings.owner = owner;
            self.search_settings.editor = None;
            self.search_settings.menu = None;
            self.search_settings
                .scroll
                .set_offset(gpui::point(px(0.), px(0.)));
        }
    }
    fn managed_search(&self) -> Option<CommandBarSearchPreferences> {
        let owner = self.search_settings_owner()?;
        self.profile
            .handle
            .read_settings(|b| b.user_profile(owner).map(|p| p.settings.search.clone()))
    }
    fn edit_managed_search(
        &mut self,
        action: impl FnOnce(&mut CommandBarSearchPreferences),
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.search_settings_owner() else {
            return;
        };
        self.profile.handle.edit_settings(|b| {
            if let Some(profile) = b.user_profile_mut(owner) {
                action(&mut profile.settings.search)
            }
        });
        self.refresh_profile_projection();
        cx.notify();
    }
    fn present_search_editor(
        &mut self,
        id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.search_settings_owner() else {
            return;
        };
        let Some(preferences) = self.managed_search() else {
            return;
        };
        let provider = id.and_then(|id| preferences.provider(id));
        if id.is_some() && provider.is_none() {
            return;
        }
        let name = cx.new(|cx| {
            let mut input = TextInput::new("", cx);
            if let Some(p) = provider {
                input.set_text(p.name.clone(), cx);
            }
            input
        });
        let shortcut = cx.new(|cx| {
            let mut input = TextInput::new("", cx);
            if let Some(p) = provider {
                input.set_text(p.shortcut.clone(), cx);
            }
            input
        });
        let template = cx.new(|cx| {
            let mut input = TextInput::new("", cx);
            if let Some(p) = provider {
                input.set_text(p.template.clone(), cx);
            }
            input
        });
        let changed = [&name, &shortcut, &template]
            .into_iter()
            .map(|input| cx.subscribe(input, |_, _, _: &Changed, cx| cx.notify()))
            .collect();
        name.read(cx).focus_handle(cx).focus(window);
        self.search_settings.editor = Some(Editor {
            owner,
            id,
            name,
            shortcut,
            template,
            readonly: provider.is_some_and(|p| p.builtin.is_some()),
            cancel: cx.focus_handle(),
            save: cx.focus_handle(),
            _changed: changed,
        });
        self.search_settings.menu = None;
        cx.notify();
    }
    fn search_editor_value(&self, cx: &Context<Self>) -> Option<CommandBarSearchPreferences> {
        let e = self.search_settings.editor.as_ref()?;
        if self.search_settings_owner() != Some(e.owner) {
            return None;
        }
        let mut preferences = self.managed_search()?;
        let (name, shortcut, template) = (
            e.name.read(cx).text(),
            e.shortcut.read(cx).text(),
            e.template.read(cx).text(),
        );
        match e.id {
            Some(id) => preferences
                .edit_provider(id, name, shortcut, template)
                .ok()?,
            None => {
                preferences.add_provider(name, shortcut, template).ok()?;
            }
        }
        Some(preferences)
    }
    fn save_search_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(value) = self.search_editor_value(cx) else {
            return;
        };
        self.edit_managed_search(|search| *search = value, cx);
        self.search_settings.editor = None;
        self.search_settings.focus.focus(window);
        cx.notify();
    }
    fn render_search_row(
        &self,
        provider: &SearchProvider,
        preferences: &CommandBarSearchPreferences,
        inactive: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = provider.id;
        let selected = preferences.default_provider().is_some_and(|p| p.id == id);
        let label = if selected {
            format!("{} (Default)", provider.name)
        } else {
            provider.name.clone()
        };
        div()
            .id(("managed-search-row", id))
            .debug_selector(move || format!("managed-search-row-{id}"))
            .h(px(49.))
            .w_full()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(0x3c4043))
            .child(
                div()
                    .w(px(212.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(4.))
                    .child(
                        div()
                            .size(px(16.))
                            .flex_none()
                            .text_color(rgb(0x9aa0a6))
                            .child(icons::control_sized(Icon::Search, 16.)),
                    )
                    .child(div().min_w_0().truncate().child(label)),
            )
            .child(div().flex_1().min_w_0().truncate().child(if inactive {
                provider.template.clone()
            } else {
                provider.shortcut.clone()
            }))
            .when(inactive, |row| {
                row.child(
                    outline_button("Activate", 86.)
                        .id(("search-activate", id))
                        .debug_selector(move || format!("search-activate-{id}"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_managed_search(
                                |p| {
                                    let _ = p.set_provider_active(id, true);
                                },
                                cx,
                            )
                        })),
                )
            })
            .when(!inactive, |row| {
                row.child(
                    div()
                        .w(px(48.))
                        .h(px(48.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .id(("search-edit", id))
                        .debug_selector(move || format!("search-edit-{id}"))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.present_search_editor(Some(id), window, cx)
                        }))
                        .child(pencil()),
                )
            })
            .child(
                div()
                    .w(px(48.))
                    .h(px(48.))
                    .flex_none()
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .id(("search-more", id))
                    .debug_selector(move || format!("search-more-{id}"))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if selected {
                            return;
                        }
                        this.search_settings.menu = Some(id);
                        cx.notify();
                    }))
                    .child(ellipsis())
                    .when(self.search_settings.menu == Some(id), |button| {
                        button.child(
                            gpui::deferred(
                                div()
                                    .absolute()
                                    .right(px(8.))
                                    .top(px(30.))
                                    .w(px(128.))
                                    .py(px(8.))
                                    .rounded(px(4.))
                                    .bg(rgb(0x202124))
                                    .shadow_lg()
                                    .when(!selected, |menu| {
                                        menu.child(menu_item("Make default").on_click(cx.listener(
                                            move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.edit_managed_search(
                                                    |p| {
                                                        let _ = p.make_default(id);
                                                    },
                                                    cx,
                                                );
                                                this.search_settings.menu = None;
                                            },
                                        )))
                                    })
                                    .when(!provider.show_in_engine_list && !inactive, |menu| {
                                        menu.child(menu_item("Deactivate").on_click(cx.listener(
                                            move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.edit_managed_search(
                                                    |p| {
                                                        let _ = p.set_provider_active(id, false);
                                                    },
                                                    cx,
                                                );
                                                this.search_settings.menu = None;
                                            },
                                        )))
                                    })
                                    .child(
                                        menu_item("Delete")
                                            .when(selected, |row| row.opacity(0.4))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.edit_managed_search(
                                                    |p| {
                                                        let _ = p.delete_provider(id);
                                                    },
                                                    cx,
                                                );
                                                this.search_settings.menu = None;
                                            })),
                                    ),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .into_any_element()
    }
    pub(crate) fn render_search_settings(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(preferences) = self.managed_search() else {
            return div().into_any_element();
        };
        let filter = self
            .search_settings
            .filter
            .read(cx)
            .text()
            .trim()
            .to_lowercase();
        let matches = |p: &&SearchProvider| {
            filter.is_empty()
                || p.name.to_lowercase().contains(&filter)
                || p.shortcut.to_lowercase().contains(&filter)
                || p.template.to_lowercase().contains(&filter)
        };
        let engines = preferences
            .providers
            .iter()
            .filter(|p| p.active && p.show_in_engine_list)
            .filter(matches)
            .collect::<Vec<_>>();
        let mut sites = preferences
            .providers
            .iter()
            .filter(|p| p.active && !p.show_in_engine_list)
            .filter(matches)
            .collect::<Vec<_>>();
        sites.sort_by_key(|p| p.name.to_lowercase());
        let inactive = preferences
            .providers
            .iter()
            .filter(|p| !p.active)
            .filter(matches)
            .collect::<Vec<_>>();
        div().id("search-settings-page").debug_selector(||"search-settings-page".into()).on_click(cx.listener(|this,_,_,cx|{this.search_settings.menu=None;cx.notify();})).absolute().inset_0().min_w_0().whitespace_normal().flex().flex_col().bg(rgb(0x202124)).text_color(rgb(0xe3e3e3)).font_family(".SystemUIFont").text_size(px(13.)).line_height(px(20.))
            .track_focus(&self.search_settings.focus)
            .key_context("ManagedSearchPage")
            .when(self.search_settings.editor.is_some(),|page|page.key_context("ManagedSearchEditor"))
            .on_action(cx.listener(|this,_:&Accept,window,cx|this.accept_search_editor(window,cx)))
            .on_action(cx.listener(|this,_:&Cancel,window,cx|this.cancel_search_editor(window,cx)))
            .on_action(cx.listener(|this,_:&Next,window,cx|this.move_search_editor_focus(false,window,cx)))
            .on_action(cx.listener(|this,_:&Previous,window,cx|this.move_search_editor_focus(true,window,cx)))
            .on_action(cx.listener(|this,_:&PageDown,window,cx|this.scroll_search_settings(true,window,cx)))
            .on_action(cx.listener(|this,_:&PageUp,window,cx|this.scroll_search_settings(false,window,cx)))
            .child(div().h(px(56.)).flex_none().flex().items_center().px(px(24.)).gap(px(16.)).shadow_sm()
                .child(hamburger()).child(div().flex_1().text_size(px(22.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Settings"))
                .child(icons::control(Icon::Search)))
            .child(div().id("search-settings-scroll").w_full().min_w_0().flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.search_settings.scroll)
                .child(div().w_full().min_w_0().max_w(px(680.)).mx_auto().min_h_full().bg(rgb(0x292a2d)).px(px(20.)).pb(px(24.))
                    .child(div().h(px(56.)).flex().items_center().gap(px(16.)).font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(icons::control(Icon::Back)).child(div().flex_1().child("Manage search engines and site search"))
                        .child(div().w(px(160.)).h(px(32.)).font_weight(gpui::FontWeight::NORMAL).border_b_1().border_color(rgb(0x9aa0a6)).child(self.search_settings.filter.clone())))
                    .child(div().pt(px(20.)).child("You can use shortcuts in the address bar to quickly search a specific site or to use a different search engine"))
                    .child(div().mt(px(16.)).child("Keyboard shortcut"))
                    .child(div().child("In the address bar, use this keyboard shortcut with shortcuts for search engines and site search"))
                    .children([(ShortcutMode::SpaceOrTab,"Space or Tab"),(ShortcutMode::Tab,"Tab")].into_iter().map(|(mode,label)|
                        div().id(("search-shortcut-mode",if mode==ShortcutMode::Tab {1u32}else {0u32})).h(px(48.)).when(mode==ShortcutMode::SpaceOrTab,|row|row.mt(px(12.))).pl(px(40.)).flex().items_center().gap(px(20.)).cursor_pointer()
                            .debug_selector(move||if mode==ShortcutMode::Tab {"search-mode-tab".into()} else {"search-mode-space-tab".into()})
                            .on_click(cx.listener(move|this,_,_,cx|this.edit_managed_search(|p|p.shortcut_mode=mode,cx)))
                            .child(radio(preferences.shortcut_mode==mode)).child(label)))
                    .child(div().mt(px(24.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Search engines"))
                    .child(div().child("To use a search engine other than the default, type its shortcut in the address bar followed by your preferred keyboard shortcut. You can also change your default search engine here."))
                    .child(table_header("Shortcut"))
                    .children(engines.into_iter().map(|p|self.render_search_row(p,&preferences,false,cx)))
                    .child(div().mt(px(28.)).flex().items_center().gap(px(16.))
                        .child(div().flex_1().min_w_0().whitespace_normal().child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Site search"))
                            .child("To search a specific site or part of Echo, type its shortcut in the address bar, followed by your preferred keyboard shortcut."))
                        .child(outline_button("Add",66.).id("search-add").debug_selector(||"search-add".into())
                            .on_click(cx.listener(|this,_,window,cx|this.present_search_editor(None,window,cx)))))
                    .child(table_header("Shortcut"))
                    .children(sites.iter().take(if self.search_settings.additional {usize::MAX}else {5}).map(|p|self.render_search_row(p,&preferences,false,cx)))
                    .when(sites.len()>5,|page|page.child(div().id("search-additional").h(px(48.)).flex().items_center().justify_between().cursor_pointer()
                        .on_click(cx.listener(|this,_,_,cx|{this.search_settings.additional=!this.search_settings.additional;cx.notify();}))
                        .child("Additional sites").child(icons::control(if self.search_settings.additional {Icon::Up}else {Icon::Down}))))
                    .child(div().mt(px(32.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Inactive shortcuts"))
                    .child(table_header("Query URL"))
                    .children(inactive.into_iter().map(|p|self.render_search_row(p,&preferences,true,cx)))))
            .when_some(self.search_settings.editor.as_ref(),|page,e| {
                let valid=self.search_editor_value(cx).is_some();
                page.child(div().absolute().inset_0().bg(gpui::black().opacity(0.6)).flex().items_center().justify_center()
                    .on_mouse_down(gpui::MouseButton::Left,cx.listener(|_,_,_,cx|cx.stop_propagation()))
                    .child(div().w(px(512.)).h(px(356.)).relative().rounded(px(8.)).bg(rgb(0x292a2d)).shadow_lg()
                        .debug_selector(||"search-editor".into())
                        .child(div().absolute().left(px(20.)).top(px(20.)).font_weight(gpui::FontWeight::MEDIUM).text_size(px(15.)).line_height(px(15.)).child(if e.id.is_some() {"Edit search engine"} else {"Add site search"}))
                        .children([(0,"Name",&e.name),(1,"Shortcut",&e.shortcut),(2,"URL with %s in place of query",&e.template)].into_iter().map(|(index,label,input)|
                            div().absolute().left(px(20.)).right(px(20.)).top(px(52.+79.*index as f32))
                                .child(div().h(px(23.)).text_size(px(11.)).line_height(px(16.)).font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(0xc4c7c5)).child(label))
                                .child(div().h(px(36.)).rounded_t(px(8.)).when(index==2&&e.readonly,|field|field.rounded(px(8.))).bg(rgb(0x444746)).px(px(10.)).flex().items_center()
                                    .when(index!=2||!e.readonly,|field|field.border_b_1().when(input.read(cx).focus_handle(cx).is_focused(window),|field|field.border_b_2()).border_color(rgb(if input.read(cx).focus_handle(cx).is_focused(window) {0xa8c7fa}else {0x9aa0a6})))
                                    .when(index!=2||!e.readonly,|field|field.child(input.clone()))
                                    .when(index==2&&e.readonly,|field|field.text_color(rgb(0x9aa0a6)).child(div().truncate().child(input.read(cx).text().to_owned()))))))
                        .child(div().absolute().right(px(16.)).bottom(px(16.)).flex().gap(px(8.))
                            .child(outline_button("Cancel",76.).id("search-editor-cancel").track_focus(&e.cancel).key_context("ManagedSearchButton").debug_selector(||"search-editor-cancel".into())
                                .on_click(cx.listener(|this,_,window,cx|{this.search_settings.editor=None;this.search_settings.focus.focus(window);cx.notify();})))
                            .child(div().w(px(67.)).h(px(36.)).rounded_full().flex().items_center().justify_center()
                                .bg(rgb(if valid {0xa8c7fa}else {0x444746})).text_color(rgb(if valid {0x062e6f}else {0x8c8e8d}))
                                .id("search-editor-save").track_focus(&e.save).key_context("ManagedSearchButton").debug_selector(||"search-editor-save".into()).cursor_pointer()
                                .on_click(cx.listener(|this,_,window,cx|this.save_search_editor(window,cx)))
                                .font_weight(gpui::FontWeight::MEDIUM).child(if e.id.is_some() {"Save"}else {"Add"})))))
            }).into_any_element()
    }
}
fn outline_button(label: &'static str, width: f32) -> gpui::Div {
    div()
        .w(px(width))
        .h(px(36.))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(rgb(0x0984b5))
        .text_color(rgb(0xa8c7fa))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(label)
}
fn menu_item(label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(label)
        .debug_selector(move || format!("search-menu-{label}"))
        .h(px(36.))
        .px(px(24.))
        .flex()
        .items_center()
        .cursor_pointer()
        .child(label)
}
fn table_header(second: &'static str) -> gpui::Div {
    div()
        .mt(px(16.))
        .h(px(36.))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgb(0x3c4043))
        .pl(px(4.))
        .child(div().w(px(212.)).child("Name"))
        .child(second)
}
fn radio(selected: bool) -> gpui::Div {
    div()
        .size(px(16.))
        .rounded_full()
        .border_2()
        .border_color(rgb(if selected { 0xa8c7fa } else { 0x9aa0a6 }))
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |r| {
            r.child(div().size(px(6.)).rounded_full().bg(rgb(0xa8c7fa)))
        })
}
fn hamburger() -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            for y in [4., 9., 14.] {
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(
                        bounds.origin + gpui::point(px(1.), px(y)),
                        gpui::size(px(16.), px(1.)),
                    ),
                    rgb(0x9aa0a6),
                ));
            }
        },
    )
    .size(px(18.))
}
fn pencil() -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::stroke(px(1.7));
            let p = |x, y| bounds.origin + gpui::point(px(x), px(y));
            path.move_to(p(3., 12.));
            path.line_to(p(12., 3.));
            path.line_to(p(15., 6.));
            path.line_to(p(6., 15.));
            path.line_to(p(3., 15.));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(0x9aa0a6));
            }
        },
    )
    .size(px(18.))
}
fn ellipsis() -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            for y in [4., 8., 12.] {
                let mut quad = gpui::fill(
                    gpui::Bounds::new(
                        bounds.origin + gpui::point(px(7.), px(y)),
                        gpui::size(px(2.), px(2.)),
                    ),
                    rgb(0x9aa0a6),
                );
                quad.corner_radii = px(1.).into();
                window.paint_quad(quad);
            }
        },
    )
    .size(px(18.))
}
