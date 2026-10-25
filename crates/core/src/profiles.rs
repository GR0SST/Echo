//! User profiles own browsing data inside one globally ordered workspace.
mod repair;
use crate::{
    Browser, DEFAULT_ARCHIVE_AFTER, SpaceId, Tab, TabId, favicons::SiteIcon, history::HistoryEntry,
    splits::SplitView,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Opaque, persistent workspace identity. Names and storage paths are not IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserProfileId(pub(crate) u64);
impl UserProfileId {
    pub(crate) const DEFAULT: Self = Self(1);
    /// Opaque engine key. Canonical Default uses the engine's existing
    /// default context; every other profile has an immutable lowercase key.
    pub fn storage_key(self) -> Option<String> {
        (self != Self::DEFAULT).then(|| format!("user-profile-{}", self.0))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub settings: crate::settings::ProfileSettings,
    pub(crate) id: UserProfileId,
    pub(crate) name: String,
    #[serde(default)]
    pub favorites: Vec<Tab>,
    #[serde(default)]
    pub favorite_splits: Vec<SplitView>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    pub archive_after: Option<Duration>,
    #[serde(default = "crate::default_enabled")]
    pub include_favorites_in_ordering: bool,
    #[serde(default)]
    pub empty_favorites_dismissed: bool,
    #[serde(default)]
    pub favicon_cache: Vec<SiteIcon>,
    #[serde(default)]
    pub(crate) downloads: crate::downloads::CompletedDownloads,
    #[serde(default)]
    pub boosts: crate::boosts::Boosts,
    #[serde(default)]
    pub captures: crate::captures::Captures,
    #[serde(default)]
    pub site_settings: crate::site_settings::SiteSettings,
}
impl UserProfile {
    pub fn id(&self) -> UserProfileId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn default_profile() -> Self {
        Self::empty(
            UserProfileId::DEFAULT,
            "Default".into(),
            Some(DEFAULT_ARCHIVE_AFTER),
        )
    }
    fn empty(id: UserProfileId, name: String, archive_after: Option<Duration>) -> Self {
        Self {
            settings: crate::settings::ProfileSettings::default(),
            id,
            name,
            include_favorites_in_ordering: true,
            favorites: Vec::new(),
            favorite_splits: Vec::new(),
            history: Vec::new(),
            archive_after,
            empty_favorites_dismissed: false,
            favicon_cache: Vec::new(),
            downloads: crate::downloads::CompletedDownloads::default(),
            boosts: crate::boosts::Boosts::default(),
            captures: crate::captures::Captures::default(),
            site_settings: crate::site_settings::SiteSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserProfileError {
    UnknownProfile,
    UnknownSpace,
    EmptyName,
    DefaultProfile,
    AssignedProfile,
    WindowLocalModel,
}

/// The adapter MUST retire old CEF pages and advance their generation before
/// recreating these IDs in `to`'s storage context. IDs and Space order survive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserProfileTransition {
    pub space: SpaceId,
    pub from: UserProfileId,
    pub to: UserProfileId,
    pub recreate_tabs: Vec<TabId>,
}

impl Browser {
    pub fn default_user_profile_id(&self) -> UserProfileId {
        self.default_user_profile_id
    }
    pub fn active_user_profile_id(&self) -> UserProfileId {
        self.space().user_profile_id
    }
    pub fn user_profiles(&self) -> impl Iterator<Item = &UserProfile> {
        self.profiles.values()
    }
    pub fn user_profile(&self, id: UserProfileId) -> Option<&UserProfile> {
        self.profiles.get(&id)
    }
    /// Explicit ownership for settings/data changes; identity and name stay guarded.
    pub fn user_profile_mut(&mut self, id: UserProfileId) -> Option<&mut UserProfile> {
        self.profiles.get_mut(&id)
    }
    pub fn active_user_profile(&self) -> &UserProfile {
        &self.profiles[&self.active_user_profile_id()]
    }
    pub fn active_user_profile_mut(&mut self) -> &mut UserProfile {
        let id = self.active_user_profile_id();
        self.profiles.get_mut(&id).expect("validated Space profile")
    }
    pub fn user_profile_id_for_tab(&self, id: TabId) -> Option<UserProfileId> {
        self.spaces
            .iter()
            .find(|space| space.tabs().any(|tab| tab.id == id))
            .map(|space| space.user_profile_id)
            .or_else(|| {
                self.profiles
                    .values()
                    .find(|profile| profile.favorites.iter().any(|tab| tab.id == id))
                    .map(|profile| profile.id)
            })
    }
    /// New data is empty. Retention is initialized from Default once; this is
    /// an implementation policy, not a claim of native live propagation.
    pub fn create_user_profile(&mut self, name: &str) -> Result<UserProfileId, UserProfileError> {
        if self.is_blank() || self.is_private() {
            return Err(UserProfileError::WindowLocalModel);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(UserProfileError::EmptyName);
        }
        let id = UserProfileId(self.next_user_profile_id);
        self.next_user_profile_id = self
            .next_user_profile_id
            .checked_add(1)
            .expect("profile identity exhausted");
        let interval = self.profiles[&self.default_user_profile_id].archive_after;
        self.profiles
            .insert(id, UserProfile::empty(id, name.into(), interval));
        Ok(id)
    }
    pub fn rename_user_profile(
        &mut self,
        id: UserProfileId,
        name: &str,
    ) -> Result<(), UserProfileError> {
        if self.is_blank() || self.is_private() { return Err(UserProfileError::WindowLocalModel); }
        let name = name.trim();
        if name.is_empty() {
            return Err(UserProfileError::EmptyName);
        }
        self.profiles
            .get_mut(&id)
            .ok_or(UserProfileError::UnknownProfile)?
            .name = name.into();
        Ok(())
    }
    pub fn delete_user_profile(&mut self, id: UserProfileId) -> Result<(), UserProfileError> {
        if self.is_blank() || self.is_private() { return Err(UserProfileError::WindowLocalModel); }
        if !self.profiles.contains_key(&id) {
            return Err(UserProfileError::UnknownProfile);
        }
        if id == self.default_user_profile_id {
            return Err(UserProfileError::DefaultProfile);
        }
        if self.spaces.iter().any(|space| space.user_profile_id == id) {
            return Err(UserProfileError::AssignedProfile);
        }
        self.profiles.remove(&id);
        self.clear_sidebar_history();
        // Disk cleanup belongs to the future storage adapter. Never decrement the allocator.
        Ok(())
    }
    /// Delete only the captured unused inventory. A newly assigned, missing or
    /// default profile invalidates the whole operation before any removal.
    pub fn delete_unused_user_profiles(&mut self, ids: &[UserProfileId]) -> Result<usize, UserProfileError> {
        if self.is_blank() || self.is_private() { return Err(UserProfileError::WindowLocalModel); }
        let ids: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        for id in &ids {
            if !self.profiles.contains_key(id) { return Err(UserProfileError::UnknownProfile); }
            if *id == self.default_user_profile_id { return Err(UserProfileError::DefaultProfile); }
            if self.spaces.iter().any(|space| space.user_profile_id == *id) { return Err(UserProfileError::AssignedProfile); }
        }
        for id in &ids { self.profiles.remove(id); }
        if !ids.is_empty() { self.clear_sidebar_history(); }
        Ok(ids.len())
    }

    pub fn assign_space_user_profile(
        &mut self,
        space: SpaceId,
        to: UserProfileId,
    ) -> Result<Option<UserProfileTransition>, UserProfileError> {
        if self.is_blank() || self.is_private() {
            return Err(UserProfileError::WindowLocalModel);
        }
        if !self.profiles.contains_key(&to) {
            return Err(UserProfileError::UnknownProfile);
        }
        let space = self
            .spaces
            .iter_mut()
            .find(|entry| entry.id == space)
            .ok_or(UserProfileError::UnknownSpace)?;
        let from = space.user_profile_id;
        if from == to {
            return Ok(None);
        }
        let recreate_tabs = space.tabs().map(|tab| tab.id).collect();
        if space
            .active_tab
            .is_some_and(|id| !space.tabs().any(|tab| tab.id == id))
        {
            space.active_tab = None;
        }
        space.user_profile_id = to;
        let transition = UserProfileTransition {
            space: space.id,
            from,
            to,
            recreate_tabs,
        };
        self.clear_sidebar_history();
        Ok(Some(transition))
    }
    /// Explicit source identity; the caller resolves the normal source when
    /// invoked from private mode. No profile data is copied back on close.
    pub fn new_blank_for_user_profile(&self, id: UserProfileId) -> Result<Self, UserProfileError> {
        let source = self
            .user_profile(id)
            .ok_or(UserProfileError::UnknownProfile)?;
        let mut blank = Self::new_blank();
        blank.settings.theme_appearance = self.settings.theme_appearance;
        blank.profiles.insert(
            id,
            UserProfile::empty(id, source.name.clone(), source.archive_after),
        );
        blank.profiles.get_mut(&id).unwrap().settings = source.settings.clone();
        blank.profiles.get_mut(&id).unwrap().boosts = source.boosts.clone();
        blank.profiles.get_mut(&id).unwrap().site_settings = source.site_settings.clone();
        blank.spaces[0].user_profile_id = id;
        blank.blank_source_user_profile_id = Some(id);
        blank.next_user_profile_id = self.next_user_profile_id;
        Ok(blank)
    }
    pub fn blank_source_user_profile_id(&self) -> Option<UserProfileId> {
        self.blank_source_user_profile_id
    }
}

#[cfg(test)]
mod tests;
