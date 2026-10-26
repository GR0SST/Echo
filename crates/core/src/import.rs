//! A local import is published atomically into newly allocated Spaces.
use crate::{Browser, SpaceId, UserProfileId, folders::FolderId, history::HistoryEntry};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedPage {
    pub title: String,
    pub url: String,
}
#[derive(Clone, Debug)]
pub enum Bookmark {
    Folder {
        title: String,
        children: Vec<Bookmark>,
    },
    Page(ImportedPage),
}
#[derive(Clone, Debug, Default)]
pub struct ImportedData {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    pub tabs: Vec<ImportedPage>,
    /// Whole profiles from a browser with Spaces (Arc), recreated as they were.
    pub profiles: Vec<ImportedProfile>,
}
/// A source profile with its own Spaces and Favorites.
#[derive(Clone, Debug, Default)]
pub struct ImportedProfile {
    /// `None` imports into the chosen destination profile; a name imports
    /// into the local profile of that name, created if needed.
    pub name: Option<String>,
    pub spaces: Vec<ImportedSpace>,
    pub favorites: Vec<ImportedPage>,
    pub history: Vec<HistoryEntry>,
}
#[derive(Clone, Debug, Default)]
pub struct ImportedSpace {
    pub title: String,
    pub hue: Option<f32>,
    pub pinned: Vec<Bookmark>,
    pub tabs: Vec<ImportedPage>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub bookmarks: usize,
    pub folders: usize,
    pub history: usize,
    pub tabs: usize,
}

fn web(url: &str) -> bool {
    url::Url::parse(url)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}
