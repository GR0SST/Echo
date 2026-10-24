//! Window-owned Library Downloads controller. Reads canonical completed metadata only.
//! No engine events, destination decisions, file actions or navigation live here.
use crate::{
    downloads::project_completed_search,
    downloads_view::{
        DownloadsColors, DownloadsIntent, DownloadsList, DownloadsPreview, DownloadsProjection,
        DownloadsView,
    },
    profile::ProfileHandle,
};
use browser_core::{
    downloads::{CompletedDownload, CompletedDownloadId},
    UserProfileId,
};
use gpui::{App, AppContext, Entity, EntityId, FocusHandle, Window};
use std::{cell::{Cell, RefCell}, rc::Rc};

/// Parent supplies observed chronology/headings/subtitles. SearchEmpty is always
/// chosen by the existing router; the formatter receives only matching records.
/// For zero records with an explicit Profile filter, choose FilteredEmpty.
/// Empty database presentation and unknown dates remain the parent's evidence gate.
pub(crate) type CompletedPresentation =
    Rc<dyn Fn(Vec<CompletedDownload>, bool) -> DownloadsList<CompletedDownloadId>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DownloadsOwner {
    presenter: EntityId,
    generation: u64,
}

pub(crate) struct LibraryDownloadsHost {
    normal: ProfileHandle,
    view: Entity<DownloadsView<CompletedDownloadId>>,
    format: CompletedPresentation,
    artwork: Rc<RefCell<crate::download_files::FileArtwork>>,
    query: String,
    // Native single-chip on/off is observed. Other live-chip combinations stay gated.
    // A removed selection stays a tombstone, never silently broadens to All.
    selected: Option<UserProfileId>,
    generation: u64,
    active: bool,
    preview_active: Rc<Cell<bool>>,
}

