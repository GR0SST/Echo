//! Chromium (via CEF) web views embedded as child views of a native window.
//! The rest of the app talks to the engine only through this crate.
//!
//! Everything here runs on the main thread: CEF is configured with an
//! external message pump, and the UI toolkit calls [`Engine::pump`].

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cef::*;

#[cfg(target_os = "macos")]
mod mac;

/// Identifies one web view (one tab's page).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(u64);

impl ViewId {
    /// For alternative engines, e.g. test fakes.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// A rectangle in logical pixels, top-left origin, relative to the parent view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A key combination the browser UI handles itself, even while a page has
/// keyboard focus. `key` is lowercase: a character, or `tab`/`up`/`down`/`left`/`right`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shortcut {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    TitleChanged { view: ViewId, title: String },
    Favicon { view: ViewId, page_url: String, png: Vec<u8> },
    FindResult { view: ViewId, count: i32, active: i32, final_update: bool },
    UrlChanged { view: ViewId, url: String },
    LoadingChanged {
        view: ViewId,
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// The page asked for a new window (target=_blank, window.open, ⌘-click).
    OpenInNewTab { opener: ViewId, url: String },
    /// A browser shortcut pressed while the page had focus; the page did not get it.
    Shortcut(Shortcut),
}

/// Events keyed by CEF browser id, filled in by CEF callbacks.
#[derive(Debug)]
enum RawEvent {
    Title(String),
    Favicon(String, Vec<u8>),
    Find(i32, i32, bool),
    Url(String),
    Loading(bool, bool, bool),
    Popup(String),
    Shortcut(Shortcut),
}

type EventQueue = Arc<Mutex<Vec<(i32, RawEvent)>>>;
type Shortcuts = Arc<Vec<Shortcut>>;

struct View {
    browser: Browser,
    parent: *mut c_void,
    /// What gets positioned and hidden: a rounded clip view holding the
    /// page on macOS, the page's own native view elsewhere.
    host: *mut c_void,
    frame: Bounds,
    /// The native view has not been positioned yet.
    needs_layout: bool,
}

pub struct Engine {
    client: Client,
    events: EventQueue,
    views: HashMap<ViewId, View>,
    by_browser_id: HashMap<i32, ViewId>,
    next_id: u64,
    #[cfg(target_os = "macos")]
    _library: library_loader::LibraryLoader,
}


impl Engine {
    /// Starts Chromium. Fails when the CEF framework is not next to the
    /// executable, e.g. when running the bare binary outside the app bundle.
    /// `shortcuts` are intercepted before pages see them and reported as
    /// [`Event::Shortcut`]. Chromium keeps its data (cookies, cache) in
    /// `data_dir`.
    pub fn start(data_dir: &Path, shortcuts: Vec<Shortcut>) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;

        #[cfg(target_os = "macos")]
        let library = {
            let framework = exe
                .parent()
                .map(|dir| dir.join("../Frameworks/Chromium Embedded Framework.framework"))
                .filter(|path| path.exists());
            if framework.is_none() {
                return Err("Chromium Embedded Framework is missing; run the app bundle".into());
            }
            let loader = library_loader::LibraryLoader::new(&exe, false);
            if !loader.load() {
                return Err("cannot load Chromium Embedded Framework".into());
            }
            loader
        };

        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

        #[cfg(target_os = "macos")]
        mac::install_app_protocols();

        let root_cache = data_dir;
        let settings = Settings {
            no_sandbox: 0,
            external_message_pump: 1,
            multi_threaded_message_loop: 0,
            root_cache_path: CefString::from(root_cache.to_string_lossy().as_ref()),
            cache_path: CefString::from(root_cache.join("Default").to_string_lossy().as_ref()),
            persist_session_cookies: 1,
            log_severity: LogSeverity::WARNING,
            ..Default::default()
        };
        let args = args::Args::new();
        let mut app = EngineApp::new();
        if initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        ) != 1
        {
            return Err("CEF failed to initialize".into());
        }