fn bookmarks(
    browser: &mut Browser,
    space: SpaceId,
    parent: Option<FolderId>,
    nodes: &[Bookmark],
    now: SystemTime,
    counts: &mut ImportCounts,
) -> Result<(), &'static str> {
    for node in nodes {
        match node {
            Bookmark::Folder { title, children } => {
                let folder = browser
                    .create_folder(
                        space,
                        parent,
                        if title.trim().is_empty() {
                            "Untitled"
                        } else {
                            title
                        },
                    )
                    .ok_or("A bookmark folder exceeds the supported nesting depth.")?;
                counts.folders += 1;
                bookmarks(browser, space, Some(folder), children, now, counts)?;
            }
            Bookmark::Page(page) if web(&page.url) => {
                let id = browser.new_tab(page.url.clone(), now);
                browser.set_title(id, page.title.clone());
                if !page.title.trim().is_empty() {
                    browser.rename_tab(id, &page.title);
                }
                browser.toggle_pin(id);
                if let Some(parent) = parent {
                    if !browser.move_item_to_folder(id, parent) {
                        return Err("Cannot place an imported bookmark.");
                    }
                }
                counts.bookmarks += 1;
            }
            _ => {}
        }
    }
    Ok(())
}
impl Browser {
    /// Adds a Space owned by `profile` and makes it current.
    fn import_space(
        &mut self,
        profile: UserProfileId,
        name: &str,
        hue: f32,
    ) -> Result<SpaceId, &'static str> {
        let space = self.add_space(name, hue);
        self.assign_space_user_profile(space, profile)
            .map_err(|_| "Cannot assign the imported Space.")?;
        let index = self.spaces.iter().position(|candidate| candidate.id == space).unwrap();
        self.switch_space(index);
        Ok(space)
    }
    /// Tabs open in the current Space, in source order.
    fn import_tabs(&mut self, tabs: &[ImportedPage], now: SystemTime, counts: &mut ImportCounts) {
        // new_tab inserts at the front; preserve source window/tab ordering.
        for page in tabs.iter().rev().filter(|page| web(&page.url)) {
            let id = self.new_tab(page.url.clone(), now);
            self.set_title(id, page.title.clone());
            counts.tabs += 1;
        }
    }
    fn import_history(
        &mut self,
        profile: UserProfileId,
        history: &[HistoryEntry],
        counts: &mut ImportCounts,
    ) {
        let target = self.user_profile_mut(profile).unwrap();
        let mut history_index = target
            .history
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.url.clone(), i))
            .collect::<std::collections::HashMap<_, _>>();
        for entry in history {
            if !web(&entry.url) {
                continue;
            }
            if let Some(index) = history_index.get(&entry.url) {
                let previous = &mut target.history[*index];
                let visits = previous.visit_count.max(entry.visit_count);
                if previous.visited_at < entry.visited_at {
                    *previous = entry.clone();
                }
                previous.visit_count = visits.max(1);
            } else {
                history_index.insert(entry.url.clone(), target.history.len());
                let mut entry = entry.clone();
                entry.visit_count = entry.visit_count.max(1);
                target.history.push(entry);
            }
            counts.history += 1;
        }
        target
            .history
            .sort_by_key(|entry| std::cmp::Reverse(entry.visited_at));
    }
    pub fn import_local(
        &mut self,
        profile: UserProfileId,
        name: &str,
        data: &ImportedData,
        now: SystemTime,
    ) -> Result<(SpaceId, ImportCounts), &'static str> {
        if self.is_private() || self.is_blank() || self.user_profile(profile).is_none() {
            return Err("Choose an existing local profile.");
        }
        if name.trim().is_empty() {
            return Err("The imported Space needs a name.");
        }
        let mut next = self.clone();
        let mut counts = ImportCounts::default();
        let mut first = None;
        // Plain bookmarks and tabs go to one Space named after the source.
        if data.profiles.is_empty() || !data.bookmarks.is_empty() || !data.tabs.is_empty() {
            let space = next.import_space(profile, name, self.space().hue)?;
            bookmarks(&mut next, space, None, &data.bookmarks, now, &mut counts)?;
            next.import_tabs(&data.tabs, now, &mut counts);
            first = Some(space);
        }
        next.import_history(profile, &data.history, &mut counts);
        for imported in &data.profiles {
            let owner = match imported.name.as_deref().map(str::trim) {
                None | Some("") => profile,
                Some(name) => {
                    let existing = next.user_profiles().find(|p| p.name() == name).map(|p| p.id());
                    match existing {
                        Some(existing) => existing,
                        None => next
                            .create_user_profile(name)
                            .map_err(|_| "The import profile could not be created.")?,
                    }
                }
            };
            let mut spaces = imported.spaces.clone();
            if spaces.is_empty() && !imported.favorites.is_empty() {
                // Favorites live in a profile's Spaces; give them one.
                spaces.push(ImportedSpace {
                    title: imported.name.clone().unwrap_or_else(|| name.to_owned()),
                    ..Default::default()
                });
            }
            let mut home = None;
            for imported_space in &spaces {
                let title = imported_space.title.trim();
                let space = next.import_space(
                    owner,
                    if title.is_empty() { "Space" } else { title },
                    imported_space.hue.unwrap_or(self.space().hue),
                )?;
                bookmarks(&mut next, space, None, &imported_space.pinned, now, &mut counts)?;
                next.import_tabs(&imported_space.tabs, now, &mut counts);
                home.get_or_insert(space);
                first.get_or_insert(space);
            }
            if let Some(home) = home {
                let index = next.spaces.iter().position(|s| s.id == home).unwrap();
                next.switch_space(index);
                for page in imported.favorites.iter().filter(|page| web(&page.url)) {
                    let id = next.new_tab(page.url.clone(), now);
                    next.set_title(id, page.title.clone());
                    if !page.title.trim().is_empty() {
                        next.rename_tab(id, &page.title);
                    }
                    if next.favorite_tab(id) {
                        counts.bookmarks += 1;
                    } else {
                        // Over the Favorites limit: keep it as a pinned tab.
                        next.toggle_pin(id);
                    }
                }
            }
            next.import_history(owner, &imported.history, &mut counts);
        }
        if counts == ImportCounts::default() && first.is_none() {
            return Err("There was nothing to import.");
        }
        let first = first.unwrap_or(next.space().id);
        let index = next.spaces.iter().position(|s| s.id == first).unwrap();
        next.switch_space(index);
        *self = next;
        Ok((first, counts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_preserves_order_folders_and_existing_profile_data_and_skips_unsafe_urls() {
        let mut b = Browser::new();
        let original = b.spaces[0].id;
        let other = b.create_user_profile("Imported").unwrap();
        let time = SystemTime::UNIX_EPOCH;
        b.record_visit(
            other,
            "https://old.example",
            time + std::time::Duration::from_secs(50),
        );
        let page = |title: &str, url: &str| ImportedPage {
            title: title.into(),
            url: url.into(),
        };
        let data = ImportedData {
            bookmarks: vec![
                Bookmark::Folder {
                    title: "Work".into(),
                    children: vec![
                        Bookmark::Page(page("A", "https://a.example")),
                        Bookmark::Folder {
                            title: "Empty".into(),
                            children: vec![],
                        },
                    ],
                },
                Bookmark::Page(page("Bad", "javascript:alert(1)")),
            ],
            tabs: vec![
                page("First", "https://first.example"),
                page("Second", "https://second.example"),
            ],
            history: vec![HistoryEntry {
                url: "https://old.example".into(),
                title: "Older".into(),
                visited_at: time,
                visit_count: 6,
            }],
            profiles: vec![],
        };
        let (space, counts) = b.import_local(other, "From Chrome", &data, time).unwrap();
        assert_ne!(space, original);
        assert_eq!(
            counts,
            ImportCounts {
                bookmarks: 1,
                folders: 2,
                tabs: 2,
                history: 1
            }
        );
        assert_eq!(b.space().user_profile_id, other);
        assert_eq!(
            b.space()
                .today
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            ["First", "Second"]
        );
        let pin = b.space().pinned[0].id;
        b.set_title(pin, "Live title".to_owned());
        assert_eq!(b.space().pinned[0].title, "A");
        assert_eq!(b.space().folders.len(), 2);
        assert_eq!(b.folder_path(b.space().folders[1].id), "Work / Empty");
        assert_eq!(
            b.user_profile(other).unwrap().history[0].visited_at,
            time + std::time::Duration::from_secs(50)
        );
        assert!(b.spaces[0].pinned.is_empty());
        let saved = crate::session::from_json(&crate::session::to_json(&b)).unwrap();
        assert_eq!(saved.space().pinned[0].title, "A");
        assert_eq!(saved.user_profile(other).unwrap().history[0].visit_count, 6);
        b.import_local(other, "Repeated import", &data, time).unwrap();
        assert_eq!(b.user_profile(other).unwrap().history.len(), 1);
        assert_eq!(b.user_profile(other).unwrap().history[0].visit_count, 6);
    }
    #[test]
    fn overdeep_or_private_import_cannot_publish_half_a_space() {
        let mut b = Browser::new();
        let before = crate::session::to_json(&b);
        let mut node = Bookmark::Folder {
            title: "Bottom".into(),
            children: vec![],
        };
        for _ in 0..40 {
            node = Bookmark::Folder {
                title: "Nested".into(),
                children: vec![node],
            };
        }
        assert!(
            b.import_local(
                b.active_user_profile_id(),
                "Import",
                &ImportedData {
                    bookmarks: vec![node],
                    ..Default::default()
                },
                SystemTime::UNIX_EPOCH
            )
            .is_err()
        );
        assert_eq!(crate::session::to_json(&b), before);
        let mut private = Browser::new_private();
        assert!(
            private
                .import_local(
                    private.active_user_profile_id(),
                    "Import",
                    &ImportedData::default(),
                    SystemTime::UNIX_EPOCH
                )
                .is_err()
        );
    }
    #[test]
    fn whole_browser_import_recreates_spaces_favorites_and_profiles() {
        let mut b = Browser::new();
        let main = b.space().user_profile_id;
        let time = SystemTime::UNIX_EPOCH;
        let page = |url: &str| ImportedPage { title: "T".into(), url: url.into() };
        let visit = |url: &str| HistoryEntry { url: url.into(), title: String::new(), visited_at: time, visit_count: 1 };
        let data = ImportedData {
            profiles: vec![
                ImportedProfile {
                    name: None,
                    spaces: vec![
                        ImportedSpace {
                            title: "Home".into(),
                            hue: Some(200.),
                            pinned: vec![Bookmark::Folder { title: "Docs".into(), children: vec![Bookmark::Page(page("https://docs.example"))] }],
                            tabs: vec![page("https://open.example")],
                        },
                        ImportedSpace { title: "Side".into(), ..Default::default() },
                    ],
                    favorites: vec![page("https://mail.example")],
                    history: vec![visit("https://seen.example")],
                },
                ImportedProfile {
                    name: Some("Job".into()),
                    spaces: vec![],
                    favorites: vec![],
                    history: vec![visit("https://job.example")],
                },
            ],
            ..Default::default()
        };
        let before = b.spaces.len();
        let (space, counts) = b.import_local(main, "From Arc", &data, time).unwrap();
        // No extra "From Arc" Space: the source's own Spaces arrive instead.
        assert_eq!(b.spaces.len(), before + 2);
        let home = b.spaces.iter().find(|s| s.id == space).unwrap();
        assert_eq!((home.name.as_str(), home.hue, home.user_profile_id), ("Home", 200., main));
        assert_eq!(home.folders.len(), 1);
        assert_eq!(home.today.len(), 1);
        assert!(b.spaces.iter().any(|s| s.name == "Side"));
        assert_eq!(b.user_profile(main).unwrap().favorites.len(), 1);
        let job = b.user_profiles().find(|p| p.name() == "Job").unwrap();
        assert_eq!(job.history.len(), 1);
        assert_eq!(counts.history, 2);
        assert_eq!(counts.tabs, 1);
    }
}
