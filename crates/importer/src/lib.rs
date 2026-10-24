//! Read-only browser discovery and import. No source profile is modified.
mod chromium;
mod firefox;
mod passwords;
mod safari;
use browser_core::import::ImportedData;
pub use passwords::{Credential, CredentialStore, parse_password_csv, read_password_file};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserKind {
    Chrome,
    Safari,
    Firefox,
    Edge,
    Brave,
}
impl BrowserKind {
    pub const ALL: [Self; 5] = [
        Self::Chrome,
        Self::Safari,
        Self::Firefox,
        Self::Edge,
        Self::Brave,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Safari => "Safari",
            Self::Firefox => "Firefox",
            Self::Edge => "Microsoft Edge",
            Self::Brave => "Brave",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceProfile {
    pub browser: BrowserKind,
    pub name: String,
    pub path: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}
#[derive(Clone, Debug)]
pub struct HostPaths {
    pub platform: Platform,
    pub home: PathBuf,
    pub local_app_data: PathBuf,
    pub roaming_app_data: PathBuf,
    pub config: PathBuf,
}
impl HostPaths {
    pub fn current() -> Self {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .unwrap_or_default();
        Self {
            platform: if cfg!(target_os = "macos") {
                Platform::Mac
            } else if cfg!(windows) {
                Platform::Windows
            } else {
                Platform::Linux
            },
            local_app_data: std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Local")),
            roaming_app_data: std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Roaming")),
            config: std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config")),
            home,
        }
    }
    pub fn browser_root(&self, kind: BrowserKind) -> Option<PathBuf> {
        use BrowserKind::*;
        use Platform::*;
        Some(match (self.platform, kind) {
            (Mac, Safari) => self.home.join("Library/Safari"),
            (_, Safari) => return None,
            (Mac, Firefox) => self.home.join("Library/Application Support/Firefox"),
            (Windows, Firefox) => self.roaming_app_data.join("Mozilla/Firefox"),
            (Linux, Firefox) => self.home.join(".mozilla/firefox"),
            (Mac, kind) => self
                .home
                .join("Library/Application Support")
                .join(match kind {
                    Chrome => "Google/Chrome",
                    Edge => "Microsoft Edge",
                    Brave => "BraveSoftware/Brave-Browser",
                    _ => unreachable!(),
                }),
            (Windows, kind) => self.local_app_data.join(match kind {
                Chrome => "Google/Chrome/User Data",
                Edge => "Microsoft/Edge/User Data",
                Brave => "BraveSoftware/Brave-Browser/User Data",
                _ => unreachable!(),
            }),
            (Linux, kind) => self.config.join(match kind {
                Chrome => "google-chrome",
                Edge => "microsoft-edge",
                Brave => "BraveSoftware/Brave-Browser",
                _ => unreachable!(),
            }),
        })
    }
}
/// Filesystem locations are supplied explicitly, including in cross-platform tests.
pub trait ProfileLocator {
    fn profiles(&self) -> Vec<SourceProfile>;
}
impl ProfileLocator for HostPaths {
    fn profiles(&self) -> Vec<SourceProfile> {
        let mut found = Vec::new();
        for browser in BrowserKind::ALL {
            let Some(root) = self.browser_root(browser).filter(|root| root.is_dir()) else {
                continue;
            };
            match browser {
                BrowserKind::Safari => found.push(SourceProfile {
                    browser,
                    name: "Safari".into(),
                    path: root,
                }),
                BrowserKind::Firefox => found.extend(firefox::profiles(&root)),
                _ => {
                    let state = read_json(&root.join("Local State")).unwrap_or_default();
                    if let Some(profiles) = state["profile"]["info_cache"].as_object() {
                        for (directory, profile) in profiles {
                            if Path::new(directory).components().count() != 1
                                || directory == ".."
                                || directory == "."
                            {
                                continue;
                            }
                            let path = root.join(directory);
                            if path.is_dir() {
                                found.push(SourceProfile {
                                    browser,
                                    name: profile["name"].as_str().unwrap_or(directory).into(),
                                    path,
                                });
                            }
                        }
                    }
                    if !found.iter().any(|source| source.browser == browser)
                        && root.join("Default").is_dir()
                    {
                        found.push(SourceProfile {
                            browser,
                            name: "Default".into(),
                            path: root.join("Default"),
                        });
                    }
                }
            }
        }
        found
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Bookmarks,
    History,
    OpenTabs,
}
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub bookmarks: bool,
    pub history: bool,
    pub tabs: bool,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            bookmarks: true,
            history: true,
            tabs: true,
        }
    }
}
#[derive(Debug)]
pub struct ImportIssue {
    pub category: Category,
    pub message: String,
}
#[derive(Debug, Default)]
pub struct ReadReport {
    pub data: ImportedData,
    pub issues: Vec<ImportIssue>,
    pub cancelled: bool,
}
/// Import is cancelable between categories and during large row loops.
pub fn read_profile(
    source: &SourceProfile,
    selection: Selection,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Category),
) -> ReadReport {
    let mut report = ReadReport::default();
    for (category, enabled) in [
        (Category::Bookmarks, selection.bookmarks),
        (Category::History, selection.history),
        (Category::OpenTabs, selection.tabs),
    ] {
        if !enabled || cancel.load(Ordering::Relaxed) {
            continue;
        }
        progress(category);
        let mut data = ImportedData::default();
        let result = match source.browser {
            BrowserKind::Firefox => firefox::read(&source.path, category, cancel, &mut data),
            BrowserKind::Safari => safari::read(&source.path, category, cancel, &mut data),
            _ => chromium::read(&source.path, category, cancel, &mut data),
        };
        match result {
            Err(message) => report.issues.push(ImportIssue { category, message }),
            Ok(()) => {
                report.data.bookmarks.extend(data.bookmarks);
                report.data.history.extend(data.history);
                report.data.tabs.extend(data.tabs);
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        report.data = ImportedData::default();
        report.cancelled = true;
    }
    report
}
pub(crate) type Result<T> = std::result::Result<T, String>;
pub(crate) fn bytes(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
        return Err("The browser data file is too large to import.".into());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err("The browser data file grew while being read. Try again.".into());
    }
    Ok(bytes)
}
pub(crate) fn read_json(path: &Path) -> Result<serde_json::Value> {
    serde_json::from_slice(&bytes(path)?).map_err(|e| e.to_string())
}
pub(crate) fn database(path: &Path) -> Result<rusqlite::Connection> {
    let db = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    Ok(db)
}
pub(crate) fn timestamp(value: f64, epoch: f64) -> std::time::SystemTime {
    let seconds = value + epoch;
    if !seconds.is_finite() || seconds < 0. || seconds > 253402300799. {
        return std::time::UNIX_EPOCH;
    }
    std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(seconds)
}
