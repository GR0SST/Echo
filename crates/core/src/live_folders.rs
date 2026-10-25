//! Local GitHub Live Folder state. Credentials are opaque keychain references.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullRequestFilter {
    CreatedByMe,
    ReviewRequests,
    Assigned,
    TeamReviewRequests,
}
impl PullRequestFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::CreatedByMe => "Created by Me",
            Self::ReviewRequests => "Review Requests",
            Self::Assigned => "Assigned",
            Self::TeamReviewRequests => "Team Review Requests",
        }
    }
    pub fn qualifier(self) -> &'static str {
        match self {
            Self::CreatedByMe => "author",
            Self::ReviewRequests => "review-requested",
            Self::Assigned => "assignee",
            Self::TeamReviewRequests => "team-review-requested",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveItem {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub repository: String,
    pub updated_at: String,
    #[serde(default)]
    pub read_revision: Option<String>,
}
impl LiveItem {
    pub fn unread(&self) -> bool {
        self.read_revision.as_ref() != Some(&self.updated_at)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitHubAccount {
    pub credential_id: String,
    pub login: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveFolder {
    pub credential_id: String,
    pub login: String,
    pub filters: Vec<PullRequestFilter>,
    #[serde(default)]
    pub hidden_repositories: Vec<String>,
    #[serde(default)]
    pub items: Vec<LiveItem>,
}
impl LiveFolder {
    /// A complete successful snapshot replaces results; errors never erase cached rows.
    pub fn disconnect(&mut self) {
        self.credential_id.clear();
        self.login.clear();
        self.items.clear();
    }
    pub fn replace(&mut self, mut items: Vec<LiveItem>) {
        let read: HashMap<_, _> = self
            .items
            .iter()
            .map(|i| (i.id, i.read_revision.clone()))
            .collect();
        let mut unique = std::collections::HashSet::new();
        items.retain(|i| unique.insert(i.id));
        for item in &mut items {
            item.read_revision = read.get(&item.id).cloned().flatten();
        }
        self.items = items;
    }
    pub fn visible_items(&self) -> impl Iterator<Item = &LiveItem> {
        self.items
            .iter()
            .filter(|i| !self.hidden_repositories.contains(&i.repository))
    }
    pub fn unread_count(&self) -> usize {
        self.visible_items().filter(|i| i.unread()).count()
    }
    pub fn mark_read(&mut self, id: u64) {
        if let Some(i) = self.items.iter_mut().find(|i| i.id == id) {
            i.read_revision = Some(i.updated_at.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(revision: &str) -> LiveItem {
        LiveItem {
            id: 42,
            title: "Fix parser".into(),
            url: "https://github.com/echo/test/pull/1".into(),
            repository: "echo/test".into(),
            updated_at: revision.into(),
            read_revision: None,
        }
    }
    #[test]
    fn updates_preserve_read_state_until_revision_changes() {
        let mut folder = LiveFolder {
            credential_id: "github:profile".into(),
            login: "tester".into(),
            filters: vec![PullRequestFilter::CreatedByMe],
            hidden_repositories: vec![],
            items: vec![],
        };
        folder.replace(vec![item("one")]);
        assert_eq!(folder.unread_count(), 1);
        folder.mark_read(42);
        folder.replace(vec![item("one"), item("one")]);
        assert_eq!(folder.items.len(), 1);
        assert_eq!(folder.unread_count(), 0);
        folder.replace(vec![item("two")]);
        assert_eq!(folder.unread_count(), 1);
        folder.hidden_repositories.push("echo/test".into());
        assert_eq!(folder.unread_count(), 0);
        let encoded = serde_json::to_string(&folder).unwrap();
        let decoded: LiveFolder = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.items, folder.items);
        folder.replace(vec![]);
        assert!(folder.items.is_empty());
    }
}

impl crate::Browser {
    pub fn create_github_live_folder(
        &mut self,
        space: crate::SpaceId,
    ) -> Option<crate::folders::FolderId> {
        if self.is_private() || self.is_blank() {
            return None;
        }
        if let Some(folder) = self
            .spaces
            .iter()
            .find(|s| s.id == space)?
            .folders
            .iter()
            .find(|f| f.live.is_some())
        {
            return Some(folder.id);
        }
        let profile = self.spaces.iter().find(|s| s.id == space)?.user_profile_id;
        let account = self.user_profile(profile)?.github.clone();
        let id = self.create_folder(space, None, "Pull Requests")?;
        let folder = self
            .spaces
            .iter_mut()
            .flat_map(|s| &mut s.folders)
            .find(|f| f.id == id)?;
        folder.live = Some(LiveFolder {
            credential_id: account
                .as_ref()
                .map(|a| a.credential_id.clone())
                .unwrap_or_default(),
            login: account
                .as_ref()
                .map(|a| a.login.clone())
                .unwrap_or_default(),
            filters: vec![
                PullRequestFilter::CreatedByMe,
                PullRequestFilter::ReviewRequests,
                PullRequestFilter::TeamReviewRequests,
            ],
            hidden_repositories: vec![],
            items: vec![],
        });
        Some(id)
    }
    pub fn connect_github_profile(
        &mut self,
        profile: crate::UserProfileId,
        account: GitHubAccount,
    ) -> bool {
        if !self.can_edit_persistent_sidebar() || self.is_private() || self.is_blank() {
            return false;
        }
        let Some(owner) = self.user_profile_mut(profile) else {
            return false;
        };
        owner.github = Some(account.clone());
        for folder in self
            .spaces
            .iter_mut()
            .filter(|s| s.user_profile_id == profile)
            .flat_map(|s| &mut s.folders)
        {
            if let Some(live) = &mut folder.live {
                live.credential_id = account.credential_id.clone();
                live.login = account.login.clone();
                live.items.clear();
            }
        }
        true
    }
    pub fn live_folder_mut(&mut self, id: crate::folders::FolderId) -> Option<&mut LiveFolder> {
        if !self.can_edit_persistent_sidebar() {
            return None;
        }
        self.spaces
            .iter_mut()
            .flat_map(|s| &mut s.folders)
            .find(|f| f.id == id)?
            .live
            .as_mut()
    }
}
#[cfg(test)]
mod ownership_tests {
    use super::*;
    use crate::Browser;
    #[test]
    fn profile_reassignment_disconnects_live_folder_and_removes_private_results() {
        let mut browser = Browser::new();
        let space = browser.space().id;
        let id = browser.create_github_live_folder(space).unwrap();
        assert_eq!(browser.create_github_live_folder(space), Some(id));
        let live = browser.live_folder_mut(id).unwrap();
        live.credential_id = "keychain-reference".into();
        live.login = "tester".into();
        live.items.push(LiveItem {
            id: 1,
            title: "Private title".into(),
            url: "https://github.com/echo/private/pull/1".into(),
            repository: "echo/private".into(),
            updated_at: "one".into(),
            read_revision: None,
        });
        let profile = browser.create_user_profile("Other").unwrap();
        browser.assign_space_user_profile(space, profile).unwrap();
        let live = browser.folder(id).unwrap().live.as_ref().unwrap();
        assert!(live.credential_id.is_empty());
        assert!(live.login.is_empty());
        assert!(live.items.is_empty());
    }
    #[test]
    fn cross_profile_folder_move_is_rejected_and_same_profile_move_preserves_connection() {
        let mut browser = Browser::new();
        let source = browser.space().id;
        let target = browser.add_space("Other", 120.);
        let id = browser.create_github_live_folder(source).unwrap();
        browser.live_folder_mut(id).unwrap().credential_id = "key".into();
        assert!(browser.move_folder(id, target, None));
        assert_eq!(
            browser
                .folder(id)
                .unwrap()
                .live
                .as_ref()
                .unwrap()
                .credential_id,
            "key"
        );
        let profile = browser.create_user_profile("Other").unwrap();
        browser.assign_space_user_profile(source, profile).unwrap();
        assert!(!browser.move_folder(id, source, None));
        assert_eq!(
            browser
                .folder(id)
                .unwrap()
                .live
                .as_ref()
                .unwrap()
                .credential_id,
            "key"
        );
    }
}
#[cfg(test)]
mod connection_tests {
    use super::*;
    #[test]
    fn profile_connection_survives_folder_deletion_and_is_not_inherited_by_other_profiles() {
        let mut browser = crate::Browser::new();
        let space = browser.space().id;
        let profile = browser.active_user_profile_id();
        let folder = browser.create_github_live_folder(space).unwrap();
        assert!(browser.connect_github_profile(
            profile,
            GitHubAccount {
                credential_id: "opaque-keychain-reference".into(),
                login: "tester".into()
            }
        ));
        assert!(browser.delete_empty_folder(folder));
        let recreated = browser.create_github_live_folder(space).unwrap();
        assert_eq!(
            browser
                .folder(recreated)
                .unwrap()
                .live
                .as_ref()
                .unwrap()
                .credential_id,
            "opaque-keychain-reference"
        );
        let other = browser.create_user_profile("Separate identity").unwrap();
        browser.assign_space_user_profile(space, other).unwrap();
        assert!(browser
            .folder(recreated)
            .unwrap()
            .live
            .as_ref()
            .unwrap()
            .credential_id
            .is_empty());
        let json = crate::session::to_json(&browser);
        let restored = crate::session::from_json(&json).unwrap();
        assert_eq!(
            restored
                .user_profile(profile)
                .unwrap()
                .github
                .as_ref()
                .unwrap()
                .login,
            "tester"
        );
        assert!(restored.user_profile(other).unwrap().github.is_none());
    }
}
#[cfg(test)]
mod dynamic_contents_tests {
    #[test]
    fn live_folders_reject_static_children_without_moving_or_allocating_items() {
        let mut browser = crate::Browser::new();
        let space = browser.space().id;
        let live = browser.create_github_live_folder(space).unwrap();
        let normal = browser.create_folder(space, None, "Normal").unwrap();
        let tab = browser.new_tab("https://example.com/", std::time::SystemTime::UNIX_EPOCH);
        let before = crate::session::to_json(&browser);
        assert!(browser.create_folder(space, Some(live), "Nested").is_none());
        assert!(!browser.move_item_to_folder(tab, live));
        assert!(!browser.can_move_folder(normal, space, Some(live)));
        assert_eq!(crate::session::to_json(&browser), before);
        assert!(browser.folder(live).unwrap().items.is_empty());
        assert!(browser.space().today.iter().any(|t| t.id == tab));
    }
}
