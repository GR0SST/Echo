//! Window-local navigation. Virtual pages never allocate workspace identities.
use crate::{
    library::{
        self, LibraryActions, LibraryColors, LibraryIntent, LibraryProjection, LibrarySection,
        LibrarySpace,
    },
    sidebar_pages::SidebarPage,
    space_creation::SpaceCreationController,
    space_creation_view::{SpaceCreationEvent, SpaceCreationView},
    window::{BrowserWindow, PopupFocus},
};
use browser_core::{Browser, SpaceId, UserProfileId};
use gpui::{AnyElement, Context, Entity, Subscription, Window, div, hsla, prelude::*, px};
use std::rc::Rc;

pub(crate) struct SidebarNavigation {
    pub(crate) controller: SpaceCreationController,
    pub(crate) library_section: LibrarySection,
    pub(crate) library_origin: SpaceId,
    pub(crate) form: Option<Entity<SpaceCreationView>>,
    events: Option<Subscription>,
    generation: u64,
}
impl SidebarNavigation {
    pub(crate) fn new(browser: &Browser) -> Self {
        Self {
            controller: SpaceCreationController::new(browser),
            library_section: LibrarySection::Media,
            library_origin: browser.space().id,
            form: None,
            events: None,
            generation: 0,
        }
    }
}
impl BrowserWindow {
    pub(crate) fn invalidate_navigation_focus(&mut self) {
        self.sidebar_navigation.generation = self.sidebar_navigation.generation.wrapping_add(1);
    }
    pub(crate) fn sidebar_root_may_take_focus(&self) -> bool {
        self.user_profile_dialog.is_none()
            && self.command_bar.is_none()
            && self.find_bar.is_none()
            && self.tab_menu.is_none()
            && self.new_menu.is_none()
            && self.site_controls.is_none()
            && self.tab_rename.is_none()
            && self.tab_rename_pending.is_none()
    }

