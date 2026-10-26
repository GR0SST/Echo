use crate::{Category, Result, bytes, database, timestamp};
use browser_core::{
    history::HistoryEntry,
    import::{Bookmark, ImportedData, ImportedPage},
};
use plist::Value;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
fn plist(path: &Path) -> Result<Value> {
    Value::from_reader(std::io::Cursor::new(bytes(path)?)).map_err(|e| e.to_string())
}
fn text<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.as_dictionary()?.get(key)?.as_string()
}
pub fn read(
    root: &Path,
    category: Category,
    cancel: &AtomicBool,
    out: &mut ImportedData,
) -> Result<()> {
    match category {
        Category::Bookmarks => {
            let path = root.join("Bookmarks.plist");
            if !path.exists() {
                return Ok(());
            }
            let node = plist(&path)?;
            out.bookmarks = children(&node, 0, cancel)?;
        }
        Category::History => {
            let path = root.join("History.db");
            if !path.exists() {
                return Ok(());
            }
            let db = database(&path)?;
            let count = if db.prepare("SELECT visit_count FROM history_items LIMIT 0").is_ok() { "MAX(1,COALESCE(i.visit_count,1),COUNT(*))" } else { "COUNT(*)" };
            let mut query=db.prepare(&format!("SELECT i.url,COALESCE(v.title,''),MAX(v.visit_time),{count} FROM history_items i JOIN history_visits v ON v.history_item=i.id GROUP BY i.id ORDER BY MAX(v.visit_time) DESC")).map_err(|e|e.to_string())?;
            let mut rows = query.query([]).map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Import cancelled.".into());
                }
                out.history.push(HistoryEntry {
                    url: row.get(0).map_err(|e| e.to_string())?,
                    title: row.get(1).map_err(|e| e.to_string())?,
                    visited_at: timestamp(
                        row.get::<_, f64>(2).map_err(|e| e.to_string())?,
                        978307200.,
                    ),
                    visit_count: u32::try_from(row.get::<_, i64>(3).map_err(|e| e.to_string())?.max(1)).unwrap_or(u32::MAX),
                });
            }
        }
        Category::Passwords => {}
        Category::OpenTabs => {
            let path = root.join("LastSession.plist");
            if !path.exists() {
                return Ok(());
            }
            let node = plist(&path)?;
            let windows = node
                .as_dictionary()
                .and_then(|d| d.get("SessionWindows"))
                .and_then(Value::as_array)
                .ok_or("This Safari session format is not supported.")?;
            for window in windows {
                for tab in window
                    .as_dictionary()
                    .and_then(|d| d.get("TabStates"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Import cancelled.".into());
                    }
                    if let Some(url) = text(tab, "TabURL") {
                        out.tabs.push(ImportedPage {
                            url: url.into(),
                            title: text(tab, "TabTitle").unwrap_or(url).into(),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
fn children(node: &Value, depth: usize, cancel: &AtomicBool) -> Result<Vec<Bookmark>> {
    if depth > 64 {
        return Err("The Safari bookmarks are nested too deeply.".into());
    }
    let Some(nodes) = node
        .as_dictionary()
        .and_then(|d| d.get("Children"))
        .and_then(Value::as_array)
    else {
        return Ok(vec![]);
    };
    let mut out = vec![];
    for node in nodes {
        if cancel.load(Ordering::Relaxed) {
            return Err("Import cancelled.".into());
        }
        match text(node, "WebBookmarkType") {
            Some("WebBookmarkTypeList") => out.push(Bookmark::Folder {
                title: text(node, "Title").unwrap_or("Untitled").into(),
                children: children(node, depth + 1, cancel)?,
            }),
            Some("WebBookmarkTypeLeaf") => {
                if let Some(url) = text(node, "URLString") {
                    let title = node
                        .as_dictionary()
                        .and_then(|d| d.get("URIDictionary"))
                        .and_then(|n| text(n, "title"))
                        .unwrap_or(url);
                    out.push(Bookmark::Page(ImportedPage {
                        title: title.into(),
                        url: url.into(),
                    }));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}
