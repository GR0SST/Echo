//! Ordinary normal-profile admission and destination policy. Native effects and
//! publication stay in the single AppDownloads consumer.
use crate::{
    downloads::{AppDownloads, DestinationDecision, StagedDestination},
    pages::PageIdentity,
    profile::ProfileHandle,
};
use browser_core::UserProfileId;
use engine::DownloadOwner;
use gpui::{AnyWindowHandle, App, Context, Global, Window};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

type Admission = Rc<dyn Fn(PageIdentity) -> Option<DownloadOwner>>;
thread_local! { static SOURCES: RefCell<HashMap<usize, Admission>> = RefCell::new(HashMap::new()); }
#[derive(Clone)]
struct SourceInfo {
    handle: ProfileHandle,
    profile: UserProfileId,
    window: AnyWindowHandle,
    explicit_url: Option<String>,
}
#[derive(Clone)]
struct OrdinaryDownloads {
    downloads: AppDownloads,
    profiles: Rc<RefCell<HashMap<u64, SourceInfo>>>,
    prompt: Rc<Cell<Option<engine::DownloadToken>>>,
}
impl Global for OrdinaryDownloads {}

pub(crate) fn bind(
    parent: *mut std::ffi::c_void,
    handle: ProfileHandle,
    window: AnyWindowHandle,
    cx: &mut App,
) {
    if parent.is_null() {
        return;
    }
    if handle.read_settings(|b| b.is_private() || b.is_blank()) {
        SOURCES.with_borrow_mut(|sources| {
            sources.remove(&(parent as usize));
        });
        return;
    }
    // Explicit disposable admission experiments keep their existing exclusive
    // consumer and deliberately chosen destination policy.
    if std::env::var("BROWSER_SCRIPT")
        .is_ok_and(|s| s.contains("app-download-arm") || s.contains("download-probe-arm"))
    {
        return;
    }
    let state = if let Some(state) = cx.try_global::<OrdinaryDownloads>() {
        state.clone()
    } else {
        if cx.has_global::<AppDownloads>() {
            return;
        }
        #[cfg(not(test))]
        let downloads = AppDownloads::default();
        #[cfg(test)]
        let downloads = AppDownloads::idle_for_tests();
        let profiles = Rc::new(RefCell::new(HashMap::<u64, SourceInfo>::new()));
        let captured = profiles.clone();
        downloads
            .service
            .borrow_mut()
            .set_staged_destination_policy(Rc::new(move |profile, request| {
                let source = captured.borrow().get(&request.provenance.owner()).cloned();
                let Some(source) = source else {
                    return DestinationDecision::Reject;
                };
                if source.profile != profile {
                    return DestinationDecision::Reject;
                }
                if let Some(url) = &source.explicit_url {
                    return if url == &request.info.original_url {
                        DestinationDecision::Defer
                    } else {
                        DestinationDecision::Reject
                    };
                }
                source.handle.read_settings(|browser| {
                    let Some(settings) = browser.user_profile(profile).map(|p| &p.settings) else {
                        return DestinationDecision::Reject;
                    };
                    if settings.ask_for_download_location {
                        return DestinationDecision::Defer;
                    }
                    let folder = if settings.download_location == "Downloads" {
                        dirs::download_dir()
                    } else {
                        let path = PathBuf::from(&settings.download_location);
                        path.is_absolute().then_some(path)
                    };
                    folder
                        .and_then(|folder| plan(folder, &request.suggested_name))
                        .map_or(DestinationDecision::Reject, DestinationDecision::Staged)
                })
            }))
            .expect("fresh staged download service");
        cx.set_global(downloads.clone());
        let state = OrdinaryDownloads {
            downloads,
            profiles,
            prompt: Rc::new(Cell::new(None)),
        };
        cx.set_global(state.clone());
        state
    };
    let owners = RefCell::new(HashMap::new());
    let admission: Admission = Rc::new(move |identity| {
        if let Some(owner) = owners.borrow().get(&identity).copied() {
            return Some(DownloadOwner::continue_normal_after_source_close(owner));
        }
        let owner = state
            .downloads
            .service
            .borrow_mut()
            .register_distinct_source(&handle, identity.profile)
            .ok()?;
        state.profiles.borrow_mut().insert(
            owner,
            SourceInfo {
                handle: handle.clone(),
                profile: identity.profile,
                window,
                explicit_url: None,
            },
        );
        owners.borrow_mut().insert(identity, owner);
        Some(DownloadOwner::continue_normal_after_source_close(owner))
    });
    SOURCES.with_borrow_mut(|sources| {
        sources.insert(parent as usize, admission);
    });
}

