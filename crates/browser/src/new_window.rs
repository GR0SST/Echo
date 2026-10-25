//! Normal windows share a retained profile and one live page pool.
use std::{cell::RefCell, rc::Rc};

use browser_core::windows::WindowSelection;
use gpui::{
    point, prelude::*, px, size, App, Bounds, Context, Global, TitlebarOptions, WindowBounds,
    WindowOptions,
};

use crate::{
    native,
    pages::Pages,
    profile::ProfileHandle,
    window::{self, BrowserWindow, Target},
};

/// Retaining this after the last window closes avoids reloading the session
/// or creating a second persistent engine when the Dock opens another window.
#[derive(Clone)]
pub(crate) struct NormalProfile {
    pub(crate) profile: ProfileHandle,
    pub(crate) pages: Option<Rc<RefCell<Pages>>>,
}

impl Global for NormalProfile {}

struct NewWindow {
    normal: NormalProfile,
    selection: WindowSelection,
    restore: Option<browser_core::window_restore::RestoredWindow>,
    dormant: bool,
}

impl BrowserWindow {
    pub(crate) fn retain_normal_source(&self, cx: &mut App) {
        if crate::quit::is_quitting() {
            return;
        }
        if self.transient.is_none() && !self.browser.is_private() && !self.browser.is_blank() {
            cx.set_global(NormalProfile {
                profile: self.profile.handle.clone(),
                pages: self.pages.clone(),
            });
        }
    }

    pub(crate) fn new_window_for_link(&mut self, url: String, cx: &mut Context<Self>) {
        if self.browser.is_private() {
            cx.defer(move |cx| crate::open_incognito_url(url, cx));
            return;
        }
        let profile = self.browser.active_user_profile_id();
        let (normal, space) = if let Some(transient) = &self.transient {
            (transient.source.clone(), transient.space)
        } else if !self.browser.is_blank() {
            (
                NormalProfile {
                    profile: self.profile.handle.clone(),
                    pages: self.pages.clone(),
                },
                self.browser.space().id,
            )
        } else {
            let Ok(normal) = normal_source(cx) else {
                return;
            };
            let space = normal.profile.read_settings(|browser| {
                browser
                    .spaces
                    .iter()
                    .find(|space| space.user_profile_id == profile)
                    .map(|space| space.id)
            });
            let Some(space) = space else {
                return;
            };
            (normal, space)
        };
        let selection = normal.profile.edit_settings(|browser| {
            let index = browser
                .spaces
                .iter()
                .position(|entry| entry.id == space && entry.user_profile_id == profile)
                .or_else(|| {
                    browser
                        .spaces
                        .iter()
                        .position(|entry| entry.user_profile_id == profile)
                })?;
            let mut previous = browser.window_selection();
            browser.switch_space(index);
            let mut selection = browser.new_window_selection();
            browser.edit_in_window(&mut selection, |browser| {
                let id = browser.new_tab(&url, std::time::SystemTime::now());
                browser.set_title(id, crate::window::display_url(&url));
            });
            browser.edit_in_window(&mut previous, |_| {});
            Some(selection)
        });
        let Some(selection) = selection else {
            return;
        };
        cx.defer(move |cx| {
            open_with(
                NewWindow {
                    normal,
                    selection,
                    restore: None,
                    dormant: false,
                },
                false,
                cx,
            )
        });
    }

    pub(crate) fn new_window(&mut self, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() {
            return;
        }
        self.refresh_profile_projection();
        self.close_command_bar(cx);
        if self.browser.is_private() || self.browser.is_blank() {
            // Private models and request contexts must never enter the normal pool.
            cx.defer(|cx| open(true, cx));
            return;
        }
        let request = NewWindow {
            normal: NormalProfile {
                profile: self.profile.handle.clone(),
                pages: self.pages.clone(),
            },
            selection: self.browser.new_window_selection(),
            restore: None,
            dormant: false,
        };
        cx.defer(move |cx| open_with(request, true, cx));
    }
}

