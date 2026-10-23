//! App-owned download routing. Dormant until the parent accepts engine/native
//! continuation and explicitly registers owners, installs a destination policy,
//! and starts the pump. No page/window identity is consulted here.
#[path = "downloads_shutdown.rs"]
mod shutdown;
use crate::profile::ProfileHandle;
use browser_core::{
    UserProfileId,
    downloads::{CompletedDownload, CompletedDownloadId, DownloadCompletion, DownloadTime},
};
use engine::{
    DownloadControl, DownloadDestination, DownloadEvent, DownloadInfo, DownloadPhase,
    DownloadProvenance, DownloadToken, Engine,
};
use gpui::{App, Global};
use shutdown::{ConsumerGate, ConsumerOwner};
pub(crate) use shutdown::{ShutdownDownloadError, ShutdownDownloads};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    hash::Hash,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Normal,
    Blank,
}
#[derive(Clone)]
struct Owner<H> {
    profile: UserProfileId,
    handle: H,
    kind: SourceKind,
}
#[derive(Clone, Debug)]
pub(crate) enum DestinationDecision {
    Defer,
    Path(PathBuf),
    Reject,
}
/// Frozen at DestinationRequested, independently of later mutable item updates.
/// The callback argument is authoritative even when info's name is empty or different.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DestinationRequest {
    pub provenance: DownloadProvenance,
    pub suggested_name: String,
    pub info: DownloadInfo,
}
#[derive(Clone, Debug)]
pub(crate) enum DownloadEffect<T> {
    Destination(T, PathBuf),
    Control(T, DownloadControl),
}
#[derive(Clone, Debug)]
pub(crate) struct TransferEvent<T> {
    pub token: T,
    pub provenance: DownloadProvenance,
    pub info: DownloadInfo,
    pub destination_requested: bool,
    pub destination: DownloadDestination,
}
impl From<DownloadEvent> for TransferEvent<DownloadToken> {
    fn from(event: DownloadEvent) -> Self {
        let (snapshot, destination_requested) = match event {
            DownloadEvent::DestinationRequested(snapshot) => (snapshot, true),
            DownloadEvent::Updated(snapshot) => (snapshot, false),
        };
        Self::from_snapshot_parts(
            snapshot.token,
            snapshot.provenance,
            snapshot.info,
            snapshot.destination,
            destination_requested,
        )
    }
}
impl<T> TransferEvent<T> {
    // Shared production conversion seam; tests use fake keys, never forge engine tokens.
    fn from_snapshot_parts(
        token: T,
        provenance: DownloadProvenance,
        info: DownloadInfo,
        destination: DownloadDestination,
        destination_requested: bool,
    ) -> Self {
        Self {
            token,
            provenance,
            info,
            destination_requested,
            destination,
        }
    }
}
struct Transfer {
    provenance: DownloadProvenance,
    info: DownloadInfo,
    completion: Option<DownloadCompletion>,
    committed: Option<CompletedDownloadId>,
    destination_dispatched: bool,
    destination_request: Option<DestinationRequest>,
    destination_blocked: bool,
}
/// Process tokens stay intact; never convert a token to a persisted ID. This
/// ledger outlives the source/retired host and retains terminal tombstones.
pub(crate) struct DownloadRouter<T, H> {
    last_owner: u64,
    owners: HashMap<u64, Owner<H>>,
    ledger: HashMap<T, Transfer>,
}
impl<T, H> Default for DownloadRouter<T, H> {
    fn default() -> Self {
        Self {
            last_owner: 0,
            owners: HashMap::new(),
            ledger: HashMap::new(),
        }
    }
}
impl<T: Copy + Eq + Hash, H: Clone> DownloadRouter<T, H> {
    /// No private owner API. The production caller validates a retained canonical
    /// normal handle first; engine receives this opaque key, never UserProfileId.
    pub(crate) fn register_normal(
        &mut self,
        profile: UserProfileId,
        handle: H,
        kind: SourceKind,
    ) -> Result<u64, String> {
        let key = self
            .last_owner
            .checked_add(1)
            .ok_or("Download owner identity exhausted")?;
        self.owners.insert(
            key,
            Owner {
                profile,
                handle,
                kind,
            },
        );
        self.last_owner = key;
        Ok(key)
    }
    pub(crate) fn reduce(
        &mut self,
        events: impl IntoIterator<Item = TransferEvent<T>>,
        mut policy: impl FnMut(UserProfileId, &DestinationRequest) -> DestinationDecision,
    ) -> (Vec<DownloadEffect<T>>, Vec<String>) {
        let mut effects = Vec::new();
        let mut errors = Vec::new();
        for event in events {
            // Even a malicious collision with a normal key cannot adopt private data.
            if event.provenance.is_private() {
                errors.push("Private download remains unadopted".into());
                continue;
            }
            let Some(_owner) = self.owners.get(&event.provenance.owner()) else {
                errors.push("Unregistered download owner".into());
                continue;
            };
            // Only the matching event variant can authorize a callback request.
            // Invalid request metadata must not reserve a token or terminal tombstone.
            if event.destination_requested
                && !matches!(&event.destination, DownloadDestination::Requested { .. })
            {
                errors.push("Download request lacks requested destination metadata".into());
                continue;
            }
            let item = self.ledger.entry(event.token).or_insert_with(|| Transfer {
                provenance: event.provenance,
                info: event.info.clone(),
                completion: None,
                committed: None,
                destination_dispatched: false,
                destination_request: None,
                destination_blocked: false,
            });
            if item.provenance != event.provenance {
                errors.push("Download provenance changed".into());
                continue;
            }
            if item.info.phase.is_terminal()
                && (item.completion.is_some() || item.info.phase == DownloadPhase::Canceled)
            {
                continue;
            }
            if let DownloadDestination::Requested { suggested_name } = &event.destination {
                if let Some(request) = &item.destination_request {
                    if request.suggested_name != *suggested_name {
                        errors.push("Download requested destination changed".into());
                        // Reject spoofed request/progress data, but retain genuine
                        // terminal Updated metadata under the exact bound provenance.
                        if event.destination_requested || !event.info.phase.is_terminal() {
                            continue;
                        }
                    }
                } else if event.destination_requested {
                    item.destination_request = Some(DestinationRequest {
                        provenance: event.provenance,
                        suggested_name: suggested_name.clone(),
                        info: event.info.clone(),
                    });
                }
            }
            item.info = event.info;
            // Only a real Updated(Complete), never byte counts/destination/file size.
            if !event.destination_requested
                && item.info.phase == DownloadPhase::Complete
                && item.completion.is_none()
            {
                item.completion = Some(DownloadCompletion {
                    final_path: item.info.full_path.clone(),
                    source_url: item.info.url.clone(),
                    original_url: item.info.original_url.clone(),
                    mime_type: item.info.mime_type.clone(),
                    finished_at: DownloadTime::from_cef_basetime(item.info.end_time),
                });
            }
        }
        for (token, item) in &mut self.ledger {
            if item.destination_request.is_some()
                && !item.destination_dispatched
                && !item.destination_blocked
                && !item.info.phase.is_terminal()
            {
                let owner = &self.owners[&item.provenance.owner()];
                match policy(owner.profile, item.destination_request.as_ref().unwrap()) {
                    DestinationDecision::Defer => (),
                    DestinationDecision::Path(path) => {
                        item.destination_dispatched = true;
                        effects.push(DownloadEffect::Destination(*token, path));
                    }
                    DestinationDecision::Reject => {
                        item.destination_dispatched = true;
                        effects.push(DownloadEffect::Control(*token, DownloadControl::Cancel));
                    }
                }
            }
        }
        (effects, errors)
    }
    /// Snapshot work before invoking a canonical writer. Failure leaves the same
    /// item pending; success acknowledges exactly once for this process token.
    pub(crate) fn persist(
        &mut self,
        mut writer: impl FnMut(
            &H,
            UserProfileId,
            DownloadCompletion,
        ) -> Result<CompletedDownloadId, String>,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for transfer in self.ledger.values_mut().filter(|t| t.committed.is_none()) {
            let Some(completion) = transfer.completion.clone() else {
                continue;
            };
            let owner = &self.owners[&transfer.provenance.owner()];
            match writer(&owner.handle, owner.profile, completion) {
                Ok(id) if id.profile() == owner.profile => transfer.committed = Some(id),
                Ok(_) => errors.push("Canonical download writer returned another profile".into()),
                Err(error) => errors.push(error),
            }
        }
        errors
    }
    pub(crate) fn has_pending_records(&self) -> bool {
        self.ledger
            .values()
            .any(|t| t.completion.is_some() && t.committed.is_none())
    }
    pub(crate) fn control(
        &self,
        token: T,
        profile: UserProfileId,
        command: DownloadControl,
    ) -> Result<DownloadEffect<T>, String> {
        let transfer = self.ledger.get(&token).ok_or("Unknown download token")?;
        if self.owners[&transfer.provenance.owner()].profile != profile {
            return Err("Download belongs to another profile".into());
        }
        if transfer.info.phase.is_terminal() {
            return Err("Download is terminal".into());
        }
        Ok(DownloadEffect::Control(token, command))
    }
    /// Failure consumes this authorization. Polls/new callbacks cannot retry it.
    pub(crate) fn destination_failed(&mut self, token: T) {
        if let Some(transfer) = self.ledger.get_mut(&token) {
            transfer.destination_blocked = true;
        }
    }
    pub(crate) fn retry_destination(
        &mut self,
        token: T,
        profile: UserProfileId,
    ) -> Result<(), String> {
        let transfer = self
            .ledger
            .get_mut(&token)
            .ok_or("Unknown download token")?;
        if self.owners[&transfer.provenance.owner()].profile != profile {
            return Err("Download belongs to another profile".into());
        }
        if !transfer.destination_blocked || transfer.info.phase.is_terminal() {
            return Err("No failed destination attempt to retry".into());
        }
        transfer.destination_blocked = false;
        transfer.destination_dispatched = false;
        Ok(())
    }
    pub(crate) fn unique_handles_by(&self, same: impl Fn(&H, &H) -> bool) -> Vec<H> {
        let mut handles = Vec::new();
        for owner in self.owners.values() {
            if !handles.iter().any(|handle| same(handle, &owner.handle)) {
                handles.push(owner.handle.clone());
            }
        }
        handles
    }
}

