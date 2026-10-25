//! Explicitly dropped browser exports. Archives are read in memory, never extracted.
use crate::{bytes, BrowserKind, Category, Result, SourceProfile};
use browser_core::{
    history::HistoryEntry,
    import::{Bookmark, ImportedData, ImportedPage},
};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn export_source(path: &Path) -> Option<SourceProfile> {
    if !path.is_file() {
        return None;
    }
    let browser = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "zip" | "html" | "htm" => BrowserKind::Safari,
        "json" | "jsonlz4" => BrowserKind::Firefox,
        _ => return None,
    };
    Some(SourceProfile {
        browser,
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.into(),
    })
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("Import cancelled.".into())
    } else {
        Ok(())
    }
}
pub(crate) fn json(path: &Path) -> Result<serde_json::Value> {
    let input = bytes(path)?;
    let decoded;
    let input = if input.starts_with(b"mozLz40\0") {
        let size = u32::from_le_bytes(
            input
                .get(8..12)
                .ok_or("Truncated Firefox backup.")?
                .try_into()
                .unwrap(),
        );
        if size > 128 * 1024 * 1024 {
            return Err("The Firefox backup is too large.".into());
        }
        decoded = lz4_flex::block::decompress_size_prepended(&input[8..])
            .map_err(|_| "Cannot decode the Firefox backup.")?;
        &decoded
    } else {
        &input
    };
    serde_json::from_slice(input).map_err(|_| "The browser export contains invalid JSON.".into())
}
pub(crate) fn firefox_bookmarks(path: &Path, cancel: &AtomicBool) -> Result<Vec<Bookmark>> {
    fn walk(
        node: &serde_json::Value,
        depth: usize,
        budget: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<Bookmark>> {
        cancelled(cancel)?;
        if depth > 64 || *budget == 0 {
            return Err("The bookmark backup exceeds import limits.".into());
        }
        *budget -= 1;
        match node["type"].as_str() {
            Some("text/x-moz-place-container") => {
                let empty = Vec::new();
                let children = match node.get("children") {
                    None => &empty,
                    Some(value) => value.as_array().ok_or("Invalid bookmark children list.")?,
                };
                let mut out = Vec::new();
                for child in children {
                    out.extend(walk(child, depth + 1, budget, cancel)?);
                }
                Ok(vec![Bookmark::Folder {
                    title: node["title"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .unwrap_or("Untitled")
                        .into(),
                    children: out,
                }])
            }
            Some("text/x-moz-place") => Ok(vec![Bookmark::Page(ImportedPage {
                title: node["title"].as_str().unwrap_or("").into(),
                url: node["uri"]
                    .as_str()
                    .ok_or("The bookmark has no address.")?
                    .into(),
            })]),
            Some("text/x-moz-place-separator") => Ok(vec![]),
            _ => Err("Unsupported Firefox bookmark backup format.".into()),
        }
    }
    let node = json(path)?;
    let mut result = walk(&node, 0, &mut 100_000, cancel)?;
    // The JSON root is the Places database container, not a visible folder.
    if let Some(Bookmark::Folder { children, .. }) = result.pop() {
        Ok(children)
    } else {
        Err("The bookmark backup has no root folder.".into())
    }
}
pub(crate) fn read(
    source: &SourceProfile,
    category: Category,
    cancel: &AtomicBool,
    out: &mut ImportedData,
) -> Result<()> {
    cancelled(cancel)?;
    if source.browser == BrowserKind::Firefox {
        if category == Category::Bookmarks {
            out.bookmarks = firefox_bookmarks(&source.path, cancel)?;
        }
        return Ok(());
    }
    if source
        .path
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("zip"))
    {
        if category == Category::OpenTabs {
            return Ok(());
        }
        let extensions: &[&str] = if category == Category::Bookmarks {
            &[".html", ".htm"]
        } else {
            &[".json"]
        };
        for_each_entry(&source.path, extensions, 128 * 1024 * 1024, |name, data| {
            cancelled(cancel)?;
            if category == Category::Bookmarks
                && (name.ends_with(".html") || name.ends_with(".htm"))
            {
                out.bookmarks.extend(bookmarks(data, cancel)?);
            } else if category == Category::History && name.ends_with(".json") {
                let node: serde_json::Value =
                    serde_json::from_slice(data).map_err(|_| "Invalid Safari export JSON.")?;
                if node["metadata"]["data_type"] == "history" {
                    if node["metadata"]["schema_version"] != 1 {
                        return Err("Unsupported Safari history schema version.".into());
                    }
                    let rows = node["history"]
                        .as_array()
                        .ok_or("Safari history has no entries list.")?;
                    for row in rows {
                        cancelled(cancel)?;
                        let micros = row["time_usec"]
                            .as_u64()
                            .ok_or("Invalid Safari history timestamp.")?;
                        out.history.push(HistoryEntry {
                            visit_count: row["visits_count"]
                                .as_u64()
                                .unwrap_or(1)
                                .clamp(1, u32::MAX as u64)
                                as u32,
                            url: row["url"]
                                .as_str()
                                .ok_or("Invalid Safari history address.")?
                                .into(),
                            title: row["title"].as_str().unwrap_or("").into(),
                            visited_at: if micros <= 253402300799999999 {
                                std::time::UNIX_EPOCH
                                    .checked_add(std::time::Duration::from_micros(micros))
                                    .ok_or("Invalid Safari history timestamp.")?
                            } else {
                                return Err("Invalid Safari history timestamp.".into());
                            },
                        });
                    }
                }
            }
            Ok(())
        })?;
    } else if category == Category::Bookmarks {
        out.bookmarks = bookmarks(&bytes(&source.path)?, cancel)?;
    }
    Ok(())
}
/// Localized filenames are not interpreted. No entry is written to disk, and
/// archive path tricks, links, encryption and decompression bombs fail closed.
pub(crate) fn for_each_entry(
    path: &Path,
    extensions: &[&str],
    limit: u64,
    mut visit: impl FnMut(&str, &[u8]) -> Result<()>,
) -> Result<()> {
    let file = std::fs::File::open(path).map_err(|_| "Cannot open the browser export.")?;
    if file
        .metadata()
        .map_err(|_| "Cannot inspect the browser export.")?
        .len()
        > 128 * 1024 * 1024
    {
        return Err("The browser export is too large.".into());
    }
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| "Invalid or encrypted browser export archive.")?;
    if archive.len() > 256 {
        return Err("The browser export has too many files.".into());
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|_| "Cannot read an encrypted browser export.")?;
        if entry.enclosed_name().is_none() || entry.is_symlink() {
            return Err("Unsafe path in browser export archive.".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("The browser export is too large.")?;
        if total > 128 * 1024 * 1024 {
            return Err("The browser export expands beyond import limits.".into());
        }
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_ascii_lowercase();
        // Password CSV is read only by the explicit password-upload action.
        if !extensions.iter().any(|suffix| name.ends_with(suffix)) {
            continue;
        }
        if entry.size() > limit {
            return Err("A browser export entry exceeds import limits.".into());
        }
        let mut data = zeroize::Zeroizing::new(Vec::new());
        entry
            .by_ref()
            .take(limit + 1)
            .read_to_end(&mut data)
            .map_err(|_| "Cannot read the browser export.")?;
        if data.len() as u64 > limit {
            return Err("The browser export expands beyond import limits.".into());
        }
        visit(&name, &data)?;
    }
    Ok(())
}

fn bookmarks(data: &[u8], cancel: &AtomicBool) -> Result<Vec<Bookmark>> {
    use html5ever::tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
    };
    use std::cell::RefCell;
    #[derive(Default)]
    struct State {
        frames: Vec<(Option<String>, Vec<Bookmark>)>,
        heading: Option<String>,
        folder: Option<String>,
        link: Option<ImportedPage>,
        count: usize,
        invalid: bool,
    }
    struct Sink<'a>(RefCell<State>, &'a AtomicBool);
    impl TokenSink for Sink<'_> {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            let mut s = self.0.borrow_mut();
            if s.invalid || self.1.load(Ordering::Relaxed) {
                return TokenSinkResult::Continue;
            }
            match token {
                Token::TagToken(tag) => match (tag.kind, tag.name.as_ref()) {
                    (TagKind::StartTag, "h3") => {
                        s.count += 1;
                        if s.count > 100_000 {
                            s.invalid = true;
                        } else {
                            s.heading = Some(String::new());
                        }
                    }
                    (TagKind::EndTag, "h3") => s.folder = s.heading.take(),
                    (TagKind::StartTag, "dl") => {
                        let name = s.folder.take();
                        s.frames.push((name, vec![]));
                        if s.frames.len() > 65 {
                            s.invalid = true;
                        }
                    }
                    (TagKind::EndTag, "dl") => {
                        if s.frames.len() > 1 {
                            let (name, rows) = s.frames.pop().unwrap();
                            let parent = &mut s.frames.last_mut().unwrap().1;
                            if let Some(title) = name {
                                parent.push(Bookmark::Folder {
                                    title,
                                    children: rows,
                                });
                            } else {
                                parent.extend(rows);
                            }
                        }
                    }
                    (TagKind::StartTag, "a") => {
                        s.link = tag
                            .attrs
                            .iter()
                            .find(|a| a.name.local == html5ever::local_name!("href"))
                            .map(|a| ImportedPage {
                                title: String::new(),
                                url: a.value.to_string(),
                            })
                    }
                    (TagKind::EndTag, "a") => {
                        if let Some(page) = s.link.take() {
                            s.count += 1;
                            if s.count > 100_000 {
                                s.invalid = true;
                            } else if let Some(frame) = s.frames.last_mut() {
                                frame.1.push(Bookmark::Page(page));
                            }
                        }
                    }
                    _ => (),
                },
                Token::CharacterTokens(text) => {
                    if let Some(title) = &mut s.heading {
                        title.push_str(&text);
                    }
                    if let Some(page) = &mut s.link {
                        page.title.push_str(&text);
                    }
                }
                _ => (),
            }
            TokenSinkResult::Continue
        }
    }
    let text = std::str::from_utf8(data).map_err(|_| "The bookmark export is not UTF-8.")?;
    let sink = Sink(RefCell::new(State::default()), cancel);
    let tokenizer = Tokenizer::new(sink, Default::default());
    let input = BufferQueue::default();
    input.push_back(text.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    cancelled(cancel)?;
    let mut s = tokenizer.sink.0.into_inner();
    if s.invalid || s.frames.len() != 1 {
        return Err("Invalid or oversized bookmark export.".into());
    }
    Ok(s.frames.pop().unwrap().1)
}