/// `command_bar` distinguishes New Window from initial launch/session restore.
pub(crate) fn open(command_bar: bool, cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    let initial = !cx.has_global::<NormalProfile>();
    let normal = match normal_source(cx) {
        Ok(normal) => normal,
        Err(error) => {
            crate::show_session_error(error, cx);
            return;
        }
    };
    if initial && !command_bar {
        let saved = normal.profile.read_settings(|b| {
            if b.settings
                .enabled(browser_core::settings::Toggle::RestoreWindows)
            {
                b.windows.clone()
            } else {
                Vec::new()
            }
        });
        if !saved.is_empty() {
            let last = saved.len() - 1;
            for (index, restore) in saved.into_iter().enumerate() {
                open_with(
                    NewWindow {
                        normal: cx
                            .try_global::<NormalProfile>()
                            .cloned()
                            .unwrap_or_else(|| normal.clone()),
                        selection: restore.selection.clone(),
                        restore: Some(restore),
                        dormant: index != last,
                    },
                    false,
                    cx,
                );
            }
            return;
        }
        normal.profile.edit_settings(|b| b.windows.clear());
    }
    let selection = if command_bar {
        normal.profile.new_window_selection()
    } else {
        normal.profile.selection()
    };
    open_with(
        NewWindow {
            normal,
            selection,
            restore: None,
            dormant: false,
        },
        command_bar,
        cx,
    );
}

fn open_with(request: NewWindow, command_bar: bool, cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    let (bounds, display_id) = request
        .restore
        .as_ref()
        .map(|saved| crate::window_restore::placement(saved, cx))
        .unwrap_or_else(|| {
            (
                WindowBounds::Windowed(Bounds::centered(None, size(px(1200.), px(800.)), cx)),
                None,
            )
        });
    let focused = !request.dormant;
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                display_id,
                focus: focused,
                titlebar: Some(TitlebarOptions {
                    title: Some(crate::APP_NAME.into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(
                        px(window::TRAFFIC_LIGHT_X),
                        px(window::TRAFFIC_LIGHT_Y),
                    )),
                }),
                ..Default::default()
            },
            move |window, cx| {
                let NewWindow {
                    mut normal,
                    selection,
                    restore,
                    dormant,
                } = request;
                let pages = match &normal.pages {
                    Some(pages) => Some(
                        pages
                            .borrow()
                            .for_host(native::view_of(window).unwrap_or(std::ptr::null_mut())),
                    ),
                    None => {
                        crate::start_engine(window, &crate::profile_dir().join("Chromium"), false)
                            .map(Pages::new)
                    }
                };
                let view = cx.new(|cx| {
                    let mut view = BrowserWindow::with_profile_pages(
                        normal.profile.clone(),
                        Some(selection),
                        pages,
                        window,
                        cx,
                    );
                    view.attach_restoration(restore, dormant, window, cx);
                    view.save_session_to(crate::profile_dir().join("session.json"), cx);
                    view
                });
                normal.pages = view.read(cx).pages.clone();
                cx.set_global(normal);
                view.read(cx).focus(window);
                view
            },
        )
        .unwrap();
    if focused {
        cx.activate(true);
    }
    let onboarding = handle
        .read(cx)
        .is_ok_and(|view| view.browser.onboarding.is_some());
    if onboarding {
        cx.defer(move |cx| {
            let _ = handle.update(cx, |view, _, cx| view.open_setup(true, cx));
        });
    } else if command_bar {
        // Wait for the old overlay's key event to finish returning focus.
        cx.spawn(async move |cx| {
            let _ = handle.update(cx, |view, window, cx| {
                window.activate_window();
                view.open_command_bar(Target::NewTab, "", window, cx);
            });
        })
        .detach();
    }
}

/// The retained regular model is also the storage/history source for Blank.
pub(crate) fn normal_source(cx: &mut App) -> std::io::Result<NormalProfile> {
    if crate::quit::is_quitting() {
        return Err(std::io::Error::other("Browser is quitting"));
    }
    if let Some(normal) = cx.try_global::<NormalProfile>() {
        Ok(normal.clone())
    } else {
        let session = crate::profile_dir().join("session.json");
        source_from_session(&session, cx)
    }
}

/// Construct the retained owner only after a successful restore. No engine,
/// save subscription, reset or file mutation occurs on the error path.
pub(crate) fn source_from_session(
    path: &std::path::Path,
    cx: &mut App,
) -> std::io::Result<NormalProfile> {
    if crate::quit::is_quitting() {
        return Err(std::io::Error::other("Browser is quitting"));
    }
    let browser = crate::load_session(path)?;
    crate::app_icons::apply(browser.settings.app_icon);
    crate::shortcuts::apply(&browser.settings.shortcuts, cx);
    let profile = ProfileHandle::new(browser, cx);
    profile.save_session_to(path.to_path_buf(), cx);
    Ok(NormalProfile {
        profile,
        pages: None,
    })
}
