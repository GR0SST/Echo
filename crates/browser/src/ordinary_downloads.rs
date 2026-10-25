//! Ordinary normal-profile admission and destination policy. Native effects and
//! publication stay in the single AppDownloads consumer.
use crate::{
    downloads::{AppDownloads, DestinationDecision, StagedDestination},
    pages::PageIdentity,
    profile::ProfileHandle,
};
use engine::DownloadOwner;
use gpui::{App, Global};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

type Admission = Rc<dyn Fn(PageIdentity) -> Option<DownloadOwner>>;
thread_local! { static SOURCES: RefCell<HashMap<usize, Admission>> = RefCell::new(HashMap::new()); }
#[derive(Clone)]
struct OrdinaryDownloads {
    downloads: AppDownloads,
    profiles: Rc<RefCell<HashMap<u64, ProfileHandle>>>,
}
impl Global for OrdinaryDownloads {}

pub(crate) fn bind(parent: *mut std::ffi::c_void, handle: ProfileHandle, cx: &mut App) {
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
        let downloads = AppDownloads::default();
        let profiles = Rc::new(RefCell::new(HashMap::<u64, ProfileHandle>::new()));
        let captured = profiles.clone();
        downloads
            .service
            .borrow_mut()
            .set_staged_destination_policy(Rc::new(move |profile, request| {
                let handle = captured.borrow().get(&request.provenance.owner()).cloned();
                let Some(handle) = handle else {
                    return DestinationDecision::Reject;
                };
                handle.read_settings(|browser| {
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
        };
        cx.set_global(state.clone());
        state
    };
    let admission: Admission = Rc::new(move |identity| {
        let owner = state
            .downloads
            .service
            .borrow_mut()
            .register_source(&handle, identity.profile)
            .ok()?;
        state.profiles.borrow_mut().insert(owner, handle.clone());
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