pub(crate) type DestinationPolicy =
    Rc<dyn Fn(UserProfileId, &DestinationRequest) -> DestinationDecision>;
pub(crate) struct DownloadService<T = DownloadToken, H = ProfileHandle> {
    router: DownloadRouter<T, H>,
    policy: Option<DestinationPolicy>,
    pub errors: Vec<String>,
}
impl<T, H> Default for DownloadService<T, H> {
    fn default() -> Self {
        Self {
            router: DownloadRouter::default(),
            policy: None,
            errors: Vec::new(),
        }
    }
}
impl DownloadService {
    /// Pass the captured source ID before changing any Space/profile. Blank
    /// resolves ONLY its retained source; a private handle is always rejected.
    pub(crate) fn register_source(
        &mut self,
        source: &ProfileHandle,
        captured: UserProfileId,
    ) -> Result<u64, String> {
        let (handle, profile, blank) = source
            .download_normal_owner(captured)
            .ok_or("No canonical normal download owner")?;
        let kind = if blank {
            SourceKind::Blank
        } else {
            SourceKind::Normal
        };
        if let Some((key, _)) = self.router.owners.iter().find(|(_, owner)| {
            owner.profile == profile && owner.kind == kind && owner.handle.same_workspace(&handle)
        }) {
            return Ok(*key);
        }
        self.router.register_normal(profile, handle, kind)
    }
    pub(crate) fn retry_destination(
        &mut self,
        token: DownloadToken,
        profile: UserProfileId,
    ) -> Result<(), String> {
        self.router.retry_destination(token, profile)
    }
    pub(crate) fn set_destination_policy(&mut self, policy: DestinationPolicy) {
        self.policy = Some(policy);
    }
    fn receive(&mut self, events: Vec<DownloadEvent>) -> Vec<DownloadEffect<DownloadToken>> {
        let policy = self.policy.clone();
        let (effects, errors) = self.router.reduce(
            events.into_iter().map(TransferEvent::from),
            |profile, request| {
                policy
                    .as_ref()
                    .map_or(DestinationDecision::Defer, |p| p(profile, request))
            },
        );
        self.errors.extend(errors);
        effects
    }
    fn persist(&mut self) -> Result<(), String> {
        let errors = self.router.persist(|handle, profile, completion| {
            handle.record_completed_download(profile, completion)
        });
        if errors.is_empty() {
            Ok(())
        } else {
            self.errors.extend(errors.clone());
            Err(errors.join("; "))
        }
    }
}

