//! Save an existing document without navigating or replacing its renderer.
use super::{BrowserWindow, PopupFocus};
use crate::{pages::PageIdentity, save_dialog};
use browser_core::{
    save_page::{self as export, Format},
    TabId,
};
use gpui::{AppContext, Context};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
struct Source {
    tab: TabId,
    identity: PageIdentity,
    epoch: u64,
    url: String,
    generation: u64,
}
impl BrowserWindow {
    pub(crate) fn is_saving_page(&self) -> bool {
        self.save_page_task.is_some()
    }
    pub(crate) fn save_page_available(&self) -> bool {
        !self.is_saving_page()
            && self.native_page_content_visible()
            && self.browser.active_tab().is_some_and(|tab| {
                !browser_core::omnibox::is_internal_url(&tab.url)
                    && self
                        .pages
                        .as_ref()
                        .is_some_and(|pages| pages.borrow().can_archive_page(tab.id))
            })
    }
    fn save_source_matches(&self, source: &Source) -> bool {
        !crate::quit::is_quitting()
            && self.save_page_generation == source.generation
            && self
                .browser
                .active_tab()
                .is_some_and(|tab| tab.id == source.tab && tab.url == source.url)
            && self.pages.as_ref().is_some_and(|pages| {
                let pages = pages.borrow();
                pages.media_identity(source.tab) == Some(source.identity)
                    && pages.page_document_epoch(source.tab) == Some(source.epoch)
            })
    }
    pub(crate) fn save_page(&mut self, cx: &mut Context<Self>) {
        trace("action received");
        self.refresh_profile_projection();
        if !self.save_page_available() || crate::quit::is_quitting() {
            return;
        }
        let tab = self.browser.active_tab().unwrap();
        let Some((identity, epoch)) = self.pages.as_ref().and_then(|pages| {
            let pages = pages.borrow();
            Some((
                pages.media_identity(tab.id)?,
                pages.page_document_epoch(tab.id)?,
            ))
        }) else {
            return;
        };
        self.save_page_generation = self
            .save_page_generation
            .checked_add(1)
            .expect("Save request generation exhausted");
        let source = Source {
            tab: tab.id,
            identity,
            epoch,
            url: tab.url.clone(),
            generation: self.save_page_generation,
        };
        let name = export::suggested_name(&tab.title, Format::Complete);
        self.close_user_profile_dialog_with_focus(PopupFocus::Restore, cx);
        self.close_site_controls_with_focus(PopupFocus::Restore, cx);
        self.close_new_menu_with_focus(PopupFocus::Restore, cx);
        self.close_command_bar_with_focus(PopupFocus::Restore, cx);
        self.close_find_with_focus(PopupFocus::Restore, cx);
        self.close_tab_menu_with_focus(PopupFocus::Restore, cx);
        self.finish_recent_items(false, PopupFocus::Restore, cx);
        self.close_floating_sidebar(cx);
        let main = self.main_window;
        let task = cx.spawn(async move |owner, cx| {
            let deadline = cx.background_executor().now() + Duration::from_millis(350);
            let prepared = loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let state = cx
                    .update(|cx| {
                        main.update(cx, |_, window, cx| {
                            owner
                                .update(cx, |view, cx| {
                                    if !view.save_source_matches(&source) {
                                        return None;
                                    }
                                    if view.popup_focus_pending > 0 || !window.is_window_active() {
                                        return (cx.background_executor().now() < deadline)
                                            .then_some(None);
                                    }
                                    view.save_prompt_active = true;
                                    view.invalidate_sidebar_focus();
                                    view.invalidate_navigation_focus();
                                    if let Some(pages) = &view.pages {
                                        pages.borrow_mut().request_ui_focus();
                                    }
                                    Some(Some(save_dialog::prepare(
                                        crate::native::view_of(window),
                                        &name,
                                        cx,
                                    )))
                                })
                                .ok()
                                .flatten()
                        })
                        .ok()
                        .flatten()
                    })
                    .ok()
                    .flatten();
                match state {
                    Some(Some(request)) => break Some(request),
                    Some(None) => continue,
                    None => break None,
                }
            };
            trace(if prepared.is_some() {
                "prepared dialog"
            } else {
                "dialog preparation cancelled"
            });
            let result = if let Some(prepared) = prepared {
                match prepared.choose(cx).await {
                    Ok(Some(choice)) => {
                        trace("save accepted");
                        let request = owner
                            .update(cx, |view, _| {
                                view.save_prompt_active = false;
                                if !view.save_source_matches(&source) {
                                    return None;
                                }
                                view.pages
                                    .as_ref()?
                                    .borrow_mut()
                                    .request_page_archive(source.tab)
                            })
                            .ok()
                            .flatten();
                        if let Some(mut request) = request {
                            let captured = loop {
                                if let Some(result) = request.poll() {
                                    break Some(result);
                                }
                                if !owner
                                    .read_with(cx, |view, _| view.save_source_matches(&source))
                                    .unwrap_or(false)
                                {
                                    break None;
                                }
                                cx.background_executor()
                                    .timer(Duration::from_millis(16))
                                    .await;
                            };
                            drop(request);
                            match captured {
                                Some(Ok(bytes))
                                    if owner
                                        .read_with(cx, |view, _| view.save_source_matches(&source))
                                        .unwrap_or(false) =>
                                {
                                    cx.background_executor()
                                        .spawn(async move {
                                            publish(&bytes, &choice.path, choice.format)
                                        })
                                        .await
                                        .map(|_| ())
                                }
                                Some(Err(error)) => Err(error.to_string()),
                                _ => Ok(()),
                            }
                        } else {
                            Ok(())
                        }
                    }
                    Ok(None) => {
                        trace("dialog cancelled");
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            } else {
                Ok(())
            };
            let _ = owner.update(cx, |view, cx| {
                view.save_prompt_active = false;
                if view.save_page_generation != source.generation {
                    return;
                }
                view.popup_focus_on_activation = true;
                if let Err(error) = result {
                    view.show_toast(error, cx);
                }
                let owner = cx.weak_entity();
                let main = view.main_window;
                cx.defer(move |cx| {
                    let active = main
                        .update(cx, |_, window, _| window.is_window_active())
                        .unwrap_or(false);
                    if active {
                        let _ = owner.update(cx, |view, cx| {
                            view.popup_focus_on_activation = false;
                            view.close_popup_overlay(None, PopupFocus::Restore, true, cx);
                        });
                    }
                });
                view.save_page_task = None;
                cx.notify();
            });
        });
        self.save_page_task = Some(task);
        cx.notify();
    }
}

struct NewAssets {
    path: PathBuf,
    keep: bool,
}
impl Drop for NewAssets {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
fn publish(bytes: &[u8], path: &Path, format: Format) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("The filename is unavailable.")?;
    let base = export::assets_directory(filename);
    let mut directory = base.clone();
    let mut assets = None;
    let mut output = export::export(bytes, format, filename, &directory)?;
    if !output.assets.is_empty() {
        for attempt in 0..10_000 {
            directory = if attempt == 0 {
                base.clone()
            } else {
                format!("{base} ({attempt})")
            };
            let directory_path = parent.join(&directory);
            match std::fs::create_dir(&directory_path) {
                Ok(()) => {
                    assets = Some(NewAssets {
                        path: directory_path,
                        keep: false,
                    });
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("The page could not be saved: {error}")),
            }
        }
        let target = assets
            .as_ref()
            .ok_or("The page resources directory is unavailable.")?;
        if directory != base {
            output = export::export(bytes, format, filename, &directory)?;
        }
        for (name, contents) in &output.assets {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target.path.join(name))
                .map_err(|error| format!("A page resource could not be saved: {error}"))?;
            file.write_all(contents)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("A page resource could not be saved: {error}"))?;
        }
    }
    let mut document = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("The page could not be saved: {error}"))?;
    document
        .write_all(&output.document)
        .and_then(|_| document.as_file().sync_all())
        .map_err(|error| format!("The page could not be saved: {error}"))?;
    document
        .persist(path)
        .map_err(|error| format!("The page could not be saved: {}", error.error))?;
    if let Some(assets) = &mut assets {
        assets.keep = true;
    }
    Ok(())
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    const ARCHIVE: &[u8] = include_bytes!("save_page_test_fixture.mhtml");
    #[test]
    fn complete_save_does_not_overwrite_an_existing_resources_directory() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("Keep_files");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("owner.txt"), b"keep this").unwrap();
        let path = directory.path().join("Keep.html");
        publish(ARCHIVE, &path, Format::Complete).unwrap();
        assert_eq!(std::fs::read(old.join("owner.txt")).unwrap(), b"keep this");
        let html = std::fs::read_to_string(path).unwrap();
        assert!(html.contains("Keep_files%20(1)/1_theme.css"), "{html}");
        assert!(directory
            .path()
            .join("Keep_files (1)/2_image.png")
            .is_file());
    }
    #[test]
    fn a_failed_document_publication_removes_only_its_new_resources() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Blocked.html");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("owner.txt"), b"unchanged").unwrap();
        assert!(publish(ARCHIVE, &path, Format::Complete).is_err());
        assert!(!directory.path().join("Blocked_files").exists());
        assert_eq!(std::fs::read(path.join("owner.txt")).unwrap(), b"unchanged");
    }
    #[test]
    fn single_file_and_html_only_do_not_create_resource_sidecars() {
        let directory = tempfile::tempdir().unwrap();
        let mhtml = directory.path().join("Single.mhtml");
        publish(ARCHIVE, &mhtml, Format::SingleFile).unwrap();
        assert_eq!(std::fs::read(mhtml).unwrap(), ARCHIVE);
        let html = directory.path().join("Only.html");
        publish(ARCHIVE, &html, Format::HtmlOnly).unwrap();
        assert!(std::fs::read_to_string(html)
            .unwrap()
            .contains("https://saved.example/theme.css"));
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}

fn trace(message: &str) {
    if std::env::var_os("BROWSER_SCRIPT").is_some() {
        eprintln!("[save-page] {message}");
    }
}
