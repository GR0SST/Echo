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
}

impl BrowserWindow {
    pub(crate) fn retain_normal_source(&self, cx: &mut App) {
        if !self.browser.is_private() && !self.browser.is_blank() {
            cx.set_global(NormalProfile { profile: self.profile.handle.clone(), pages: self.pages.clone() });
        }
    }

    pub(crate) fn new_window(&mut self, cx: &mut Context<Self>) {
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
        };
        cx.defer(move |cx| open_with(request, true, cx));
    }
}

/// `command_bar` distinguishes New Window from initial launch/session restore.
pub(crate) fn open(command_bar: bool, cx: &mut App) {
    let normal = normal_source(cx);
    let selection = if command_bar {
        normal.profile.new_window_selection()
    } else {
        normal.profile.selection()
    };
    open_with(NewWindow { normal, selection }, command_bar, cx);
}

fn open_with(request: NewWindow, command_bar: bool, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
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
    cx.activate(true);
    if command_bar {
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
pub(crate) fn normal_source(cx: &mut App) -> NormalProfile {
    if let Some(normal) = cx.try_global::<NormalProfile>() {
        normal.clone()
    } else {
        let session = crate::profile_dir().join("session.json");
        NormalProfile {
            profile: ProfileHandle::new(crate::load_session(&session), cx),
            pages: None,
        }
    }
}