        let events = EventQueue::default();
        Ok(Self {
            client: EngineClient::new(events.clone(), Arc::new(shortcuts)),
            events,
            views: HashMap::new(),
            by_browser_id: HashMap::new(),
            next_id: 1,
            #[cfg(target_os = "macos")]
            _library: library,
        })
    }

    /// Runs pending Chromium work. Call often (every few milliseconds).
    /// Chromium may process native UI events inside this call, so the
    /// caller must not hold state that those events' handlers need.
    pub fn pump() {
        // Chromium's work also takes native events off the queue and
        // dispatches them itself. AppKit's mouse tracking (window buttons,
        // resizing) then misses the button going up, so events are left to
        // the toolkit's loop: Chromium runs once the queue is empty, or
        // after a few skips so an event nobody takes cannot stall pages.
        #[cfg(target_os = "macos")]
        {
            const MAX_SKIPS: u32 = 6;
            thread_local!(static SKIPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
            if mac::is_tracking_events() {
                return;
            }
            if mac::has_pending_events() && SKIPS.get() < MAX_SKIPS {
                SKIPS.set(SKIPS.get() + 1);
                return;
            }
            SKIPS.set(0);
        }
        do_message_loop_work();
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        let raw = std::mem::take(&mut *self.events.lock().unwrap());
        raw.into_iter()
            .filter_map(|(browser_id, event)| {
                if let RawEvent::Shortcut(shortcut) = event {
                    return Some(Event::Shortcut(shortcut));
                }
                let view = *self.by_browser_id.get(&browser_id)?;
                Some(match event {
                    RawEvent::Shortcut(_) => unreachable!(),
                    RawEvent::Title(title) => Event::TitleChanged { view, title },
                    RawEvent::Favicon(page_url, png) => Event::Favicon { view, page_url, png },
                    RawEvent::Find(count, active, final_update) => Event::FindResult { view, count, active, final_update },
                    RawEvent::Url(url) => Event::UrlChanged { view, url },
                    RawEvent::Loading(loading, can_go_back, can_go_forward) => {
                        Event::LoadingChanged {
                            view,
                            loading,
                            can_go_back,
                            can_go_forward,
                        }
                    }
                    RawEvent::Popup(url) => Event::OpenInNewTab { opener: view, url },
                })
            })
            .collect()
    }

    /// Creates a page over `ui_view` (the toolkit's `NSView*` on macOS),
    /// with `frame` in that view's coordinates, clipped to `corner_radius`.
    pub fn create_view(
        &mut self,
        ui_view: *mut c_void,
        url: &str,
        frame: Bounds,
        corner_radius: f64,
    ) -> Option<ViewId> {
        #[cfg(target_os = "macos")]
        let parent = mac::content_view_of(ui_view);
        #[cfg(not(target_os = "macos"))]
        let parent = ui_view;
        #[cfg(target_os = "macos")]
        let (page_parent, bounds) = (
            mac::create_clip_view(parent, corner_radius),
            Rect {
                x: 0,
                y: 0,
                width: frame.width as i32,
                height: frame.height as i32,
            },
        );
        #[cfg(not(target_os = "macos"))]
        let (page_parent, bounds) = (
            parent,
            Rect {
                x: frame.x as i32,
                y: frame.y as i32,
                width: frame.width as i32,
                height: frame.height as i32,
            },
        );
        let window_info = WindowInfo {
            runtime_style: RuntimeStyle::ALLOY,
            ..Default::default()
        }
        .set_as_child(page_parent as _, &bounds);
        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut self.client.clone()),
            Some(&CefString::from(url)),
            Some(&BrowserSettings::default()),
            None,
            None,
        );
        let Some(browser) = browser else {
            #[cfg(target_os = "macos")]
            mac::remove_from_superview(page_parent);
            return None;
        };
        let page_view = browser
            .host()
            .map_or(std::ptr::null_mut(), |host| host.window_handle() as *mut c_void);
        #[cfg(target_os = "macos")]
        let host = {
            mac::fill_superview(page_view);
            page_parent
        };
        #[cfg(not(target_os = "macos"))]
        let host = page_view;

        let id = ViewId(self.next_id);
        self.next_id += 1;
        self.by_browser_id.insert(browser.identifier(), id);
        self.views.insert(
            id,
            View {
                browser,
                parent,
                host,
                frame,
                needs_layout: true,
            },
        );
        self.set_frame(id, frame);
        Some(id)
    }

    pub fn set_frame(&mut self, id: ViewId, frame: Bounds) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if view.frame == frame && !view.needs_layout {
            return;
        }
        view.frame = frame;
        view.needs_layout = false;
        #[cfg(target_os = "macos")]
        mac::set_frame(view.host, view.parent, frame);
        if let Some(host) = view.browser.host() {
            host.was_resized();
        }
    }

    pub fn frame(&self, id: ViewId) -> Option<Bounds> {
        self.views.get(&id).map(|view| view.frame)
    }

    pub fn set_visible(&mut self, id: ViewId, visible: bool) {
        #[cfg(target_os = "macos")]
        if let Some(view) = self.views.get(&id) {
            mac::set_hidden(view.host, !visible);
        }
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.was_hidden((!visible).into());
        }
    }

    /// Gives keyboard focus to the page.
    pub fn focus(&mut self, id: ViewId) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.set_focus(1);
        }
    }

    /// Takes keyboard focus away from all pages, back to `ui_view`.
    pub fn focus_parent(&mut self, ui_view: *mut c_void) {
        #[cfg(target_os = "macos")]
        mac::make_first_responder(ui_view);
    }

    pub fn navigate(&mut self, id: ViewId, url: &str) {
        if let Some(frame) = self.views.get(&id).and_then(|view| view.browser.main_frame()) {
            frame.load_url(Some(&CefString::from(url)));
        }
    }

    pub fn go_back(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) {
            view.browser.go_back();
        }
    }

    pub fn go_forward(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) {
            view.browser.go_forward();
        }
    }

    pub fn reload(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) {
            view.browser.reload();
        }
    }

    /// Reloads from the network, bypassing the HTTP cache.
    pub fn force_reload(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) { view.browser.reload_ignore_cache(); }
    }

    pub fn stop_loading(&mut self, id: ViewId) {
        if let Some(view) = self.views.get(&id) { view.browser.stop_load(); }
    }

    pub fn set_audio_muted(&mut self, id: ViewId, muted: bool) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) { host.set_audio_muted(muted.into()); }
    }

    pub fn audio_muted(&self, id: ViewId) -> Option<bool> {
        self.views.get(&id).and_then(|view| view.browser.host()).map(|host| host.is_audio_muted() != 0)
    }

    /// Steps through Chromium's standard zoom factors. Zero resets to 100%.
    pub fn zoom(&mut self, id: ViewId, direction: i32) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            let factor = next_zoom_factor(1.2_f64.powf(host.zoom_level()), direction);
            host.set_zoom_level(factor.ln() / 1.2_f64.ln());
        }
    }

    pub fn find(&mut self, id: ViewId, text: &str, forward: bool, next: bool) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.find(Some(&CefString::from(text)), forward.into(), 0, next.into());
        }
    }

    pub fn stop_finding(&mut self, id: ViewId) {
        if let Some(host) = self.views.get(&id).and_then(|view| view.browser.host()) {
            host.stop_finding(1);
        }
    }

    pub fn close(&mut self, id: ViewId) {
        if let Some(view) = self.views.remove(&id) {
            self.by_browser_id.remove(&view.browser.identifier());
            if let Some(host) = view.browser.host() {
                host.close_browser(1);
            }
            #[cfg(target_os = "macos")]
            mac::remove_from_superview(view.host);
        }
    }
}