pub(crate) fn owner(
    parent: *mut std::ffi::c_void,
    identity: PageIdentity,
) -> Option<DownloadOwner> {
    let source = SOURCES.with_borrow(|sources| sources.get(&(parent as usize)).cloned());
    source?(identity)
}

pub(crate) fn registered(parent: *mut std::ffi::c_void) -> bool {
    SOURCES.with_borrow(|sources| sources.contains_key(&(parent as usize)))
}
impl crate::window::BrowserWindow {
    pub(crate) fn request_save_download(
        &mut self,
        identity: PageIdentity,
        epoch: u64,
        url: String,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_private() || self.browser.is_blank() {
            return;
        }
        let Some(state) = cx.try_global::<OrdinaryDownloads>().cloned() else {
            return;
        };
        let Some(request) = self
            .pages
            .as_ref()
            .and_then(|p| p.borrow_mut().prepare_file_download(identity, epoch, &url))
        else {
            return;
        };
        let Ok(owner) = state
            .downloads
            .service
            .borrow_mut()
            .register_distinct_source(&self.profile.handle, identity.profile)
        else {
            return;
        };
        state.profiles.borrow_mut().insert(
            owner,
            SourceInfo {
                handle: self.profile.handle.clone(),
                profile: identity.profile,
                window: self.main_window,
                explicit_url: Some(url),
            },
        );
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(8))
                .await;
            if crate::quit::is_quitting() {
                return;
            }
            // Native creation/start may enter AppKit. No App or page borrow.
            if let Err(error) = request(owner) {
                eprintln!("Cannot start Save As: {error}");
            }
            let _ = cx.update(|cx| state.downloads.wake(cx));
        })
        .detach();
    }
    pub(crate) fn poll_download_destinations(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.is_private()
            || self.browser.is_blank()
            || !window.is_window_active()
            || self.popup_owns_focus()
            || crate::quit::is_quitting()
        {
            return;
        }
        let Some(state) = cx.try_global::<OrdinaryDownloads>().cloned() else {
            return;
        };
        if state.prompt.get().is_some() {
            return;
        }
        let pending = state.downloads.service.borrow().pending_destinations();
        let Some((pending, source)) = pending.into_iter().find_map(|pending| {
            let source = state
                .profiles
                .borrow()
                .get(&pending.request.provenance.owner())
                .cloned()?;
            let root_live = cx
                .windows()
                .iter()
                .any(|w| w.window_id() == source.window.window_id());
            let owns_root = source.window == self.main_window
                || (!root_live && source.handle.same_workspace(&self.profile.handle));
            (owns_root && source.profile == pending.profile).then_some((pending, source))
        }) else {
            return;
        };
        let folder = source.handle.read_settings(|b| {
            b.user_profile(source.profile).and_then(|p| {
                if p.settings.download_location == "Downloads" {
                    dirs::download_dir()
                } else {
                    let path = PathBuf::from(&p.settings.download_location);
                    path.is_absolute().then_some(path)
                }
            })
        });
        let Some(folder) = folder else {
            let _ = state
                .downloads
                .service
                .borrow_mut()
                .queue_cancel(pending.token, pending.profile);
            state.downloads.wake(cx);
            return;
        };
        let dialog = crate::download_dialog::prepare(
            self.native_view,
            &folder,
            &pending.request.suggested_name,
            cx,
        );
        state.prompt.set(Some(pending.token));
        cx.spawn(async move |_, cx| {
            let choice = dialog.choose(cx).await;
            let display_name=choice.as_ref().ok().and_then(|p|p.as_ref()).and_then(|p|p.file_name()).map(|n|n.to_string_lossy().into_owned());
            if !crate::quit::is_quitting() {
                let stage = match choice {
                    Ok(Some(path)) => crate::downloads_publication::Staging::reserve_chosen(
                        (pending.token, pending.request.provenance), &path,
                    ).map_err(|e| format!("{e:?}")).map(Some),
                    Ok(None) => Ok(None),
                    Err(error) => Err(error),
                };
                let mut service = state.downloads.service.borrow_mut();
                let result = match stage {
                    Ok(Some(stage)) => {
                        let result=service.retain_chosen_staging(pending.token, pending.profile, stage);
                        if result.is_ok(){if let Some(name)=display_name {service.set_display_name(pending.token,pending.profile,name);}}result
                    }
                    Ok(None) => service.queue_cancel(pending.token, pending.profile),
                    Err(error) => {
                        eprintln!("Cannot choose download destination: {error}");
                        service.queue_cancel(pending.token, pending.profile)
                    }
                };
                if let Err(error) = result {
                    eprintln!("Cannot resolve download destination: {error}");
                }
                drop(service);
                let _ = cx.update(|cx| state.downloads.wake(cx));
            }
            if state.prompt.get() == Some(pending.token) {
                state.prompt.set(None);
            }
        })
        .detach();
    }
}

