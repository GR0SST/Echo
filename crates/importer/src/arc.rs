//! Arc keeps a Chromium profile per Arc profile for history, but its sidebar
//! (Spaces, pinned items, Favorites and open tabs) lives in
//! `StorableSidebar.json` next to `User Data`. All of it is imported, with
//! every Arc Space recreated in the profile it belongs to.
use crate::{Category, ImportIssue, Result, Selection, chromium, read_json};
use browser_core::import::{Bookmark, ImportedData, ImportedPage, ImportedProfile, ImportedSpace};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

fn sidebar_path(root: &Path) -> Option<PathBuf> {
    Some(root.parent()?.join("StorableSidebar.json"))
}
/// Whether `root` (Arc's `User Data`) has Arc's sidebar beside it.
pub fn has_sidebar(root: &Path) -> bool {
    sidebar_path(root).is_some_and(|path| path.is_file())
}

pub fn read_all(
    root: &Path,
    selection: Selection,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Category),
) -> Result<(Vec<ImportedProfile>, Vec<ImportIssue>)> {
    let profiles = crate::browser_profiles(crate::BrowserKind::Arc, root);
    progress(Category::Bookmarks);
    let json = read_json(&sidebar_path(root).ok_or("Arc's sidebar file is missing.")?)?;
    let sidebar = Sidebar::parse(&json)?;
    let directories: Vec<String> = profiles
        .iter()
        .filter_map(|p| p.path.file_name()?.to_str().map(str::to_owned))
        .collect();
    let mut out: Vec<ImportedProfile> = profiles
        .iter()
        .enumerate()
        .map(|(index, p)| ImportedProfile {
            name: (index > 0).then(|| p.name.clone()),
            ..Default::default()
        })
        .collect();
    if out.is_empty() {
        out.push(ImportedProfile::default());
    }
    // Spaces of a profile Arc no longer lists go to the main profile.
    let owner = |directory: &str| directories.iter().position(|d| d == directory).unwrap_or(0);
    for (title, directory, hue, pinned, unpinned) in &sidebar.spaces {
        let mut space = ImportedSpace { title: title.clone(), hue: *hue, ..Default::default() };
        if selection.bookmarks {
            if let Some(pinned) = pinned {
                space.pinned = sidebar.nodes(pinned, 0, cancel)?;
            }
        }
        if selection.tabs {
            if let Some(unpinned) = unpinned {
                flatten(sidebar.nodes(unpinned, 0, cancel)?, &mut space.tabs);
            }
        }
        out[owner(directory)].spaces.push(space);
    }
    if selection.bookmarks {
        for (directory, id) in &sidebar.favorites {
            flatten(sidebar.nodes(id, 0, cancel)?, &mut out[owner(directory)].favorites);
        }
    }
    let mut issues = Vec::new();
    if selection.history {
        progress(Category::History);
        for (profile, imported) in profiles.iter().zip(out.iter_mut()) {
            if cancel.load(Ordering::Relaxed) {
                return Err("Import cancelled.".into());
            }
            let mut data = ImportedData::default();
            match chromium::read(&profile.path, Category::History, cancel, &mut data) {
                Ok(()) => imported.history = data.history,
                Err(message) => issues.push(ImportIssue {
                    category: Category::History,
                    message: format!("{}: {message}", profile.name),
                }),
            }
        }
    }
    Ok((out, issues))
}

fn flatten(nodes: Vec<Bookmark>, out: &mut Vec<ImportedPage>) {
    for node in nodes {
        match node {
            Bookmark::Page(page) => out.push(page),
            Bookmark::Folder { children, .. } => flatten(children, out),
        }
    }
}

/// The hue, in degrees, of a Space's theme color.
fn hue(space: &Value) -> Option<f32> {
    let color = &space["customInfo"]["windowTheme"]["primaryColorPalette"]["midTone"];
    let [r, g, b] = ["red", "green", "blue"].map(|key| color[key].as_f64().map(|v| v as f32));
    let (r, g, b) = (r?, g?, b?);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    if d < 0.02 {
        return None;
    }
    Some(if max == r {
        60. * ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / d + 2.)
    } else {
        60. * ((r - g) / d + 4.)
    })
}