fn push(events: &EventQueue, browser: Option<&mut Browser>, event: RawEvent) {
    if let Some(browser) = browser {
        events.lock().unwrap().push((browser.identifier(), event));
    }
}

wrap_client! {
    struct EngineClient {
        events: EventQueue,
        shortcuts: Shortcuts,
    }

    impl Client {
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(EngineKeyboardHandler::new(self.events.clone(), self.shortcuts.clone()))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(EngineDisplayHandler::new(self.events.clone()))
        }

        fn find_handler(&self) -> Option<FindHandler> {
            Some(EngineFindHandler::new(self.events.clone()))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(EngineLoadHandler::new(self.events.clone()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(EngineLifeSpanHandler::new(self.events.clone()))
        }
    }
}

wrap_find_handler! {
    struct EngineFindHandler { events: EventQueue }

    impl FindHandler {
        fn on_find_result(
            &self,
            browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            final_update: ::std::os::raw::c_int,
        ) {
            push(&self.events, browser, RawEvent::Find(count, active_match_ordinal, final_update != 0));
        }
    }
}

wrap_display_handler! {
    struct EngineDisplayHandler {
        events: EventQueue,
    }

    impl DisplayHandler {
        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let (Some(browser), Some(urls)) = (browser, icon_urls) else { return; };
            // clone() produces an immutable borrowed wrapper, whose consuming
            // iterator is empty. Move the callback's mutable wrapper instead.
            let Some(url) = std::mem::take(urls).into_iter().next() else { return; };
            let Some(host) = browser.host() else { return; };
            let Some(frame) = browser.main_frame() else { return; };
            let mut callback = FaviconDownload::new(self.events.clone(), browser.identifier(), CefString::from(&frame.url()).to_string());
            host.download_image(Some(&CefString::from(url.as_str())), 1, 64, 0, Some(&mut callback));
        }

        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            let title = title.map(CefString::to_string).unwrap_or_default();
            push(&self.events, browser, RawEvent::Title(title));
        }

        fn on_address_change(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut cef::Frame>,
            url: Option<&CefString>,
        ) {
            if frame.is_some_and(|frame| frame.is_main() == 1) {
                let url = url.map(CefString::to_string).unwrap_or_default();
                push(&self.events, browser, RawEvent::Url(url));
            }
        }
    }
}

