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
#[derive(Clone)]
pub(crate) struct Registration(pub Arc<dyn BrowserRegistration>);
impl gpui::Global for Registration {}
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
    Complete,
    Shortcuts,
}
#[derive(Clone)]
enum Event {
    Close,
    Advance(SetupStep),
    Theme(usize),
    Favorite(usize),
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
    registration: Arc<dyn BrowserRegistration>,
    default_state: DefaultState,
    brand: Option<Arc<gpui::RenderImage>>,
    artwork: std::collections::HashMap<BrowserKind, Arc<gpui::RenderImage>>,
}
impl Drop for SetupDialog {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Focusable for SetupDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
// URLs are present verbatim in the installed Arc resource strings.
const FAVORITES: [(&str, &str); 3] = [
    ("Gmail", "https://mail.google.com"),
    ("Google Calendar", "https://calendar.google.com"),
    ("YouTube", "https://www.youtube.com"),
];
impl SetupDialog {
    fn new(browser: &browser_core::Browser, onboarding: bool, cx: &mut Context<Self>) -> Self {
        let sources = cx
            .try_global::<ImportSources>()
            .map(|s| s.0.clone())
            .unwrap_or_else(|| HostPaths::current().profiles());
        let registration = cx
            .try_global::<Registration>()
            .map(|r| r.0.clone())
            .unwrap_or_else(|| Arc::new(SystemRegistration));
        let step = if onboarding {
            Step::Onboard(browser.onboarding.unwrap_or(SetupStep::Theme))
        } else {
            Step::Sources
        };
        let brand = std::env::current_exe().ok().and_then(|exe| {
            exe.ancestors()
                .nth(3)
                .filter(|p| p.extension().is_some_and(|e| e == "app"))
                .and_then(crate::download_files::native_icon)
        });
        let mut artwork = std::collections::HashMap::new();
        if cfg!(target_os = "macos") && !cfg!(test) {
            for (kind, app) in [
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
                        artwork.insert(kind, icon);
                    }
                }
            }
        }
        Self {
            step,
            onboarding,
            sources,
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
                    browser
                        .active_user_profile()
                        .favorites
                        .iter()
                        .any(|t| t.url == *url)
                        .then_some(i)
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
            default_state: registration.state(),
            credential_store: cx
                .try_global::<CredentialStorage>()
                .map(|s| s.0.clone())
                .unwrap_or_else(|| Arc::new(crate::import_credentials::SystemCredentialStore)),
            registration,
            brand,
            artwork,
        }
    }
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        self.cancel.store(true, Ordering::Relaxed);
        cx.emit(Event::Close);
    }
    fn change(&mut self, step: Step, cx: &mut Context<Self>) {
        self.step = step;
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
                _ => true,
            }
    }
    fn next(&mut self, cx: &mut Context<Self>) {
        if !self.can_next() {
            return;
        }
        match self.step {
            Step::Onboard(SetupStep::Import) => self.change(Step::Sources, cx),
            Step::Onboard(SetupStep::DefaultBrowser) => self.request_default(cx),
            Step::Onboard(step) => self.advance(step, cx),
            Step::Sources => {
                let needs_access =
                    self.source
                        .and_then(|i| self.sources.get(i))
                        .is_some_and(|source| {
                            source.browser == BrowserKind::Safari
                                && ((std::env::var_os("BROWSER_SCRIPT").is_some()
                                    && std::env::var("BROWSER_TEST_IMPORT_ACCESS").as_deref()
                                        == Ok("denied"))
                                    || std::fs::File::open(source.path.join("History.db"))
                                        .is_err_and(|error| {
                                            error.kind() == std::io::ErrorKind::PermissionDenied
                                        }))
                        });
                if needs_access {
                    self.change(Step::SafariAccess, cx);
                    open_privacy_settings();
                } else {
                    self.change(Step::Destination, cx);
                }
            }
            Step::Destination => self.read(cx),
            Step::Bookmarks => self.change(Step::Passwords, cx),
            Step::Passwords => self.publish(cx),
            Step::Complete => {
                if self.onboarding {
                    self.advance(SetupStep::Import, cx)
                } else {
                    self.close(cx)
                }
            }
            Step::Reading | Step::SafariAccess => {}
            Step::Shortcuts => self.change(Step::Onboard(SetupStep::Basics), cx),
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
    fn read(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.and_then(|i| self.sources.get(i)).cloned() else {
            return;
        };
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
                this.report = Some(Rc::new(report));
                this.change(Step::Bookmarks, cx);
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
        self.busy = true;
        cx.emit(Event::Publish(
            Rc::new(self.selected_data()),
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
            self.change(Step::Complete, cx);
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
                    Ok(()) => this.change(Step::Complete, cx),
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
    fn request_default(&mut self, cx: &mut Context<Self>) {
        let registration = self.registration.clone();
        self.busy = true;
        self.error = None;
        let Some(task) = registration_task(registration, cx) else {
            self.busy = false;
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(state) => {
                        this.default_state = state;
                        if state == DefaultState::Ours {
                            this.advance(SetupStep::DefaultBrowser, cx);
                        }
                    }
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn title(&self) -> &str {
        match self.step {
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
            Step::Complete => "Finished importing",
        }
    }
    fn back(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.published.is_some() {
            return;
        }
        match self.step {
            Step::Destination => self.change(Step::Sources, cx),
            Step::Bookmarks => self.change(Step::Destination, cx),
            Step::Passwords => self.change(Step::Bookmarks, cx),
            _ => {}
        }
    }
    fn description(&self) -> String {
        match self.step {
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
        Step::Complete=>if self.report.as_ref().is_some_and(|r|!r.issues.is_empty()) {"Some items could not be imported.".into()}else{"Your tabs and bookmarks are ready in your sidebar.".into()},
    }
    }
    fn button(
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
            .border_color(rgb(if primary { 0x5955fe } else { 0x5c5998 }))
            .bg(rgb(if primary { 0x3029ff } else { 0x302e61 }))
            .text_size(px(13.))
            .text_color(gpui::white())
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
                hsla(0., 0., 1., 0.07)
            })
            .bg(rgb(0x2b3142))
            .text_size(px(13.))
            .text_color(hsla(0., 0., 1., 0.7))
            .child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        rgb(0x635aff)
                    } else {
                        rgb(0xaeb0b7)
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
                rgb(0x5047ff)
            } else {
                rgb(0x383e4e)
            })
            .bg(rgb(0x2b3142))
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
                    .text_color(rgb(0xb9bac4))
                    .child(
                        div()
                            .flex_none()
                            .size(px(16.))
                            .rounded_full()
                            .border_2()
                            .border_color(if selected {
                                rgb(0x635aff)
                            } else {
                                rgb(0xaeb0b7)
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
                    .child(self.artwork.get(&browser).map_or_else(
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
impl gpui::Render for SetupDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .child(div().text_color(hsla(0., 0., 1., 0.6)).child(key)),
                    );
                }
            }
            Step::Sources => {
                right = right.left(px(332.)).overflow_y_scroll().flex_shrink_0();
                if self.sources.is_empty() {
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
                right = right.overflow_y_scroll();
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
            Step::Onboard(SetupStep::Theme) => {
                right = right.flex_row().flex_wrap().gap(px(12.));
                for i in 0..9 {
                    let c = crate::theme_editor::preset_color(0, i);
                    right = right.child(
                        div()
                            .id(("setup-theme", i))
                            .debug_selector(move || format!("setup-theme-{i}"))
                            .size(px(48.))
                            .rounded_full()
                            .border_2()
                            .border_color(hsla(0., 0., 1., 0.3))
                            .bg(gpui::Rgba {
                                r: c.red,
                                g: c.green,
                                b: c.blue,
                                a: 1.,
                            })
                            .on_click(cx.listener(move |_, _, _, cx| cx.emit(Event::Theme(i)))),
                    );
                }
            }
            Step::Onboard(SetupStep::Favorites) => {
                for (i, (name, _)) in FAVORITES.iter().enumerate() {
                    right = right.child(
                        self.choice(
                            format!("setup-favorite-{i}"),
                            (*name).into(),
                            self.selected_favorites.contains(&i),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.selected_favorites.insert(i) {
                                this.selected_favorites.remove(&i);
                            }
                            cx.emit(Event::Favorite(i));
                            cx.notify();
                        })),
                    );
                }
            }
            Step::Onboard(SetupStep::Basics) => {
                for (label, key) in [
                    ("Open your first tab", "⌘T"),
                    ("Pin tabs to save for later", "⌘D"),
                    ("Multitask with Split View", "⌃⇧="),
                    ("Spaces for work and life", "⌃1 … ⌃9"),
                ] {
                    right = right.child(
                        div()
                            .min_h(px(40.))
                            .text_size(px(13.))
                            .child(label)
                            .child(div().text_color(hsla(0., 0., 1., 0.6)).child(key)),
                    );
                }
                right = right.child(
                    Self::button("Learn Essential Shortcuts", "setup-shortcuts", false, true)
                        .on_click(cx.listener(|this, _, _, cx| this.change(Step::Shortcuts, cx))),
                );
            }
            _ => {}
        }
        let enabled = self.can_next();
        let access = self.step == Step::SafariAccess;
        let label = match self.step {
            Step::Onboard(SetupStep::DefaultBrowser) => "Set as Default",
            Step::Complete | Step::Shortcuts | Step::Onboard(SetupStep::Basics) => "Done",
            Step::SafariAccess => "Restart Echo",
            Step::Reading => "Importing…",
            _ => "Next",
        };
        let errors = self.error.clone().or_else(|| {
            self.report
                .as_ref()
                .filter(|_| self.step == Step::Bookmarks || self.step == Step::Complete)
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
            .key_context("LocalSetup")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family(".SystemUIFont")
            .text_color(gpui::white())
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
            .child(
                div()
                    .id("setup-card")
                    .debug_selector(|| "setup-card".into())
                    .relative()
                    .opacity(amount)
                    .w(px(if access { 790. } else { 580. }))
                    .h(px(if access { 632. } else { 454. }))
                    .rounded(px(20.))
                    .bg(rgb(0x151c30))
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
                                    .child(self.brand.as_ref().map_or_else(
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
                                    .text_color(hsla(0., 0., 1., 0.5))
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
                                Self::button("Back", "setup-back", false, !self.busy)
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
                            .text_color(hsla(0., 0., 1., 0.7))
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
                                Self::button(
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
                                Self::button(label, "setup-next", true, enabled)
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
            let Ok(bounds) = main.update(cx, |_, w, _| w.bounds()) else {
                return;
            };
            let Ok(overlay) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
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
                        crate::native::attach_overlay(parent, child, false);
                    }
                    cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::Setup, cx))
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
            Event::Advance(step) => {
                if !self.edit_browser(|b| b.advance_setup(step)) {
                    if let Some(current) = self.browser.onboarding {
                        sender.update(cx, |d, cx| d.change(Step::Onboard(current), cx));
                    } else {
                        self.close_setup(cx);
                    }
                }
            }
            Event::Theme(index) => {
                let space = self.browser.space().id;
                let mut theme = self.browser.space().theme();
                theme.colors = vec![crate::theme_editor::preset_color(0, index)];
                self.edit_browser(|b| b.update_space_theme(space, theme));
            }
            Event::Favorite(index) => {
                let (name, url) = FAVORITES[index];
                self.edit_browser(|b| {
                    let existing = b
                        .active_user_profile()
                        .favorites
                        .iter()
                        .find(|t| t.url == url)
                        .map(|t| t.id);
                    if let Some(id) = existing {
                        b.unfavorite_tab(id);
                    } else {
                        let id = b.new_tab(url, std::time::SystemTime::now());
                        b.set_title(id, name);
                        b.favorite_tab(id);
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
        let registration = cx
            .try_global::<Registration>()
            .map(|r| r.0.clone())
            .unwrap_or_else(|| Arc::new(SystemRegistration));
        let Some(task) = registration_task(registration, cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(DefaultState::Ours) => {
                    this.show_toast("Echo is your default browser".into(), cx)
                }
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
