#[cfg(target_os = "macos")]
mod automation;
mod native;
mod quit;
mod find;
mod toast;
mod archive;
mod command_suggestions;
mod menus;
mod history;
mod tab_menu;
mod site_controls;
mod panel;
mod material;
mod favicon_style;
mod window_snapshots;
mod new_menu;
mod new_window;
mod blank_window;
mod audio;
mod split_layout;
mod sidebar_title;
mod split;
mod icons;
mod folders;
mod space_menu;
mod space_drag;
mod tab_drag;
mod folder_drag;
mod inline_rename;
mod link_status;
mod status_url;
mod private;
mod profile;
mod sidebar_pages;
mod sidebar_resize_state;
mod space_creation;
mod space_creation_view;
mod sidebar_navigation;
mod library;
mod downloads_view;
mod downloads;
mod downloads_publication;
mod download_acceptance;
mod library_downloads_host;
mod library_downloads;
mod downloads_presentation;
mod user_profile_dialog;
mod user_profiles;
pub mod pages;
pub(crate) use pages::devtools;
pub mod text_input;
pub mod window;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod new_window_tests;
#[cfg(test)]
mod empty_favorites_tests;
#[cfg(test)]
mod window_snapshot_tests;
#[cfg(test)]
mod session_startup_tests;

use std::path::PathBuf;
use std::time::SystemTime;

use browser_core::Browser;
use gpui::{
    actions, point, prelude::*, px, size, App, Application, Bounds, KeyBinding,
    TitlebarOptions, WindowBounds, WindowOptions,
};

use pages::{Chromium, WebEngine};
use window::BrowserWindow;

actions!(app, [Quit]);

pub fn init(cx: &mut App) {
    quit::init(cx);
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    text_input::bind_keys(cx);
    window::bind_keys(cx);
    find::bind_keys(cx);
    menus::init(cx);
    history::bind_keys(cx);
    tab_menu::bind_keys(cx);
    site_controls::bind_keys(cx);
    new_menu::bind_keys(cx);
    user_profile_dialog::bind_keys(cx);
    space_creation_view::bind_keys(cx);
    downloads_view::bind_keys(cx);
}

/// Content of the first launch.
pub fn demo_browser() -> Browser {
    let now = SystemTime::now();
    let mut browser = Browser::new();
    browser.add_space("Work", 210.0);
    browser.add_space("Research", 150.0);
    for (title, url) in [("GitHub", "https://github.com"), ("Gmail", "https://mail.google.com")] {
        let id = browser.new_tab(url, now);
        browser.set_title(id, title);
        browser.toggle_pin(id);
    }
    for (title, url) in [
        ("Arc is shutting down", "https://thebrowser.company"),
        ("GPUI", "https://www.gpui.rs"),
        ("Chromium", "https://www.chromium.org"),
    ] {
        let id = browser.new_tab(url, now);
        browser.set_title(id, title);
    }
    browser.clear_sidebar_history();
    browser
}

/// Where the profile lives: the saved session and Chromium's data.
/// `BROWSER_DATA_DIR` overrides it, e.g. so automation runs alongside a
/// running instance (Chromium allows one process per profile).
fn profile_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BROWSER_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map_or_else(std::env::temp_dir, PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = home.join("Library/Application Support");
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map_or(home, PathBuf::from);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(|| home.join(".local/share"), PathBuf::from);
    let dir = base.join(APP_NAME);
    let legacy = base.join(LEGACY_APP_NAME);
    // Keep the existing session and site data after the rename.
    if !dir.exists() && legacy.is_dir() {
        let _ = std::fs::rename(&legacy, &dir);
    }
    dir
}

/// Only a genuinely absent session is a first launch. Existing invalid or
/// unreadable state is preserved byte-for-byte and must never select Default.
fn load_session(path: &std::path::Path) -> std::io::Result<Browser> {
    browser_core::session::load(path).map(|browser| browser.unwrap_or_else(demo_browser))
}

struct SessionRestoreError { message: String }

