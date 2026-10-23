//! Staged publication owned by the existing Running/shutdown consumer.
//! Admission and destination plans remain explicit; no default policy is installed.
//! All methods on Work must run outside ALL App/service/model/Engine borrows.
//! Only the existing consumer's lease may detach work; Drop returns its exact
//! state and publishes this batch's failures before releasing that lease.
use super::*;
use crate::downloads_publication::{
    Observation, Publication, PublishedPath, Registration, Staging,
};
use shutdown::ConsumerLease;
use std::path::Path;

type Key<T> = (T, DownloadProvenance);

/// Pure injected policy output. No default directory or candidate generation.
/// Reserve with Staging::reserve((token, request.provenance), ...) OUTSIDE borrows,
/// then install the owned result with DownloadService::retain_staging.
pub(super) struct Plan {
    pub folder: PathBuf,
    pub candidates: Vec<PathBuf>,
}

impl Plan {
    /// Filesystem effect. Consume this plan only after leaving all borrows.
    pub(super) fn reserve<T: Copy + Eq>(
        self,
        token: T,
        provenance: DownloadProvenance,
    ) -> Result<Staging<Key<T>>, crate::downloads_publication::Error> {
        Staging::reserve((token, provenance), &self.folder, self.candidates)
    }
}

/// Definite failure means the writer guarantees it made NO canonical mutation.
/// Any uncertain mutation must use Unknown; it cannot be automatically retried.
/// The ProfileHandle adapter below preserves this pre-mutation error boundary.
pub(super) enum WriteOutcome {
    Recorded(CompletedDownloadId),
    NotWritten(String),
    Unknown(String),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Handoff {
    Pending,
    Uncertain,
    Accepted,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteState {
    Ready,
    Uncertain,
}
struct Job<T, H> {
    key: Key<T>,
    owner: Owner<H>,
    request: DestinationRequest,
    stage: Staging<Key<T>>,
    registration: Registration<Key<T>>,
    handoff: Handoff,
    canceled: bool,
    publication: Option<Publication>,
    complete: Option<DownloadInfo>,
    proof: Option<PublishedPath>,
    writer: WriteState,
    filesystem_unacknowledged: bool,
    committed: Option<CompletedDownloadId>,
    failure: Option<String>,
    history: Vec<String>,
}
impl<T: Copy + Eq, H> Job<T, H> {
    fn fail(&mut self, error: String) -> String {
        self.history.push(error.clone());
        if self.failure.is_none() {
            self.failure = Some(error.clone());
        }
        error
    }
    fn observe(&mut self, event: TransferEvent<T>) {
        if (event.token, event.provenance) != self.key {
            self.fail("Staged download identity changed".into());
            return;
        }
        if self.complete.is_some() || self.canceled {
            return;
        }
        if let DownloadDestination::Requested { suggested_name } = &event.destination {
            if suggested_name != &self.request.suggested_name {
                self.fail("Staged download request changed".into());
                if event.destination_requested || !event.info.phase.is_terminal() {
                    return;
                }
            }
        }
        if event.destination_requested {
            return;
        }
        // Interrupted is resumable. Cancellation retains all owned staged bytes
        // and cannot be resurrected by a later Complete, including during quit.
        if event.info.phase == DownloadPhase::Canceled {
            self.canceled = true;
            return;
        }
        if event.info.phase != DownloadPhase::Complete {
            return;
        }
        if self.handoff != Handoff::Accepted {
            self.fail("Staged Complete preceded an acknowledged handoff".into());
            return;
        }
        match self.stage.observe(
            &self.registration,
            &self.key,
            Observation::UpdatedComplete,
            &event.info.full_path,
        ) {
            Ok(Some(publication)) => {
                self.publication = Some(publication);
                self.complete = Some(event.info);
            }
            Ok(None) => {
                self.fail("Staged Complete did not detach publication".into());
            }
            Err(error) => {
                self.fail(format!("Staged completion: {error:?}"));
            }
        }
    }
    fn pending(&self) -> bool {
        self.failure.is_some() || (!self.canceled && self.committed.is_none())
    }
}
struct Slot<T, H> {
    job: Rc<Cell<Option<Job<T, H>>>>,
    queued: Rc<RefCell<Vec<TransferEvent<T>>>>,
}
impl<T, H> Clone for Slot<T, H> {
    fn clone(&self) -> Self {
        Self {
            job: self.job.clone(),
            queued: self.queued.clone(),
        }
    }
}
pub(super) struct StagedTransfers<T, H> {
    slots: HashMap<T, Slot<T, H>>,
}
impl<T, H> Default for StagedTransfers<T, H> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }
}
impl<T: Copy + Eq + Hash, H: Clone> StagedTransfers<T, H> {
    fn insert(
        &mut self,
        token: T,
        owner: Owner<H>,
        request: DestinationRequest,
        mut stage: Staging<Key<T>>,
    ) -> Result<(), String> {
        if self.slots.contains_key(&token) {
            return Err("Staging already retained".into());
        }
        let key = (token, request.provenance);
        let registration = stage.registration();
        // Pure identity check; NOT filesystem validation or a native observation.
        stage
            .observe(
                &registration,
                &key,
                Observation::UpdatedPending,
                Path::new(""),
            )
            .map_err(|e| format!("Staging registration: {e:?}"))?;
        self.slots.insert(
            token,
            Slot {
                job: Rc::new(Cell::new(Some(Job {
                    key,
                    owner,
                    request,
                    stage,
                    registration,
                    handoff: Handoff::Pending,
                    canceled: false,
                    publication: None,
                    complete: None,
                    proof: None,
                    writer: WriteState::Ready,
                    filesystem_unacknowledged: false,
                    committed: None,
                    failure: None,
                    history: Vec::new(),
                }))),
                queued: Rc::new(RefCell::new(Vec::new())),
            },
        );
        Ok(())
    }
    /// Pure ingress. Queues copied events if a detached effect is reentered;
    /// they remain owned and are reduced before each subsequent Work effect
    /// and before the work lease is released.
    pub(super) fn ingest(&self, event: TransferEvent<T>) -> Result<Vec<String>, TransferEvent<T>> {
        let Some(slot) = self.slots.get(&event.token) else {
            return Err(event);
        };
        slot.queued.borrow_mut().push(event);
        let mut errors = Vec::new();
        if let Some(mut job) = slot.job.take() {
            let before = job.history.len();
            for event in slot.queued.take() {
                job.observe(event);
            }
            errors.extend_from_slice(&job.history[before..]);
            slot.job.set(Some(job));
        }
        Ok(errors)
    }
    pub(super) fn has_pending(&self) -> bool {
        self.slots.values().any(|slot| {
            let Some(job) = slot.job.take() else {
                return true;
            };
            let pending = job.pending();
            slot.job.set(Some(job));
            pending
        })
    }
    pub(super) fn retry(&self, token: T, profile: UserProfileId) -> Result<(), String> {
        let slot = self.slots.get(&token).ok_or("Unknown staged transfer")?;
        let mut job = slot.job.take().ok_or("Staged work is in flight")?;
        let result = if job.owner.profile != profile {
            Err("Staged transfer belongs to another profile".into())
        } else if job.failure.is_none()
            || job.handoff == Handoff::Uncertain
            || job.writer == WriteState::Uncertain
            || job.filesystem_unacknowledged
        {
            Err("No safely retryable staged failure".into())
        } else {
            job.failure = None; // Historical diagnostics remain sticky.
            Ok(())
        };
        slot.job.set(Some(job));
        result
    }
    /// Pure snapshot/detach under one service borrow. Returning the same lease
    /// when no work is eligible avoids acquiring another consumer capability.
    fn detach_next(
        &self,
        lease: ConsumerLease,
        destinations: bool,
    ) -> Result<Work<T, H>, ConsumerLease> {
        for slot in self.slots.values() {
            let Some(job) = slot.job.take() else {
                continue;
            };
            let ready = job.failure.is_none()
                && !job.canceled
                && job.committed.is_none()
                && ((destinations && job.handoff == Handoff::Pending) || job.publication.is_some());
            if ready {
                let before = job.history.len();
                return Ok(Work {
                    slot: slot.clone(),
                    job: Some(job),
                    lease: Some(lease),
                    before,
                });
            }
            slot.job.set(Some(job));
        }
        Err(lease)
    }
    pub(super) fn detach(&self, token: T, lease: ConsumerLease) -> Result<Work<T, H>, String> {
        let slot = self
            .slots
            .get(&token)
            .ok_or("Unknown staged transfer")?
            .clone();
        let job = slot.job.take().ok_or("Staged work is in flight")?;
        let before = job.history.len();
        Ok(Work {
            slot,
            job: Some(job),
            lease: Some(lease),
            before,
        })
    }
}

