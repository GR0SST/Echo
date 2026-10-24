//! Shared entry point for links delivered by the operating system.
use crate::{
    new_window::{self, NormalProfile},
    window::BrowserWindow,
};
use browser_core::{Browser, SpaceId, routing::LinkDestination};
use gpui::{App, prelude::*};

pub(crate) fn destination_space(
    browser: &Browser,
    destination: LinkDestination,
) -> Option<SpaceId> {
    match destination {
        LinkDestination::Space(id) => browser
            .spaces
            .iter()
            .find(|space| space.id == id)
            .map(|space| space.id),
        LinkDestination::Profile(id) => {
            if browser.space().user_profile_id == id {
                Some(browser.space().id)
            } else {
                browser
                    .spaces
                    .iter()
                    .filter(|space| space.user_profile_id == id)
                    .max_by_key(|space| {
                        space
                            .pinned
                            .iter()
                            .chain(&space.today)
                            .map(|tab| tab.last_active)
                            .max()
                    })
                    .map(|space| space.id)
            }
        }
        LinkDestination::MostRecentSpace => Some(browser.space().id),
        LinkDestination::LittleWindow => None,
    }
}

pub(crate) fn open(url: String, application: Option<&str>, cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    let source = match new_window::normal_source(cx) {
        Ok(source) => source,
        Err(error) => {
            crate::show_session_error(error, cx);
            return;
        }
    };
    let mut browser = source.profile.projection(&source.profile.selection());
    if let Some(active) = cx
        .active_window()
        .and_then(|handle| handle.downcast::<BrowserWindow>())
    {
        if let Ok(view) = active.read(cx) {
            if view.transient.is_none()
                && !view.browser.is_blank()
                && !view.browser.is_private()
                && view.profile.handle.same_workspace(&source.profile)
            {
                browser = view.browser.clone();
            }
        }
    }
    let Some(destination) = browser
        .settings
        .routing()
        .resolve(&browser, &url, application)
    else {
        return;
    };
    if destination == LinkDestination::LittleWindow {
        let space = browser.space();
        crate::transient::open_with(source, space.id, space.user_profile_id, Some(url), cx);
        return;
    }
    let Some(space) = destination_space(&browser, destination) else {
        return;
    };
    open_in_space(source, space, url, cx);
}

fn open_in_space(source: NormalProfile, space: SpaceId, url: String, cx: &mut App) {
    let find = |cx: &App| {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<BrowserWindow>())
            .find(|handle| {
                handle.read(cx).is_ok_and(|view| {
                    view.transient.is_none()
                        && !view.browser.is_private()
                        && !view.browser.is_blank()
                        && view.profile.handle.same_workspace(&source.profile)
                })
            })
    };
    let target = find(cx).or_else(|| {
        new_window::open(false, cx);
        find(cx)
    });
    if let Some(target) = target {
        let _ = target.update(cx, |view, window, cx| {
            view.refresh_profile_projection();
            let Some(index) = view
                .browser
                .spaces
                .iter()
                .position(|candidate| candidate.id == space)
            else {
                return;
            };
            view.switch_space(index, cx);
            view.edit_browser(|browser| browser.new_tab(url, std::time::SystemTime::now()));
            view.settle_pager(cx);
            view.focus(window);
            window.activate_window();
            cx.notify();
        });
        cx.activate(true);
    }
}
