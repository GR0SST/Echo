//! Transfer intents retain their presenter lease and resolve the original
//! backend token/profile; neither filenames nor current Space identify a job.
use crate::{
    downloads::{AppDownloads, TransferPresentation},
    downloads_view::TransferId,
    library_downloads_host::DownloadsOwner,
    profile::ProfileHandle,
    tab_menu::{self, Command, Entry},
    window::BrowserWindow,
};
use browser_core::UserProfileId;
use gpui::Context;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    Copy,
    Reveal,
    Cancel,
    Hide,
}
impl Action {
    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::Open => "download-open",
            Self::Copy => "download-copy",
            Self::Reveal => "download-reveal",
            Self::Cancel => "download-cancel",
            Self::Hide => "download-hide",
        }
    }
}
impl BrowserWindow {
    pub(crate) fn poll_download_ui(&mut self, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() {
            self.sidebar_navigation.downloads.progress = None;
            return;
        }
        let Some(downloads) = cx.try_global::<AppDownloads>().cloned() else {
            return;
        };
        let revision = downloads.service.borrow().ui_revision;
        let navigation = &mut self.sidebar_navigation.downloads;
        navigation.service = Some(downloads.clone());
        if navigation.ui_revision == revision {
            return;
        }
        navigation.ui_revision = revision;
        // Never let a selected Cancel row change into Hide when the native
        // transfer becomes terminal while its menu is still tracking input.
        let terminal_menu = self
            .tab_menu
            .as_ref()
            .and_then(|menu| match menu.target {
                crate::tab_menu::MenuTarget::Transfer(id, _, _) => Some(
                    downloads
                        .service
                        .borrow()
                        .transfer_for_id(id)
                        .is_none_or(|row| row.info.phase.is_terminal()),
                ),
                _ => None,
            })
            .unwrap_or(false);
        if terminal_menu {
            self.close_tab_menu(cx);
        }
        let rows = crate::ordinary_downloads::transfer_rows(&self.profile.handle, None, "", cx);
        self.sidebar_navigation.downloads.progress = rows
            .iter()
            .find(|row| !row.canceled)
            .map(|row| row.progress);
        self.sidebar_navigation.downloads.dirty = true;
        cx.notify();
    }
    pub(crate) fn transfer_menu_record(
        &self,
        id: TransferId,
        profile: UserProfileId,
        owner: DownloadsOwner,
    ) -> Option<TransferPresentation<engine::DownloadToken, ProfileHandle>> {
        if crate::quit::is_quitting() || !self.downloads_route_active() {
            return None;
        }
        let navigation = &self.sidebar_navigation.downloads;
        if !navigation
            .host
            .as_ref()?
            .transfer_visible(owner, &self.profile.handle, id, profile)
        {
            return None;
        }
        let row = navigation
            .service
            .as_ref()?
            .service
            .borrow()
            .transfer_for_id(id)?;
        (row.profile == profile
            && row.handle.same_workspace(&self.profile.handle)
            && self
                .profile
                .handle
                .read_settings(|b| b.user_profile(profile).is_some()))
        .then_some(row)
    }
    pub(crate) fn transfer_menu_commands(
        &self,
        id: TransferId,
        profile: UserProfileId,
        owner: DownloadsOwner,
    ) -> Vec<Entry> {
        let Some(row) = self.transfer_menu_record(id, profile, owner) else {
            return Vec::new();
        };
        let kind = crate::download_files::kind_for(
            std::path::Path::new(&row.title),
            &row.request.info.mime_type,
        );
        let canceled = row.info.phase == engine::DownloadPhase::Canceled;
        let mut entries = vec![
            tab_menu::item(format!("Open {kind}"), Command::Transfer(Action::Open))
                .enabled(!canceled)
                .into(),
            tab_menu::item(format!("Copy {kind}"), Command::Transfer(Action::Copy))
                .enabled(false)
                .into(),
            tab_menu::item("Show in Finder", Command::Transfer(Action::Reveal))
                .enabled(false)
                .into(),
            Entry::Separator,
        ];
        if !canceled {
            entries
                .push(tab_menu::item("Cancel Download", Command::Transfer(Action::Cancel)).into());
            entries.push(Entry::Separator);
        }
        entries.push(tab_menu::item("Hide from Echo", Command::Transfer(Action::Hide)).into());
        entries
    }
    pub(crate) fn confirm_transfer(
        &mut self,
        id: TransferId,
        profile: UserProfileId,
        owner: DownloadsOwner,
        action: Action,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.transfer_menu_record(id, profile, owner) else {
            return;
        };
        let Some(downloads) = self.sidebar_navigation.downloads.service.clone() else {
            return;
        };
        let result = {
            let mut service = downloads.service.borrow_mut();
            match action {
                Action::Open if row.info.phase != engine::DownloadPhase::Canceled => {
                    service.request_open(row.token, profile)
                }
                Action::Cancel if !row.info.phase.is_terminal() => {
                    service.queue_cancel(row.token, profile)
                }
                Action::Hide => service.request_hide(row.token, profile),
                _ => return,
            }
        };
        // A stale terminal/profile intent is refused, never retargeted.
        if result.is_ok() {
            downloads.wake(cx);
            self.sidebar_navigation.downloads.dirty = true;
            cx.notify();
        }
    }
}
