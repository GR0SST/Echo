//! Owner-scoped lifecycle for Profile sheets in transparent child windows.
use crate::{
    user_profile_dialog::{CreationSource, DialogEvent, UserProfileDialog},
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use browser_core::{Browser, SpaceId, UserProfileId};
use gpui::{div, prelude::*, AnyElement, App, Context, Entity, Subscription, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions};

#[derive(Clone, PartialEq)]
struct Inventory {
    spaces: Vec<(SpaceId, String, UserProfileId)>,
    profiles: Vec<(UserProfileId, String)>,
}
impl Inventory {
    fn capture(browser: &Browser) -> Self {
        Self {
            spaces: browser.spaces.iter().map(|space| (space.id, space.name.clone(), space.user_profile_id)).collect(),
            profiles: browser.user_profiles().map(|profile| (profile.id(), profile.name().to_owned())).collect(),
        }
    }
}

pub(crate) struct ProfileDialogHost {
    pub(crate) dialog: Entity<UserProfileDialog>,
    pub(crate) overlay: Option<WindowHandle<Overlay>>,
    inventory: Inventory,
    _events: Subscription,
}

#[derive(Clone, Copy)]
enum Route { Create(CreationSource), Switch(SpaceId) }

impl BrowserWindow {
    pub(crate) fn open_user_profile_creation(&mut self, source: CreationSource, cx: &mut Context<Self>) {
        self.open_user_profile_sheet(Route::Create(source), cx);
    }

    pub(crate) fn open_user_profile_switch(&mut self, space: SpaceId, cx: &mut Context<Self>) {
        self.open_user_profile_sheet(Route::Switch(space), cx);
    }

    fn open_user_profile_sheet(&mut self, route: Route, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() { return; }
        self.refresh_profile_projection();
        if self.browser.is_private() || self.browser.is_blank() { return; }
        let source = match route { Route::Create(CreationSource::File(space)) | Route::Switch(space) => Some(space), Route::Create(CreationSource::Settings) => None };
        if source.is_some_and(|id| !self.browser.spaces.iter().any(|space| space.id == id)) { return; }
        self.finish_tab_rename(true, cx);
        // The replacement slot is filled before any deferred peer restoration.
        self.close_user_profile_dialog_with_focus(PopupFocus::OnActivation, cx);
        self.close_command_bar(cx);
        self.close_find(cx);
        self.close_tab_menu(cx);
        self.close_site_controls(cx);
        self.close_new_menu(cx);
        self.close_link_status(cx);
        self.close_floating_sidebar(cx);
        if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
        let dialog = cx.new(|cx| match route {
            Route::Create(source) => UserProfileDialog::creation(&self.browser, source, cx),
            Route::Switch(space) => UserProfileDialog::switching(&self.browser, space, cx),
        }.expect("validated normal profile sheet source"));
        let events = cx.subscribe(&dialog, |_, sender, event: &DialogEvent, cx| {
            let sender = sender.clone();
            let event = event.clone();
            let owner = cx.entity().downgrade();
            // Form events never mutate the form reentrantly. Identity remains
            // attached across queued submit, replacement and Closed events.
            cx.defer(move |cx| { let _ = owner.update(cx, |this, cx| this.receive_user_profile_event(&sender, event, cx)); });
        });
        self.user_profile_dialog = Some(ProfileDialogHost { dialog: dialog.clone(), overlay: None, inventory: Inventory::capture(&self.browser), _events: events });
        let owner = cx.entity().downgrade();
        let main = self.main_window;
        let parent = self.native_view;
        cx.notify();
        cx.defer(move |cx| {
            if crate::quit::is_quitting() { return; }
            let wanted = owner.update(cx, |this, cx| {
                this.validate_user_profile_dialog(cx);
                this.user_profile_dialog.as_ref().is_some_and(|host| host.dialog == dialog)
            }).unwrap_or(false);
            if !wanted { return; }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            };
            let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) { crate::native::attach_overlay(parent, child, false); }
                cx.new(|cx| Overlay::new(owner.clone(), OverlayKind::UserProfileDialog, cx))
            }) else {
                let _ = owner.update(cx, |this, cx| { if this.user_profile_dialog.as_ref().is_some_and(|host| host.dialog == dialog) { this.close_user_profile_dialog(cx); } });
                return;
            };
            let attached = owner.update(cx, |this, cx| {
                this.validate_user_profile_dialog(cx);
                if let Some(host) = this.user_profile_dialog.as_mut().filter(|host| host.dialog == dialog) {
                    host.overlay = Some(overlay); true
                } else { false }
            }).unwrap_or(false);
            if !attached { let _ = overlay.update(cx, |_, window, _| window.remove_window()); return; }
            let _ = overlay.update(cx, |_, window, cx| {
                if owner.read_with(cx, |this, _| this.user_profile_dialog.as_ref().is_some_and(|host| host.dialog == dialog && host.overlay == Some(overlay))).unwrap_or(false) {
                    window.activate_window();
                    if owner.read_with(cx, |this, _| this.user_profile_dialog.as_ref().is_some_and(|host| host.dialog == dialog && host.overlay == Some(overlay))).unwrap_or(false) {
                        dialog.read(cx).focus(window, cx);
                    }
                }
            });
        });
    }

    fn receive_user_profile_event(&mut self, sender: &Entity<UserProfileDialog>, event: DialogEvent, cx: &mut Context<Self>) {
        if crate::quit::is_quitting() { return; }
        self.refresh_profile_projection();
        self.validate_user_profile_dialog(cx);
        if !self.user_profile_dialog.as_ref().is_some_and(|host| host.dialog == *sender) { return; }
        match event {
            DialogEvent::Closed => self.close_user_profile_dialog(cx),
            DialogEvent::NewProfile(source) => self.open_user_profile_creation(source, cx),
            DialogEvent::Submit(request) => match self.apply_user_profile_dialog(&request) {
                Ok(applied) => {
                    // publish_profile_edit already reconciles the entire shared
                    // PageStore synchronously, including pages on other hosts.
                    // Keep this explicit at the sheet/adapter boundary as well.
                    if !applied.transitions.is_empty() {
                        if let Some(pages) = &self.pages { pages.borrow_mut().reconcile_inventory(&self.browser); }
                    }
                    self.close_user_profile_dialog(cx);
                }
                Err(_) => sender.update(cx, |dialog, cx| dialog.submission_failed("The workspace changed. Close this sheet and try again.".into(), cx)),
            },
        }
    }

    pub(crate) fn validate_user_profile_dialog(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        if self.user_profile_dialog.as_ref().is_some_and(|host| host.inventory != Inventory::capture(&self.browser)) {
            self.close_user_profile_dialog(cx);
        }
    }

    pub(crate) fn close_user_profile_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_user_profile_dialog_with_focus(PopupFocus::Restore, cx);
    }

    pub(crate) fn close_user_profile_dialog_with_focus(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        let Some(host) = self.user_profile_dialog.take() else { return; };
        host.dialog.update(cx, |dialog, cx| dialog.close(cx));
        self.close_popup_overlay(host.overlay, focus, true, cx);
        cx.notify();
    }

    /// Used by owner release, where Context<BrowserWindow> is no longer present.
    pub(crate) fn take_user_profile_child(&mut self, cx: &mut App) -> Option<WindowHandle<Overlay>> {
        let host = self.user_profile_dialog.take()?;
        host.dialog.update(cx, |dialog, cx| dialog.close(cx));
        host.overlay
    }

    pub(crate) fn render_user_profile_dialog(&self) -> AnyElement {
        self.user_profile_dialog.as_ref().map_or_else(|| div().into_any_element(), |host| host.dialog.clone().into_any_element())
    }
}