/// Not Clone. The service keeps the return slot while this value owns effects.
/// No cleanup on failure/drop. No filesystem operation appears in Drop.
pub(super) struct Work<T: Copy + Eq, H> {
    slot: Slot<T, H>,
    job: Option<Job<T, H>>,
    lease: Option<ConsumerLease>,
    before: usize,
}
impl<T: Copy + Eq, H> Work<T, H> {
    /// Pure barrier before EVERY external effect. Detached ingress may have
    /// queued cancellation or a sticky identity/request failure since the last
    /// effect. Keep the same lease while applying observations, then release
    /// the queue borrow before authorizing filesystem/native/writer execution.
    fn authorize_effect(&mut self) -> Result<(), String> {
        let events = self.slot.queued.take();
        let job = self.job.as_mut().unwrap();
        for event in events {
            job.observe(event);
        }
        if let Some(error) = &job.failure {
            return Err(error.clone());
        }
        if job.canceled {
            return Err("Staged transfer is canceled".into());
        }
        Ok(())
    }
    pub(super) fn handoff(
        &mut self,
        destination: impl FnOnce(T, PathBuf) -> Result<(), String>,
    ) -> Result<(), String> {
        self.authorize_effect()?;
        let job = self.job.as_mut().unwrap();
        if job.handoff != Handoff::Pending {
            return Err("Staged handoff is unavailable".into());
        }
        if crate::quit::is_quitting() || !self.lease.as_ref().unwrap().allows_destination() {
            return Err("Staged destination suppressed after consumer handoff".into());
        }
        job.filesystem_unacknowledged = true;
        let handoff_validation = job.stage.validate_handoff().map(Path::to_owned);
        job.filesystem_unacknowledged = false;
        let path = match handoff_validation {
            Ok(path) => path,
            Err(error) => return Err(job.fail(format!("Staged handoff validation: {error:?}"))),
        };
        // Validation was an external effect. Reduce any observations queued
        // during it before authorizing Continue against the current job/gate.
        self.authorize_effect()?;
        if crate::quit::is_quitting() || !self.lease.as_ref().unwrap().allows_destination() {
            return Err("Staged destination suppressed after consumer handoff".into());
        }
        let job = self.job.as_mut().unwrap();
        // Native error/panic cannot authorize a second Continue. Reconciliation
        // of an uncertain native outcome is an explicit remaining activation gate.
        job.handoff = Handoff::Uncertain;
        match destination(job.key.0, path) {
            Ok(()) => {
                job.handoff = Handoff::Accepted;
                Ok(())
            }
            Err(error) => Err(job.fail(error)),
        }
    }
    pub(super) fn publish_and_write(
        &mut self,
        writer: impl FnOnce(&H, UserProfileId, DownloadCompletion, &PublishedPath) -> WriteOutcome,
    ) -> Result<CompletedDownloadId, String> {
        self.authorize_effect()?;
        let job = self.job.as_mut().unwrap();
        if let Some(id) = job.committed {
            return Ok(id);
        }
        if job.writer == WriteState::Uncertain {
            return Err(job.fail("Canonical writer outcome is unknown".into()));
        }
        if job.publication.is_none() {
            return Err("No genuine bound Complete to publish".into());
        }
        // The entry barrier authorizes publication. No external call occurred
        // between that barrier and this effect; all borrows here are local Work
        // ownership, never App/service/model/Engine borrows.
        if job.proof.is_none() {
            job.filesystem_unacknowledged = true;
            let publication_result = job.publication.as_mut().unwrap().execute();
            job.filesystem_unacknowledged = false;
            match publication_result {
                Ok(proof) => job.proof = Some(proof),
                Err(error) => return Err(job.fail(format!("Staged publication: {error:?}"))),
            }
        }
        // Publication may have allowed reentrant copied ingress. Its queued
        // failures must overtake proof validation and canonical mutation.
        self.authorize_effect()?;
        let job = self.job.as_mut().unwrap();
        job.filesystem_unacknowledged = true;
        let proof_validation = job
            .publication
            .as_ref()
            .unwrap()
            .validate_proof(job.proof.as_ref().unwrap());
        job.filesystem_unacknowledged = false;
        if let Err(error) = proof_validation {
            return Err(job.fail(format!("Published proof validation: {error:?}")));
        }
        // Recheck after the filesystem validation, immediately before creating
        // the proof-derived completion and invoking the external writer.
        self.authorize_effect()?;
        let job = self.job.as_mut().unwrap();
        let proof = job.proof.as_ref().unwrap();
        let info = job.complete.as_ref().unwrap();
        // Only the validated final proof supplies final_path. The original
        // request, owner/profile and first actual Complete metadata stay frozen.
        let completion = DownloadCompletion {
            final_path: proof.path().to_owned(),
            source_url: info.url.clone(),
            original_url: info.original_url.clone(),
            mime_type: info.mime_type.clone(),
            finished_at: DownloadTime::from_cef_basetime(info.end_time),
        };
        job.writer = WriteState::Uncertain; // Installed BEFORE external writer.
        match writer(&job.owner.handle, job.owner.profile, completion, proof) {
            WriteOutcome::Recorded(id) if id.profile() == job.owner.profile => {
                job.committed = Some(id);
                job.writer = WriteState::Ready;
                Ok(id)
            }
            WriteOutcome::Recorded(_) => {
                Err(job.fail("Canonical writer returned another profile; outcome unknown".into()))
            }
            WriteOutcome::NotWritten(error) => {
                job.writer = WriteState::Ready;
                Err(job.fail(error))
            }
            WriteOutcome::Unknown(error) => Err(job.fail(error)),
        }
    }
}
impl<T: Copy + Eq, H> Work<T, H> {
    /// Return state and failures before giving the SAME lease back to its batch.
    /// This is also the unwind path; neither path performs filesystem/native I/O.
    fn restore(&mut self) -> Vec<String> {
        let Some(mut job) = self.job.take() else {
            return Vec::new();
        };
        if (job.handoff == Handoff::Uncertain
            || job.writer == WriteState::Uncertain
            || job.filesystem_unacknowledged)
            && job.failure.is_none()
        {
            job.fail("Detached staged effect returned with an unknown outcome".into());
        }
        for event in self.slot.queued.take() {
            job.observe(event);
        }
        let errors = job.history[self.before..].to_vec();
        self.lease
            .as_ref()
            .unwrap()
            .record_errors(errors.iter().cloned());
        self.slot.job.set(Some(job));
        errors
    }
    fn finish(mut self) -> (ConsumerLease, Vec<String>) {
        let errors = self.restore();
        (self.lease.take().unwrap(), errors)
    }
}
impl<T: Copy + Eq, H> Drop for Work<T, H> {
    fn drop(&mut self) {
        self.restore();
        drop(self.lease.take());
    }
}

