//! Window-owned draft/controller, independent of GPUI and persistence.
//! Parent hooks and registration are in ../SPACE_CREATION_INTEGRATION.md.
use crate::sidebar_pages::{SidebarPage, sidebar_pages};
use browser_core::{
    Browser, SpaceId,
    space_creation::{SpaceCreation, SpaceCreationError},
};

#[derive(Clone, Debug)]
pub(crate) struct SpaceCreationController {
    selected: SidebarPage,
    draft: Option<SpaceCreation>,
}

impl SpaceCreationController {
    pub(crate) fn new(browser: &Browser) -> Self {
        Self {
            selected: SidebarPage::Space(browser.space().id),
            draft: None,
        }
    }

    pub(crate) fn selected(&self) -> SidebarPage {
        self.selected
    }
    pub(crate) fn draft(&self) -> Option<&SpaceCreation> {
        self.draft.as_ref()
    }

    /// Form fields only: the origin cannot be changed by a picker or index.
    pub(crate) fn edit_draft(&mut self, edit: impl FnOnce(&mut SpaceCreation)) {
        if let Some(draft) = &mut self.draft {
            let origin = draft.origin;
            edit(draft);
            draft.origin = origin;
        }
    }

    pub(crate) fn pages(&self, browser: &Browser) -> Vec<SidebarPage> {
        sidebar_pages(browser, self.draft.as_ref().map(|draft| draft.origin))
    }

    /// All New Space routes call this with the originating real Space ID.
    /// Reopening an existing draft preserves its text and selected profile.
    /// The name is initially empty; parent focus belongs to its TextInput.
    pub(crate) fn begin(
        &mut self,
        browser: &Browser,
        origin: SpaceId,
    ) -> Result<(), SpaceCreationError> {
        if browser.is_blank() || browser.is_private() {
            return Err(SpaceCreationError::WindowLocalModel);
        }
        let source = browser
            .spaces
            .iter()
            .find(|space| space.id == origin)
            .ok_or(SpaceCreationError::UnknownOrigin)?;
        self.reconcile(browser);
        if self.draft.is_none() {
            self.draft = Some(SpaceCreation {
                origin,
                name: String::new(),
                user_profile_id: source.user_profile_id,
                custom_icon: None,
                hue: source.hue,
            });
        }
        self.selected = SidebarPage::CreateSpace;
        Ok(())
    }

    /// Selecting a virtual page is window-local. Parent selects a real Space
    /// in WindowSelection only for SidebarPage::Space; no model edit here.
    pub(crate) fn select(&mut self, browser: &Browser, page: SidebarPage) -> bool {
        if page == SidebarPage::CreateSpace {
            return self.begin(browser, browser.space().id).is_ok();
        }
        self.reconcile(browser);
        if !self.pages(browser).contains(&page) {
            return false;
        }
        self.selected = page;
        true
    }

    pub(crate) fn cancel(&mut self, browser: &Browser) -> SidebarPage {
        if let Some(draft) = self.draft.take() {
            self.selected = SidebarPage::Space(draft.origin);
        }
        self.reconcile(browser);
        self.selected
    }

    /// Pass an owned request into the parent's edit_browser closure. Retain
    /// the draft on errors; call committed only after that closure succeeds.
    pub(crate) fn request(&self) -> Option<SpaceCreation> {
        self.draft.clone()
    }

    pub(crate) fn committed(&mut self, id: SpaceId) {
        self.draft = None;
        self.selected = SidebarPage::Space(id);
    }

    /// Call after every shared-model refresh. Reordering never changes a
    /// draft's origin; disappearance cancels it without allocating anything.
    /// A removed profile remains selected in the draft so submission fails
    /// explicitly instead of silently changing browsing-data ownership.
    pub(crate) fn reconcile(&mut self, browser: &Browser) {
        let exists = |id| browser.spaces.iter().any(|space| space.id == id);
        if self
            .draft
            .as_ref()
            .is_some_and(|draft| !exists(draft.origin))
        {
            self.draft = None;
        }
        if matches!(self.selected, SidebarPage::Space(id) if !exists(id))
            || (self.selected == SidebarPage::CreateSpace && self.draft.is_none())
        {
            self.selected = SidebarPage::Space(browser.space().id);
        }
    }
}

#[cfg(test)]
#[path = "space_creation_tests.rs"]
mod tests;
