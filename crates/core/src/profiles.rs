//! User profiles own browsing data inside one globally ordered workspace.
mod repair;
use crate::{
    Browser, DEFAULT_ARCHIVE_AFTER, SpaceId, Tab, TabId, favicons::SiteIcon,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<crate::live_folders::GitHubAccount>,
    #[serde(default)]
    pub settings: crate::settings::ProfileSettings,
    pub(crate) id: UserProfileId,
    pub(crate) name: String,
    #[serde(default)]
    pub favorites: Vec<Tab>,
    #[serde(default)]
    pub favorite_splits: Vec<SplitView>,
    #[serde(default)]
    pub history: crate::history::History,
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
    #[serde(default)]
    pub browsing_data: crate::browsing_data::BrowsingDataPreferences,
    #[serde(default)]
    pub pinned_extensions: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub extension_developer_mode: bool,
    #[serde(default)]
    pub developer_sites: std::collections::BTreeSet<String>,
    #[serde(default)]
    developer_disabled_sites: std::collections::BTreeSet<String>,
}
impl UserProfile {
    /// Arc enables the developer toolbar for localhost, including its subdomains.
    /// An explicit per-profile choice survives reloads and takes precedence.
    pub fn developer_mode_for_host(&self, host: &str) -> bool {
        !self.developer_disabled_sites.contains(host)
            && (self.developer_sites.contains(host)
                || host == "localhost" || host.ends_with(".localhost"))
    }
    pub fn set_developer_mode_for_host(&mut self, host: String, enabled: bool) {
        if enabled {
            self.developer_disabled_sites.remove(&host);
            self.developer_sites.insert(host);
        } else {
            self.developer_sites.remove(&host);
            self.developer_disabled_sites.insert(host);
        }
    }
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
            github: None,
            settings: crate::settings::ProfileSettings::default(),
            id,
            name,
            include_favorites_in_ordering: true,
            favorites: Vec::new(),
            favorite_splits: Vec::new(),
            history: Default::default(),
            archive_after,
            empty_favorites_dismissed: false,
            favicon_cache: Vec::new(),
            downloads: crate::downloads::CompletedDownloads::default(),
            boosts: crate::boosts::Boosts::default(),
            captures: crate::captures::Captures::default(),
            site_settings: crate::site_settings::SiteSettings::default(),
            browsing_data: Default::default(),
            developer_sites: Default::default(),
            developer_disabled_sites: Default::default(),
            pinned_extensions: Default::default(),
            extension_developer_mode: false,
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
    /// Every tab's profile in one pass, for callers that look up many tabs.
    pub fn tab_profiles(&self) -> std::collections::HashMap<TabId, UserProfileId> {
        let mut map: std::collections::HashMap<_, _> = self
            .spaces
            .iter()
            .flat_map(|space| space.tabs().map(move |tab| (tab.id, space.user_profile_id)))
            .collect();
        for profile in self.profiles.values() {
            for tab in &profile.favorites {
                map.entry(tab.id).or_insert(profile.id);
            }
        }
        map
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
        self.retired_user_profiles.insert(id);
        self.clear_sidebar_history();
        // Retain a durable reclamation request; never decrement the allocator.
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
        self.retired_user_profiles.extend(ids.iter().copied());
        if !ids.is_empty() { self.clear_sidebar_history(); }
        Ok(ids.len())
    }

    /// Only explicit, persisted deletions may reclaim named engine storage.
    /// Live contexts refuse reclamation until a later process starts.
    pub fn retired_profile_storage_keys(&self) -> Vec<String> {
        if self.is_private() || self.is_blank() { return Vec::new(); }
        self.retired_user_profiles.iter().filter_map(|id| id.storage_key()).collect()
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
        let github = self.profiles.get(&to).and_then(|p|p.github.clone());
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
        for folder in &mut space.folders { if let Some(live) = &mut folder.live { live.disconnect(); if let Some(account)=&github { live.credential_id=account.credential_id.clone();live.login=account.login.clone(); } } }
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
        blank.profiles.get_mut(&id).unwrap().developer_sites = source.developer_sites.clone();
        blank.profiles.get_mut(&id).unwrap().developer_disabled_sites = source.developer_disabled_sites.clone();
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