    pub(crate) fn page_content_visible(&self) -> bool {
        self.browser.is_blank()
            || self.browser.is_private()
            || matches!(
                self.sidebar_navigation.controller.selected(),
                SidebarPage::Space(_)
            )
    }
    pub(crate) fn sidebar_page_index(&self) -> usize {
        if self.browser.is_blank() || self.browser.is_private() {
            return self.browser.active_space;
        }
        self.sidebar_navigation
            .controller
            .pages(&self.browser)
            .iter()
            .position(|page| *page == self.sidebar_navigation.controller.selected())
            .unwrap_or(0)
    }
    pub(crate) fn reconcile_sidebar_navigation(&mut self) {
        let old = self.sidebar_navigation.controller.selected();
        self.sidebar_navigation.controller.reconcile(&self.browser);
        // Real selection may change through tab commands; virtual selection is local.
        if matches!(old, SidebarPage::Space(_)) {
            self.sidebar_navigation
                .controller
                .select(&self.browser, SidebarPage::Space(self.browser.space().id));
        }
        if !self
            .browser
            .spaces
            .iter()
            .any(|space| space.id == self.sidebar_navigation.library_origin)
        {
            self.sidebar_navigation.library_origin = self.browser.space().id;
        }
        if self.sidebar_navigation.controller.draft().is_none() {
            self.sidebar_navigation.form = None;
            self.sidebar_navigation.events = None;
        }
        if old != self.sidebar_navigation.controller.selected() {
            self.reset_sidebar_motion();
            self.pager.position = self.sidebar_page_index() as f32;
        }
    }
    fn close_navigation_peers(&mut self, cx: &mut Context<Self>) {
        // Invalidate restoration before removing any child; the new root owns focus.
        self.invalidate_sidebar_focus();
        self.finish_tab_rename(true, cx);
        self.close_user_profile_dialog_with_focus(PopupFocus::OnActivation, cx);
        self.close_command_bar_with_focus(PopupFocus::OnActivation, cx);
        self.close_find_with_focus(PopupFocus::OnActivation, cx);
        self.close_tab_menu_with_focus(PopupFocus::OnActivation, cx);
        self.close_site_controls_with_focus(PopupFocus::OnActivation, cx);
        self.close_new_menu_with_focus(PopupFocus::OnActivation, cx);
        self.close_link_status(cx);
        // A Space hover changes selection inside the current drag. Keep its
        // floating source surface until the typed drag completes.
        if !cx.has_active_drag() {
            self.close_floating_sidebar(cx);
        }
    }
    pub(crate) fn select_sidebar_page(&mut self, page: SidebarPage, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() || self.browser.is_blank() || self.browser.is_private() {
            return;
        }
        self.refresh_profile_projection();
        if page == SidebarPage::CreateSpace {
            self.begin_space_creation(self.browser.space().id, cx);
            return;
        }
        if !self
            .sidebar_navigation
            .controller
            .pages(&self.browser)
            .contains(&page)
        {
            return;
        }
        self.close_navigation_peers(cx);
        if page == SidebarPage::Library
            && self.sidebar_navigation.controller.selected() != SidebarPage::Library
        {
            self.sidebar_navigation.library_origin = self.browser.space().id;
            self.sidebar_navigation.library_section = LibrarySection::Media;
        }
        self.sidebar_navigation
            .controller
            .select(&self.browser, page);
        if let SidebarPage::Space(id) = page {
            if let Some(pages) = &self.pages {
                pages.borrow_mut().request_page_focus();
            }
            self.edit_browser(|browser| {
                if let Some(index) = browser.spaces.iter().position(|space| space.id == id) {
                    browser.switch_space(index);
                }
            });
        }
        // Hide virtual pages synchronously. Real selection can be an
        // intermediate step in a drag/folder edit; apply its final document
        // only after that synchronous operation has completed.
        if !self.page_content_visible() {
            self.sync_navigation_visibility();
        }
        self.settle_sidebar_pager(cx);
        self.defer_sidebar_focus(cx);
        cx.notify();
    }
    pub(crate) fn begin_space_creation(&mut self, origin: SpaceId, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() {
            return;
        }
        self.refresh_profile_projection();
        if self
            .sidebar_navigation
            .controller
            .begin(&self.browser, origin)
            .is_err()
        {
            return;
        }
        self.close_navigation_peers(cx);
        self.sidebar_visible = true;
        if self.sidebar_navigation.form.is_none() {
            let draft = self
                .sidebar_navigation
                .controller
                .request()
                .expect("begun draft");
            let name = self
                .browser
                .user_profile(draft.user_profile_id)
                .map(|profile| profile.name())
                .unwrap_or("");
            let form = cx.new(|cx| SpaceCreationView::new(&draft, name, cx));
            let events =
                cx.subscribe(&form, |this, sender, event: &SpaceCreationEvent, cx| {
                    if this
                        .sidebar_navigation
                        .form
                        .as_ref()
                        .is_none_or(|form| form.entity_id() != sender.entity_id())
                        || this.sidebar_navigation.controller.selected() != SidebarPage::CreateSpace
                    {
                        return;
                    }
                    match event {
                        SpaceCreationEvent::NameChanged(name) => this
                            .sidebar_navigation
                            .controller
                            .edit_draft(|draft| draft.name = name.clone()),
                        SpaceCreationEvent::Submit(draft) => {
                            this.sidebar_navigation
                                .controller
                                .edit_draft(|current| *current = draft.clone());
                            // Release the form's update before disposing it on success.
                            let sender = sender.clone();
                            let generation = this.sidebar_navigation.generation;
                            let owner = cx.entity().downgrade();
                            cx.defer(move |cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.sidebar_navigation.generation == generation
                                        && this.sidebar_navigation.form.as_ref().is_some_and(
                                            |form| form.entity_id() == sender.entity_id(),
                                        )
                                    {
                                        this.submit_space_creation(cx);
                                    }
                                });
                            });
                        }
                        SpaceCreationEvent::Cancel => {
                            let sender = sender.clone();
                            let generation = this.sidebar_navigation.generation;
                            let owner = cx.entity().downgrade();
                            cx.defer(move |cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.sidebar_navigation.generation == generation
                                        && this.sidebar_navigation.form.as_ref().is_some_and(
                                            |form| form.entity_id() == sender.entity_id(),
                                        )
                                    {
                                        this.cancel_space_creation(cx);
                                    }
                                });
                            });
                        }
                        // Native icon/profile/theme controls have not been approved for
                        // this form. Preserve captured intents without inventing a picker.
                        SpaceCreationEvent::ChooseIcon(_)
                        | SpaceCreationEvent::ChooseProfile(_)
                        | SpaceCreationEvent::ProfileInfo(_)
                        | SpaceCreationEvent::ChooseTheme(_) => {}
                    }
                    cx.notify();
                });
            self.sidebar_navigation.form = Some(form);
            self.sidebar_navigation.events = Some(events);
        }
        self.sync_navigation_visibility();
        self.settle_sidebar_pager(cx);
        self.defer_sidebar_focus(cx);
        cx.notify();
    }
    pub(crate) fn cancel_space_creation(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        let destination = self.sidebar_navigation.controller.cancel(&self.browser);
        self.sidebar_navigation.form = None;
        self.sidebar_navigation.events = None;
        self.select_sidebar_page(destination, cx);
    }
    pub(crate) fn submit_space_creation(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_navigation.controller.selected() != SidebarPage::CreateSpace {
            return;
        }
        let Some(request) = self.sidebar_navigation.controller.request() else {
            return;
        };
        match self.commit_space_creation_request(&request) {
            Ok(_) => {
                self.sidebar_navigation.form = None;
                self.sidebar_navigation.events = None;
                self.sync_navigation_visibility();
                self.settle_sidebar_pager(cx);
                self.defer_sidebar_focus(cx);
            }
            Err(_) => self.show_toast(
                "Cannot create this Space. Check its name and profile.".into(),
                cx,
            ),
        }
        cx.notify();
    }
    pub(crate) fn sync_navigation_visibility(&self) {
        if let Some(pages) = &self.pages {
            let mut pages = pages.borrow_mut();
            if !self.page_content_visible() {
                pages.request_ui_focus();
            }
            pages.sync(&self.browser, self.page_content_visible());
        }
    }
    pub(crate) fn defer_sidebar_focus(&mut self, cx: &mut Context<Self>) {
        self.sidebar_navigation.generation = self.sidebar_navigation.generation.wrapping_add(1);
        let generation = self.sidebar_navigation.generation;
        let form = self.sidebar_navigation.form.clone();
        let owner = cx.entity().downgrade();
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    if crate::quit::is_quitting()
                        || this.sidebar_navigation.generation != generation
                        || !this.sidebar_root_may_take_focus()
                    {
                        return;
                    }
                    this.refresh_profile_projection();
                    this.sync_navigation_visibility();
                    this.focus_sidebar_page(window, cx, form.as_ref());
                });
            });
        });
    }
    pub(crate) fn focus_sidebar_page(
        &self,
        window: &mut Window,
        cx: &gpui::App,
        expected: Option<&Entity<SpaceCreationView>>,
    ) {
        if self.sidebar_navigation.controller.selected() == SidebarPage::CreateSpace {
            if let Some(form) = self
                .sidebar_navigation
                .form
                .as_ref()
                .filter(|form| expected.is_none_or(|expected| expected == *form))
            {
                form.read(cx).focus_name(window, cx);
            }
        } else {
            self.focus_handle.focus(window);
            if self.page_content_visible() {
                if let Some(pages) = &self.pages {
                    pages.borrow_mut().focus_shown();
                }
            }
        }
    }
    /// Revalidate the painted card owner before any callback uses active-profile APIs.
    pub(crate) fn prepare_sidebar_target(
        &mut self,
        space: SpaceId,
        profile: UserProfileId,
    ) -> bool {
        self.refresh_profile_projection();
        let Some(index) = self
            .browser
            .spaces
            .iter()
            .position(|entry| entry.id == space && entry.user_profile_id == profile)
        else {
            return false;
        };
        if self.browser.active_space != index {
            self.edit_browser(|browser| browser.switch_space(index));
        }
        true
    }
    pub(crate) fn handle_library_intent(
        &mut self,
        intent: LibraryIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_profile_projection();
        if self.sidebar_navigation.controller.selected() != SidebarPage::Library {
            return;
        }
        match intent {
            LibraryIntent::Navigate(section @ (LibrarySection::Media | LibrarySection::Spaces)) => {
                self.sidebar_navigation.library_section = section;
                cx.notify();
            }
            LibraryIntent::SelectSpace(id) => self.select_sidebar_page(SidebarPage::Space(id), cx),
            LibraryIntent::BeginSpaceCreation { origin } => self.begin_space_creation(origin, cx),
            LibraryIntent::OpenProfile { space, profile } => {
                if self
                    .browser
                    .spaces
                    .iter()
                    .any(|entry| entry.id == space && entry.user_profile_id == profile)
                {
                    self.open_user_profile_switch(space, cx);
                }
            }
            LibraryIntent::Overflow(space) => {
                if self.browser.spaces.iter().any(|entry| entry.id == space) {
                    self.open_space_menu(space, gpui::point(px(0.), px(48.)), window, cx);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn render_library_host(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(projection) =
            LibraryProjection::from_browser(&self.browser, self.sidebar_navigation.library_origin)
        else {
            return div().into_any_element();
        };
        let owner = cx.entity().downgrade();
        let events: library::LibraryEvents = Rc::new(move |intent, window, cx| {
            let _ = owner.update(cx, |this, cx| {
                this.handle_library_intent(intent, window, cx)
            });
        });
        let colors = LibraryColors {
            background: hsla(0., 0., 0.96, 1.),
            card_surface: |hue| crate::window::tint(hue / 360., 0.82),
            text: hsla(0., 0., 0.1, 1.),
            muted: hsla(0., 0., 0.4, 1.),
            hover: hsla(0., 0., 0., 0.06),
            selected: hsla(0., 0., 0., 0.08),
        };
        let section = self.sidebar_navigation.library_section;
        let empty = (section == LibrarySection::Media).then(|| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors.muted)
                .child("Nothing here yet!")
                .into_any_element()
        });
        library::render_library(
            &projection,
            section,
            colors,
            LibraryActions {
                profile: true,
                media: true,
                ..Default::default()
            },
            events,
            |card, _| self.render_library_sidebar(card, cx),
            empty,
        )
    }
    fn render_library_sidebar(
        &mut self,
        card: &LibrarySpace,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // A rendering-only projection supplies the exact card profile to the
        // existing Favorites/folder/split/drag components. Never publish it.
        let mut model: Browser = (*self.browser).clone();
        let Some(index) = model
            .spaces
            .iter()
            .position(|space| space.id == card.id && space.user_profile_id == card.profile)
        else {
            return div().into_any_element();
        };
        model.switch_space(index);
        let projection = model.window_projection(&model.window_selection());
        let saved = std::mem::replace(&mut self.browser, projection);
        let saved_interactive = self.sidebar_page_interactive;
        self.sidebar_page_interactive = true;
        let saved_namespace = self
            .sidebar_debug_namespace
            .replace(format!("library-{:?}", card.id));
        let favorites = self
            .browser
            .active_user_profile()
            .favorites
            .iter()
            .filter(|tab| !self.browser.is_split_secondary(tab.id))
            .map(|tab| self.render_favorite(tab, 60., cx).into_any_element())
            .collect::<Vec<_>>();
        let pinned = self
            .render_folder_rows(card.id, card.hue / 360., cx)
            .into_iter()
            .map(|row| {
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(row)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let today = self
            .browser
            .space()
            .today
            .iter()
            .filter(|tab| !self.browser.is_split_secondary(tab.id))
            .map(|tab| {
                self.render_tab(
                    tab,
                    browser_core::Section::Today,
                    false,
                    card.hue / 360.,
                    cx,
                )
                .w_full()
                .min_w_0()
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let target = card.id;
        let drag = crate::space_drag::SpaceDrag {
            id: card.id,
            workspace: self.profile.handle.clone(),
            name: card.name.clone(),
            icon: card.icon.clone(),
            hue: card.hue,
        };
        let content = div()
            .id(gpui::SharedString::from(format!(
                "library-sidebar-{target:?}"
            )))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .on_drop(
                cx.listener(move |this, drag: &crate::space_drag::SpaceDrag, _, cx| {
                    this.refresh_profile_projection();
                    if !this.profile.handle.same_workspace(&drag.workspace) {
                        return;
                    }
                    if let Some(index) = this
                        .browser
                        .spaces
                        .iter()
                        .position(|space| space.id == target)
                    {
                        this.reorder_space(drag.id, index, cx);
                    }
                }),
            )
            .min_w_0()
            .w_full()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .overflow_hidden()
            .child(div().flex().flex_wrap().gap(px(6.)).children(favorites))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(pinned),
            )
            .child(div().h(px(1.)).my(px(8.)).bg(hsla(0., 0., 0., 0.12)))
            .child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "library-today-{target:?}"
                    )))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(today),
            )
            .into_any_element();
        self.browser = saved;
        self.sidebar_debug_namespace = saved_namespace;
        self.sidebar_page_interactive = saved_interactive;
        content
    }
}

#[cfg(test)]
#[path = "sidebar_navigation_tests.rs"]
mod tests;