pub(crate) fn plan(folder: PathBuf, name: &str) -> Option<StagedDestination> {
    let path = Path::new(name);
    if !folder.is_absolute()
        || name.is_empty()
        || name.contains('\0')
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let extension = path.extension().and_then(|s| s.to_str());
    let candidates = std::iter::once(path.to_owned())
        .chain((1..=999).map(|index| {
            PathBuf::from(match extension {
                Some(ext) => format!("{stem} ({index}).{ext}"),
                None => format!("{stem} ({index})"),
            })
        }))
        .collect();
    Some(StagedDestination { folder, candidates })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collision_candidates_preserve_the_extension_and_do_not_target_a_parent() {
        let folder = std::env::temp_dir();
        let result = plan(folder.clone(), "sample.bin").unwrap();
        assert_eq!(result.folder, folder);
        assert_eq!(
            &result.candidates[..3],
            &[
                PathBuf::from("sample.bin"),
                PathBuf::from("sample (1).bin"),
                PathBuf::from("sample (2).bin")
            ]
        );
        assert_eq!(
            plan(std::env::temp_dir(), "README").unwrap().candidates[1],
            PathBuf::from("README (1)")
        );
        for invalid in ["", "..", ".", "../sample.bin", "/sample.bin", "x\0y"] {
            assert!(plan(std::env::temp_dir(), invalid).is_none(), "{invalid:?}");
        }
        assert!(plan(PathBuf::from("relative"), "sample.bin").is_none());
    }
}

pub(crate) fn transfer_rows(handle:&ProfileHandle, selected:Option<UserProfileId>, query:&str, cx:&App)->Vec<crate::downloads_view::TransferRow> {
    let Some(downloads)=cx.try_global::<AppDownloads>() else{return Vec::new();};
    downloads.service.borrow().transfer_presentations().into_iter().filter(|row|
        row.handle.same_workspace(handle) && selected.is_none_or(|p|p==row.profile)
        && handle.read_settings(|b|b.user_profile(row.profile).is_some())
        && crate::downloads::filename_matches_ascii(&row.title,query)
    ).map(|row|crate::downloads_view::TransferRow {
        id:row.id,profile:row.profile,title:row.title,
        subtitle:crate::downloads_progress::subtitle(&row.info,row.opening),
        progress:crate::downloads_progress::percent(&row.info),canceled:row.info.phase==engine::DownloadPhase::Canceled,
    }).collect()
}