/// Run eligible staged effects with the existing batch lease. No App reference,
/// engine drain, task, pump or second lease is created here. All callbacks and
/// filesystem work execute after the service borrow has ended.
pub(super) fn drive<T: Copy + Eq + Hash, H: Clone>(
    service: &Rc<RefCell<DownloadService<T, H>>>,
    mut lease: ConsumerLease,
    mut destination: impl FnMut(T, PathBuf) -> Result<(), String>,
    mut writer: impl FnMut(&H, UserProfileId, DownloadCompletion, &PublishedPath) -> WriteOutcome,
) -> (ConsumerLease, Result<(), String>) {
    loop {
        let destinations = lease.allows_destination() && !crate::quit::is_quitting();
        let detached = { service.borrow().staged.detach_next(lease, destinations) };
        let mut work = match detached {
            Ok(work) => work,
            Err(lease) => return (lease, Ok(())),
        };
        let mut result = Ok(());
        if work.job.as_ref().unwrap().handoff == Handoff::Pending {
            result = work.handoff(&mut destination);
        }
        // Continue can synchronously enqueue Complete. Apply those observations
        // before deciding whether this same owned Work can publish and record.
        if result.is_ok() {
            result = work.authorize_effect();
        }
        if result.is_ok() && work.job.as_ref().unwrap().publication.is_some() {
            result = work.publish_and_write(&mut writer).map(|_| ());
        }
        // A genuine cancellation is successful disposal, not a sticky failure.
        // It retains staged bytes and never authorizes a later effect.
        if work
            .job
            .as_ref()
            .is_some_and(|job| job.canceled && job.failure.is_none())
        {
            result = Ok(());
        }
        let (returned, errors) = work.finish();
        lease = returned;
        if !errors.is_empty() {
            service.borrow_mut().errors.extend(errors);
        }
        if let Err(error) = result {
            return (lease, Err(error));
        }
    }
}

