//! A local import is published atomically into a newly allocated Space.
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
impl Browser {
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
        let space = next.add_space(name, self.space().hue);
        next.assign_space_user_profile(space, profile)
            .map_err(|_| "Cannot assign the imported Space.")?;
        let index = next
            .spaces
            .iter()
            .position(|candidate| candidate.id == space)
            .unwrap();
        next.switch_space(index);
        let mut counts = ImportCounts::default();
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
        bookmarks(&mut next, space, None, &data.bookmarks, now, &mut counts)?;
        // new_tab inserts at the front; preserve source window/tab ordering.
        for page in data.tabs.iter().rev().filter(|page| web(&page.url)) {
            let id = next.new_tab(page.url.clone(), now);
            next.set_title(id, page.title.clone());
            counts.tabs += 1;
        }
        let target = next.user_profile_mut(profile).unwrap();
        let mut history_index = target
            .history
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.url.clone(), i))
            .collect::<std::collections::HashMap<_, _>>();
        for entry in &data.history {
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
        *self = next;
        Ok((space, counts))
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
}
