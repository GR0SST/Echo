//! Browser-window lifecycle for the canonical Downloads presenter. This route
//! never creates a transfer, consumes engine events, or chooses a destination.
use crate::{
    downloads_view::{DownloadsColors, DownloadsControlFocused, DownloadsIntent},
    library::LibrarySection,
    library_downloads_host::{DownloadsOwner, LibraryDownloadsHost},
    sidebar_pages::SidebarPage,
    window::BrowserWindow,
};
use browser_core::downloads::CompletedDownloadId;
use gpui::{div, prelude::*, Context, FocusHandle, Focusable, Subscription, Window};
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct DownloadsNavigation {
    pub(crate) host: Option<LibraryDownloadsHost>,
    pub(crate) dirty: bool,
    active: bool,
    subscriptions: Vec<Subscription>,
    presentation_clock: Option<gpui::Task<()>>,
    pub(crate) focus: Option<(DownloadsOwner, FocusHandle)>,
}

impl BrowserWindow {
    pub(crate) fn retire_library_downloads_route(&mut self) {
        let navigation = &mut self.sidebar_navigation.downloads;
        navigation.subscriptions.clear();
        navigation.presentation_clock = None;
        navigation.focus = None;
        navigation.active = false;
        navigation.dirty = true;
        if let Some(host) = &mut navigation.host {
            host.set_active(false);
        }
    }

    pub(crate) fn open_library_downloads(&mut self, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() || self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        self.select_sidebar_page(SidebarPage::Library, cx);
        self.invalidate_sidebar_focus();
        self.sidebar_navigation.library_section = LibrarySection::Downloads;
        self.sync_navigation_visibility();
        self.defer_sidebar_focus(cx);
        cx.notify();
    }