/// Only Work calls this adapter, after validating the private publication proof
/// outside all service/model/App borrows. The existing ProfileHandle method
/// returns Err only before canonical mutation; unwind remains Unknown in Work.
pub(super) fn record_profile(
    handle: &ProfileHandle,
    profile: UserProfileId,
    completion: DownloadCompletion,
    proof: &PublishedPath,
) -> WriteOutcome {
    if completion.final_path.as_os_str() != proof.path().as_os_str() {
        return WriteOutcome::NotWritten("Completion does not match publication proof".into());
    }
    match handle.record_completed_download(profile, completion) {
        Ok(id) => WriteOutcome::Recorded(id),
        Err(error) => WriteOutcome::NotWritten(error),
    }
}

impl<T: Copy + Eq + Hash, H: Clone> DownloadService<T, H> {
    /// Internal-only registration, not called by production policy or fixture.
    /// The existing request must have been ingested with Defer. Reservation and
    /// all later Work calls are outside borrows; this method does no I/O.
    pub(super) fn retain_staging(
        &mut self,
        token: T,
        stage: Staging<Key<T>>,
    ) -> Result<(), String> {
        if self.fixture.is_some() {
            return Err("Direct disposable fixture cannot stage".into());
        }
        let transfer = self
            .router
            .ledger
            .get(&token)
            .ok_or("Unknown download token")?;
        if transfer.destination_dispatched
            || transfer.info.phase.is_terminal()
            || transfer.completion.is_some()
        {
            return Err("Download has already passed staging admission".into());
        }
        let request = transfer
            .destination_request
            .clone()
            .ok_or("Missing frozen destination request")?;
        let owner = self
            .router
            .owners
            .get(&transfer.provenance.owner())
            .ok_or("Missing captured owner")?
            .clone();
        self.staged.insert(token, owner, request, stage)?;
        // Prevent the legacy direct-path policy from dispatching this transfer.
        self.router
            .ledger
            .get_mut(&token)
            .unwrap()
            .destination_dispatched = true;
        Ok(())
    }
}

#[cfg(test)]
#[path = "downloads_staged_tests.rs"]
mod tests;
