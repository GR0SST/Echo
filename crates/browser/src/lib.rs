#[cfg(target_os = "macos")]
mod automation;
mod native;
mod find;
mod toast;
mod archive;
mod menus;
mod history;
mod tab_menu;
mod audio;
mod split_layout;
mod split;
mod icons;
mod folders;
pub mod pages;
pub mod text_input;
pub mod window;

#[cfg(test)]
mod tests;

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
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    text_input::bind_keys(cx);
    window::bind_keys(cx);
    find::bind_keys(cx);
    menus::init(cx);
    history::bind_keys(cx);
    tab_menu::bind_keys(cx);
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
    base.join(APP_NAME)
}

/// The saved session, or demo content on first launch. A session that
/// cannot be read is kept aside rather than overwritten.
fn load_session(path: &std::path::Path) -> Browser {
    match browser_core::session::load(path) {
        Ok(Some(browser)) => browser,
        Ok(None) => demo_browser(),
        Err(error) => {
            eprintln!("Cannot restore the session from {}: {error}", path.display());
            let _ = std::fs::rename(path, path.with_extension("json.broken"));
            demo_browser()
        }
    }
}

/// Starts Chromium inside `window`, or returns `None` (pages are then
/// shown as placeholders), e.g. when not running from the app bundle.
fn start_engine(window: &gpui::Window, data_dir: &std::path::Path) -> Option<Box<dyn WebEngine>> {
    let parent = native::view_of(window)?;
    match engine::Engine::start(data_dir, window::page_shortcuts()) {
        Ok(engine) => Some(Box::new(Chromium { engine, parent })),
        Err(error) => {
            eprintln!("Web engine unavailable: {error}");
            None
        }
    }
}

const APP_NAME: &str = "Browser";

pub fn run() {
    Application::new().run(|cx: &mut App| {
        init(cx);

        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Browser".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(window::TRAFFIC_LIGHT_X), px(window::TRAFFIC_LIGHT_Y))),
                }),
                ..Default::default()
            },
            |window, cx| {
                let profile = profile_dir();
                let session = profile.join("session.json");
                let engine = start_engine(window, &profile.join("Chromium"));
                let view = cx.new(|cx| {
                    let mut view = BrowserWindow::new(load_session(&session), engine, window, cx);
                    view.save_session_to(session, cx);
                    view
                });
                #[cfg(target_os = "macos")]
                if let (Ok(script), Some(ns_view)) = (std::env::var("BROWSER_SCRIPT"), native::view_of(window)) {
                    automation::start(script, view.downgrade(), ns_view, cx);
                }
                view.read(cx).focus(window);
                view
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