struct Sidebar<'a> {
    items: HashMap<&'a str, &'a Value>,
    /// (title, profile directory, hue, pinned container, unpinned container)
    spaces: Vec<(String, String, Option<f32>, Option<&'a str>, Option<&'a str>)>,
    /// (profile directory, Favorites container)
    favorites: Vec<(String, &'a str)>,
}

/// Arc's profile reference: `{"default": …}` or
/// `{"custom": {"_0": {"directoryBasename": "Profile 2"}}}`.
fn directory(profile: &Value) -> String {
    profile["custom"]["_0"]["directoryBasename"]
        .as_str()
        .unwrap_or("Default")
        .to_owned()
}

/// Arc encodes Swift dictionaries as flat `[key, value, key, value]` arrays.
fn pairs(value: &Value) -> impl Iterator<Item = (&Value, &Value)> {
    value
        .as_array()
        .map(|a| a.chunks_exact(2).map(|p| (&p[0], &p[1])).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
}

impl<'a> Sidebar<'a> {
    fn parse(json: &'a Value) -> Result<Self> {
        let container = json["sidebar"]["containers"]
            .as_array()
            .and_then(|all| all.iter().find(|c| c["spaces"].is_array()))
            .ok_or("Arc's sidebar file has no Spaces.")?;
        let items = container["items"]
            .as_array()
            .ok_or("Arc's sidebar file has no items.")?
            .iter()
            .filter_map(|item| Some((item["id"].as_str()?, item)))
            .collect();
        let spaces = pairs(&container["spaces"])
            .filter_map(|(_, space)| space.as_object().map(|_| space))
            .map(|space| {
                let mut pinned = None;
                let mut unpinned = None;
                for (kind, id) in pairs(&space["containerIDs"]) {
                    match kind.as_str() {
                        Some("pinned") => pinned = id.as_str(),
                        Some("unpinned") => unpinned = id.as_str(),
                        _ => {}
                    }
                }
                let title = space["title"].as_str().filter(|t| !t.trim().is_empty());
                (
                    title.unwrap_or("Space").to_owned(),
                    directory(&space["profile"]),
                    hue(space),
                    pinned,
                    unpinned,
                )
            })
            .collect();
        let favorites = pairs(&container["topAppsContainerIDs"])
            .filter_map(|(profile, id)| Some((directory(profile), id.as_str()?)))
            .collect();
        Ok(Self { items, spaces, favorites })
    }

    fn children(&self, id: &str) -> impl Iterator<Item = &'a Value> + '_ {
        self.items
            .get(id)
            .and_then(|item| item["childrenIds"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|child| self.items.get(child.as_str()?).copied())
    }

    /// Tabs become pages, folders become folders; split views and tab groups
    /// keep their tabs in place.
    fn nodes(&self, id: &str, depth: usize, cancel: &AtomicBool) -> Result<Vec<Bookmark>> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Import cancelled.".into());
        }
        if depth > 64 {
            return Err("Arc's sidebar is nested too deeply.".into());
        }
        let mut nodes = Vec::new();
        for item in self.children(id) {
            let id = item["id"].as_str().unwrap_or_default();
            let data = &item["data"];
            if let Some(page) = page(item) {
                nodes.push(Bookmark::Page(page));
            } else if data.get("list").is_some() {
                nodes.push(Bookmark::Folder {
                    title: item["title"].as_str().unwrap_or("Untitled").to_owned(),
                    children: self.nodes(id, depth + 1, cancel)?,
                });
            } else {
                nodes.extend(self.nodes(id, depth + 1, cancel)?);
            }
        }
        Ok(nodes)
    }
}

