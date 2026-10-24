use crate::{Category, Result, bytes, database, read_json, timestamp};
use browser_core::{
    history::HistoryEntry,
    import::{Bookmark, ImportedData, ImportedPage},
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn read(
    root: &Path,
    category: Category,
    cancel: &AtomicBool,
    out: &mut ImportedData,
) -> Result<()> {
    match category {
        Category::Bookmarks => {
            let path = root.join("Bookmarks");
            if !path.exists() {
                return Ok(());
            }
            let json = read_json(&path)?;
            let roots = json["roots"]
                .as_object()
                .ok_or("This browser's bookmarks file has no roots.")?;
            for key in ["bookmark_bar", "other", "synced"] {
                if let Some(root) = roots.get(key) {
                    out.bookmarks.push(bookmark(root, 0, cancel)?);
                }
            }
        }
        Category::History => {
            let path = root.join("History");
            if !path.exists() {
                return Ok(());
            }
            let db = database(&path)?;
            let mut query = db
                .prepare(
                    "SELECT url, title, last_visit_time FROM urls ORDER BY last_visit_time DESC",
                )
                .map_err(|e| e.to_string())?;
            let mut rows = query.query([]).map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Import cancelled.".into());
                }
                out.history.push(HistoryEntry {
                    url: row.get(0).map_err(|e| e.to_string())?,
                    title: row
                        .get::<_, Option<String>>(1)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default(),
                    visited_at: timestamp(
                        row.get::<_, f64>(2).map_err(|e| e.to_string())? / 1_000_000.,
                        -11_644_473_600.,
                    ),
                });
            }
        }
        Category::OpenTabs => {
            let directory = root.join("Sessions");
            let newest = std::fs::read_dir(&directory)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("Session_"))
                .max_by_key(|e| e.file_name());
            let file = newest
                .map(|e| e.path())
                .or_else(|| {
                    root.join("Current Session")
                        .is_file()
                        .then(|| root.join("Current Session"))
                })
                .or_else(|| {
                    root.join("Last Session")
                        .is_file()
                        .then(|| root.join("Last Session"))
                });
            if let Some(path) = file {
                out.tabs = session(&bytes(&path)?, cancel)?;
            }
        }
    }
    Ok(())
}
fn bookmark(node: &serde_json::Value, depth: usize, cancel: &AtomicBool) -> Result<Bookmark> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Import cancelled.".into());
    }
    if depth > 64 {
        return Err("The bookmarks file is nested too deeply.".into());
    }
    let title = node["name"].as_str().unwrap_or("Untitled").to_owned();
    if node["type"] == "url" {
        return Ok(Bookmark::Page(ImportedPage {
            title,
            url: node["url"]
                .as_str()
                .ok_or("A bookmark URL is missing.")?
                .into(),
        }));
    }
    if node["type"] != "folder" {
        return Err("An unknown bookmark entry was found.".into());
    }
    let children = node["children"]
        .as_array()
        .ok_or("A bookmark folder has no children list.")?
        .iter()
        .map(|child| bookmark(child, depth + 1, cancel))
        .collect::<Result<Vec<_>>>()?;
    Ok(Bookmark::Folder { title, children })
}
#[derive(Default)]
struct Tab {
    window: i32,
    index: i32,
    selected: i32,
    navigations: BTreeMap<i32, ImportedPage>,
}
fn int(data: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(
        data.get(offset..offset + 4)
            .ok_or("A browser session record is truncated.")?
            .try_into()
            .unwrap(),
    ))
}
fn string(data: &[u8], position: &mut usize, wide: bool) -> Result<String> {
    let length =
        usize::try_from(int(data, *position)?).map_err(|_| "Invalid session string length.")?;
    *position += 4;
    let length = length
        .checked_mul(if wide { 2 } else { 1 })
        .ok_or("Invalid session string length.")?;
    let value = data
        .get(
            *position
                ..position
                    .checked_add(length)
                    .ok_or("Invalid session string length.")?,
        )
        .ok_or("A browser session string is truncated.")?;
    *position = (*position + length + 3) & !3;
    if wide {
        String::from_utf16(
            &value
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| "Invalid session title.".into())
    } else {
        String::from_utf8(value.to_vec()).map_err(|_| "Invalid session URL.".into())
    }
}
/// SNSS commands and Pickle field order follow Chromium's sessions/core format.
fn session(data: &[u8], cancel: &AtomicBool) -> Result<Vec<ImportedPage>> {
    if data.get(..4) != Some(b"SNSS") || !matches!(int(data, 4)?, 1 | 3) {
        return Err("This browser session format is not supported.".into());
    }
    let version = int(data, 4)?;
    let mut marker = version == 1;
    let mut tabs = BTreeMap::<i32, Tab>::new();
    let mut offset = 8;
    while offset < data.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Import cancelled.".into());
        }
        let size = u16::from_le_bytes(
            data.get(offset..offset + 2)
                .ok_or("A browser session command is truncated.")?
                .try_into()
                .unwrap(),
        ) as usize;
        offset += 2;
        let command = data
            .get(offset..offset + size)
            .filter(|x| !x.is_empty())
            .ok_or("A browser session command is truncated.")?;
        offset += size;
        let payload = &command[1..];
        match command[0] {
            0 => {
                let window = int(payload, 0)?;
                let id = int(payload, 4)?;
                tabs.entry(id).or_default().window = window;
            }
            2 => {
                let id = int(payload, 0)?;
                tabs.entry(id).or_default().index = int(payload, 4)?;
            }
            5 => {
                let id = int(payload, 0)?;
                let index = int(payload, 4)?;
                tabs.entry(id)
                    .or_default()
                    .navigations
                    .retain(|key, _| *key < index);
            }
            11 | 24 => {
                let id = int(payload, 0)?;
                let (index, count) = if command[0] == 11 {
                    (0, int(payload, 4)?)
                } else {
                    (int(payload, 4)?, int(payload, 8)?)
                };
                if index < 0 || count <= 0 {
                    return Err("Invalid session navigation range.".into());
                }
                let end = index
                    .checked_add(count)
                    .ok_or("Invalid session navigation range.")?;
                let tab = tabs.entry(id).or_default();
                if tab.selected >= index {
                    tab.selected = if tab.selected < end {
                        index - 1
                    } else {
                        tab.selected - count
                    };
                }
                tab.navigations = std::mem::take(&mut tab.navigations)
                    .into_iter()
                    .filter_map(|(key, page)| {
                        if key < index {
                            Some((key, page))
                        } else if key >= end {
                            Some((key - count, page))
                        } else {
                            None
                        }
                    })
                    .collect();
            }
            255 => marker = true,
            6 => {
                // Pickle begins with its payload size, followed by tab ID and entry index.
                let id = int(payload, 4)?;
                let entry = int(payload, 8)?;
                let mut p = 12;
                let url = string(payload, &mut p, false)?;
                let title = string(payload, &mut p, true)?;
                tabs.entry(id)
                    .or_default()
                    .navigations
                    .insert(entry, ImportedPage { title, url });
            }
            7 => {
                let id = int(payload, 0)?;
                tabs.entry(id).or_default().selected = int(payload, 4)?;
            }
            16 => {
                tabs.remove(&int(payload, 0)?);
            }
            17 => {
                let window = int(payload, 0)?;
                tabs.retain(|_, tab| tab.window != window);
            }
            _ => {}
        }
    }
    if !marker {
        return Err(
            "This browser has not finished writing its session. Close it and try again.".into(),
        );
    }
    let mut tabs = tabs.into_values().collect::<Vec<_>>();
    tabs.sort_by_key(|tab| (tab.window, tab.index));
    Ok(tabs
        .into_iter()
        .filter_map(|mut tab| {
            tab.navigations
                .remove(&tab.selected)
                .or_else(|| tab.navigations.pop_last().map(|(_, page)| page))
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snss_restores_selected_navigation_and_ignores_closed_tabs() {
        let mut data = b"SNSS\x01\0\0\0".to_vec();
        fn command(data: &mut Vec<u8>, id: u8, payload: &[u8]) {
            data.extend(((payload.len() + 1) as u16).to_le_bytes());
            data.push(id);
            data.extend(payload);
        }
        fn navigation(data: &mut Vec<u8>, tab: i32, index: i32, url: &str) {
            let mut payload = vec![0; 4];
            payload.extend(tab.to_le_bytes());
            payload.extend(index.to_le_bytes());
            payload.extend((url.len() as i32).to_le_bytes());
            payload.extend(url.as_bytes());
            while payload.len() % 4 != 0 {
                payload.push(0);
            }
            payload.extend(0i32.to_le_bytes());
            let size = (payload.len() - 4) as u32;
            payload[..4].copy_from_slice(&size.to_le_bytes());
            command(data, 6, &payload);
        }
        navigation(&mut data, 4, 0, "https://old.example");
        navigation(&mut data, 4, 1, "https://current.example");
        command(&mut data, 7, &[4, 0, 0, 0, 1, 0, 0, 0]);
        navigation(&mut data, 5, 0, "https://closed.example");
        command(&mut data, 16, &5i32.to_le_bytes());
        let pages = session(&data, &AtomicBool::new(false)).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].url, "https://current.example");
        // Rebase the selected entry when a navigation range is pruned.
        command(
            &mut data,
            24,
            &[4i32.to_le_bytes(), 0i32.to_le_bytes(), 1i32.to_le_bytes()].concat(),
        );
        assert_eq!(
            session(&data, &AtomicBool::new(false)).unwrap()[0].url,
            "https://current.example"
        );
        navigation(&mut data, 4, 1, "https://discarded.example");
        command(
            &mut data,
            5,
            &[4i32.to_le_bytes(), 1i32.to_le_bytes()].concat(),
        );
        assert_eq!(
            session(&data, &AtomicBool::new(false)).unwrap()[0].url,
            "https://current.example"
        );
        data[4..8].copy_from_slice(&3i32.to_le_bytes());
        assert!(session(&data, &AtomicBool::new(false)).is_err());
        command(&mut data, 255, &[]);
        assert_eq!(
            session(&data, &AtomicBool::new(false)).unwrap()[0].url,
            "https://current.example"
        );
        data.pop();
        assert!(session(&data, &AtomicBool::new(false)).is_err());
    }
}
