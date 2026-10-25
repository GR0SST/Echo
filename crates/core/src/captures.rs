//! Locally saved page captures. Files live behind the application's storage
//! boundary; the model retains a basename rather than an arbitrary file path.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::SystemTime};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CaptureId(u64);
impl CaptureId {
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capture {
    pub id: CaptureId,
    pub name: String,
    pub source_title: String,
    pub source_url: String,
    pub created: SystemTime,
    pub width: u32,
    pub height: u32,
    filename: String,
}
impl Capture {
    pub fn filename(&self) -> &str {
        &self.filename
    }
    fn valid(&self) -> bool {
        self.id.0 > 0 && self.width > 0 && self.height > 0 && valid_filename(&self.filename)
    }
}
fn valid_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
        && name.ends_with(".png")
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "Stored")]
pub struct Captures {
    rows: Vec<Capture>,
    next: u64,
}
fn first() -> u64 {
    1
}
#[derive(Deserialize)]
struct Stored {
    #[serde(default)]
    rows: Vec<Capture>,
    #[serde(default = "first")]
    next: u64,
}
impl From<Stored> for Captures {
    fn from(stored: Stored) -> Self {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let rows: Vec<_> = stored
            .rows
            .into_iter()
            .filter(|row| row.valid() && ids.insert(row.id) && names.insert(row.filename.clone()))
            .collect();
        let next = stored
            .next
            .max(
                rows.iter()
                    .map(|r| r.id.0.saturating_add(1))
                    .max()
                    .unwrap_or(1),
            )
            .max(1);
        Self { rows, next }
    }
}
impl Default for Captures {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            next: 1,
        }
    }
}
impl Captures {
    pub fn items(&self) -> impl DoubleEndedIterator<Item = &Capture> {
        self.rows.iter()
    }
    pub fn get(&self, id: CaptureId) -> Option<&Capture> {
        self.rows.iter().find(|r| r.id == id)
    }
    pub fn add(
        &mut self,
        name: String,
        source_title: String,
        source_url: String,
        filename: String,
        width: u32,
        height: u32,
        created: SystemTime,
    ) -> Option<CaptureId> {
        if width == 0
            || height == 0
            || !valid_filename(&filename)
            || self.rows.iter().any(|r| r.filename == filename)
        {
            return None;
        }
        let next = self.next.checked_add(1)?;
        let id = CaptureId(self.next);
        self.rows.push(Capture {
            id,
            name,
            source_title,
            source_url,
            filename,
            width,
            height,
            created,
        });
        self.next = next;
        Some(id)
    }
    pub fn remove(&mut self, id: CaptureId) -> Option<Capture> {
        let index = self.rows.iter().position(|r| r.id == id)?;
        Some(self.rows.remove(index))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn add(c: &mut Captures, file: &str) -> Option<CaptureId> {
        c.add(
            "Capture".into(),
            "Fixture".into(),
            "https://example.com".into(),
            file.into(),
            150,
            55,
            SystemTime::UNIX_EPOCH,
        )
    }
    #[test]
    fn captured_files_have_stable_ids_and_cannot_escape_storage() {
        let mut captures = Captures::default();
        let a = add(&mut captures, "a.png").unwrap();
        assert!(add(&mut captures, "a.png").is_none());
        captures.remove(a).unwrap();
        let b = add(&mut captures, "b.png").unwrap();
        assert!(b > a);
        for file in [
            "../a.png", "x/a.png", "x\\a.png", "C:a.png", ".", "", "a.txt",
        ] {
            assert!(add(&mut captures, file).is_none(), "{file}");
        }
        let mut value = serde_json::to_value(&captures).unwrap();
        value["rows"][0]["filename"] = "../outside.png".into();
        let repaired: Captures = serde_json::from_value(value).unwrap();
        assert_eq!(repaired.items().count(), 0);
    }
    #[test]
    fn capture_inventory_persists_in_its_own_profile_and_legacy_sessions_load_empty() {
        let mut browser = crate::Browser::new();
        let work = browser.create_user_profile("Work").unwrap();
        let id = add(
            &mut browser.user_profile_mut(work).unwrap().captures,
            "work.png",
        )
        .unwrap();
        let loaded = crate::session::from_json(&crate::session::to_json(&browser)).unwrap();
        assert_eq!(
            loaded
                .user_profile(work)
                .unwrap()
                .captures
                .get(id)
                .unwrap()
                .filename(),
            "work.png"
        );
        assert_eq!(loaded.active_user_profile().captures.items().count(), 0);
        let mut old: serde_json::Value =
            serde_json::from_str(&crate::session::to_json(&browser)).unwrap();
        for profile in old["profiles"].as_object_mut().unwrap().values_mut() {
            profile.as_object_mut().unwrap().remove("captures");
        }
        let loaded = crate::session::from_json(&old.to_string()).unwrap();
        assert!(loaded
            .user_profiles()
            .all(|p| p.captures.items().count() == 0));
    }
}