    fn downloads_route_active(&self) -> bool {
        !self.browser.is_private()
            && !self.browser.is_blank()
            && self.sidebar_navigation.controller.selected() == SidebarPage::Library
            && self.sidebar_navigation.library_section == LibrarySection::Downloads
    }
    fn downloads_colors(&self) -> DownloadsColors {
        // Same semantic surfaces as the existing Library Media host. Material
        // adoption stays separate from navigation and canonical ownership.
        let theme = self.theme_colors();
        DownloadsColors {
            column: gpui::transparent_black(),
            preview: gpui::transparent_black(),
            text: theme.foreground,
            muted: theme.muted,
            control: theme.control,
            selected: theme.selected,
            hover: theme.hover,
        }
    }
    pub(crate) fn sync_library_downloads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.downloads_route_active();
        let preview_left = self.library_preview_left();
        let colors = self.downloads_colors();
        if active && self.sidebar_navigation.downloads.host.is_none() {
            let owner = cx.entity().downgrade();
            let preview = Rc::new(move |_: &mut Window, cx: &mut gpui::App| {
                owner
                    .update(cx, |this, cx| {
                        if this.downloads_route_active() {
                            this.render_library_page_preview(cx)
                        } else {
                            div().into_any_element()
                        }
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            });
            self.sidebar_navigation.downloads.host = LibraryDownloadsHost::new(
                self.profile.handle.clone(),
                colors,
                Rc::new(crate::downloads_presentation::completed),
                preview,
                window,
                cx,
            );
            self.sidebar_navigation.downloads.dirty = true;
        }
        if active {
            if let Some(host) = &self.sidebar_navigation.downloads.host {
                host.view().update(cx, |view, cx| view.set_preview_left(preview_left, cx));
            }
        }
        let changed = active != self.sidebar_navigation.downloads.active;
        if changed {
            let navigation = &mut self.sidebar_navigation.downloads;
            navigation.active = active;
            navigation.subscriptions.clear();
            navigation.focus = None;
            if let Some(host) = &mut navigation.host {
                host.set_active(active);
            }
        }
        if (changed || self.sidebar_navigation.downloads.dirty)
            && self.sidebar_navigation.downloads.host.is_some()
        {
            self.sidebar_navigation.downloads.dirty = false;
            if !self
                .sidebar_navigation
                .downloads
                .host
                .as_mut()
                .unwrap()
                .refresh(&self.profile.handle, colors, window, cx)
            {
                self.sidebar_navigation.downloads = Default::default();
                return;
            }
        }
        if !active || !self.sidebar_navigation.downloads.subscriptions.is_empty() {
            return;
        }
        let Some(host) = &self.sidebar_navigation.downloads.host else {
            return;
        };
        let owner = host.owner();
        let view = host.view();
        let intents = cx.subscribe_in(
            &view,
            window,
            move |this, sender, event: &DownloadsIntent<CompletedDownloadId>, window, cx| {
                if !this.downloads_route_active() {
                    return;
                }
                let colors = this.downloads_colors();
                if let Some(host) = &mut this.sidebar_navigation.downloads.host {
                    host.handle_intent(
                        owner,
                        sender,
                        &this.profile.handle,
                        event,
                        colors,
                        window,
                        cx,
                    );
                }
            },
        );
        let focus = cx.subscribe_in(
            &view,
            window,
            move |this, sender, event: &DownloadsControlFocused, window, cx| {
                if !this.downloads_route_active() {
                    return;
                }
                let focus = this
                    .sidebar_navigation
                    .downloads
                    .host
                    .as_ref()
                    .and_then(|host| {
                        host.control_focus(owner, sender, &this.profile.handle, window, cx)
                    });
                if let Some(focus) = focus {
                    match event {
                        DownloadsControlFocused::Requested => {
                            this.library_downloads_controls_took_focus()
                        }
                        DownloadsControlFocused::Observed => {
                            // AppKit can reactivate GPUI's retained search handle
                            // while an overlay is returning to a native document.
                            // Reconciliation may update an existing UI cache,
                            // but cannot create intent or steal native focus.
                            if this.sidebar_navigation.downloads.focus.is_none()
                                || !this.sidebar_root_may_take_focus()
                                || this.pages.as_ref().is_some_and(|pages| {
                                    pages.borrow().capture_resize_focus().has_native_focus()
                                })
                            {
                                return;
                            }
                        }
                    }
                    this.sidebar_navigation.downloads.focus = Some((owner, focus));
                }
            },
        );
        self.sidebar_navigation.downloads.subscriptions = vec![intents, focus];
        // Refresh presentation as relative labels, the local calendar and locale
        // change while the section stays open. This owns no download consumer
        // and never pumps CEF. Route retirement drops the timer immediately.
        self.sidebar_navigation.downloads.presentation_clock =
            Some(cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(60))
                    .await;
                if !this
                    .update(cx, |this, cx| {
                        if !this.downloads_route_active()
                            || this
                                .sidebar_navigation
                                .downloads
                                .host
                                .as_ref()
                                .is_none_or(|host| host.owner() != owner)
                        {
                            return false;
                        }
                        this.sidebar_navigation.downloads.dirty = true;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }));
    }
    pub(crate) fn remember_downloads_control(&mut self, window: &Window, cx: &gpui::App) {
        if !self.downloads_route_active()
            || !self.sidebar_root_may_take_focus()
            || self
                .pages
                .as_ref()
                .is_some_and(|pages| pages.borrow().capture_resize_focus().has_native_focus())
        {
            return;
        }
        if let Some(host) = &self.sidebar_navigation.downloads.host {
            let owner = host.owner();
            if let Some(focus) =
                host.control_focus(owner, &host.view(), &self.profile.handle, window, cx)
            {
                self.sidebar_navigation.downloads.focus = Some((owner, focus));
            }
        }
    }
    pub(crate) fn restore_downloads_control(&self, window: &mut Window, cx: &gpui::App) -> bool {
        if !self.downloads_route_active() {
            return false;
        }
        let Some(host) = &self.sidebar_navigation.downloads.host else {
            return false;
        };
        if let Some((owner, focus)) = &self.sidebar_navigation.downloads.focus {
            if host.restore_control(
                *owner,
                &host.view(),
                &self.profile.handle,
                focus,
                window,
                cx,
            ) {
                return true;
            }
        }
        host.view().read(cx).focus_handle(cx).focus(window);
        true
    }
}