/// Effects are detached values; execute only after dropping the service borrow
/// AND leaving cx.update. No GPUI App reference can be passed to this function.
pub(crate) fn execute_effects<T: Copy>(
    effects: Vec<DownloadEffect<T>>,
    mut destination: impl FnMut(T, PathBuf) -> Result<(), String>,
    mut control: impl FnMut(T, DownloadControl) -> Result<(), String>,
) -> Vec<(T, String)> {
    effects
        .into_iter()
        .filter_map(|effect| {
            let (token, result) = match effect {
                DownloadEffect::Destination(token, path) => (token, destination(token, path)),
                DownloadEffect::Control(token, command) => (token, control(token, command)),
            };
            result.err().map(|error| (token, error))
        })
        .collect()
}

/// Retain as ONE app global, not in BrowserWindow or Pages. Starting is explicit;
/// no owner, admission, destination or UI route is enabled by construction.
#[derive(Clone, Default)]
pub(crate) struct AppDownloads {
    pub service: Rc<RefCell<DownloadService>>,
    running: Rc<Cell<bool>>,
    consumer: ConsumerGate,
}
impl Global for AppDownloads {}
impl AppDownloads {
    /// Transfers the sole event-consumer capability to quit, even after the app
    /// seal. Existing in-flight ingestion must finish before shutdown can drain.
    pub(crate) fn begin_shutdown(&self) -> Result<ShutdownDownloads, String> {
        ShutdownDownloads::take(self.service.clone(), self.consumer.clone())
    }
    /// Call after parent admission or retirement queues work, including after
    /// removing the last BrowserWindow. Duplicate wake calls do not add pumps.
    pub(crate) fn wake(&self, cx: &mut App) {
        if !self.consumer.is_running() || self.running.replace(true) {
            return;
        }
        let this = self.clone();
        cx.spawn(async move |cx| {
            struct Running(Rc<Cell<bool>>);
            impl Drop for Running {
                fn drop(&mut self) {
                    self.0.set(false);
                }
            }
            let _running = Running(this.running.clone());
            loop {
                // Each check and CEF operation is outside App/model/engine borrows.
                // Quit owns CEF from this point; it must perform its final drain
                // and normal writer flush under its own shutdown ordering.
                if crate::quit::is_quitting() || !this.consumer.is_running() {
                    break;
                }
                if Engine::download_work_pending() {
                    Engine::pump();
                }
                // Always drain once on wake and once before sleeping the task:
                // a terminal event can outlive engine retirement work.
                if crate::quit::is_quitting() || !this.consumer.is_running() {
                    break;
                }
                let lease = match this.consumer.acquire(ConsumerOwner::Running) {
                    Ok(lease) => lease,
                    Err(_) => break,
                };
                let events = match Engine::take_download_events() {
                    Ok(events) => events,
                    Err(error) => {
                        lease.record_errors([error.clone()]);
                        this.service.borrow_mut().errors.push(error);
                        break;
                    }
                };
                let drained_empty = events.is_empty();
                // An already-drained batch MUST be ingested even if native quit
                // sealed/reclaimed the consumer during take_download_events.
                // No App update is needed: canonical writes notify asynchronously.
                let (effects, saved) = {
                    let mut service = this.service.borrow_mut();
                    lease.with_service_errors(&mut service, |service| {
                        let effects = service.receive(events);
                        let saved = service.persist();
                        (effects, saved)
                    })
                };
                if crate::quit::is_quitting() || !this.consumer.is_running() {
                    break;
                }
                let effects_empty = effects.is_empty();
                let failures = execute_effects(
                    effects,
                    |token, path| {
                        if crate::quit::is_quitting() || !this.consumer.is_running() {
                            return Ok(()); // Intentional suppression is not a native failure.
                        }
                        Engine::resolve_download_destination(token, path)
                    },
                    |token, command| {
                        if crate::quit::is_quitting() || !this.consumer.is_running() {
                            return Ok(()); // Intentional suppression is not a native failure.
                        }
                        Engine::control_download(token, command)
                    },
                );
                {
                    let mut service = this.service.borrow_mut();
                    for (token, error) in failures {
                        lease.record_errors([error.clone()]);
                        service.router.destination_failed(token);
                        service.errors.push(error);
                    }
                }
                drop(lease); // No old batch/effect can overtake the final drain/flush.
                if saved.is_err() {
                    break;
                } // Retain pending metadata; explicit wake retries.
                let records_pending = this.service.borrow().router.has_pending_records();
                if drained_empty
                    && effects_empty
                    && !records_pending
                    && !Engine::download_work_pending()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
            }
        })
        .detach();
    }
}

