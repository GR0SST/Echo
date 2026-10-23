//! Exclusive quit ingestion of copied events through the canonical reducer.
//! quit.rs selects the Running or shutdown-owned engine drain outside state
//! borrows, then performs the final persist/flush/finish barrier before exit.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShutdownDownloadError {
    InFlight,
    Inactive,
    Failure(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConsumerOwner {
    Running,
    Shutdown,
    Finished,
}
#[derive(Clone)]
pub(super) struct ConsumerGate {
    owner: Rc<Cell<ConsumerOwner>>,
    busy: Rc<Cell<bool>>,
    handoff_failure: Rc<RefCell<Option<String>>>,
}
impl Default for ConsumerGate {
    fn default() -> Self {
        Self {
            owner: Rc::new(Cell::new(ConsumerOwner::Running)),
            busy: Rc::new(Cell::new(false)),
            handoff_failure: Rc::new(RefCell::new(None)),
        }
    }
}
pub(super) struct ConsumerLease {
    gate: ConsumerGate,
    failures: RefCell<Vec<String>>,
}
impl ConsumerLease {
    /// Pure check immediately before an outside-borrows destination effect.
    pub(super) fn allows_destination(&self) -> bool {
        self.gate.is_running()
    }
    /// Only genuine errors from THIS batch, never historical service diagnostics
    /// or effects intentionally suppressed by the quit seal.
    pub(super) fn with_service_errors<T: Copy + Eq + Hash, H: Clone, R>(
        &self,
        service: &mut DownloadService<T, H>,
        operation: impl FnOnce(&mut DownloadService<T, H>) -> R,
    ) -> R {
        let before = service.errors.len();
        let result = operation(service);
        self.record_errors(service.errors[before..].iter().cloned());
        result
    }
    pub(super) fn record_errors(&self, errors: impl IntoIterator<Item = String>) {
        self.failures.borrow_mut().extend(errors);
    }
}
impl Drop for ConsumerLease {
    fn drop(&mut self) {
        if self.gate.owner.get() == ConsumerOwner::Shutdown {
            let failures = self.failures.borrow();
            if !failures.is_empty() {
                let mut sticky = self.gate.handoff_failure.borrow_mut();
                if sticky.is_none() {
                    *sticky = Some(failures.join("; "));
                }
            }
        }
        self.gate.busy.set(false);
    }
}
impl ConsumerGate {
    pub(super) fn snapshot(&self) -> (ConsumerOwner, bool) {
        (self.owner.get(), self.busy.get())
    }
    pub(super) fn is_running(&self) -> bool {
        self.owner.get() == ConsumerOwner::Running
    }
    pub(super) fn handoff(&self) -> Result<(), String> {
        if !self.is_running() {
            return Err("Download consumer already handed off".into());
        }
        self.owner.set(ConsumerOwner::Shutdown);
        Ok(())
    }
    pub(super) fn acquire(
        &self,
        owner: ConsumerOwner,
    ) -> Result<ConsumerLease, ShutdownDownloadError> {
        if self.owner.get() != owner {
            return Err(ShutdownDownloadError::Inactive);
        }
        if self.busy.replace(true) {
            return Err(ShutdownDownloadError::InFlight);
        }
        if owner == ConsumerOwner::Shutdown {
            if let Some(error) = self.handoff_failure.borrow().as_ref() {
                self.busy.set(false);
                return Err(ShutdownDownloadError::Failure(error.clone()));
            }
        }
        Ok(ConsumerLease {
            gate: self.clone(),
            failures: RefCell::new(Vec::new()),
        })
    }
}

/// Shared reducer never plans new destinations/controls after seal.
pub(super) fn ingest_shutdown<T: Copy + Eq + Hash, H: Clone>(
    router: &mut DownloadRouter<T, H>,
    events: impl IntoIterator<Item = TransferEvent<T>>,
    writer: impl FnMut(&H, UserProfileId, DownloadCompletion) -> Result<CompletedDownloadId, String>,
) -> Vec<String> {
    let (effects, mut errors) = router.reduce(events, |_, _| DestinationDecision::Defer);
    debug_assert!(effects.is_empty());
    errors.extend(router.persist(writer));
    errors
}

/// Non-Clone quit capability. Generic copied-event/writer seams let pure tests
/// exercise the SAME service API with fictional opaque tokens, never constructing
/// or reconstructing a real engine DownloadToken. Production defaults stay exact.
pub(crate) struct ShutdownDownloads<T = DownloadToken, H = ProfileHandle> {
    service: Rc<RefCell<DownloadService<T, H>>>,
    gate: ConsumerGate,
    needs_flush: Cell<bool>,
}
impl<T: Copy + Eq + Hash, H: Clone> ShutdownDownloads<T, H> {
    pub(super) fn take(
        service: Rc<RefCell<DownloadService<T, H>>>,
        gate: ConsumerGate,
    ) -> Result<Self, String> {
        gate.handoff()?;
        Ok(Self {
            service,
            gate,
            needs_flush: Cell::new(true),
        })
    }
    pub(super) fn publish_staged_with(
        &self,
        writer: impl FnMut(
            &H,
            UserProfileId,
            DownloadCompletion,
            &crate::downloads_publication::PublishedPath,
        ) -> staged::WriteOutcome,
    ) -> Result<(), ShutdownDownloadError> {
        let lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        let (_lease, result) = staged::drive(
            &self.service,
            lease,
            |_, _| Err("Shutdown cannot authorize a destination".into()),
            writer,
        );
        result.map_err(ShutdownDownloadError::Failure)
    }
    /// Fault-injection tests may detach an exact item without running the batch.
    /// Production always transfers its existing lease through staged::drive.
    #[cfg(test)]
    pub(super) fn detach_staged_work(
        &self,
        token: T,
    ) -> Result<staged::Work<T, H>, ShutdownDownloadError> {
        let lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        self.service
            .borrow()
            .staged
            .detach(token, lease)
            .map_err(ShutdownDownloadError::Failure)
    }
    fn ingest_copied(
        &self,
        events: impl IntoIterator<Item = TransferEvent<T>>,
        writer: impl FnMut(&H, UserProfileId, DownloadCompletion) -> Result<CompletedDownloadId, String>,
    ) -> Result<(), ShutdownDownloadError> {
        self.needs_flush.set(true);
        let mut service = self.service.borrow_mut();
        let (events, mut errors) = service.prepare_fixture_events(events);
        errors.extend(ingest_shutdown(&mut service.router, events, writer));
        if errors.is_empty() {
            Ok(())
        } else {
            service.errors.extend(errors.clone());
            Err(ShutdownDownloadError::Failure(errors.join("; ")))
        }
    }
    /// Supplied copied events only; this performs no CEF read/pump.
    pub(crate) fn ingest_copied_and_persist_with(
        &self,
        events: impl IntoIterator<Item = TransferEvent<T>>,
        writer: impl FnMut(&H, UserProfileId, DownloadCompletion) -> Result<CompletedDownloadId, String>,
    ) -> Result<(), ShutdownDownloadError> {
        let _lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        self.ingest_copied(events, writer)
    }
    /// One accepted outside-borrows source drain, then the same canonical ingestion.
    /// The supplied source must already support the actual engine lifecycle phase.
    pub(crate) fn drain_copied_and_persist_with(
        &self,
        drain: impl FnOnce() -> Result<Vec<TransferEvent<T>>, String>,
        writer: impl FnMut(&H, UserProfileId, DownloadCompletion) -> Result<CompletedDownloadId, String>,
    ) -> Result<(), ShutdownDownloadError> {
        let _lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        let events = drain().map_err(ShutdownDownloadError::Failure)?;
        self.ingest_copied(events, writer)
    }
    pub(crate) fn retained_handles_by(
        &self,
        same: impl Fn(&H, &H) -> bool,
    ) -> Result<Vec<H>, ShutdownDownloadError> {
        let _lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        Ok(self.service.borrow().router.unique_handles_by(same))
    }
    /// Snapshot handles, RELEASE service/model borrows, flush once per workspace.
    pub(crate) fn flush_retained_with(
        &self,
        same: impl Fn(&H, &H) -> bool,
        mut flush: impl FnMut(H) -> Result<(), String>,
    ) -> Result<(), ShutdownDownloadError> {
        let _lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        self.needs_flush.set(true);
        if self.service.borrow().has_pending_records() {
            return Err(ShutdownDownloadError::Failure(
                "Download records remain uncommitted".into(),
            ));
        }
        let handles = self.service.borrow().router.unique_handles_by(same);
        for handle in handles {
            flush(handle).map_err(ShutdownDownloadError::Failure)?;
        }
        self.needs_flush.set(false);
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<(), ShutdownDownloadError> {
        let _lease = self.gate.acquire(ConsumerOwner::Shutdown)?;
        if self.needs_flush.get() || self.service.borrow().has_pending_records() {
            return Err(ShutdownDownloadError::Failure(
                "Final download persistence/session flush is pending".into(),
            ));
        }
        self.gate.owner.set(ConsumerOwner::Finished);
        Ok(())
    }
}
impl ShutdownDownloads {
    /// Complete already-authorized publications before final writer flush.
    /// Shutdown cannot Continue a new destination. The same capability is held
    /// through publication, proof validation, canonical mutation and return.
    fn publish_staged(&self) -> Result<(), ShutdownDownloadError> {
        self.publish_staged_with(staged::record_profile)
    }
    /// Public quit ingress remains usable after the app seal; no new effect policy.
    pub(crate) fn ingest_and_persist(
        &self,
        events: Vec<DownloadEvent>,
    ) -> Result<(), ShutdownDownloadError> {
        self.ingest_copied_and_persist_with(
            events.into_iter().map(TransferEvent::from),
            |handle, profile, completion| handle.record_completed_download(profile, completion),
        )?;
        self.publish_staged()
    }
    /// Requires a SEPARATELY ACCEPTED shutdown-safe engine copied-event source.
    /// Engine::take_download_events is NOT available after Engine::begin_shutdown.
    pub(crate) fn drain_and_persist_with(
        &self,
        drain: impl FnOnce() -> Result<Vec<DownloadEvent>, String>,
    ) -> Result<(), ShutdownDownloadError> {
        self.drain_copied_and_persist_with(
            || drain().map(|events| events.into_iter().map(TransferEvent::from).collect()),
            |handle, profile, completion| handle.record_completed_download(profile, completion),
        )?;
        self.publish_staged()
    }
    pub(crate) fn retained_normal_handles(
        &self,
    ) -> Result<Vec<ProfileHandle>, ShutdownDownloadError> {
        self.retained_handles_by(ProfileHandle::same_workspace)
    }
    pub(crate) fn flush_retained(&self, cx: &mut App) -> Result<(), ShutdownDownloadError> {
        self.flush_retained_with(ProfileHandle::same_workspace, |handle| {
            handle.flush_for_quit(cx)
        })
    }
}
