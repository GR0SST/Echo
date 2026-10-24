//! Completed local-profile downloads. No active transfers or engine tokens are serialized.
#[path = "download_paths.rs"]
mod path_encoding;
use crate::{Browser, UserProfileId};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Independent of tab, Space, window, native-download and process-token IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompletedDownloadId {
    profile: UserProfileId,
    sequence: u64,
}
impl CompletedDownloadId {
    pub fn profile(self) -> UserProfileId {
        self.profile
    }
    pub fn sequence(self) -> u64 {
        self.sequence
    }
}

/// CEF's raw base::Time value: microseconds from 1601, never Unix/nanoseconds.
/// Null and infinity are retained rather than replaced with the current time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadTime {
    Unknown,
    WindowsEpochMicros(i64),
    Invalid(i64),
}
impl DownloadTime {
    pub fn from_cef_basetime(raw: i64) -> Self {
        match raw {
            0 => Self::Unknown,
            i64::MIN | i64::MAX => Self::Invalid(raw),
            value => Self::WindowsEpochMicros(value),
        }
    }
    fn valid(self) -> bool {
        match self {
            Self::Unknown => true,
            Self::WindowsEpochMicros(n) => n != 0 && n != i64::MIN && n != i64::MAX,
            Self::Invalid(n) => n == i64::MIN || n == i64::MAX,
        }
    }
}

/// Construct only from an actual engine Complete snapshot. This type itself
/// makes no claim about destination policy or filesystem existence after completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadCompletion {
    pub final_path: PathBuf,
    pub source_url: String,
    pub original_url: String,
    pub mime_type: String,
    pub finished_at: DownloadTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedDownload {
    id: CompletedDownloadId,
    #[serde(with = "path_encoding::path")]
    final_path: PathBuf,
    #[serde(with = "path_encoding::os_string")]
    final_name: OsString,
    source_url: String,
    original_url: String,
    mime_type: String,
    finished_at: DownloadTime,
}
impl CompletedDownload {
    pub fn id(&self) -> CompletedDownloadId {
        self.id
    }
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }
    pub fn final_name(&self) -> &std::ffi::OsStr {
        &self.final_name
    }
    pub fn source_url(&self) -> &str {
        &self.source_url
    }
    pub fn original_url(&self) -> &str {
        &self.original_url
    }
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }
    pub fn finished_at(&self) -> DownloadTime {
        self.finished_at
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct CompletedDownloads {
    /// Persist the watermark even if records are removed in a future implementation.
    last_sequence: u64,
    records: Vec<CompletedDownload>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadRecordError {
    WindowLocalModel,
    UnknownProfile,
    InvalidFinalPath,
    InvalidTime,
    IdentityExhausted,
}
impl Browser {
    /// Canonical normal workspace only. Blank must route through its retained normal handle.
    pub fn record_completed_download(
        &mut self,
        profile: UserProfileId,
        completion: DownloadCompletion,
    ) -> Result<CompletedDownloadId, DownloadRecordError> {
        if self.is_private() || self.is_blank() {
            return Err(DownloadRecordError::WindowLocalModel);
        }
        let registry = &mut self
            .profiles
            .get_mut(&profile)
            .ok_or(DownloadRecordError::UnknownProfile)?
            .downloads;
        let name = completion
            .final_path
            .file_name()
            .filter(|n| !n.is_empty())
            .ok_or(DownloadRecordError::InvalidFinalPath)?
            .to_os_string();
        if !completion.final_path.is_absolute() {
            return Err(DownloadRecordError::InvalidFinalPath);
        }
        if !completion.finished_at.valid() {
            return Err(DownloadRecordError::InvalidTime);
        }
        let sequence = registry
            .last_sequence
            .checked_add(1)
            .ok_or(DownloadRecordError::IdentityExhausted)?;
        let id = CompletedDownloadId { profile, sequence };
        registry.records.push(CompletedDownload {
            id,
            final_name: name,
            final_path: completion.final_path,
            source_url: completion.source_url,
            original_url: completion.original_url,
            mime_type: completion.mime_type,
            finished_at: completion.finished_at,
        });
        registry.last_sequence = sequence;
        Ok(id)
    }
    /// Hiding a completed record never deletes its file or reuses its identity.
    pub fn hide_completed_download(&mut self, id: CompletedDownloadId) -> bool {
        if self.is_private() || self.is_blank() { return false; }
        let Some(profile) = self.profiles.get_mut(&id.profile) else { return false; };
        let records = &mut profile.downloads.records;
        let before = records.len();
        records.retain(|record| record.id != id);
        records.len() != before
    }
    pub fn completed_downloads(&self, profile: UserProfileId) -> Option<&[CompletedDownload]> {
        self.profiles
            .get(&profile)
            .map(|p| p.downloads.records.as_slice())
    }
    pub(crate) fn validate_downloads(&self) -> Result<(), &'static str> {
        for (profile, data) in &self.profiles {
            let registry = &data.downloads;
            let mut seen = HashSet::new();
            for record in &registry.records {
                if record.id.profile != *profile
                    || record.id.sequence == 0
                    || record.id.sequence > registry.last_sequence
                    || !seen.insert(record.id)
                {
                    return Err("invalid completed download ownership or watermark");
                }
                if !record.final_path.is_absolute()
                    || record.final_path.file_name() != Some(record.final_name.as_os_str())
                    || !record.finished_at.valid()
                {
                    return Err("invalid completed download metadata");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
