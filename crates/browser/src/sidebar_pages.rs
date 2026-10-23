//! Window-local pager identities. Never persist these or use a pager index as
//! a model Space index. Only `SidebarPage::Space` can be a tab drop destination.
use browser_core::{Browser, SpaceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarPage {
    Library,
    Space(SpaceId),
    CreateSpace,
}

impl SidebarPage {
    pub(crate) fn drop_destination(self) -> Option<SpaceId> {
        match self {
            Self::Space(id) => Some(id),
            Self::Library | Self::CreateSpace => None,
        }
    }
}

/// Central placement policy: Library leads; a draft follows its origin;
/// without a draft the creation entry is the right edge. Real order is exact.
/// Missing origins fall back to the right edge until the controller reconciles.
pub(crate) fn sidebar_pages(browser: &Browser, draft_origin: Option<SpaceId>) -> Vec<SidebarPage> {
    let mut pages = Vec::with_capacity(browser.spaces.len() + 2);
    pages.push(SidebarPage::Library);
    let mut placed = false;
    for space in &browser.spaces {
        pages.push(SidebarPage::Space(space.id));
        if draft_origin == Some(space.id) {
            pages.push(SidebarPage::CreateSpace);
            placed = true;
        }
    }
    if !placed {
        pages.push(SidebarPage::CreateSpace);
    }
    pages
}

#[cfg(test)]
#[path = "sidebar_pages_tests.rs"]
mod tests;
