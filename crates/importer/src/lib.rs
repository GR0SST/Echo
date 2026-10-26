//! Read-only browser discovery and import. No source profile is modified.
mod arc;
mod chromium;
mod firefox;
mod passwords;
mod exports;
pub use exports::export_source;
mod safari;
use browser_core::import::ImportedData;
pub use passwords::{Credential, CredentialStore, parse_password_csv, read_password_file};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserKind {
    Arc,
    Chrome,
    Safari,
    Firefox,
    Edge,
    Brave,
}
impl BrowserKind {
    pub const ALL: [Self; 6] = [
        Self::Arc,
        Self::Chrome,
        Self::Safari,
        Self::Firefox,
        Self::Edge,
        Self::Brave,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Arc => "Arc",
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
    /// Where macOS apps are installed. Browsers whose app is gone are not
    /// offered even if their data folder remains.
    pub applications: Vec<PathBuf>,
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
            applications: vec!["/Applications".into(), home.join("Applications")],
            home,
        }
    }
    /// Whether the browser itself is installed. Only macOS has a reliable
    /// install location; elsewhere its data folder is the signal.
    pub fn installed(&self, kind: BrowserKind) -> bool {
        if self.platform != Platform::Mac {
            return true;
        }
        let app = match kind {
            BrowserKind::Arc => "Arc.app",
            BrowserKind::Chrome => "Google Chrome.app",
            BrowserKind::Safari => "Safari.app",
            BrowserKind::Firefox => "Firefox.app",
            BrowserKind::Edge => "Microsoft Edge.app",
            BrowserKind::Brave => "Brave Browser.app",
        };
        self.applications.iter().any(|dir| dir.join(app).exists())
    }
    pub fn browser_root(&self, kind: BrowserKind) -> Option<PathBuf> {
        use BrowserKind::*;
        use Platform::*;
        Some(match (self.platform, kind) {
            (Mac, Safari) => self.home.join("Library/Safari"),
            (_, Safari) => return None,
            (Mac, Arc) => self.home.join("Library/Application Support/Arc/User Data"),
            // The Microsoft Store package name carries a publisher hash.
            (Windows, Arc) => std::fs::read_dir(self.local_app_data.join("Packages"))
                .ok()?
                .filter_map(|entry| entry.ok())
                .find(|entry| {
                    entry.file_name().to_string_lossy().starts_with("TheBrowserCompany.Arc_")
                })?
                .path()
                .join("LocalCache/Local/Arc/User Data"),
            (Linux, Arc) => return None,
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
    /// One source per installed browser; importing it brings all its profiles.
    fn profiles(&self) -> Vec<SourceProfile> {
        let mut found = Vec::new();
        for browser in BrowserKind::ALL {
            if !self.installed(browser) {
                continue;
            }
            let Some(root) = self.browser_root(browser).filter(|root| root.is_dir()) else {
                continue;
            };
            // A protected folder can't be listed until the OS grants access
            // on the first read, so offer it and report a refusal then.
            let readable = std::fs::read_dir(&root).is_ok();
            if readable && browser != BrowserKind::Safari && browser_profiles(browser, &root).is_empty() {
                continue;
            }
            found.push(SourceProfile { browser, name: browser.name().into(), path: root });
        }
        found
    }
}
/// The profiles inside a browser's data folder, the default one first.
pub fn browser_profiles(browser: BrowserKind, root: &Path) -> Vec<SourceProfile> {
    if browser == BrowserKind::Firefox {
        return firefox::profiles(root);
    }
    if browser == BrowserKind::Safari {
        return vec![SourceProfile { browser, name: "Safari".into(), path: root.into() }];
    }
    let mut found = Vec::new();
    let state = read_json(&root.join("Local State")).unwrap_or_default();
    if let Some(profiles) = state["profile"]["info_cache"].as_object() {
        for (directory, profile) in profiles {
            // Arc keeps an internal profile for its own pages.
            if directory.starts_with("__")
                || profile["name"].as_str().is_some_and(|n| n.starts_with("__"))
                || Path::new(directory).components().count() != 1
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
    if !found.iter().any(|p| p.path.ends_with("Default")) && root.join("Default").is_dir() {
        found.push(SourceProfile { browser, name: "Default".into(), path: root.join("Default") });
    }
    found.sort_by_key(|p| (!p.path.ends_with("Default"), p.path.clone()));
    found
}
/// Whether `source` is a whole browser rather than one profile or export.
fn is_browser(source: &SourceProfile) -> bool {
    match source.browser {
        BrowserKind::Safari => false,
        BrowserKind::Firefox => source.path.join("profiles.ini").exists(),
        // A protected data folder reads as a browser; reading it reports why.
        _ => {
            source.path.join("Local State").exists()
                || std::fs::read_dir(&source.path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
        }
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
    if source.path.is_dir() && is_browser(source) {
        read_browser(source, selection, cancel, &mut progress, &mut report);
        if cancel.load(Ordering::Relaxed) {
            report.data = ImportedData::default();
            report.cancelled = true;
        }
        return report;
    }
    let part = read_categories(source, selection, cancel, &mut progress);
    report.data = part.data;
    report.issues = part.issues;
    if cancel.load(Ordering::Relaxed) {
        report.data = ImportedData::default();
        report.cancelled = true;
    }
    report
}
/// One profile directory or export file.
fn read_categories(
    source: &SourceProfile,
    selection: Selection,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Category),
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
        let result = if source.path.is_file() {
            exports::read(source, category, cancel, &mut data)
        } else { match source.browser {
            BrowserKind::Firefox => firefox::read(&source.path, category, cancel, &mut data),
            BrowserKind::Safari => safari::read(&source.path, category, cancel, &mut data),
            _ => chromium::read(&source.path, category, cancel, &mut data),
        }};
        match result {
            Err(message) => report.issues.push(ImportIssue { category, message }),
            Ok(()) => {
                report.data.bookmarks.extend(data.bookmarks);
                report.data.history.extend(data.history);
                report.data.tabs.extend(data.tabs);
            }
        }
    }
    report
}
/// Every profile of a browser, each as an imported profile: the default one
/// goes to the chosen destination, the others to local profiles of their
/// names. Bookmarks become pinned items in a Space named after the browser.
fn read_browser(
    source: &SourceProfile,
    selection: Selection,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Category),
    report: &mut ReadReport,
) {
    if source.browser == BrowserKind::Arc && arc::has_sidebar(&source.path) {
        match arc::read_all(&source.path, selection, cancel, progress) {
            Ok((profiles, issues)) => {
                report.data.profiles = profiles;
                report.issues.extend(issues);
            }
            Err(message) => report.issues.push(ImportIssue { category: Category::Bookmarks, message }),
        }
        return;
    }
    let profiles = browser_profiles(source.browser, &source.path);
    if profiles.is_empty() {
        let message = match std::fs::read_dir(&source.path) {
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => format!(
                "Echo isn't allowed to read {}'s data. Allow access in System Settings › Privacy & Security, then try again.",
                source.browser.name()
            ),
            _ => format!("No {} profiles were found.", source.browser.name()),
        };
        report.issues.push(ImportIssue { category: Category::Bookmarks, message });
        return;
    }
    for (index, profile) in profiles.iter().enumerate() {
        let mut single = SourceProfile { path: profile.path.clone(), ..profile.clone() };
        single.browser = source.browser;
        let part = read_categories(&single, selection, cancel, progress);
        report.issues.extend(part.issues);
        let data = part.data;
        let mut imported = browser_core::import::ImportedProfile {
            name: (index > 0).then(|| profile.name.clone()),
            history: data.history,
            ..Default::default()
        };
        if !data.bookmarks.is_empty() || !data.tabs.is_empty() {
            imported.spaces.push(browser_core::import::ImportedSpace {
                title: format!("From {}", source.browser.name()),
                hue: None,
                pinned: data.bookmarks,
                tabs: data.tabs,
            });
        }
        report.data.profiles.push(imported);
    }
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
fn open_database(path: &Path) -> Result<rusqlite::Connection> {
    let db = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    Ok(db)
}
/// A read-only database. A running Chromium browser (Arc, Chrome…) keeps its
/// History locked exclusively, so it is then read from a private snapshot.
pub(crate) struct Database {
    connection: Option<rusqlite::Connection>,
    snapshot: Option<PathBuf>,
}
impl std::ops::Deref for Database {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target {
        self.connection.as_ref().unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        // Close before removing: Windows cannot delete an open file.
        drop(self.connection.take());
        if let Some(snapshot) = &self.snapshot {
            let _ = std::fs::remove_file(snapshot);
        }
    }
}
pub(crate) fn database(path: &Path) -> Result<Database> {
    let connection = open_database(path)?;
    match connection.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(())) {
        Err(rusqlite::Error::SqliteFailure(error, _))
            if matches!(
                error.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            drop(connection);
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let snapshot = std::env::temp_dir().join(format!(
                "echo-import-{}-{}.sqlite",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            if let Err(error) = std::fs::copy(path, &snapshot) {
                let _ = std::fs::remove_file(&snapshot);
                return Err(error.to_string());
            }
            let mut database = Database { connection: None, snapshot: Some(snapshot) };
            database.connection = Some(open_database(database.snapshot.as_deref().unwrap())?);
            Ok(database)
        }
        Err(error) => Err(error.to_string()),
        Ok(()) => Ok(Database { connection: Some(connection), snapshot: None }),
    }
}
pub(crate) fn timestamp(value: f64, epoch: f64) -> std::time::SystemTime {
    let seconds = value + epoch;
    if !seconds.is_finite() || seconds < 0. || seconds > 253402300799. {
        return std::time::UNIX_EPOCH;
    }
    std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(seconds)
}
