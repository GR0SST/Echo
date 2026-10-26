//! Completed-file menu effects, resolved from canonical data at activation time.
use crate::{
    download_files::{self, FileAction},
    library_downloads_host::DownloadsOwner,
    tab_menu::{self, Command, Entry},
    window::BrowserWindow,
};
use browser_core::downloads::{CompletedDownload, CompletedDownloadId};
use gpui::{Context, Window};

impl BrowserWindow {
    pub(crate) fn download_menu_record(
        &self,
        id: CompletedDownloadId,
        owner: DownloadsOwner,
    ) -> Option<CompletedDownload> {
        if crate::quit::is_quitting() || !self.downloads_route_active() {
            return None;
        }
        let host = self.sidebar_navigation.downloads.host.as_ref()?;
        if host.owner() != owner {
            return None;
        }
        self.profile
            .handle
            .download_inventory()?
            .1
            .into_iter()
            .find(|record| record.id() == id)
    }
    pub(crate) fn download_menu_commands(
        &self,
        id: CompletedDownloadId,
        owner: DownloadsOwner,
    ) -> Vec<Entry> {
        let Some(record) = self.download_menu_record(id, owner) else {
            return vec![];
        };
        let available = download_files::available(&record);
        let kind = download_files::kind(&record);
        vec![
            tab_menu::item(format!("Open {kind}"), Command::Download(FileAction::Open))
                .enabled(available)
                .into(),
            tab_menu::item(format!("Copy {kind}"), Command::Download(FileAction::Copy))
                .enabled(available)
                .into(),
            tab_menu::item("Show in Finder", Command::Download(FileAction::Reveal))
                .enabled(available)
                .into(),
            Entry::Separator,
            tab_menu::item("Hide", Command::Download(FileAction::Hide)).into(),
            tab_menu::item("Move to Trash", Command::Download(FileAction::Trash))
                .enabled(available)
                .into(),
        ]
    }
    pub(crate) fn confirm_download_menu(
        &mut self,
        id: CompletedDownloadId,
        owner: DownloadsOwner,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(record) = self.download_menu_record(id, owner) else {
            return;
        };
        let result = match action {
            FileAction::Hide => {
                self.profile.handle.hide_completed_download(id);
                Ok(())
            }
            _ if !download_files::available(&record) => Err(download_files::MISSING_FILE.into()),
            FileAction::Open => {
                cx.open_with_system(record.final_path());
                Ok(())
            }
            FileAction::Copy => download_files::copy_file(record.final_path(), cx),
            FileAction::Reveal => {
                cx.reveal_path(record.final_path());
                Ok(())
            }
            FileAction::Trash => download_files::move_to_trash(record.final_path()).map(|()| {
                self.profile.handle.hide_completed_download(id);
            }),
        };
        if let Err(error) = result {
            self.show_toast(error.into(), cx);
        }
        self.sidebar_navigation.downloads.dirty = true;
        self.sync_library_downloads(window, cx);
        cx.notify();
    }
}
