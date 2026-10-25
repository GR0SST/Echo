use crate::{BrowserKind, Category, Result, SourceProfile, bytes, database, read_json, timestamp};
use browser_core::{
    history::HistoryEntry,
    import::{Bookmark, ImportedData, ImportedPage},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub fn profiles(root: &Path) -> Vec<SourceProfile> {
    let Ok(ini) = std::fs::read_to_string(root.join("profiles.ini")) else {
        return vec![];
    };
    let mut sections = Vec::<HashMap<String, String>>::new();
    for line in ini.lines().map(str::trim) {
        if line.starts_with("[Profile") {
            sections.push(HashMap::new());
        } else if line.starts_with('[') {
            sections.push(HashMap::new());
        } else if let Some((key, value)) = line.split_once('=') {
            if let Some(section) = sections.last_mut() {
                section.insert(key.into(), value.into());
            }
        }
    }
    sections
        .into_iter()
        .filter_map(|s| {
            let name = s.get("Name")?.clone();
            let path = s.get("Path")?;
            let path = if s.get("IsRelative").is_some_and(|v| v == "0") {
                path.into()
            } else {
                root.join(path)
            };
            Path::new(&path).is_dir().then_some(SourceProfile {
                browser: BrowserKind::Firefox,
                name,
                path,
            })
        })
        .collect()
}
pub fn read(
    root: &Path,
    category: Category,
    cancel: &AtomicBool,
    out: &mut ImportedData,
) -> Result<()> {
    if category == Category::OpenTabs {
        let path = [
            "sessionstore.jsonlz4",
            "sessionstore-backups/recovery.jsonlz4",
            "sessionstore-backups/previous.jsonlz4",
            "sessionstore.js",
        ]
        .into_iter()
        .map(|p| root.join(p))
        .find(|p| p.is_file());
        let Some(path) = path else {
            return Ok(());
        };
        let data = bytes(&path)?;
        let json = if data.starts_with(b"mozLz40\0") {
            let size = data.get(8..12).ok_or("The Firefox session is truncated.")?;
            let size = u32::from_le_bytes(size.try_into().unwrap()) as usize;
            if size > 128 * 1024 * 1024 {
                return Err("The Firefox session is too large to import.".into());
            }
            let data = lz4_flex::block::decompress_size_prepended(&data[8..])
                .map_err(|_| "Cannot decode the Firefox session.")?;
            serde_json::from_slice(&data).map_err(|e| e.to_string())?
        } else {
            read_json(&path)?
        };
        out.tabs = session_pages(&json, cancel)?;
        return Ok(());
    }
    let path = root.join("places.sqlite");
    if !path.exists() {
        return Ok(());
    }
    let db = database(&path)?;
    if category == Category::History {
        let count = if db.prepare("SELECT visit_count FROM moz_places LIMIT 0").is_ok() { "MAX(1,COALESCE(visit_count,1))" } else { "1" };
        let mut query=db.prepare(&format!("SELECT url, COALESCE(title,''), last_visit_date,{count} FROM moz_places WHERE last_visit_date IS NOT NULL ORDER BY last_visit_date DESC")).map_err(|e|e.to_string())?;
        let mut rows = query.query([]).map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            if cancel.load(Ordering::Relaxed) {
                return Err("Import cancelled.".into());
            }
            out.history.push(HistoryEntry {
                url: row.get(0).map_err(|e| e.to_string())?,
                title: row.get(1).map_err(|e| e.to_string())?,
                visited_at: timestamp(row.get::<_, f64>(2).map_err(|e| e.to_string())? / 1e6, 0.),
                visit_count: u32::try_from(row.get::<_, i64>(3).map_err(|e| e.to_string())?.max(1)).unwrap_or(u32::MAX),
            });
        }
    } else {
        let mut query=db.prepare("SELECT b.id,b.parent,b.type,COALESCE(b.title,''),COALESCE(p.url,''),b.guid FROM moz_bookmarks b LEFT JOIN moz_places p ON p.id=b.fk ORDER BY b.position,b.id").map_err(|e|e.to_string())?;
        let rows = query
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i32>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        fn walk(
            id: i64,
            rows: &[(i64, i64, i32, String, String, String)],
            seen: &mut HashSet<i64>,
            depth: usize,
            cancel: &AtomicBool,
        ) -> Result<Vec<Bookmark>> {
            if depth > 64 || !seen.insert(id) {
                return Err(
                    "The Firefox bookmarks contain a cyclic or deeply nested folder.".into(),
                );
            }
            if cancel.load(Ordering::Relaxed) {
                return Err("Import cancelled.".into());
            }
            let mut out = vec![];
            for (child, parent, kind, title, url, _) in rows.iter().filter(|row| row.1 == id) {
                let _ = parent;
                if *kind == 1 {
                    out.push(Bookmark::Page(ImportedPage {
                        title: title.clone(),
                        url: url.clone(),
                    }));
                } else if *kind == 2 {
                    out.push(Bookmark::Folder {
                        title: if title.is_empty() {
                            "Untitled".into()
                        } else {
                            title.clone()
                        },
                        children: walk(*child, rows, seen, depth + 1, cancel)?,
                    });
                }
            }
            seen.remove(&id);
            Ok(out)
        }
        let root = rows
            .iter()
            .find(|row| row.5 == "root________")
            .map(|row| row.0)
            .unwrap_or(1);
        out.bookmarks = walk(root, &rows, &mut HashSet::new(), 0, cancel)?;
    }
    Ok(())
}
fn session_pages(json: &serde_json::Value, cancel: &AtomicBool) -> Result<Vec<ImportedPage>> {
    let windows = json["windows"]
        .as_array()
        .ok_or("The Firefox session has no windows list.")?;
    let mut pages = vec![];
    for window in windows {
        for tab in window["tabs"].as_array().into_iter().flatten() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Import cancelled.".into());
            }
            let Some(entries) = tab["entries"].as_array() else {
                continue;
            };
            let index = tab["index"]
                .as_u64()
                .unwrap_or(entries.len() as u64)
                .saturating_sub(1) as usize;
            if let Some(entry) = entries.get(index) {
                if let Some(url) = entry["url"].as_str() {
                    pages.push(ImportedPage {
                        url: url.into(),
                        title: entry["title"].as_str().unwrap_or(url).into(),
                    });
                }
            }
        }
    }
    Ok(pages)
}
