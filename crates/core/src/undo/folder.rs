//! Folder transactions retain tree positions, never copies of live pages.
use super::*;
use crate::folders::Folder;

#[derive(Clone, Debug)]
pub(crate) struct Slot {
    pub space: SpaceId,
    pub parent: Option<FolderId>,
    order: Vec<PinnedItem>,
}
impl Slot {
    pub(crate) fn capture(browser: &Browser, id: FolderId) -> Option<Self> {
        let space = browser
            .spaces
            .iter()
            .find(|s| s.folders.iter().any(|f| f.id == id))?;
        let item = PinnedItem::Folder(id);
        if space.pinned_items.contains(&item) {
            Some(Self {
                space: space.id,
                parent: None,
                order: space.pinned_items.clone(),
            })
        } else {
            let parent = space.folders.iter().find(|f| f.items.contains(&item))?;
            Some(Self {
                space: space.id,
                parent: Some(parent.id),
                order: parent.items.clone(),
            })
        }
    }
    fn place(&self, browser: &mut Browser, id: FolderId) -> bool {
        let Some(space) = browser.spaces.iter_mut().find(|s| s.id == self.space) else {
            return false;
        };
        let pool = match self.parent {
            Some(id) => {
                let Some(folder) = space.folders.iter_mut().find(|f| f.id == id) else {
                    return false;
                };
                &mut folder.items
            }
            None => &mut space.pinned_items,
        };
        let item = PinnedItem::Folder(id);
        pool.retain(|i| *i != item);
        pool.insert(insertion_index(pool, &self.order, item), item);
        true
    }
}

#[derive(Clone, Debug)]
pub(crate) enum FolderEdit {
    Rename {
        id: FolderId,
        space: SpaceId,
        profile: crate::UserProfileId,
        before: String,
        after: String,
    },
    Presence {
        folder: Folder,
        slot: Slot,
        profile: crate::UserProfileId,
        created: bool,
    },
    Move {
        id: FolderId,
        before: Slot,
        after: Slot,
        profile: crate::UserProfileId,
    },
}
impl FolderEdit {
    pub(crate) fn presence(browser: &Browser, id: FolderId, created: bool) -> Option<Self> {
        let folder = browser.folder(id)?.clone();
        if !folder.items.is_empty() || folder.live.is_some() {
            return None;
        }
        let slot = Slot::capture(browser, id)?;
        let profile = browser
            .spaces
            .iter()
            .find(|s| s.id == slot.space)?
            .user_profile_id;
        Some(Self::Presence {
            folder,
            slot,
            profile,
            created,
        })
    }
    pub(crate) fn apply(&mut self, redo: bool, browser: &mut Browser) -> bool {
        match self {
            Self::Rename {
                id,
                space,
                profile,
                before,
                after,
            } => {
                let (source, target) = if redo {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                if browser.folder_space(*id) != Some(*space)
                    || !browser
                        .spaces
                        .iter()
                        .any(|s| s.id == *space && s.user_profile_id == *profile)
                    || browser.folder(*id).is_none_or(|f| f.name != *source)
                {
                    return false;
                }
                browser.rename_folder(*id, target)
            }
            Self::Presence {
                folder,
                slot,
                profile,
                created,
            } => {
                if !browser
                    .spaces
                    .iter()
                    .any(|s| s.id == slot.space && s.user_profile_id == *profile)
                {
                    return false;
                }
                if redo == *created {
                    if browser.folder(folder.id).is_some()
                        || slot.parent.is_some_and(|id| {
                            browser.folder_space(id) != Some(slot.space)
                                || browser.folder(id).is_none_or(|f| f.live.is_some())
                        })
                    {
                        return false;
                    }
                    if slot.parent.is_some_and(|parent| {
                        browser
                            .pinned_rows(slot.space, true)
                            .iter()
                            .find(|row| row.item == PinnedItem::Folder(parent))
                            .is_none_or(|row| row.depth + 1 >= crate::folders::MAX_FOLDER_DEPTH)
                    }) {
                        return false;
                    }
                    browser
                        .spaces
                        .iter_mut()
                        .find(|s| s.id == slot.space)
                        .unwrap()
                        .folders
                        .push(folder.clone());
                    slot.place(browser, folder.id)
                } else {
                    let Some(current) = browser.folder(folder.id) else {
                        return false;
                    };
                    if !current.items.is_empty() || current.live.is_some() {
                        return false;
                    }
                    if Slot::capture(browser, folder.id)
                        .is_none_or(|s| s.space != slot.space || s.parent != slot.parent)
                    {
                        return false;
                    }
                    let saved = current.clone();
                    if browser.delete_empty_folder(folder.id) {
                        *folder = saved;
                        true
                    } else {
                        false
                    }
                }
            }
            Self::Move {
                id,
                before,
                after,
                profile,
            } => {
                let (source, target) = if redo {
                    (before, after)
                } else {
                    (after, before)
                };
                if ![source.space, target.space].iter().all(|id| {
                    browser
                        .spaces
                        .iter()
                        .any(|s| s.id == *id && s.user_profile_id == *profile)
                }) || Slot::capture(browser, *id)
                    .is_none_or(|s| s.space != source.space || s.parent != source.parent)
                    || !browser.move_folder(*id, target.space, target.parent)
                {
                    return false;
                }
                target.place(browser, *id)
            }
        }
    }
}

impl Browser {
    pub fn rename_folder_with_undo(&mut self, id: FolderId, name: &str) -> bool {
        let Some(space) = self.folder_space(id) else {
            return false;
        };
        let profile = self
            .spaces
            .iter()
            .find(|s| s.id == space)
            .unwrap()
            .user_profile_id;
        let before = self.folder(id).unwrap().name.clone();
        if !self.rename_folder(id, name) {
            return false;
        }
        let after = self.folder(id).unwrap().name.clone();
        if before != after {
            self.record_folder_edit(FolderEdit::Rename {
                id,
                space,
                profile,
                before,
                after,
            });
        }
        true
    }
    /// Collect already-supported edits into one user-visible Undo transaction.
    pub(crate) fn sidebar_batch<T>(&mut self, action: impl FnOnce(&mut Self) -> T) -> T {
        if self.sidebar_history.collecting {
            return action(self);
        }
        let history = std::mem::take(&mut self.sidebar_history);
        // The history limit counts user actions, not children of one action.
        self.sidebar_history.collecting = true;
        let result = action(self);
        let edits = std::mem::take(&mut self.sidebar_history.undo);
        self.sidebar_history = history;
        if !edits.is_empty() {
            self.sidebar_history.record(SidebarEdit::Batch(edits));
        }
        result
    }
    pub(crate) fn record_folder_edit(&mut self, edit: FolderEdit) {
        self.sidebar_history.record(SidebarEdit::Folder(edit));
    }
}
