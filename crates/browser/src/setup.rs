//! Local setup/import sheets. Source data is read on a worker and published once.
use crate::{
    browser_registration::{BrowserRegistration, DefaultState, SystemRegistration},
    icons::{self, Icon},
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use browser_core::{
    UserProfileId,
    import::{Bookmark, ImportedData},
    onboarding::SetupStep,
};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    Subscription, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, actions, div, hsla, prelude::*, px, rgb,
};
use importer::{BrowserKind, HostPaths, ProfileLocator, ReadReport, Selection, SourceProfile};
use std::{
    collections::HashSet,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

actions!(setup, [Cancel, Next]);
pub(crate) fn init(cx: &mut App) {
    if std::env::var_os("BROWSER_SCRIPT").is_some() {
        if let Some(path) = std::env::var_os("BROWSER_TEST_IMPORT_FIXTURE") {
            let browser = if std::env::var("BROWSER_TEST_IMPORT_BROWSER").as_deref() == Ok("safari")
            {
                BrowserKind::Safari
            } else {
                BrowserKind::Chrome
            };
            cx.set_global(ImportSources(vec![SourceProfile {
                browser,
                name: "Test Profile".into(),
                path: path.into(),
            }]));
        }
        if std::env::var_os("BROWSER_TEST_DEFAULT_BROWSER").is_some() {
            cx.set_global(Registration(Arc::new(AutomationRegistration(
                AtomicBool::new(false),
            ))));
        }
    }
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some("LocalSetup")),
        KeyBinding::new("enter", Next, Some("LocalSetup")),
    ]);
}
#[derive(Clone)]
pub(crate) struct ImportSources(pub Vec<SourceProfile>);
impl gpui::Global for ImportSources {}
/// App icons for the setup sheet. Extracting them from the OS takes seconds
/// in all, so they are found once, off the UI thread.
#[derive(Default)]
struct Artwork {
    brand: Option<Arc<gpui::RenderImage>>,
    browsers: std::collections::HashMap<BrowserKind, Arc<gpui::RenderImage>>,
    /// Each browser icon's dominant color, for its tile in the import list.
    tints: std::collections::HashMap<BrowserKind, gpui::Rgba>,
}
#[derive(Clone)]
struct ArtworkTask(futures::future::Shared<gpui::Task<Arc<Artwork>>>);
impl gpui::Global for ArtworkTask {}
fn artwork(cx: &mut App) -> futures::future::Shared<gpui::Task<Arc<Artwork>>> {
    use futures::FutureExt;
    if let Some(task) = cx.try_global::<ArtworkTask>() {
        return task.0.clone();
    }
    let task = cx
        .background_executor()
        .spawn(async { Arc::new(if cfg!(test) { Artwork::default() } else { Artwork::load() }) })
        .shared();
    cx.set_global(ArtworkTask(task.clone()));
    task
}
impl Artwork {
    fn load() -> Self {
        let brand = std::env::current_exe().ok().and_then(|exe| {
            exe.ancestors()
                .nth(3)
                .filter(|p| p.extension().is_some_and(|e| e == "app"))
                .and_then(crate::download_files::native_icon)
        });
        let mut browsers = std::collections::HashMap::new();
        if cfg!(target_os = "macos") {
            for (kind, app) in [
                (BrowserKind::Arc, "Arc"),
                (BrowserKind::Safari, "Safari"),
                (BrowserKind::Chrome, "Google Chrome"),
                (BrowserKind::Firefox, "Firefox"),
                (BrowserKind::Edge, "Microsoft Edge"),
                (BrowserKind::Brave, "Brave Browser"),
            ] {
                let path = std::path::PathBuf::from(format!("/Applications/{app}.app"));
                if path.exists() {
                    if let Some(icon) =
                        crate::download_files::native_icon(&path.canonicalize().unwrap_or(path))
                    {
                        browsers.insert(kind, icon);
                    }
                }
            }
        }
        let tints = browsers.iter().filter_map(|(kind, icon)| Some((*kind, dominant(icon)?))).collect();
        Self { brand, browsers, tints }
    }
}
/// The average of an icon's vivid pixels (images are BGRA).
fn dominant(icon: &gpui::RenderImage) -> Option<gpui::Rgba> {
    let bytes = icon.as_bytes(0)?;
    let (mut sum, mut weight) = ([0f32; 3], 0f32);
    for p in bytes.chunks_exact(4) {
        let [b, g, r, a] = [p[0], p[1], p[2], p[3]].map(|v| v as f32 / 255.);
        let saturation = r.max(g).max(b) - r.min(g).min(b);
        let w = a * saturation * saturation;
        sum = [sum[0] + r * w, sum[1] + g * w, sum[2] + b * w];
        weight += w;
    }
    (weight > 0.).then(|| gpui::Rgba { r: sum[0] / weight, g: sum[1] / weight, b: sum[2] / weight, a: 1. })
}
#[derive(Clone)]
pub(crate) struct Registration(pub Arc<dyn BrowserRegistration>);
impl gpui::Global for Registration {}
/// Snapshot availability when the Command Bar opens, rather than querying the OS per keystroke.
pub(crate) fn default_browser_available(cx: &App) -> bool {
    default_browser_state(cx) == DefaultState::Other
}
pub(crate) fn default_browser_state(cx: &App) -> DefaultState {
    cx.try_global::<Registration>()
        .map_or_else(|| SystemRegistration.state(), |registration| registration.0.state())
}
pub(crate) fn request_default_browser_task(cx: &mut App) -> Option<gpui::Task<Result<DefaultState, String>>> {
    let registration = cx.try_global::<Registration>()
        .map(|registration| registration.0.clone())
        .unwrap_or_else(|| Arc::new(SystemRegistration));
    registration_task(registration, cx)
}
#[derive(Clone)]
pub(crate) struct CredentialStorage(pub Arc<dyn importer::CredentialStore>);
impl gpui::Global for CredentialStorage {}
#[derive(Clone)]
struct RegistrationPending(Arc<AtomicBool>);
impl gpui::Global for RegistrationPending {}
fn registration_task(
    registration: Arc<dyn BrowserRegistration>,
    cx: &mut App,
) -> Option<gpui::Task<Result<DefaultState, String>>> {
    let pending = if let Some(p) = cx.try_global::<RegistrationPending>() {
        p.0.clone()
    } else {
        let p = Arc::new(AtomicBool::new(false));
        cx.set_global(RegistrationPending(p.clone()));
        p
    };
    if pending.swap(true, Ordering::AcqRel) {
        return None;
    }
    Some(cx.background_executor().spawn(async move {
        struct Release(Arc<AtomicBool>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _release = Release(pending);
        if registration.state() == DefaultState::Ours {
            Ok(DefaultState::Ours)
        } else {
            registration.request()
        }
    }))
}
struct AutomationRegistration(AtomicBool);
impl BrowserRegistration for AutomationRegistration {
    fn state(&self) -> DefaultState {
        if self.0.load(Ordering::Relaxed) {
            DefaultState::Ours
        } else {
            DefaultState::Other
        }
    }
    fn request(&self) -> Result<DefaultState, String> {
        self.0.store(true, Ordering::Relaxed);
        Ok(DefaultState::Ours)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Onboard(SetupStep),
    Sources,
    SafariAccess,
    Destination,
    Reading,
    Bookmarks,
    Passwords,
    Shortcuts,
}
#[derive(Clone)]
enum Event {
    Close,
    Imported(bool),
    Advance(SetupStep),
    Theme(browser_core::themes::SpaceTheme),
    Appearance(browser_core::themes::ThemeAppearance),
    Favorite(usize),
    /// Back to the previous onboarding step.
    Retreat(SetupStep),
    /// Minimize or zoom the browser window under the onboarding sheet.
    Window(crate::menus::WindowOperation),
    Publish(Rc<ImportedData>, Option<UserProfileId>, String),
}
impl EventEmitter<Event> for SetupDialog {}
pub(crate) struct SetupHost {
    pub dialog: Entity<SetupDialog>,
    pub overlay: Option<WindowHandle<Overlay>>,
    _events: Subscription,
}
pub(crate) struct SetupDialog {
    pub(crate) step: Step,
    onboarding: bool,
    appearance: browser_core::themes::ThemeAppearance,
    palette: crate::material::DialogPalette,
    sources: Vec<SourceProfile>,
    pub(crate) source: Option<usize>,
    profiles: Vec<(UserProfileId, String)>,
    pub(crate) destination: Option<Option<UserProfileId>>,
    selected_favorites: HashSet<usize>,
    blocked_bookmarks: HashSet<String>,
    pub(crate) report: Option<Rc<ReadReport>>,
    cancel: Arc<AtomicBool>,
    busy: bool,
    error: Option<String>,
    current_category: Arc<std::sync::Mutex<Option<importer::Category>>>,
    focus: FocusHandle,
    opened: std::time::Instant,
    published: Option<UserProfileId>,
    credentials: Vec<importer::Credential>,
    password_file: bool,
    credential_store: Arc<dyn importer::CredentialStore>,
    /// False while installed browsers are still being looked up.
    sources_ready: bool,
    artwork: Arc<Artwork>,
    welcome_sound: bool,
    /// The browser window's frame inside the desktop-wide intro window.
    pub(crate) welcome_frame: Option<gpui::Bounds<gpui::Pixels>>,
    /// The current per-frame orb image; replaced (and released) every frame.
    welcome_orb: Option<Arc<gpui::RenderImage>>,
    welcome_field: Option<Arc<gpui::RenderImage>>,
    /// The onboarding background, redrawn each frame.
    field: Option<Arc<gpui::RenderImage>>,
    /// The profile onboarding imports into; first run has only this one.
    home_profile: UserProfileId,
    /// "No, I want to type in all my passwords again." on the import step.
    pub(crate) no_import: bool,
    /// The theme being picked, and whether the user has picked one yet.
    pub(crate) theme: browser_core::themes::SpaceTheme,
    pub(crate) theme_picked: bool,
    theme_page: usize,
    theme_field: Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    theme_wave: Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
}
impl Drop for SetupDialog {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Advancing keeps the music playing into setup, as in Arc.
        if self.welcome_sound && self.step == Step::Onboard(SetupStep::Welcome) {
            engine::stop_welcome_sound();
        }
    }
}
impl Focusable for SetupDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
/// Arc's onboarding app grid, in its order.
pub(crate) const FAVORITES: [(&str, &str); 9] = [
    ("X", "https://x.com"),
    ("Gmail", "https://mail.google.com"),
    ("Google Calendar", "https://calendar.google.com"),
    ("Spotify", "https://open.spotify.com"),
    ("YouTube", "https://www.youtube.com"),
    ("Notion", "https://www.notion.so"),
    ("Slack", "https://app.slack.com"),
    ("Figma", "https://www.figma.com"),
    ("Outlook", "https://outlook.live.com/mail"),
];
impl SetupDialog {
    fn new(browser: &browser_core::Browser, onboarding: bool, cx: &mut Context<Self>) -> Self {
        let preset = cx.try_global::<ImportSources>().map(|s| s.0.clone());
        let sources_ready = preset.is_some();
        if !sources_ready {
            let task = cx
                .background_executor()
                .spawn(async { HostPaths::current().profiles() });
            cx.spawn(async move |this, cx| {
                let sources = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.sources = sources;
                    this.sources_ready = true;
                    this.preselect_source();
                    cx.notify();
                });
            })
            .detach();
        }
        let artwork_task = artwork(cx);
        cx.spawn(async move |this, cx| {
            let artwork = artwork_task.await;
            let _ = this.update(cx, |this, cx| {
                this.artwork = artwork;
                cx.notify();
            });
        })
        .detach();
        let step = if onboarding {
            Step::Onboard(browser.onboarding.unwrap_or(SetupStep::Import))
        } else {
            Step::Sources
        };
        let mut this = Self {
            step,
            onboarding,
            appearance: browser.settings.theme_appearance,
            palette: crate::material::DialogPalette::new(true),
            sources: preset.unwrap_or_default(),
            source: None,
            profiles: browser
                .user_profiles()
                .map(|p| (p.id(), p.name().to_owned()))
                .collect(),
            destination: None,
            selected_favorites: FAVORITES
                .iter()
                .enumerate()
                .filter_map(|(i, (_, url))| {
                    browser.favorite_for_url(url).map(|_| i)
                })
                .collect(),
            blocked_bookmarks: HashSet::new(),
            report: None,
            cancel: Arc::new(AtomicBool::new(false)),
            busy: false,
            error: None,
            current_category: Arc::new(std::sync::Mutex::new(None)),
            focus: cx.focus_handle(),
            opened: cx.background_executor().now(),
            published: None,
            credentials: vec![],
            password_file: false,
            credential_store: cx
                .try_global::<CredentialStorage>()
                .map(|s| s.0.clone())
                .unwrap_or_else(|| Arc::new(crate::import_credentials::SystemCredentialStore)),
            sources_ready,
            artwork: Arc::default(),
            // Arc's music starts with the orb and keeps playing into setup.
            welcome_sound: step == Step::Onboard(SetupStep::Welcome)
                && !cfg!(test)
                && engine::play_ui_sound(engine::UiSound::Welcome),
            welcome_frame: None,
            welcome_orb: None,
            welcome_field: None,
            field: None,
            home_profile: browser.active_user_profile_id(),
            no_import: false,
            theme: browser.space().theme(),
            theme_picked: browser.space().theme.is_some(),
            theme_page: 0,
            theme_field: Rc::default(),
            theme_wave: Rc::default(),
        };
        this.preselect_source();
        this
    }
    /// The onboarding import step lists the installed browsers directly, with
    /// the first chosen so Return continues with it.
    fn preselect_source(&mut self) {
        if self.step == Step::Onboard(SetupStep::Import)
            && self.source.is_none()
            && !self.no_import
            && !self.sources.is_empty()
        {
            self.source = Some(0);
        }
    }
    fn welcome_time(&self, cx: &App) -> f32 {
        if crate::native::prefers_reduced_motion() {
            return crate::welcome::READY;
        }
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(self.opened)
            .as_secs_f32();
        if !self.welcome_sound {
            return elapsed;
        }
        // The cue is synthesized before it plays: keep the beat-matched
        // timeline at zero until the output starts. A failed device falls
        // back to the clock after a short grace period.
        match engine::welcome_sound_started() {
            Some(start) => start.elapsed().as_secs_f32(),
            None => (elapsed - 3.).max(0.),
        }
    }
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.cancel.store(true, Ordering::Relaxed);
        cx.emit(Event::Close);
    }
    fn change(&mut self, step: Step, cx: &mut Context<Self>) {
        self.step = step;
        self.preselect_source();
        self.opened = cx.background_executor().now();
        self.error = None;
        cx.notify();
    }
    fn advance(&mut self, step: SetupStep, cx: &mut Context<Self>) {
        cx.emit(Event::Advance(step));
        if let Some(next) = step.next() {
            self.change(Step::Onboard(next), cx);
        } else {
            self.close(cx);
        }
    }
    fn can_next(&self) -> bool {
        !self.busy
            && match self.step {
                Step::Sources => self.source.is_some(),
                Step::SafariAccess => false,
                Step::Destination => self.destination.is_some(),
                Step::Reading => false,
                Step::Onboard(SetupStep::Import) => self.source.is_some() || self.no_import,
                _ => true,
            }
    }
    fn next(&mut self, cx: &mut Context<Self>) {
        if !self.can_next() {
            return;
        }
        match self.step {
            Step::Onboard(SetupStep::Import) if self.no_import => self.advance(SetupStep::Import, cx),
            Step::Onboard(SetupStep::Import) => self.choose_source(cx),

            // Return or the arrow continues at once, even mid-intro.
            Step::Onboard(step) => self.advance(step, cx),
            Step::Sources => self.choose_source(cx),
            Step::Destination => self.read(cx),
            Step::Bookmarks => self.change(Step::Passwords, cx),
            Step::Passwords => self.publish(cx),
            Step::Reading | Step::SafariAccess => {}
            Step::Shortcuts => self.change(Step::Onboard(SetupStep::Basics), cx),
        }
    }
    fn choose_source(&mut self, cx: &mut Context<Self>) {
        let needs_access = self
            .source
            .and_then(|i| self.sources.get(i))
            .is_some_and(|source| {
                source.browser == BrowserKind::Safari
                    && ((std::env::var_os("BROWSER_SCRIPT").is_some()
                        && std::env::var("BROWSER_TEST_IMPORT_ACCESS").as_deref()
                            == Ok("denied"))
                        || std::fs::File::open(source.path.join("History.db")).is_err_and(
                            |error| error.kind() == std::io::ErrorKind::PermissionDenied,
                        ))
            });
        if needs_access {
            self.change(Step::SafariAccess, cx);
            open_privacy_settings();
        } else if self.onboarding {
            // First run has a single profile: import straight into it.
            self.destination = Some(Some(self.home_profile));
            self.read(cx);
        } else {
            self.change(Step::Destination, cx);
        }
    }
    /// Onboarding's Back link.
    fn retreat(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.published.is_some() {
            return;
        }
        match self.step {
            Step::Onboard(step) => {
                if let Some(previous) = step.previous() {
                    cx.emit(Event::Retreat(step));
                    self.change(Step::Onboard(previous), cx);
                }
            }
            Step::SafariAccess | Step::Bookmarks | Step::Passwords => {
                self.change(Step::Onboard(SetupStep::Import), cx)
            }
            _ => self.back(cx),
        }
    }
    fn has_back(&self) -> bool {
        !self.busy
            && self.published.is_none()
            && match self.step {
                Step::Onboard(step) => step.previous().is_some(),
                Step::SafariAccess | Step::Bookmarks | Step::Passwords => true,
                _ => false,
            }
    }
    fn later(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            self.close(cx);
            return;
        }
        match self.step {
            Step::Onboard(step) => self.advance(step, cx),
            Step::Passwords => {
                self.credentials.clear();
                self.publish(cx);
            }
            _ => {
                if self.onboarding {
                    self.advance(SetupStep::Import, cx)
                } else {
                    self.close(cx)
                }
            }
        }
    }
    pub(crate) fn accept_export(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.busy || self.step != Step::Sources { return; }
        let Some(source) = importer::export_source(&path) else { return; };
        self.sources.push(source);
        self.source = Some(self.sources.len() - 1);
        self.error = None;
        cx.notify();
    }
    fn read(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.and_then(|i| self.sources.get(i)).cloned() else {
            return;
        };
        // A new read replaces the tree addressed by checkbox paths. Never apply
        // an old tree's exclusions or password file to a different snapshot.
        self.blocked_bookmarks.clear();
        self.credentials.clear();
        self.password_file = false;
        self.report = None;
        *self.current_category.lock().unwrap() = None;
        self.busy = true;
        self.change(Step::Reading, cx);
        let cancel = self.cancel.clone();
        let category = self.current_category.clone();
        let task = cx.background_executor().spawn(async move {
            importer::read_profile(&source, Selection::default(), &cancel, |current| {
                *category.lock().unwrap() = Some(current);
            })
        });
        cx.spawn(async move |this, cx| {
            let report = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.cancel.load(Ordering::Relaxed) || report.cancelled {
                    return;
                }
                this.busy = false;
                // A whole browser comes over complete, as its Spaces and
                // profiles; only a single profile's bookmarks are picked.
                let whole = !report.data.profiles.is_empty();
                this.report = Some(Rc::new(report));
                this.change(if whole { Step::Passwords } else { Step::Bookmarks }, cx);
            });
        })
        .detach();
    }
    fn bookmark_selected(&self, path: &str) -> bool {
        !self
            .blocked_bookmarks
            .iter()
            .any(|blocked| path == blocked || path.starts_with(&format!("{blocked}.")))
    }
    fn selected_data(&self) -> ImportedData {
        let Some(report) = &self.report else {
            return ImportedData::default();
        };
        fn selected(dialog: &SetupDialog, nodes: &[Bookmark], prefix: &str) -> Vec<Bookmark> {
            nodes
                .iter()
                .enumerate()
                .filter_map(|(i, node)| {
                    let key = if prefix.is_empty() {
                        i.to_string()
                    } else {
                        format!("{prefix}.{i}")
                    };
                    if !dialog.bookmark_selected(&key) {
                        return None;
                    }
                    Some(match node {
                        Bookmark::Page(page) => Bookmark::Page(page.clone()),
                        Bookmark::Folder { title, children } => Bookmark::Folder {
                            title: title.clone(),
                            children: selected(dialog, children, &key),
                        },
                    })
                })
                .collect()
        }
        ImportedData {
            bookmarks: selected(self, &report.data.bookmarks, ""),
            history: report.data.history.clone(),
            tabs: report.data.tabs.clone(),
            profiles: report.data.profiles.clone(),
        }
    }
    fn publish(&mut self, cx: &mut Context<Self>) {
        if self.published.is_some() {
            self.save_passwords(cx);
            return;
        }
        let Some(destination) = self.destination else {
            return;
        };
        let Some(source) = self.source.and_then(|i| self.sources.get(i)) else {
            return;
        };
        let data = self.selected_data();
        if data.bookmarks.is_empty()
            && data.history.is_empty()
            && data.tabs.is_empty()
            && data.profiles.is_empty()
            && self.credentials.is_empty()
            && self.report.as_ref().is_some_and(|r| !r.issues.is_empty())
        {
            self.error = self.report.as_ref().map(|r| {
                r.issues
                    .iter()
                    .map(|i| i.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            cx.notify();
            return;
        }
        self.busy = true;
        cx.emit(Event::Publish(
            Rc::new(data),
            destination,
            format!("From {}", source.browser.name()),
        ));
        cx.notify();
    }
    fn save_passwords(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.published else {
            return;
        };
        if self.credentials.is_empty() {
            self.busy = false;
            self.finish_import(cx);
            return;
        }
        let credentials = std::mem::take(&mut self.credentials);
        let cancel = self.cancel.clone();
        let key = profile.storage_key().unwrap_or_else(|| "default".into());
        self.busy = true;
        let store = self.credential_store.clone();
        let task = cx.background_executor().spawn(async move {
            let result = credentials.iter().try_for_each(|credential| {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Import cancelled.".into());
                }
                store.store(&key, credential)
            });
            (credentials, result)
        });
        cx.spawn(async move |this, cx| {
            let (credentials, result) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => this.finish_import(cx),
                    Err(error) => {
                        this.credentials = credentials;
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn finish_import(&mut self, cx: &mut Context<Self>) {
        self.busy = false;
        if self.onboarding {
            self.advance(SetupStep::Import, cx);
        } else {
            cx.emit(Event::Imported(
                self.report.as_ref().is_some_and(|r| !r.issues.is_empty()),
            ));
        }
    }
    fn choose_passwords(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.load_password_file(path, cx));
        })
        .detach();
    }
    pub(crate) fn load_password_file(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.busy || self.step != Step::Passwords {
            return;
        }
        self.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { importer::read_password_file(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if this.cancel.load(Ordering::Relaxed) {
                    return;
                }
                match result {
                    Ok(credentials) => {
                        this.credentials = credentials;
                        this.password_file = true;
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn title(&self) -> &str {
        match self.step {
            Step::Onboard(SetupStep::Welcome) => "Meet the internet again.",
            Step::Onboard(SetupStep::Theme) => "Add some flair with a theme.",
            Step::Onboard(SetupStep::Favorites) => "Choose the apps you use most.",
            Step::Onboard(SetupStep::Import) => "Get us up to speed.",
            Step::Onboard(SetupStep::DefaultBrowser) => "Go steady with Echo?",
            Step::Onboard(SetupStep::Basics) => "Let’s settle in! Here are the basics.",
            Step::Sources => "Import from your old browser",
            Step::SafariAccess => "Import from Safari",
            Step::Shortcuts => "Essential Keyboard Shortcuts",
            Step::Destination => "Which Echo Profile should we import to?",
            Step::Reading => "Now importing",
            Step::Bookmarks => "Treat your bookmarks to an upgrade.",
            Step::Passwords => "Upload your Passwords.csv file to Echo",
        }
    }
    fn back(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.published.is_some() {
            return;
        }
        match self.step {
            Step::Destination if self.onboarding => {
                self.change(Step::Onboard(SetupStep::Import), cx)
            }
            Step::Destination => self.change(Step::Sources, cx),
            Step::Bookmarks => self.change(Step::Destination, cx),
            Step::Passwords
                if self.report.as_ref().is_some_and(|r| !r.data.profiles.is_empty()) =>
            {
                self.change(Step::Destination, cx)
            }
            Step::Passwords => self.change(Step::Bookmarks, cx),
            _ => {}
        }
    }
    fn description(&self) -> String {
        match self.step {
        Step::Onboard(SetupStep::Welcome)=>String::new(),
        Step::Onboard(SetupStep::Theme)=>"Pick a color, any color — add your favorite shade to your new home on the internet.".into(),
        Step::Onboard(SetupStep::Favorites)=>"Select your favorite apps to keep them handy in your sidebar.".into(),
        Step::Onboard(SetupStep::Import)=>"It’s a bold new internet. Hit the ground running and bring your most important tabs, passwords, history, and bookmarks with you.".into(),
        Step::Onboard(SetupStep::DefaultBrowser)=>"Make Echo your default browser so links open here. We’ll treat your links nicely — you won’t regret it, we promise!".into(),
        Step::Onboard(SetupStep::Basics)=>"Start browsing by clicking New Tab in your sidebar or typing ⌘T.".into(),
        Step::Sources=>{
            let mut text="Import everything you need from your old browser to hit the ground running with Echo.".to_string();
            if let Some(source)=self.source.and_then(|i|self.sources.get(i)){text.push_str(&format!("\n\nWe can import your history, passwords, and bookmarks from {}.",source.browser.name()));}text
        }
        Step::SafariAccess=>"To import Safari browser history, Echo will need Full Disk Access. Feel free to remove access when this step is finished.\n\nAfter you have given access, you can choose to restart Echo now, or continue and restart later.".into(),
        Step::Shortcuts=>String::new(),
        Step::Destination=>"A new Profile will keep your existing data separate from what you’re importing.".into(),
        Step::Reading=>self.current_category.lock().unwrap().map(|c|format!("{}…",match c {importer::Category::Bookmarks=>"Bookmarks",importer::Category::History=>"History",importer::Category::OpenTabs=>"Open Tabs"})).unwrap_or_else(||"Importing…".into()),
        Step::Bookmarks=>"It’s easy to import the rest later.".into(),
        Step::Passwords=>"To manually import passwords, you’ll have to export them. Once imported, your passwords will be encrypted locally on your computer.".into(),
    }
    }
    fn button(
        &self,
        label: &str,
        id: &'static str,
        primary: bool,
        enabled: bool,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .h(px(37.))
            .min_w(px(if primary { 53. } else { 101. }))
            .px(px(12.))
            .when(label == "Maybe Later", |s| s.w(px(101.)))
            .when(label == "Next", |s| s.w(px(53.)))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .border_1()
            .border_color(if primary { rgb(0x5955fe).into() } else { self.palette.border })
            .bg(if primary { rgb(0x3029ff).into() } else { self.palette.input })
            .text_size(px(13.))
            .text_color(if primary { gpui::white() } else { self.palette.text })
            .opacity(if enabled { 1. } else { 0.3 })
            .when(enabled, |s| s.cursor_pointer())
            .child(label.to_owned())
    }
    fn choice(&self, id: String, label: String, selected: bool) -> gpui::Stateful<gpui::Div> {
        div()
            .id(gpui::SharedString::from(id.clone()))
            .debug_selector(move || id.clone())
            .min_h(px(40.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(15.))
            .border_2()
            .border_color(if selected {
                rgb(0x5047ff).into()
            } else {
                hsla(0., 0., if self.palette.dark { 1. } else { 0. }, 0.07)
            })
            .bg(self.palette.control)
            .text_size(px(13.))
            .text_color(hsla(0., 0., if self.palette.dark { 1. } else { 0. }, 0.7))
            .child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        rgb(0x635aff).into()
                    } else {
                        self.palette.muted
                    })
                    .bg(gpui::white())
                    .when(selected, |c| {
                        c.child(
                            div()
                                .m(px(2.))
                                .size(px(8.))
                                .rounded_full()
                                .bg(rgb(0x4435ff)),
                        )
                    }),
            )
            .child(label)
    }
    fn source_choice(
        &self,
        index: usize,
        title: String,
        browser: BrowserKind,
    ) -> gpui::Stateful<gpui::Div> {
        let selected = self.source == Some(index);
        div()
            .id(("import-source", index))
            .debug_selector(move || format!("import-source-{index}"))
            .w(px(if selected { 182. } else { 176. }))
            .h(px(if selected { 52. } else { 49. }))
            .ml(px(if selected { 0. } else { 3. }))
            .flex_none()
            .rounded(px(if selected { 18. } else { 16. }))
            .overflow_hidden()
            .border_2()
            .border_color(if selected {
                rgb(0x5047ff).into()
            } else {
                self.palette.border
            })
            .bg(self.palette.control)
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(15.))
                    .flex()
                    .items_center()
                    .gap(px(25.))
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(self.palette.muted)
                    .child(
                        div()
                            .flex_none()
                            .size(px(16.))
                            .rounded_full()
                            .border_2()
                            .border_color(if selected {
                                rgb(0x635aff).into()
                            } else {
                                self.palette.muted
                            })
                            .bg(gpui::white())
                            .when(selected, |c| {
                                c.child(
                                    div()
                                        .m(px(2.))
                                        .size(px(8.))
                                        .rounded_full()
                                        .bg(rgb(0x4435ff)),
                                )
                            }),
                    )
                    .child(div().min_w_0().truncate().child(title)),
            )
            .child(
                div()
                    .w(px(49.))
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x5d799c))
                    .rounded_r(px(if selected { 16. } else { 14. }))
                    .child(self.artwork.browsers.get(&browser).map_or_else(
                        || icons::ionicon("globe-outline", 24., 1.),
                        |icon| gpui::img(icon.clone()).size(px(24.)).into_any_element(),
                    )),
            )
    }
    fn bookmark_rows(
        &self,
        nodes: &[Bookmark],
        prefix: &str,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = vec![];
        for (i, node) in nodes.iter().enumerate() {
            let key = if prefix.is_empty() {
                i.to_string()
            } else {
                format!("{prefix}.{i}")
            };
            let selected = self.bookmark_selected(&key);
            let title = match node {
                Bookmark::Page(p) => p.title.clone(),
                Bookmark::Folder { title, .. } => title.clone(),
            };
            let action = key.clone();
            rows.push(
                div()
                    .id(gpui::SharedString::from(format!("import-bookmark-{key}")))
                    .debug_selector(move || format!("import-bookmark-{key}"))
                    .min_h(px(28.))
                    .pl(px(depth as f32 * 12.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .child(
                        div()
                            .size(px(14.))
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(0x8d8bba))
                            .when(selected, |s| {
                                s.bg(rgb(0x4435ff))
                                    .child(icons::control_sized(Icon::Check, 12.))
                            }),
                    )
                    .child(div().truncate().child(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if selected {
                            this.blocked_bookmarks.insert(action.clone());
                        } else {
                            this.blocked_bookmarks.retain(|key| {
                                key != &action && !key.starts_with(&format!("{action}."))
                            });
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if let Bookmark::Folder { children, .. } = node {
                rows.extend(self.bookmark_rows(children, &action_key(prefix, i), depth + 1, cx));
            }
        }
        rows
    }
}
fn action_key(prefix: &str, index: usize) -> String {
    if prefix.is_empty() {
        index.to_string()
    } else {
        format!("{prefix}.{index}")
    }
}
#[path = "onboarding_ui.rs"]
mod onboarding_ui;
impl gpui::Render for SetupDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = match self.appearance {
            browser_core::themes::ThemeAppearance::Dark => true,
            browser_core::themes::ThemeAppearance::Light => false,
            browser_core::themes::ThemeAppearance::Automatic => matches!(window.appearance(), gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark),
        };
        self.palette = crate::material::DialogPalette::new(dark);
        if self.step == Step::Onboard(SetupStep::Welcome) {
            let t = self.welcome_time(cx);
            cx.on_next_frame(window, |_, _, cx| cx.notify());
            for previous in [self.welcome_orb.take(), self.welcome_field.take()].into_iter().flatten() {
                let _ = window.drop_image(previous);
            }
            if t < crate::welcome::ORB_GONE {
                self.welcome_orb = Some(crate::welcome::orb_image(t));
            }
            let size = self.welcome_frame.map_or(window.viewport_size(), |f| f.size);
            let aspect = f32::from(size.width) / f32::from(size.height).max(1.);
            self.welcome_field = Some(crate::welcome::field_image(t, aspect));
            let arrow = div()
                .id("welcome-continue")
                .debug_selector(|| "welcome-continue".into())
                .when(t >= crate::welcome::ARROW_ACTIVE, |s| {
                    s.on_click(cx.listener(|this, _, _, cx| this.next(cx)))
                });
            let pane = t >= crate::welcome::PANE_ACTIVE;
            // Closing the window during first run quits; the intro resumes
            // on the next launch.
            let close = div()
                .id("welcome-close")
                .debug_selector(|| "welcome-close".into())
                .when(pane, |s| s.on_click(|_, _, cx| crate::quit::request(cx)));
            let minimize = div()
                .id("welcome-minimize")
                .debug_selector(|| "welcome-minimize".into())
                .when(pane, |s| {
                    s.on_click(|_, window, _| window.minimize_window())
                });
            // Return continues at any moment; there is no escape.
            return div()
                .id("local-import-sheet")
                .debug_selector(|| "local-import-sheet".into())
                .key_context("LocalSetup")
                .track_focus(&self.focus)
                .size_full()
                .font_family(".SystemUIFont")
                .on_action(cx.listener(|this, _: &Next, _, cx| this.next(cx)))
                .child(crate::welcome::render(
                    t,
                    window.viewport_size(),
                    self.welcome_frame,
                    self.welcome_orb.clone(),
                    self.welcome_field.clone(),
                    window.scale_factor(),
                    arrow,
                    (close, minimize),
                ));
        }
        if self.onboarding {
            return self.render_onboarding(dark, window, cx);
        }
        let elapsed = cx
            .background_executor()
            .now()
            .saturating_duration_since(self.opened)
            .as_secs_f32();
        let amount = if crate::native::prefers_reduced_motion() {
            1.
        } else {
            (elapsed / 0.2).clamp(0., 1.)
        };
        if self.busy || amount < 1. {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let mut right = div()
            .id("setup-right")
            .absolute()
            .left(px(335.))
            .top(px(92.))
            .w(px(216.))
            .h(px(260.))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(10.));
        match self.step {
            Step::SafariAccess => {
                right = right
                    .left(px(362.))
                    .top(px(101.))
                    .w(px(386.))
                    .h(px(346.))
                    .justify_start()
                    .gap_0()
                    .text_color(rgb(0x222222))
                    .bg(rgb(0xf4f4f4))
                    .rounded(px(4.))
                    .child(
                        div()
                            .h(px(35.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Full Disk Access"),
                    )
                    .child(
                        div()
                            .h(px(20.))
                            .px(px(14.))
                            .text_size(px(10.))
                            .child("Allow the applications below to access data."),
                    )
                    .child(
                        div()
                            .mx(px(14.))
                            .mt(px(15.))
                            .h(px(44.))
                            .rounded(px(8.))
                            .bg(rgb(0xe5e5e5))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(12.))
                            .child("Echo")
                            .child(
                                div()
                                    .w(px(24.))
                                    .h(px(14.))
                                    .rounded_full()
                                    .bg(rgb(0xbcbcbc))
                                    .child(div().size(px(12.)).rounded_full().bg(gpui::white())),
                            ),
                    )
                    .child(div().mt(px(12.)).px(px(20.)).text_size(px(16.)).child("+"));
            }
            Step::Shortcuts => {
                for (label, key) in [
                    ("New Tab", "⌘T"),
                    ("Switch Tabs", "⌃Tab"),
                    ("Copy Current URL", "⇧⌘C"),
                    ("Add Split View", "⌃⇧="),
                    ("New Little Echo Window", "⌥⌘N"),
                ] {
                    right = right.child(
                        div()
                            .h(px(34.))
                            .text_size(px(13.))
                            .child(label)
                            .child(div().text_color(hsla(0., 0., if self.palette.dark { 1. } else { 0. }, 0.6)).child(key)),
                    );
                }
            }
            Step::Sources | Step::Onboard(SetupStep::Import) => {
                right = right
                    .left(px(332.))
                    .overflow_y_scroll()
                    .flex_shrink_0()
                    .when(self.sources.len() > 4, |s| s.justify_start());
                if !self.sources_ready {
                    right = right.child("Looking for browsers…");
                } else if self.sources.is_empty() {
                    right = right.child("No browser profiles found.");
                }
                for (i, source) in self.sources.iter().enumerate() {
                    let title = if self
                        .sources
                        .iter()
                        .filter(|s| s.browser == source.browser)
                        .count()
                        > 1
                    {
                        format!("{} — {}", source.browser.name(), source.name)
                    } else {
                        source.browser.name().to_uppercase()
                    };
                    right = right.child(self.source_choice(i, title, source.browser).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.source = Some(i);
                            cx.notify();
                        }),
                    ));
                }
            }
            Step::Destination => {
                right = right
                    .overflow_y_scroll()
                    .when(self.profiles.len() > 4, |s| s.justify_start());
                for (i, (id, name)) in self.profiles.iter().enumerate() {
                    let id = *id;
                    let title = if self.profiles.len() == 1 {
                        "My Existing Echo Profile".into()
                    } else {
                        name.clone()
                    };
                    right = right.child(
                        self.choice(
                            format!("import-destination-{i}"),
                            title,
                            self.destination == Some(Some(id)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.destination = Some(Some(id));
                            cx.notify();
                        })),
                    );
                }
                right = right.child(
                    self.choice(
                        "import-new-profile".into(),
                        "Create a New Profile".into(),
                        self.destination == Some(None),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.destination = Some(None);
                        cx.notify();
                    })),
                );
            }
            Step::Bookmarks => {
                if let Some(report) = self.report.clone() {
                    right = right.justify_start().overflow_y_scroll().child(
                        div().children(self.bookmark_rows(&report.data.bookmarks, "", 0, cx)),
                    );
                }
            }
            Step::Passwords => {
                right = right.child(
                    div()
                        .id("import-password-file")
                        .debug_selector(|| "import-password-file".into())
                        .h(px(120.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(rgb(0x6b67a4))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.))
                        .child(if self.password_file {
                            "Passwords.csv"
                        } else {
                            "Drag or click here to upload"
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.choose_passwords(cx)))
                        .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                            if paths.paths().len() == 1 {
                                this.load_password_file(paths.paths()[0].clone(), cx);
                            }
                        })),
                )
            }
            _ => {}
        }
        let enabled = self.can_next();
        let access = self.step == Step::SafariAccess;
        let label = match self.step {
            Step::Onboard(SetupStep::DefaultBrowser) => "Set as Default",
            Step::Shortcuts | Step::Onboard(SetupStep::Basics) => "Done",
            Step::SafariAccess => "Restart Echo",
            Step::Reading => "Importing…",
            _ => "Next",
        };
        let errors = self.error.clone().or_else(|| {
            self.report
                .as_ref()
                .filter(|r| {
                    self.step == Step::Bookmarks
                        || (self.step == Step::Passwords && !r.data.profiles.is_empty())
                })
                .and_then(|r| {
                    (!r.issues.is_empty()).then(|| {
                        r.issues
                            .iter()
                            .map(|i| {
                                format!(
                                    "{}: {}",
                                    match i.category {
                                        importer::Category::Bookmarks => "Bookmarks",
                                        importer::Category::History => "History",
                                        importer::Category::OpenTabs => "Open tabs",
                                    },
                                    i.message
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                })
        });
        div()
            .id("local-import-sheet")
            .debug_selector(|| "local-import-sheet".into())
            .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                if paths.paths().len() == 1 { this.accept_export(paths.paths()[0].clone(), cx); }
            }))
            .key_context("LocalSetup")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family(".SystemUIFont")
            .text_color(self.palette.text)
            .bg(if self.onboarding {
                rgb(0x3139fb).into()
            } else {
                hsla(0., 0., 0., 0.18)
            })
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.step == Step::Shortcuts {
                    this.change(Step::Onboard(SetupStep::Basics), cx);
                } else {
                    this.close(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.next(cx)))
            // The onboarding sheet covers the window's own traffic lights.
            .when(self.onboarding, |s| {
                s.child(crate::welcome::window_buttons(
                    div()
                        .id("setup-window-close")
                        .debug_selector(|| "setup-window-close".into())
                        .on_click(|_, _, cx| crate::quit::request(cx)),
                    div()
                        .id("setup-window-minimize")
                        .debug_selector(|| "setup-window-minimize".into())
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(Event::Window(crate::menus::WindowOperation::Minimize))
                        })),
                    Some(
                        div()
                            .id("setup-window-zoom")
                            .debug_selector(|| "setup-window-zoom".into())
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(Event::Window(crate::menus::WindowOperation::Zoom))
                            })),
                    ),
                ))
            })
            .child(
                div()
                    .id("setup-card")
                    .debug_selector(|| "setup-card".into())
                    .relative()
                    .opacity(amount)
                    .w(px(if access { 790. } else { 580. }))
                    .h(px(if access { 632. } else { 454. }))
                    .rounded(px(20.))
                    .bg(if dark { rgb(0x151c30).into() } else { self.palette.surface })
                    .child(
                        div()
                            .absolute()
                            .left(px(28.))
                            .top(px(28.))
                            .w(px(if access { 330. } else { 260. }))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(64.))
                                    .pl(px(8.))
                                    .pt(px(8.))
                                    .text_size(px(28.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(self.artwork.brand.as_ref().map_or_else(
                                        || div().child("Echo").into_any_element(),
                                        |image| {
                                            gpui::img(image.clone())
                                                .size(px(48.))
                                                .into_any_element()
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .mt(px(16.))
                                    .text_size(px(18.))
                                    .line_height(px(22.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(self.title().to_owned()),
                            )
                            .child(
                                div()
                                    .mt(px(12.))
                                    .text_size(px(16.))
                                    .line_height(px(23.))
                                    .text_color(hsla(0., 0., if self.palette.dark { 1. } else { 0. }, 0.5))
                                    .flex()
                                    .flex_col()
                                    .gap(px(12.))
                                    .children(
                                        self.description()
                                            .split("\n\n")
                                            .map(|text| div().child(text.to_owned())),
                                    ),
                            )
                            .when_some(errors, |s, error| {
                                s.child(
                                    div()
                                        .mt(px(10.))
                                        .max_h(px(90.))
                                        .overflow_hidden()
                                        .text_size(px(12.))
                                        .text_color(rgb(0xf5b7bf))
                                        .child(error),
                                )
                            }),
                    )
                    .child(right)
                    .when(
                        matches!(
                            self.step,
                            Step::Destination | Step::Bookmarks | Step::Passwords
                        ) && self.published.is_none(),
                        |card| {
                            card.child(
                                self.button("Back", "setup-back", false, !self.busy)
                                    .absolute()
                                    .left(px(28.))
                                    .bottom(px(28.))
                                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
                            )
                        },
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(20.))
                            .right(px(20.))
                            .id("setup-close")
                            .debug_selector(|| "setup-close".into())
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(hsla(0., 0., if self.palette.dark { 1. } else { 0. }, 0.7))
                            .child(icons::control_sized(Icon::Close, 24.))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(28.))
                            .bottom(px(28.))
                            .flex()
                            .gap(px(8.))
                            .child(
                                self.button(
                                    if access {
                                        "Continue without restarting"
                                    } else if self.busy {
                                        "Cancel"
                                    } else {
                                        "Maybe Later"
                                    },
                                    "setup-later",
                                    false,
                                    true,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if this.step == Step::SafariAccess {
                                            this.change(Step::Destination, cx);
                                        } else {
                                            this.later(cx);
                                        }
                                    },
                                )),
                            )
                            .child(
                                self.button(label, "setup-next", true, enabled)
                                    .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                            ),
                    ),
            )
    }
}