/// Observed for ASCII fixtures: literal filename substring, ASCII case folding,
/// no trimming/tokenization and no URL, hostname, MIME or subtitle matching.
/// Non-ASCII bytes remain exact; Arc's Unicode/diacritic behavior is unverified.
pub(crate) fn filename_matches_ascii(filename: &str, query: &str) -> bool {
    query.is_empty()
        || filename
            .as_bytes()
            .windows(query.len())
            .any(|part| part.eq_ignore_ascii_case(query.as_bytes()))
}

/// Apply native-observed filename search without choosing Profile-selection,
/// date grouping, order, subtitle or preview policy. Query is borrowed verbatim;
/// Filter/Done belongs to the presenter and must retain the same query.
/// Requires the parent's DownloadsList::SearchEmpty { query: String } addition.
pub(crate) fn project_completed_search(
    normal: &ProfileHandle,
    query: &str,
    selected: impl Fn(UserProfileId) -> bool,
    include_profile: impl Fn(UserProfileId) -> bool,
    build: impl FnOnce(
        Vec<CompletedDownload>,
    ) -> crate::downloads_view::DownloadsList<CompletedDownloadId>,
) -> Option<crate::downloads_view::DownloadsProjection<CompletedDownloadId>> {
    let (profiles, records) = normal.download_inventory()?;
    let records: Vec<_> = records
        .into_iter()
        .filter(|record| {
            include_profile(record.id().profile())
                && filename_matches_ascii(&record.final_name().to_string_lossy(), query)
        })
        .collect();
    if !query.is_empty() && records.is_empty() {
        return project_owned_completed(profiles, records, selected, |_| {
            crate::downloads_view::DownloadsList::SearchEmpty {
                query: query.to_owned(),
            }
        });
    }
    project_owned_completed(profiles, records, selected, build)
}