impl gpui::Render for SessionRestoreError {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div().size_full().p_8().flex().flex_col().gap_4()
            .bg(gpui::rgb(0x202126)).text_color(gpui::rgb(0xf2f2f2))
            .child(gpui::div().text_xl().child("Cannot open the saved workspace"))
            .child(self.message.clone())
            .child("The saved file has been preserved. Restore a valid session file, then retry.")
            .child(gpui::div().id("retry-session").debug_selector(|| "retry-session".into())
                .px_4().py_2().rounded_md().bg(gpui::rgb(0x454854)).cursor_pointer().child("Retry")
                .on_click(cx.listener(|_, _, window, cx| {
                    if quit::is_quitting() { return; }
                    window.remove_window();
                    cx.defer(|cx| new_window::open(false, cx));
                })))
    }
}

fn show_session_error(error: std::io::Error, cx: &mut App) {
    let message = format!("{}: {error}", profile_dir().join("session.json").display());
    eprintln!("Cannot restore workspace: {message}");
    let bounds = Bounds::centered(None, size(px(620.), px(300.)), cx);
    let _ = cx.open_window(WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default()
    }, |_, cx| cx.new(|_| SessionRestoreError { message }));
    cx.activate(true);
}

/// Starts Chromium inside `window`, or returns `None` (pages are then
/// shown as placeholders), e.g. when not running from the app bundle.
fn start_engine(window: &gpui::Window, data_dir: &std::path::Path, private: bool) -> Option<Box<dyn WebEngine>> {
    if quit::is_quitting() { return None; }
    let parent = native::view_of(window)?;
    let engine = if private { engine::Engine::start_private(data_dir, window::page_shortcuts()) }
        else { engine::Engine::start(data_dir, window::page_shortcuts()) };
    match engine {
        Ok(engine) => Some(Box::new(Chromium { engine, parent })),
        Err(error) => {
            eprintln!("Web engine unavailable: {error}");
            None
        }
    }
}

const APP_NAME: &str = "Echo";
/// The profile folder used before the app was named.
const LEGACY_APP_NAME: &str = "Browser";

/// Opens the main browser window, retaining the normal profile across reopen.
fn open_browser_window(cx: &mut App) {
    new_window::open(false, cx);
}

fn open_incognito_window(cx: &mut App) {
    if quit::is_quitting() { return; }
    let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Echo — Incognito".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(window::TRAFFIC_LIGHT_X), px(window::TRAFFIC_LIGHT_Y))),
            }),
            ..Default::default()
        },
        |window, cx| {
            let profile = profile_dir();
            let engine = start_engine(window, &profile.join("Chromium"), true);
            let view = cx.new(|cx| {
                BrowserWindow::new(Browser::new_private(), engine, window, cx)
            });
            view.read(cx).focus(window);
            view
        },
    )
    .unwrap();
    cx.activate(true);
    // AppKit finishes returning focus from the old Command Bar after
    // its key event unwinds. Present the input on the next event-loop turn.
    cx.spawn(async move |cx| {
        let _ = handle.update(cx, |view, window, cx| {
            window.activate_window();
            view.open_private_command_bar(window, cx);
        });
    }).detach();
}

/// Handles platform reopen requests, including clicks on the macOS Dock icon.
fn reopen(cx: &mut App) {
    reopen_with(cx, open_browser_window);
}

fn reopen_with(cx: &mut App, open: impl FnOnce(&mut App)) {
    if quit::is_quitting() { return; }
    if let Some(handle) = cx.windows().into_iter().find(|handle| handle.downcast::<BrowserWindow>().is_some()) {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        cx.activate(true);
    } else {
        open(cx);
    }
}

pub fn run() {
    let app = Application::new();
    app.on_reopen(reopen);
    app.run(|cx: &mut App| {
        init(cx);
        open_browser_window(cx);
        #[cfg(target_os = "macos")]
        if let Ok(script) = std::env::var("BROWSER_SCRIPT") {
            if let Some(handle) = cx.windows().into_iter().find_map(|handle| handle.downcast::<BrowserWindow>()) {
                let _ = handle.update(cx, |_, window, cx| {
                    if let Some(ns_view) = native::view_of(window) {
                        automation::start(script, cx.weak_entity(), ns_view, cx);
                    }
                });
            }
        }
    });
}