wrap_download_image_callback! {
    struct FaviconDownload {
        events: EventQueue,
        browser_id: i32,
        page_url: String,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(&self, _image_url: Option<&CefString>, _http_status_code: i32, image: Option<&mut Image>) {
            let Some(image) = image.filter(|image| image.is_empty() == 0) else { return; };
            let (mut width, mut height) = (0, 0);
            let Some(data) = image.as_png(1.0, 1, Some(&mut width), Some(&mut height)) else { return; };
            if width <= 0 || height <= 0 || width > 128 || height > 128 || data.size() > 64 * 1024 { return; }
            let mut png = vec![0; data.size()];
            if data.data(Some(&mut png), 0) != png.len() { return; }
            self.events.lock().unwrap().push((self.browser_id, RawEvent::Favicon(self.page_url.clone(), png)));
        }
    }
}

wrap_load_handler! {
    struct EngineLoadHandler {
        events: EventQueue,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let event = RawEvent::Loading(is_loading != 0, can_go_back != 0, can_go_forward != 0);
            push(&self.events, browser, event);
        }
    }
}

wrap_life_span_handler! {
    struct EngineLifeSpanHandler {
        events: EventQueue,
    }

    impl LifeSpanHandler {
        /// Popups become tabs instead of separate native windows.
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            if !url.is_empty() {
                push(&self.events, browser, RawEvent::Popup(url));
            }
            1
        }

        /// Pages live inside the browser window, which must stay open when
        /// one closes: by default CEF closes the page's top-level window.
        fn do_close(&self, _browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            1
        }
    }
}

/// Windows virtual key codes CEF reports for arrow keys.
const VKEY_TAB: i32 = 0x09;
const VKEY_LEFT: i32 = 0x25;
const VKEY_UP: i32 = 0x26;
const VKEY_RIGHT: i32 = 0x27;
const VKEY_DOWN: i32 = 0x28;

fn shortcut_for(event: &KeyEvent) -> Shortcut {
    let flags = event.modifiers;
    let has = |flag: sys::cef_event_flags_t| flags & flag.0 != 0;
    let key = match event.windows_key_code {
        VKEY_TAB => "tab".to_string(),
        VKEY_LEFT => "left".to_string(),
        VKEY_UP => "up".to_string(),
        VKEY_RIGHT => "right".to_string(),
        VKEY_DOWN => "down".to_string(),
        _ => char::from_u32(u32::from(event.unmodified_character))
            .map(|c| c.to_lowercase().to_string())
            .unwrap_or_default(),
    };
    Shortcut {
        cmd: has(sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN),
        ctrl: has(sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN),
        alt: has(sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN),
        shift: has(sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN),
        key,
    }
}

wrap_keyboard_handler! {
    struct EngineKeyboardHandler {
        events: EventQueue,
        shortcuts: Shortcuts,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(event) = event else {
                return 0;
            };
            if event.type_ != KeyEventType::RAWKEYDOWN {
                return 0;
            }
            let shortcut = shortcut_for(event);
            if !self.shortcuts.contains(&shortcut) {
                return 0;
            }
            push(&self.events, browser, RawEvent::Shortcut(shortcut));
            1
        }
    }
}

wrap_app! {
    struct EngineApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // Development builds are unsigned, so macOS asks for the keychain
            // password after every rebuild. Release builds use the real keychain.
            if cfg!(debug_assertions) {
                if let Some(command_line) = command_line {
                    command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
                }
            }
        }
    }
}


/// Chromium expresses zoom logarithmically, while its UI uses these factors.
fn next_zoom_factor(current: f64, direction: i32) -> f64 {
    const FACTORS: &[f64] = &[0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];
    if direction > 0 {
        FACTORS.iter().copied().find(|factor| *factor > current + 0.001).unwrap_or(5.0)
    } else if direction < 0 {
        FACTORS.iter().rev().copied().find(|factor| *factor < current - 0.001).unwrap_or(0.25)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_handle_rounding_custom_levels_limits_and_reset() {
        assert_eq!(next_zoom_factor(1.0, 1), 1.1);
        assert_eq!(next_zoom_factor(1.1 - 1e-12, 1), 1.25);
        assert_eq!(next_zoom_factor(1.0, -1), 0.9);
        assert_eq!(next_zoom_factor(1.13, -1), 1.1);
        assert_eq!(next_zoom_factor(1.13, 1), 1.25);
        assert_eq!(next_zoom_factor(5.0, 1), 5.0);
        assert_eq!(next_zoom_factor(0.25, -1), 0.25);
        assert_eq!(next_zoom_factor(2.5, 0), 1.0);
    }
}
