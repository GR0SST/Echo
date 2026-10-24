//! AI filenames remain basenames in the completed file's existing directory.
use super::{CompletedDownload, CompletedDownloadId};
use crate::Browser;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct DownloadRenamePlan {
    pub expected: CompletedDownload,
    stem: String,
    extension: String,
}
impl DownloadRenamePlan {
    pub fn new(expected: CompletedDownload, stem: &str) -> Option<Self> {
        let stem = stem.trim();
        if stem.is_empty()
            || stem.starts_with('.')
            || stem.ends_with(['.', ' '])
            || stem.chars().count() > 100 || stem.chars().any(|c| {
            c.is_control()
                || "<>:\"/\\|?*".contains(c)
                || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{200e}'|'\u{200f}')
        }) {
            return None;
        }
        let device = stem.split('.').next()?.trim_end().to_ascii_uppercase();
        if matches!(
            device.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
        ) || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && matches!(device.as_bytes()[3], b'1'..=b'9'))
        {
            return None;
        }
        let original = expected.final_name().to_str()?;
        let lower = original.to_ascii_lowercase();
        let extension = [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"]
            .into_iter()
            .find(|suffix| lower.ends_with(suffix))
            .map(|suffix| original[original.len() - suffix.len()..].to_string())
            .unwrap_or_else(|| {
                Path::new(original)
                    .extension()
                    .and_then(|v| v.to_str())
                    .filter(|v| !v.is_empty())
                    .map(|v| format!(".{v}"))
                    .unwrap_or_default()
            });
        let name = format!("{stem}{extension}");
        if name == original || name.len() > 240 {
            return None;
        }
        Some(Self {
            expected,
            stem: stem.into(),
            extension,
        })
    }
    pub fn candidate(&self, index: u8) -> PathBuf {
        let name = if index == 0 {
            format!("{}{}", self.stem, self.extension)
        } else {
            format!("{} ({index}){}", self.stem, self.extension)
        };
        self.expected.final_path().with_file_name(name)
    }
    pub fn permits(&self, path: &Path) -> bool {
        (0..=99).any(|i| self.candidate(i) == path)
    }
}
impl Browser {
    /// A filesystem worker supplies the successful path. Metadata changes are
    /// conditional, preserve ownership/identity, and cannot invent a directory.
    pub fn update_completed_download_path(
        &mut self,
        expected: &CompletedDownload,
        path: PathBuf,
    ) -> bool {
        if self.is_private()
            || self.is_blank()
            || path.parent() != expected.final_path().parent()
            || !path.is_absolute()
        {
            return false;
        }
        let Some(name) = path.file_name().map(OsString::from) else {
            return false;
        };
        let Some(profile) = self.profiles.get_mut(&expected.id().profile()) else {
            return false;
        };
        let Some(record) = profile
            .downloads
            .records
            .iter_mut()
            .find(|r| r.id() == expected.id())
        else {
            return false;
        };
        if record != expected {
            return false;
        }
        record.final_path = path;
        record.final_name = name;
        true
    }
    pub fn completed_download(&self, id: CompletedDownloadId) -> Option<&CompletedDownload> {
        self.completed_downloads(id.profile())?
            .iter()
            .find(|r| r.id() == id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloads::{DownloadCompletion, DownloadTime};
    fn fixture(name: &str) -> (Browser, CompletedDownload) {
        let mut b = Browser::new();
        let p = b.default_user_profile_id();
        let path = std::env::temp_dir().join(name);
        let id = b
            .record_completed_download(
                p,
                DownloadCompletion {
                    final_path: path,
                    source_url: "https://files.example/a?token=private".into(),
                    original_url: "https://files.example/a?token=private".into(),
                    mime_type: "application/octet-stream".into(),
                    finished_at: DownloadTime::Unknown,
                },
            )
            .unwrap();
        let record = b.completed_download(id).unwrap().clone();
        (b, record)
    }
    #[test]
    fn generated_names_keep_extensions_and_reject_paths_devices_and_spoofing() {
        let (_, record) = fixture("archive.TAR.GZ");
        let plan = DownloadRenamePlan::new(record.clone(), "Release Notes").unwrap();
        assert_eq!(
            plan.candidate(0).file_name().unwrap(),
            "Release Notes.TAR.GZ"
        );
        assert_eq!(
            plan.candidate(2).file_name().unwrap(),
            "Release Notes (2).TAR.GZ"
        );
        for invalid in [
            "../outside",
            "/absolute",
            "x\\path",
            "CON",
            "con.txt",
            "LPT1",
            "a:b",
            ".hidden",
            "ends.",
            "bad\nname",
            "name\u{202e}exe",
        ] {
            assert!(
                DownloadRenamePlan::new(record.clone(), invalid).is_none(),
                "{invalid:?}"
            );
        }
    }
    #[test]
    fn metadata_rename_and_revert_keep_the_record_id_owner_and_latest_guard() {
        let (mut b, original) = fixture("1234.pdf");
        let plan = DownloadRenamePlan::new(original.clone(), "Quarterly Report").unwrap();
        assert!(b.update_completed_download_path(&original, plan.candidate(0)));
        let renamed = b.completed_download(original.id()).unwrap().clone();
        assert_eq!(renamed.id(), original.id());
        assert_eq!(renamed.source_url(), original.source_url());
        assert!(!b.update_completed_download_path(&original, plan.candidate(1)));
        assert!(!b.update_completed_download_path(
            &renamed,
            std::env::temp_dir().join("other-directory/file.pdf")
        ));
        assert!(b.update_completed_download_path(&renamed, original.final_path().into()));
        assert_eq!(b.completed_download(original.id()), Some(&original));
        b.hide_completed_download(original.id());
        assert!(!b.update_completed_download_path(&original, plan.candidate(0)));
    }
}