/// Host-owned date headings, order, subtitles and Profile selection remain injectable.
/// Only real persisted records and actual normal-profile metadata enter this seam.
pub(crate) fn project_completed(
    normal: &ProfileHandle,
    selected: impl Fn(UserProfileId) -> bool,
    build: impl FnOnce(
        Vec<CompletedDownload>,
    ) -> crate::downloads_view::DownloadsList<CompletedDownloadId>,
) -> Option<crate::downloads_view::DownloadsProjection<CompletedDownloadId>> {
    let (profiles, records) = normal.download_inventory()?;
    project_owned_completed(profiles, records, selected, build)
}

fn project_owned_completed(
    profiles: Vec<(UserProfileId, String)>,
    records: Vec<CompletedDownload>,
    selected: impl Fn(UserProfileId) -> bool,
    build: impl FnOnce(
        Vec<CompletedDownload>,
    ) -> crate::downloads_view::DownloadsList<CompletedDownloadId>,
) -> Option<crate::downloads_view::DownloadsProjection<CompletedDownloadId>> {
    let actual: HashMap<_, _> = records
        .iter()
        .map(|record| {
            (
                record.id(),
                record.final_name().to_string_lossy().into_owned(),
            )
        })
        .collect();
    let mut list = build(records);
    if let crate::downloads_view::DownloadsList::Completed(groups) = &mut list {
        let mut seen = HashSet::new();
        for row in groups.iter_mut().flat_map(|group| &mut group.rows) {
            let title = actual.get(&row.token)?;
            if row.profile != row.token.profile() || !seen.insert(row.token) {
                return None;
            }
            row.title = title.clone();
        }
    }
    Some(crate::downloads_view::DownloadsProjection {
        profiles: profiles
            .into_iter()
            .map(|(id, name)| crate::downloads_view::DownloadsProfile {
                id,
                name,
                selected: selected(id),
            })
            .collect(),
        list,
    })
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
