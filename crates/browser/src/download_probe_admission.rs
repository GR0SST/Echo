//! Pure one-source opt-in descriptor. Native creation never holds this borrow.
use super::Config;
use crate::pages::{PageIdentity, StorageSelection};
use browser_core::SpaceId;
use engine::DownloadOwner;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub identity: PageIdentity,
    pub space: SpaceId,
    pub storage: StorageSelection,
    pub parent: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Waiting,
    Creating,
    Claimed,
    Failed,
}
pub(crate) struct Descriptor {
    pub source: Source,
    pub owner: u64,
    pub config: Config,
    pub phase: Phase,
    pub generation: Option<u64>,
}
pub(crate) type Shared = Rc<RefCell<Descriptor>>;
thread_local! { static ADMISSION: RefCell<Option<Shared>> = const { RefCell::new(None) }; }

pub(crate) fn install(config: Config, source: Source, owner: u64) -> Shared {
    let descriptor = Rc::new(RefCell::new(Descriptor {
        source,
        owner,
        config,
        phase: Phase::Waiting,
        generation: None,
    }));
    ADMISSION.with(|slot| *slot.borrow_mut() = Some(descriptor.clone()));
    descriptor
}
pub(crate) fn occupied() -> bool { ADMISSION.with(|slot| slot.borrow().is_some()) }
pub(crate) fn try_install(config: Config, source: Source, owner: u64) -> Result<Shared, String> {
    ADMISSION.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() { return Err("Download admission already occupied".into()); }
        let shared = Rc::new(RefCell::new(Descriptor { config, source, owner, phase: Phase::Waiting, generation: None }));
        *slot = Some(shared.clone());
        Ok(shared)
    })
}
pub(crate) enum Decision {
    Ordinary,
    Rejected,
    Candidate(Shared),
}

pub(crate) fn inspect(
    identity: PageIdentity,
    space: SpaceId,
    storage: &StorageSelection,
    parent: usize,
    url: &str,
) -> Decision {
    ADMISSION.with(|slot| {
        let Some(shared) = slot.borrow().clone() else {
            return Decision::Ordinary;
        };
        let mut descriptor = shared.borrow_mut();
        // Core IDs are workspace-local. A different host must keep using its
        // ordinary denied creator even if another workspace reused these IDs.
        if parent != descriptor.source.parent || identity.tab != descriptor.source.identity.tab {
            return Decision::Ordinary;
        }
        if descriptor.phase != Phase::Waiting
            || identity != descriptor.source.identity
            || space != descriptor.source.space
            || storage != &descriptor.source.storage
            || parent == 0
            || parent != descriptor.source.parent
            || matches!(storage, StorageSelection::Current)
            || url != descriptor.config.fixture_url()
        {
            if descriptor.phase == Phase::Waiting {
                descriptor.phase = Phase::Failed;
            }
            return Decision::Rejected;
        }
        drop(descriptor);
        Decision::Candidate(shared)
    })
}

/// Called only AFTER context readiness; Pending leaves the descriptor unclaimed.
/// Set Creating before CEF to reject duplicate/reentrant attempts. Return copied
/// capability and release every descriptor borrow before native creation.
pub(crate) fn begin(shared: &Shared) -> Option<DownloadOwner> {
    let mut descriptor = shared.borrow_mut();
    if descriptor.phase != Phase::Waiting {
        return None;
    }
    descriptor.phase = Phase::Creating;
    Some(descriptor.config.owner(descriptor.owner))
}
pub(crate) fn finish(shared: &Shared, created: bool) {
    let mut descriptor = shared.borrow_mut();
    if descriptor.phase != Phase::Creating {
        return;
    }
    descriptor.phase = if created {
        Phase::Claimed
    } else {
        Phase::Failed
    };
    if created {
        descriptor.generation = Some(descriptor.source.identity.generation);
    }
}
pub(crate) fn fail(shared: &Shared) {
    shared.borrow_mut().phase = Phase::Failed;
}

#[cfg(test)]
pub(crate) fn reset() {
    ADMISSION.with(|slot| *slot.borrow_mut() = None);
}

#[cfg(test)]
#[path = "download_probe_admission_tests.rs"]
mod tests;