impl BrowserWindow {
    pub(crate) fn open_setup(&mut self, onboarding: bool, cx: &mut Context<Self>) {
        if crate::quit::is_quitting()
            || self.transient.is_some()
            || self.browser.is_private()
            || self.browser.is_blank()
        {
            return;
        }
        self.refresh_profile_projection();
        self.close_setup(cx);
        self.close_command_bar(cx);
        self.close_find(cx);
        self.close_tab_menu(cx);
        self.close_site_controls(cx);
        self.close_new_menu(cx);
        self.close_user_profile_dialog(cx);
        self.close_theme_editor(cx);
        if let Some(pages) = &self.pages {
            pages.borrow_mut().request_ui_focus();
        }
        if onboarding {
            if let Some(step @ (SetupStep::DefaultBrowser | SetupStep::Basics)) = self.browser.onboarding {
                self.edit_browser(|b| b.advance_setup(step));
                return;
            }
        }
        let dialog = cx.new(|cx| SetupDialog::new(&self.browser, onboarding, cx));
        let events = cx.subscribe(&dialog, |_, sender, event: &Event, cx| {
            let sender = sender.clone();
            let event = event.clone();
            let owner = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = owner.update(cx, |this, cx| this.setup_event(&sender, event, cx));
            });
        });
        self.setup = Some(SetupHost {
            dialog: dialog.clone(),
            overlay: None,
            _events: events,
        });
        let owner = cx.weak_entity();
        let main = self.main_window;
        let parent = self.native_view;
        cx.notify();
        cx.defer(move |cx| {
            if crate::quit::is_quitting() {
                return;
            }
            if !owner
                .read_with(cx, |v, _| {
                    v.setup.as_ref().is_some_and(|s| s.dialog == dialog)
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok((bounds, area)) =
                main.update(cx, |_, w, cx| (w.bounds(), crate::native::display_work_area(w, cx)))
            else {
                return;
            };
            // Arc's intro starts on the desktop with no window: an orb that
            // later blooms into the window. Cover the work area instead of
            // the browser window, and hide the browser until setup proper.
            let welcome = dialog.read(cx).step == Step::Onboard(SetupStep::Welcome);
            let area = area.filter(|_| welcome).unwrap_or(bounds);
            if welcome {
                dialog.update(cx, |d, _| {
                    d.welcome_frame = Some(gpui::Bounds::new(
                        bounds.origin - area.origin,
                        bounds.size,
                    ))
                });
            }
            let Ok(overlay) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(area)),
                    titlebar: None,
                    kind: WindowKind::Normal,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |window, cx| {
                    if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                        if welcome {
                            crate::native::set_traffic_lights_hidden(child, true);
                            crate::native::set_window_shown(parent, false);
                        } else {
                            crate::native::attach_overlay(parent, child, false);
                        }
                    }
                    cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::Setup, window, cx))
                },
            ) else {
                let _ = owner.update(cx, |v, cx| v.close_setup(cx));
                return;
            };
            let attached = owner
                .update(cx, |v, _| {
                    if let Some(host) = v.setup.as_mut().filter(|s| s.dialog == dialog) {
                        host.overlay = Some(overlay);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if !attached {
                let _ = overlay.update(cx, |_, w, _| w.remove_window());
                return;
            }
            let _ = overlay.update(cx, |_, window, cx| {
                window.activate_window();
                dialog.read(cx).focus.focus(window);
            });
        });
    }
    fn setup_event(&mut self, sender: &Entity<SetupDialog>, event: Event, cx: &mut Context<Self>) {
        if !self
            .setup
            .as_ref()
            .is_some_and(|host| host.dialog == *sender)
            || crate::quit::is_quitting()
        {
            return;
        }
        self.refresh_profile_projection();
        match event {
            Event::Close => self.close_setup(cx),
            Event::Imported(partial) => {
                self.close_setup(cx);
                self.show_toast(
                    if partial {
                        "Only some of your data imported. Please try again!"
                    } else {
                        "Success! We imported all of your stuff"
                    }
                    .into(),
                    cx,
                );
            }
            Event::Advance(SetupStep::Welcome) => {
                // Leave the desktop-wide intro for setup over the browser window.
                self.edit_browser(|b| b.advance_setup(SetupStep::Welcome));
                self.close_setup(cx);
                if self.browser.onboarding.is_some() {
                    self.open_setup(true, cx);
                }
            }
            Event::Advance(step) => {
                if !self.edit_browser(|b| b.advance_setup(step)) {
                    if let Some(current) = self.browser.onboarding {
                        sender.update(cx, |d, cx| d.change(Step::Onboard(current), cx));
                    } else {
                        self.close_setup(cx);
                    }
                }
            }
            Event::Window(operation) => {
                // Directly on the window: the menu path would close the sheet.
                let main = self.main_window;
                cx.defer(move |cx| {
                    let _ = main.update(cx, |_, window, _| match operation {
                        crate::menus::WindowOperation::Minimize => window.minimize_window(),
                        crate::menus::WindowOperation::Zoom => window.zoom_window(),
                        _ => {}
                    });
                });
            }
            Event::Theme(theme) => {
                let space = self.browser.space().id;
                self.edit_browser(|b| b.update_space_theme(space, theme));
            }
            Event::Appearance(mode) => {
                self.edit_profile(|b| b.set_browser_appearance(mode));
            }
            Event::Retreat(step) => {
                if !self.edit_browser(|b| b.retreat_setup(step)) {
                    if let Some(current) = self.browser.onboarding {
                        sender.update(cx, |d, cx| d.change(Step::Onboard(current), cx));
                    }
                }
            }
            Event::Favorite(index) => {
                let (name, url) = FAVORITES[index];
                self.edit_browser(|b| {
                    // Match by the Favorite's saved address: Gmail and others
                    // redirect (e.g. to sign-in) once loaded.
                    if let Some(id) = b.favorite_for_url(url) {
                        b.remove_favorite(id);
                    } else {
                        b.add_favorite(url, name, std::time::SystemTime::now());
                    }
                });
            }
            Event::Publish(data, target, name) => {
                let result = self.edit_browser(|browser| {
                    let mut next = browser.clone();
                    let target = match target {
                        Some(id) => id,
                        None => {
                            let mut label = name.clone();
                            let mut suffix = 2;
                            while next.user_profiles().any(|p| p.name() == label) {
                                label = format!("{name} ({suffix})");
                                suffix += 1;
                            }
                            match next.create_user_profile(&label) {
                                Ok(id) => id,
                                Err(_) => {
                                    return Err(
                                        "The import profile could not be created.".to_owned()
                                    );
                                }
                            }
                        }
                    };
                    match next.import_local(target, &name, &data, std::time::SystemTime::now()) {
                        Ok(_) => {
                            *browser = next;
                            Ok(target)
                        }
                        Err(e) => Err(e.to_owned()),
                    }
                });
                sender.update(cx, |dialog, cx| {
                    dialog.busy = false;
                    match result {
                        Ok(profile) => {
                            dialog.published = Some(profile);
                            dialog.save_passwords(cx);
                        }
                        Err(error) => {
                            dialog.error = Some(error);
                            cx.notify();
                        }
                    }
                });
            }
        }
        cx.notify();
    }
    pub(crate) fn close_setup(&mut self, cx: &mut Context<Self>) {
        self.close_setup_with_focus(PopupFocus::Restore, cx);
    }
    pub(crate) fn close_setup_with_focus(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        if let Some(host) = self.setup.take() {
            if host.dialog.read(cx).welcome_frame.is_some() {
                if let Some(parent) = self.native_view {
                    crate::native::set_window_shown(parent, true);
                }
            }
            host.dialog
                .update(cx, |d, _| d.cancel.store(true, Ordering::Relaxed));
            self.close_popup_overlay(host.overlay, focus, true, cx);
            cx.notify();
        }
    }
    pub(crate) fn take_setup_child(&mut self, cx: &mut App) -> Option<WindowHandle<Overlay>> {
        let host = self.setup.take()?;
        host.dialog
            .update(cx, |d, _| d.cancel.store(true, Ordering::Relaxed));
        host.overlay
    }
    pub(crate) fn render_setup(&self) -> AnyElement {
        self.setup.as_ref().map_or_else(
            || div().into_any_element(),
            |host| host.dialog.clone().into_any_element(),
        )
    }
    pub(crate) fn request_default_browser(&mut self, cx: &mut Context<Self>) {
        let Some(task) = request_default_browser_task(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => {}
                Err(error) => this.show_toast(error, cx),
            });
        })
        .detach();
    }
}

impl BrowserWindow {
    pub(crate) fn resize_setup(&self, window: &Window, cx: &mut Context<Self>) {
        let Some(host) = self.setup.as_ref() else {
            return;
        };
        let Some(child) = host.overlay else {
            return;
        };
        if host.dialog.read(cx).welcome_frame.is_some() {
            return;
        }
        let dialog = host.dialog.clone();
        let size = window.viewport_size();
        cx.defer(move |cx| {
            let _ = child.update(cx, |_, window, cx| {
                // AppKit moves child windows with their owner. Setting the frame
                // during the owner's bounds notification applies that move twice.
                if window.viewport_size() != size {
                    window.resize(size);
                }
                dialog.update(cx, |_, cx| cx.notify());
                cx.notify();
            });
        });
    }
}

fn open_privacy_settings() {
    #[cfg(all(target_os = "macos", not(test)))]
    if std::env::var_os("BROWSER_SCRIPT").is_none() {
        let _ = std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .spawn();
    }
}