impl LibraryDownloadsHost {
    /// Only a normal handle can create this host. Blank must NOT resolve through
    /// its retained normal source: download ingestion and Library eligibility differ.
    pub(crate) fn new(
        normal: ProfileHandle,
        colors: DownloadsColors,
        format: CompletedPresentation,
        preview: DownloadsPreview,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Self> {
        let projection = project_completed_search(
            &normal,
            "",
            |_| false,
            |_| true,
            |records| format(records, false),
        )?;
        // A retired controller must not leave a retained presenter invoking
        // the live owner callback. The accepted callback is passed through as-is.
        let preview_active = Rc::new(Cell::new(false));
        let gate = preview_active.clone();
        let guarded_preview: DownloadsPreview = Rc::new(move |window, cx| {
            if !gate.get() {
                use gpui::IntoElement;
                return gpui::div().into_any_element();
            }
            preview(window, cx)
        });
        let view = cx.new(|cx| DownloadsView::new(projection, colors, guarded_preview, cx));
        let artwork = Rc::new(RefCell::new(crate::download_files::FileArtwork::default()));
        artwork.borrow_mut().update(normal.download_inventory()?.1);
        let render_artwork = artwork.clone();
        view.update(cx, |view, cx| {
            view.observe_control_focus(window, cx);
            view.set_row_selection_enabled(true, cx);
            view.set_row_artwork(Rc::new(move |row, _, cx| render_artwork.borrow_mut().render(row.token, cx)), cx);
        });
        Some(Self {
            normal,
            view,
            format,
            artwork,
            query: String::new(),
            selected: None,
            generation: 0,
            active: false,
            preview_active,
        })
    }

    pub(crate) fn view(&self) -> Entity<DownloadsView<CompletedDownloadId>> {
        self.view.clone()
    }
    pub(crate) fn query(&self) -> &str {
        &self.query
    }
    pub(crate) fn selected_profile(&self) -> Option<UserProfileId> {
        self.selected
    }
    pub(crate) fn owner(&self) -> DownloadsOwner {
        DownloadsOwner {
            presenter: self.view.entity_id(),
            generation: self.generation,
        }
    }

    /// Call at EVERY route eligibility change, including leaving Library,
    /// changing section, owner retirement, or entering a window-local scope.
    /// Parent drops old subscriptions and binds fresh ones with owner() on entry.
    pub(crate) fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.generation = self
                .generation
                .checked_add(1)
                .expect("Downloads owner exhausted");
            self.active = active;
            self.preview_active.set(active);
        }
    }

    fn accepts(
        &self,
        owner: DownloadsOwner,
        sender: &Entity<DownloadsView<CompletedDownloadId>>,
        normal: &ProfileHandle,
    ) -> bool {
        self.active
            && owner == self.owner()
            && sender == &self.view
            && self.normal.same_workspace(normal)
    }

    fn projection(&self) -> Option<DownloadsProjection<CompletedDownloadId>> {
        let selected = self.selected;
        project_completed_search(
            &self.normal,
            &self.query,
            |id| selected == Some(id),
            |id| selected.is_none_or(|selected| selected == id),
            |records| (self.format)(records, selected.is_some()),
        )
    }

    /// Re-read canonical data on every parent profile notification and on section entry.
    /// Do not invoke unconditionally from render: projection replacement notifies.
    /// Never read BrowserWindow's projected snapshot.
    /// Hidden sections also refresh: reopening retains query/filter visibility/ID.
    /// False means owner mismatch or rejected formatter output; parent must unmount,
    /// retire, and rebind as appropriate, never continue displaying stale rows.
    pub(crate) fn refresh(
        &mut self,
        normal: &ProfileHandle,
        colors: DownloadsColors,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if !self.normal.same_workspace(normal) {
            return false;
        }
        let Some(projection) = self.projection() else {
            return false;
        };
        if let Some((_, records)) = self.normal.download_inventory() { self.artwork.borrow_mut().update(records); }
        self.view.update(cx, |view, cx| {
            view.set_colors(colors, cx);
            view.set_projection(projection, window, cx);
        });
        true
    }

    /// The sender and lease are captured by the parent subscription, NOT looked
    /// up when a deferred callback finally runs. Unknown Profile IDs are rejected.
    pub(crate) fn handle_intent(
        &mut self,
        owner: DownloadsOwner,
        sender: &Entity<DownloadsView<CompletedDownloadId>>,
        normal: &ProfileHandle,
        event: &DownloadsIntent<CompletedDownloadId>,
        colors: DownloadsColors,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if !self.accepts(owner, sender, normal) {
            return false;
        }
        match event {
            DownloadsIntent::SearchChanged(query) => self.query = query.clone(),
            DownloadsIntent::ToggleProfile(id) => {
                let Some((profiles, _)) = self.normal.download_inventory() else {
                    return false;
                };
                if !profiles.iter().any(|(actual, _)| actual == id) {
                    return false;
                }
                // A second live Profile chip has unobserved combination semantics.
                // Do not invent exclusive replacement or multi-selection. An
                // explicit click may replace a tombstone after inventory removal.
                if self.selected.is_some_and(|selected| {
                    selected != *id && profiles.iter().any(|(actual, _)| *actual == selected)
                }) {
                    return false;
                }
                self.selected = if self.selected == Some(*id) {
                    None
                } else {
                    Some(*id)
                };
            }
            DownloadsIntent::FilterVisibilityChanged(_) => (),
            // File requests do not mutate query/filter state. The route resolves
            // their captured lease and canonical record before any effect.
            DownloadsIntent::RowPressed { .. } | DownloadsIntent::RowOpened { .. } | DownloadsIntent::RowMenuRequested { .. } => return false,
        }
        self.refresh(normal, colors, window, cx)
    }

    pub(crate) fn record_for_intent(&self, owner: DownloadsOwner,
        sender: &Entity<DownloadsView<CompletedDownloadId>>, normal: &ProfileHandle,
        token: CompletedDownloadId, profile: UserProfileId) -> Option<CompletedDownload> {
        if !self.accepts(owner, sender, normal) || token.profile() != profile { return None; }
        normal.download_inventory()?.1.into_iter().find(|record| record.id() == token)
    }

    /// Parent first validates its overlay continuation. Never fall back to the
    /// default Focusable handle if a removed chip or stale owner fails validation.
    pub(crate) fn restore_control(
        &self,
        owner: DownloadsOwner,
        sender: &Entity<DownloadsView<CompletedDownloadId>>,
        normal: &ProfileHandle,
        focus: &FocusHandle,
        window: &mut Window,
        cx: &App,
    ) -> bool {
        if !self.accepts(owner, sender, normal)
            || !self.view.read(cx).is_control_available(focus, cx)
        {
            return false;
        }
        focus.focus(window);
        true
    }

    /// Called only from DownloadsControlFocused, never around the whole root.
    /// Return the exact current control responder for the parent's popup cache.
    /// Parent must also revalidate its current Library/Downloads/normal route.
    pub(crate) fn control_focus(
        &self,
        owner: DownloadsOwner,
        sender: &Entity<DownloadsView<CompletedDownloadId>>,
        normal: &ProfileHandle,
        window: &Window,
        cx: &App,
    ) -> Option<FocusHandle> {
        if !self.accepts(owner, sender, normal) {
            return None;
        }
        self.view.read(cx).focused_control(window, cx)
    }
}

impl Drop for LibraryDownloadsHost {
    fn drop(&mut self) {
        self.preview_active.set(false);
    }
}

#[cfg(test)]
#[path = "library_downloads_host_tests.rs"]
mod tests;
