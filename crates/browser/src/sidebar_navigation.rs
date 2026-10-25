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

/// Content identity is stronger than TabId: another host can navigate or
/// reassign the same tab while an overlay's dismissal is queued.
#[derive(Clone)]
struct PreviewContent {
    viewport: crate::pages::PageViewport,
    viewport_generation: Option<u64>,
    space: SpaceId,
    profile: UserProfileId,
    tab: browser_core::TabId,
    url: String,
    owner: Option<crate::pages::UiPageFocusOwner>,
}
pub(crate) struct PreviewUiResponder {
    content: PreviewContent,
    scope: gpui::FocusHandle,
    focus: gpui::FocusHandle,
}
#[derive(Clone)]
pub(crate) struct PreviewNativeResponder {
    content: PreviewContent,
    target: crate::pages::ResizePageFocus,
}

pub(crate) struct SidebarNavigation {
    pub(crate) controller: SpaceCreationController,
    pub(crate) library_section: LibrarySection,
    pub(crate) library_origin: SpaceId,
    pub(crate) form: Option<Entity<SpaceCreationView>>,
    pub(crate) media: Option<Entity<library::media::MediaView>>,
    boosts:Option<Entity<crate::boost_library::BoostLibrary>>,
    archive: Option<Entity<crate::library_archive::ArchiveLibrary>>,
    media_events: Option<Subscription>,
    pub(crate) media_search_focus: bool,
    pub(crate) downloads: crate::library_downloads::DownloadsNavigation,
    preview_focus_scope: Option<gpui::FocusHandle>,
    preview_rendered_content: Rc<std::cell::RefCell<Option<PreviewContent>>>,
    pub(crate) preview_ui_focus: Option<PreviewUiResponder>,
    pub(crate) preview_native_focus: Option<PreviewNativeResponder>,
    preview_native_intent: Option<PreviewNativeResponder>,
    events: Option<Subscription>,
    generation: u64,
    pub(crate) normal_content_size: gpui::Size<gpui::Pixels>,
    normal_content_left: f32,
    library_slide_amount: f32,
    library_slide_target: bool,
    library_slide_generation: u64,
    library_slide_task: Option<gpui::Task<()>>,
}
impl SidebarNavigation {
    pub(crate) fn new(browser: &Browser) -> Self {
        Self {
            controller: SpaceCreationController::new(browser),
            library_section: LibrarySection::Media,
            library_origin: browser.space().id,
            form: None,
            media: None,
            boosts:None,
            archive: None,
            media_events: None,
            media_search_focus: false,
            downloads: Default::default(),
            preview_focus_scope: None,
            preview_rendered_content: Rc::default(),
            preview_ui_focus: None,
            preview_native_focus: None,
            preview_native_intent: None,
            events: None,
            generation: 0,
            normal_content_size: Default::default(),
            normal_content_left: 0.,
            library_slide_amount: 0.,
            library_slide_target: false,
            library_slide_generation: 0,
            library_slide_task: None,
        }
    }
}
impl BrowserWindow {
    /// Arc translates the original page viewport into Library without reflow.
    pub(crate) fn sync_library_geometry(&mut self, viewport: gpui::Size<gpui::Pixels>, cx: &mut Context<Self>) {
        let left = self.sidebar_width() * self.sidebar_amount()
            + crate::pages::CONTENT_INSET * (1. - self.sidebar_amount());
        self.sidebar_navigation.normal_content_left = left;
        self.sidebar_navigation.normal_content_size = gpui::size(
            px((f32::from(viewport.width) - left - crate::pages::CONTENT_INSET).max(0.)),
            px((f32::from(viewport.height) - 2. * crate::pages::CONTENT_INSET).max(0.)),
        );
        let entering = self.sidebar_navigation.controller.selected() == SidebarPage::Library;
        if self.sidebar_navigation.library_slide_target != entering {
            self.animate_library_slide(entering, cx);
        }
    }
    pub(crate) fn library_page_offset(&self) -> f32 {
        (112. + library::media::MEDIA_REFERENCE_COLUMN_WIDTH - self.sidebar_navigation.normal_content_left)
            * self.sidebar_navigation.library_slide_amount
    }
    pub(crate) fn library_preview_left(&self) -> f32 {
        self.sidebar_navigation.normal_content_left + self.library_page_offset() - 112.
    }
    fn animate_library_slide(&mut self, entering: bool, cx: &mut Context<Self>) {
        self.sidebar_navigation.library_slide_target = entering;
        let from = self.sidebar_navigation.library_slide_amount;
        let to = if entering { 1. } else { 0. };
        self.sidebar_navigation.library_slide_generation = self.sidebar_navigation.library_slide_generation.wrapping_add(1);
        let generation = self.sidebar_navigation.library_slide_generation;
        self.sidebar_navigation.library_slide_task = None;
        if crate::native::prefers_reduced_motion() || from == to {
            self.sidebar_navigation.library_slide_amount = to;
            return;
        }
        // Installed Arc recording: fast initial travel, then a short settling
        // tail; approximately 0.28 s into Library and 0.35 s on return.
        let duration = if entering { 0.28 } else { 0.35 };
        let started = cx.background_executor().now();
        self.sidebar_navigation.library_slide_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
            let t = (cx.background_executor().now().duration_since(started).as_secs_f32() / duration).clamp(0., 1.);
            let eased = (1. - (-7. * t).exp()) / (1. - (-7_f32).exp());
            let keep = this.update(cx, |this, cx| {
                if this.sidebar_navigation.library_slide_generation != generation { return false; }
                this.sidebar_navigation.library_slide_amount = if t >= 1. { to } else { from + (to - from) * eased };
                cx.notify();
                t < 1.
            }).unwrap_or(false);
            if !keep { break; }
        }));
    }
    pub(crate) fn open_library_spaces(&mut self, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() || self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        self.select_sidebar_page(SidebarPage::Library, cx);
        self.invalidate_sidebar_focus();
        self.sidebar_navigation.library_section = LibrarySection::Spaces;
        self.sync_navigation_visibility();
        self.defer_sidebar_focus(cx);
        cx.notify();
    }

    pub(crate) fn invalidate_navigation_focus(&mut self) {
        self.sidebar_navigation.generation = self.sidebar_navigation.generation.wrapping_add(1);
    }
    pub(crate) fn sidebar_root_may_take_focus(&self) -> bool {
        !self.save_prompt_active && self.user_profile_dialog.is_none()
            && self.command_bar.is_none()
            && self.tab_switcher.is_none()
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
    pub(crate) fn native_page_viewport(&self) -> crate::pages::PageViewport {
        if self.page_content_visible() { return crate::pages::PageViewport::Content; }
        if self.sidebar_navigation.controller.selected() == SidebarPage::Library {
            return self.sidebar_navigation.library_section.page_viewport();
        }
        crate::pages::PageViewport::Hidden
    }
    pub(crate) fn native_page_content_visible(&self) -> bool {
        self.native_page_viewport() != crate::pages::PageViewport::Hidden
    }
    pub(crate) fn sync_page_viewport(&self) {
        if let Some(pages) = &self.pages {
            if let Ok(mut pages) = pages.try_borrow_mut() { pages.set_viewport(self.native_page_viewport()); }
        }
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
        self.retire_library_downloads_route();
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
        let was_library = self.sidebar_navigation.controller.selected() == SidebarPage::Library;
        if was_library != (page == SidebarPage::Library) {
            self.animate_library_slide(page == SidebarPage::Library, cx);
        }
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
        self.sync_page_viewport();
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
        self.sidebar_selection.new_space_items.clear();
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
            Ok(space) => {
                let selected = std::mem::take(&mut self.sidebar_selection.new_space_items);
                if request.user_profile_id == self.browser.active_user_profile_id() {
                    self.edit_browser(|browser| { for id in selected { let pinned = browser.all_tabs().find(|tab| tab.id == id).is_some_and(|tab| tab.pinned_url.is_some()); browser.move_tab(id, space, if pinned { browser_core::Section::Pinned } else { browser_core::Section::Today }); } });
                }
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
        self.sync_page_viewport();
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
    fn library_preview_content(&self) -> Option<PreviewContent> {
        if self.sidebar_navigation.controller.selected() != SidebarPage::Library || !self.native_page_content_visible() { return None; }
        let tab = self.browser.active_tab()?;
        let (viewport_generation, owner) = if let Some(pages) = &self.pages {
            let pages = pages.try_borrow().ok()?;
            (Some(pages.viewport_generation()), Some(pages.capture_ui_page_focus_owner(tab.id)?))
        } else { (None, None) };
        Some(PreviewContent { viewport: self.native_page_viewport(), viewport_generation,
            space: self.browser.space().id, profile: self.browser.active_user_profile_id(),
            tab: tab.id, url: tab.url.clone(), owner })
    }
    fn library_preview_content_is_current(&self, content: &PreviewContent) -> bool {
        if self.sidebar_navigation.controller.selected() != SidebarPage::Library
            || content.viewport != self.native_page_viewport()
            || content.space != self.browser.space().id
            || content.profile != self.browser.active_user_profile_id()
            || !self.browser.active_tab().is_some_and(|tab| tab.id == content.tab && tab.url == content.url)
        { return false; }
        match (&self.pages, &content.owner) {
            (None, None) => content.viewport_generation.is_none(),
            (Some(pages), Some(owner)) => pages.try_borrow().is_ok_and(|pages|
                Some(pages.viewport_generation()) == content.viewport_generation && pages.accepts_ui_page_focus_owner(owner)),
            _ => false,
        }
    }
    fn library_preview_content_is_rendered(&self) -> bool {
        self.sidebar_navigation.preview_rendered_content.borrow().as_ref()
            .is_some_and(|rendered| self.library_preview_content_is_current(rendered))
    }
    /// Validate before observing main-window focus: its retained GPUI handle may
    /// describe the old document while CEF or an overlay owns the responder.
    pub(crate) fn remember_library_preview_focus(&mut self, window: &Window, cx: &gpui::App) {
        if self.sidebar_navigation.preview_ui_focus.as_ref().is_some_and(|saved|
            !self.library_preview_content_is_current(&saved.content)
                || !self.library_preview_content_is_rendered()
                || !saved.scope.contains(&saved.focus, window))
        { self.sidebar_navigation.preview_ui_focus = None; }
        if self.sidebar_navigation.preview_native_focus.as_ref().is_some_and(|saved|
            !self.library_preview_content_is_current(&saved.content))
        { self.sidebar_navigation.preview_native_focus = None; }
        if self.sidebar_navigation.preview_native_intent.as_ref().is_some_and(|saved|
            !self.library_preview_content_is_current(&saved.content))
        { self.sidebar_navigation.preview_native_intent = None; }
        if self.sidebar_navigation.controller.selected() != SidebarPage::Library
            || !self.sidebar_root_may_take_focus() || !self.native_page_content_visible()
            || self.sidebar_navigation.preview_native_focus.is_some()
            || self.sidebar_navigation.preview_native_intent.is_some()
            || !self.library_preview_content_is_rendered()
        { return; }
        if let Some(pages) = &self.pages {
            let Ok(pages) = pages.try_borrow() else { return; };
            if pages.capture_resize_focus().has_native_focus() { return; }
        }
        if let Some(scope) = self.sidebar_navigation.preview_focus_scope.as_ref().filter(|scope| scope.contains_focused(window, cx)) {
            if let (Some(focus), Some(content)) = (window.focused(cx), self.library_preview_content()) {
                self.sidebar_navigation.media_search_focus = false;
                self.sidebar_navigation.preview_native_intent = None;
                self.sidebar_navigation.preview_ui_focus = Some(PreviewUiResponder { content, scope: scope.clone(), focus });
            }
        }
    }
    /// Shared capture entry point before a popup replaces responder intent.
    pub(crate) fn remember_library_popup_focus(&mut self, window: &Window, cx: &gpui::App) {
        self.remember_downloads_control(window, cx);
        self.remember_library_preview_focus(window, cx);
        self.remember_library_native_popup_focus();
    }
    /// Save before an overlay clears Pages' responder intent. ResizePageFocus
    /// already captures exact identity/profile/attachment/host and inspector
    /// frontend generation; restoration revalidates every one of them.
    pub(crate) fn remember_library_native_popup_focus(&mut self) {
        if !self.sidebar_root_may_take_focus() { return; }
        let Some(content) = self.library_preview_content() else { return; };
        let Some(target) = self.pages.as_ref().and_then(|pages| pages.try_borrow().ok().map(|pages| pages.capture_resize_focus()))
            .filter(|target| target.has_native_focus()) else { return; };
        let saved = PreviewNativeResponder { content, target };
        self.sidebar_navigation.preview_native_intent = Some(saved.clone());
        self.sidebar_navigation.preview_native_focus = Some(saved);
        self.sidebar_navigation.preview_ui_focus = None;
        self.sidebar_navigation.media_search_focus = false;
        self.sidebar_navigation.downloads.focus = None;
    }
    /// Popup owners that already replaced Pages intent still share the same
    /// validated restore path. This dormant snapshot never moves focus by itself.
    pub(crate) fn prepare_library_popup_restore(&mut self) {
        if self.sidebar_navigation.preview_native_focus.is_none() {
            if let Some(saved) = self.sidebar_navigation.preview_native_intent.as_ref()
                .filter(|saved| self.library_preview_content_is_current(&saved.content))
            { self.sidebar_navigation.preview_native_focus = Some(saved.clone()); }
        }
    }
    pub(crate) fn focus_sidebar_page(
        &mut self,
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
            if self.sidebar_navigation.controller.selected() == SidebarPage::Library {
                self.prepare_library_popup_restore();
                if let Some(saved) = self.sidebar_navigation.preview_native_focus.take() {
                    if self.library_preview_content_is_current(&saved.content) {
                        if let Some(pages) = &self.pages {
                            if pages.borrow_mut().restore_resize_focus(saved.target.clone()) {
                                self.sidebar_navigation.preview_native_intent = Some(saved);
                                return;
                            }
                        }
                    }
                }
                self.sidebar_navigation.preview_native_intent = None;
                if let Some(saved) = self.sidebar_navigation.preview_ui_focus.take() {
                    if self.library_preview_content_is_current(&saved.content) && self.library_preview_content_is_rendered()
                        && saved.scope.contains(&saved.focus, window) {
                        saved.focus.focus(window);
                        return;
                    }
                }
            }
            if self.sidebar_navigation.controller.selected() == SidebarPage::Library
                && self.sidebar_navigation.library_section == LibrarySection::Media
                && self.sidebar_navigation.media_search_focus
            {
                if let Some(media) = &self.sidebar_navigation.media {
                    media.read(cx).focus_search(window, cx);
                    return;
                }
            }
            if self.restore_downloads_control(window, cx) { return; }
            self.focus_handle.focus(window);
            if self.page_content_visible() {
                if let Some(pages) = &self.pages {
                    pages.borrow_mut().focus_shown();
                }
            }
        }
    }
    pub(crate) fn library_downloads_controls_took_focus(&mut self) {
        self.invalidate_sidebar_focus();
        self.invalidate_navigation_focus();
        self.sidebar_navigation.media_search_focus = false;
        self.sidebar_navigation.preview_ui_focus = None;
        self.sidebar_navigation.preview_native_focus = None;
        self.sidebar_navigation.preview_native_intent = None;
        let root = self.sidebar_root_may_take_focus();
        if let Some(pages) = &self.pages {
            if root { pages.borrow_mut().focus_ui(); }
            else { pages.borrow_mut().request_ui_focus(); }
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
            LibraryIntent::Navigate(section @ (LibrarySection::Media | LibrarySection::Downloads | LibrarySection::Spaces | LibrarySection::Boosts | LibrarySection::ArchivedTabs)) => {
                self.retire_library_downloads_route();
                self.invalidate_sidebar_focus();
                self.sidebar_navigation.library_section = section;
                self.sync_navigation_visibility();
                self.defer_sidebar_focus(cx);
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
            LibraryIntent::EditTheme(space) => self.open_theme_editor(space, window, cx),
            LibraryIntent::Overflow(space) => {
                if self.browser.spaces.iter().any(|entry| entry.id == space) {
                    self.open_space_menu(space, gpui::point(px(0.), px(48.)), window, cx);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn open_library_boosts(&mut self,cx:&mut Context<Self>){
        self.open_library_section(LibrarySection::Boosts, cx);
    }
    pub(crate) fn open_library_media(&mut self,cx:&mut Context<Self>){
        self.open_library_section(LibrarySection::Media, cx);
    }
    fn open_library_section(&mut self, section: LibrarySection, cx:&mut Context<Self>){
        if crate::quit::is_quitting()||self.browser.is_private()||self.browser.is_blank(){return;}
        self.select_sidebar_page(SidebarPage::Library,cx);
        self.retire_library_downloads_route();
        self.invalidate_sidebar_focus();
        self.sidebar_navigation.library_section=section;
        self.sync_navigation_visibility();
        self.defer_sidebar_focus(cx);
        cx.notify();
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
        let theme = self.theme_colors();
        let colors = LibraryColors {
            background: gpui::transparent_black(),
            card_surface: |hue| crate::window::tint(hue / 360., 0.82),
            text: theme.foreground,
            muted: theme.muted,
            hover: theme.hover,
            selected: theme.selected,
        };
        let section = self.sidebar_navigation.library_section;
        let preview_left = self.library_preview_left();
        let preview = (section == LibrarySection::Media).then(|| self.render_library_page_preview(cx));
        let mut empty = (section == LibrarySection::Media).then(|| {
            // Reuse existing host semantic surfaces; never change App appearance.
            let media_colors = library::media::MediaColors {
                column: colors.background,
                preview: self.theme_empty_panel_fill(),
                text: colors.text,
                muted: colors.muted,
                control: colors.selected,
                hover: colors.hover,
            };
            let media = self
                .sidebar_navigation
                .media
                .get_or_insert_with(|| {
                    cx.new(|cx| library::media::MediaView::new(media_colors, cx))
                })
                .clone();
            if self.sidebar_navigation.media_events.is_none() {
                self.sidebar_navigation.media_events = Some(cx.subscribe(&media, |this, _, event: &library::media::MediaIntent, _| {
                    if *event == library::media::MediaIntent::SearchFocused {
                        this.sidebar_navigation.media_search_focus = true;
                        this.sidebar_navigation.preview_ui_focus = None;
                        this.sidebar_navigation.preview_native_focus = None;
                        this.sidebar_navigation.preview_native_intent = None;
                        if let Some(pages) = &this.pages { pages.borrow_mut().focus_ui(); }
                    }
                }));
            }
            media.update(cx, |media, cx| {
                media.set_colors(media_colors, cx);
                media.set_hosted_preview(cx);
            });
            // LearnMoreRequested has no navigation subscriber until its exact
            // target is verified. The input retains a window-local query.
            div().relative().size_full().child(media)
                .child(library::media::render_preview_at(media_colors.preview, preview, preview_left))
                .into_any_element()
        });
        if section == LibrarySection::Downloads {
            empty = self.sidebar_navigation.downloads.host.as_ref().map(|host| host.view().into_any_element());
        }
        if section==LibrarySection::Boosts {
            let owner=cx.weak_entity();
            let view=self.sidebar_navigation.boosts.get_or_insert_with(||cx.new(|cx|crate::boost_library::BoostLibrary::new(owner,colors,cx))).clone();
            view.update(cx,|view,_|view.set_colors(colors));
            empty=Some(view.into_any_element());
        }
        if section == LibrarySection::ArchivedTabs {
            let owner = cx.weak_entity();
            let fill = self.theme_empty_panel_fill();
            let view = self.sidebar_navigation.archive.get_or_insert_with(|| cx.new(|cx| crate::library_archive::ArchiveLibrary::new(owner, colors, fill, cx))).clone();
            view.update(cx, |view, _| view.set_colors(colors, fill));
            empty = Some(view.into_any_element());
        }
        library::render_library(
            &projection,
            section,
            colors,
            LibraryActions {
                theme: true,
                profile: true,
                media: true,
                downloads: true,
                boosts: true,
                archive: true,
                ..Default::default()
            },
            events,
            |card, _| self.render_library_sidebar(card, cx),
            empty,
        )
    }
    /// Mount inside the presenter-owned preview rectangle (Media or Downloads).
    /// Bounds are GPUI layout coordinates, never rail/list arithmetic.
    pub(crate) fn render_library_page_preview(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let scope = self.sidebar_navigation.preview_focus_scope.get_or_insert_with(|| cx.focus_handle()).clone();
        let mut surface = div().debug_selector(|| "library-page-canvas".into()).relative().flex_none().w(self.sidebar_navigation.normal_content_size.width).h(self.sidebar_navigation.normal_content_size.height).flex().flex_col().track_focus(&scope)
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                // Bubble after the preview's own editor/control has selected its
                // responder. CEF page clicks do not pass through this GPUI tree.
                this.sidebar_navigation.downloads.focus = None;
                this.sidebar_navigation.preview_native_focus = None;
                this.sidebar_navigation.preview_native_intent = None;
                if this.history_is_open() {
                    if let Some(pages) = &this.pages { pages.borrow_mut().focus_ui(); }
                }
                this.remember_library_preview_focus(window, cx);
                if this.sidebar_navigation.preview_focus_scope.as_ref().is_some_and(|scope| scope.contains_focused(window, cx)) {
                    window.prevent_default();
                }
            }));
        if self.browser.active_split().is_some() {
            surface = surface.child(self.render_split(cx));
        } else if self.history_is_open() {
            surface = surface.child(self.render_history(cx));
        } else if let Some(tab) = self.browser.active_tab().filter(|tab| browser_core::omnibox::is_internal_url(&tab.url)) {
            // Match the normal content contract for unsupported internal pages.
            surface = surface.child(div().size_full().flex().flex_col().items_center().justify_center()
                .child("Page not found").child(tab.url.clone()));
        }
        let rendered = self.sidebar_navigation.preview_rendered_content.clone();
        let content = self.library_preview_content();
        surface = surface.child(gpui::canvas(|_, _, _| (), move |_, _, _, _| {
            *rendered.borrow_mut() = content.clone();
        }).absolute().inset_0());
        surface = surface.children(self.render_page_viewport_canvas());
        surface.into_any_element()
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
        let saved_render_width = self.sidebar_render_width.replace(crate::window::SIDEBAR_WIDTH);
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
        self.sidebar_render_width = saved_render_width;
        self.sidebar_page_interactive = saved_interactive;
        content
    }
}

#[cfg(test)]
#[path = "sidebar_navigation_tests.rs"]
mod tests;