fn page(item: &Value) -> Option<ImportedPage> {
    let tab = item["data"].get("tab")?;
    let url = tab["savedURL"].as_str()?.to_owned();
    let title = item["title"]
        .as_str()
        .or_else(|| tab["savedTitle"].as_str())
        .unwrap_or_default()
        .to_owned();
    Some(ImportedPage { title, url })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidebar() -> Value {
        let tab = |id: &str, parent: &str, url: &str, title: Value| {
            serde_json::json!({"id": id, "parentID": parent, "title": title, "childrenIds": [],
                "data": {"tab": {"savedURL": url, "savedTitle": "Saved"}}})
        };
        let container = |id: &str, children: &[&str]| {
            serde_json::json!({"id": id, "title": null, "childrenIds": children,
                "data": {"itemContainer": {"containerType": {}}}})
        };
        let items = vec![
            container("fav", &["f1"]),
            tab("f1", "fav", "https://mail.example", "Mail".into()),
            container("pin", &["list", "split"]),
            serde_json::json!({"id": "list", "title": "Docs", "childrenIds": ["p1"], "data": {"list": {}}}),
            tab("p1", "list", "https://docs.example", Value::Null),
            serde_json::json!({"id": "split", "title": null, "childrenIds": ["p2"], "data": {"splitView": {}}}),
            tab("p2", "split", "https://split.example", "Split".into()),
            container("today", &["t1"]),
            tab("t1", "today", "https://open.example", "Open".into()),
            container("other", &["o1"]),
            tab("o1", "other", "https://other.example", "Other".into()),
        ];
        let flat: Vec<Value> = items
            .into_iter()
            .flat_map(|item| [item["id"].clone(), item])
            .collect();
        serde_json::json!({"sidebar": {"containers": [{"global": {}}, {
            "items": flat,
            "spaces": [
                "s1", {"id": "s1", "title": "Home", "profile": {"default": true},
                       "containerIDs": ["pinned", "pin", "unpinned", "today"]},
                "s2", {"id": "s2", "title": "Work",
                       "profile": {"custom": {"_0": {"directoryBasename": "Profile 2"}}},
                       "containerIDs": ["pinned", "other", "unpinned", "other"]},
            ],
            "topAppsContainerIDs": [{"default": true}, "fav"],
        }]}})
    }

    #[test]
    fn arc_imports_every_profile_with_its_spaces_favorites_and_tabs() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("User Data");
        for directory in ["Default", "Profile 2"] {
            std::fs::create_dir_all(data.join(directory)).unwrap();
        }
        std::fs::write(
            data.join("Local State"),
            r#"{"profile":{"info_cache":{"Default":{"name":"Me"},"Profile 2":{"name":"Job"},"__ARC_SYSTEM_PROFILE":{"name":"__ARC"}}}}"#,
        )
        .unwrap();
        std::fs::write(root.path().join("StorableSidebar.json"), sidebar().to_string()).unwrap();
        assert!(has_sidebar(&data));
        let (profiles, _) =
            read_all(&data, Selection::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(profiles.len(), 2);
        let (me, job) = (&profiles[0], &profiles[1]);
        assert_eq!((me.name.as_deref(), job.name.as_deref()), (None, Some("Job")));
        assert_eq!(me.favorites.iter().map(|p| p.url.as_str()).collect::<Vec<_>>(), ["https://mail.example"]);
        assert_eq!(me.spaces.len(), 1);
        let home = &me.spaces[0];
        assert_eq!(home.title, "Home");
        assert!(matches!(&home.pinned[0], Bookmark::Folder { title, children }
            if title == "Docs" && matches!(&children[0], Bookmark::Page(p) if p.title == "Saved")));
        assert!(matches!(&home.pinned[1], Bookmark::Page(p) if p.url == "https://split.example"));
        assert_eq!(home.tabs.iter().map(|t| t.url.as_str()).collect::<Vec<_>>(), ["https://open.example"]);
        assert_eq!(job.spaces[0].title, "Work");
    }
}
