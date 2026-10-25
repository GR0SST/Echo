//! App-owned download routing. Dormant until the parent accepts engine/native
//! continuation and explicitly registers owners, installs a destination policy,
//! and starts the pump. No page/window identity is consulted here.
#[path = "downloads_shutdown.rs"]
mod shutdown;
#[path = "downloads_staged.rs"]
mod staged;
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
pub(crate) use staged::Plan as StagedDestination;
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
    Staged(StagedDestination),
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
    Staging(T, StagedDestination),
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
    staged_only: bool,
    owners: HashMap<u64, Owner<H>>,
    ledger: HashMap<T, Transfer>,
}
impl<T, H> Default for DownloadRouter<T, H> {
    fn default() -> Self {
        Self {
            last_owner: 0,
            staged_only: false,
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
                    DestinationDecision::Staged(plan) => {
                        if !self.staged_only {
                            item.destination_blocked = true;
                            errors.push("Staged policy was not explicitly installed".into());
                            continue;
                        }
                        item.destination_dispatched = true;
                        effects.push(DownloadEffect::Staging(*token, plan));
                    }
                    DestinationDecision::Path(path) => {
                        if self.staged_only {
                            item.destination_blocked = true;
                            errors.push("Staged policy cannot dispatch a direct target".into());
                            continue;
                        }
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
            if self.staged_only {
                // A genuine terminal without a captured stage stays pending;
                // never persist its engine/staging path or synthesize a proof.
                errors.push("Completed download has no authorized publication".into());
                continue;
            }
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

#[derive(Clone, Debug)]
pub(crate) struct PendingDestination<T> {
    pub token: T,
    pub profile: UserProfileId,
    pub request: DestinationRequest,
}
pub(crate) type DestinationPolicy =
    Rc<dyn Fn(UserProfileId, &DestinationRequest) -> DestinationDecision>;
#[derive(Clone)]
pub(crate) struct TransferPresentation<T,H> {
    pub id: crate::downloads_view::TransferId,
    pub token:T,
    pub handle:H,
    pub profile:UserProfileId,
    pub title:String,
    pub request:DestinationRequest,
    pub info:DownloadInfo,
    pub opening:bool,
}
pub(crate) struct DownloadService<T = DownloadToken, H = ProfileHandle> {
    ui_ids: HashMap<T,crate::downloads_view::TransferId>,
    display_names: HashMap<T,String>,
    ui_open: HashSet<T>,
    ui_hidden: HashSet<T>,
    pub ui_revision:u64,
    router: DownloadRouter<T, H>,
    staged: staged::StagedTransfers<T, H>,
    policy: Option<DestinationPolicy>,
    user_cancellations: Vec<(T, UserProfileId)>,
    fixture: Option<Rc<RefCell<crate::download_acceptance::FixtureAudit<T>>>>,
    pub errors: Vec<String>,
}
impl<T, H> Default for DownloadService<T, H> {
    fn default() -> Self {
        Self {
            ui_ids:HashMap::new(), display_names:HashMap::new(), ui_open:HashSet::new(), ui_hidden:HashSet::new(), ui_revision:0,
            router: DownloadRouter::default(),
            staged: staged::StagedTransfers::default(),
            policy: None,
            user_cancellations: Vec::new(),
            fixture: None,
            errors: Vec::new(),
        }
    }
}
impl<T: Copy + Eq + Hash, H: Clone> DownloadService<T, H> {
    pub(crate) fn transfer_presentations(&self)->Vec<TransferPresentation<T,H>> {
        let mut rows:Vec<_>=self.router.ledger.iter().filter_map(|(token,item)|{
            if self.ui_hidden.contains(token){return None;}
            let id=*self.ui_ids.get(token)?;
            let request=item.destination_request.clone()?;
            let owner=self.router.owners.get(&item.provenance.owner())?;
            if owner.kind!=SourceKind::Normal{return None;}
            let info=match self.staged.presentation_info(*token) {
                Some(info)=>info?,
                None=>{if item.info.phase==DownloadPhase::Complete{return None;}item.info.clone()},
            };
            Some(TransferPresentation {id,token:*token,handle:owner.handle.clone(),profile:owner.profile,
                title:self.display_names.get(token).cloned().unwrap_or_else(||request.suggested_name.clone()),
                request,info,opening:self.ui_open.contains(token)})
        }).collect();
        rows.sort_by_key(|row|std::cmp::Reverse(row.info.start_time));rows
    }
    pub(crate) fn transfer_for_id(&self,id:crate::downloads_view::TransferId)->Option<TransferPresentation<T,H>> {
        self.transfer_presentations().into_iter().find(|row|row.id==id)
    }
    pub(crate) fn set_display_name(&mut self,token:T,profile:UserProfileId,name:String) {
        if self.router.ledger.get(&token).and_then(|i|self.router.owners.get(&i.provenance.owner())).is_some_and(|owner|owner.profile==profile){
            self.display_names.insert(token,name);self.ui_revision=self.ui_revision.wrapping_add(1);
        }
    }
    pub(crate) fn request_open(&mut self,token:T,profile:UserProfileId)->Result<(),String> {
        let item=self.router.ledger.get(&token).ok_or("Unknown transfer")?;
        if item.info.phase.is_terminal()||self.router.owners.get(&item.provenance.owner()).is_none_or(|owner|owner.profile!=profile){return Err("Transfer is terminal or belongs to another profile".into());}
        if self.staged.presentation_info(token).is_some_and(|info|info.is_none_or(|info|info.phase.is_terminal())){return Err("Transfer is terminal".into());}
        if self.ui_open.insert(token){self.ui_revision=self.ui_revision.wrapping_add(1);}Ok(())
    }
    pub(crate) fn request_hide(&mut self,token:T,profile:UserProfileId)->Result<(),String> {
        let item=self.router.ledger.get(&token).ok_or("Unknown transfer")?;
        if self.router.owners.get(&item.provenance.owner()).is_none_or(|owner|owner.profile!=profile){return Err("Transfer belongs to another profile".into());}
        self.ui_hidden.insert(token);self.ui_revision=self.ui_revision.wrapping_add(1);Ok(())
    }
    fn take_completion_intents(&mut self)->Vec<(H,UserProfileId,CompletedDownloadId,bool,bool)> {
        let tokens:HashSet<_>=self.ui_open.iter().chain(self.ui_hidden.iter()).copied().collect();
        let mut effects=Vec::new();
        for token in tokens {
            if let Some((id,_))=self.staged.completed_snapshot(token) {
                let item=&self.router.ledger[&token];let owner=&self.router.owners[&item.provenance.owner()];
                effects.push((owner.handle.clone(),owner.profile,id,self.ui_open.remove(&token),self.ui_hidden.remove(&token)));
            }
        }
        effects
    }
    pub(crate) fn pending_destinations(&self) -> Vec<PendingDestination<T>> {
        self.router.ledger.iter().filter_map(|(token, item)| {
            if item.destination_dispatched || item.destination_blocked || item.info.phase.is_terminal() { return None; }
            let request = item.destination_request.clone()?;
            let owner = self.router.owners.get(&item.provenance.owner())?;
            Some(PendingDestination { token: *token, profile: owner.profile, request })
        }).collect()
    }
    pub(crate) fn retain_chosen_staging(&mut self, token: T, profile: UserProfileId,
        stage: crate::downloads_publication::Staging<(T, DownloadProvenance)>) -> Result<(), String> {
        let item = self.router.ledger.get(&token).ok_or("Unknown download token")?;
        if self.router.owners.get(&item.provenance.owner()).is_none_or(|owner| owner.profile != profile) {
            return Err("Download belongs to another profile".into());
        }
        if item.destination_blocked { return Err("Download destination was canceled or rejected".into()); }
        self.retain_staging(token, stage)
    }
    pub(crate) fn queue_cancel(&mut self, token: T, profile: UserProfileId) -> Result<(), String> {
        self.router.control(token, profile, DownloadControl::Cancel)?;
        self.ui_open.remove(&token);self.ui_revision=self.ui_revision.wrapping_add(1);
        if let Some(item) = self.router.ledger.get_mut(&token) { item.destination_blocked = true; }
        if !self.user_cancellations.contains(&(token, profile)) { self.user_cancellations.push((token, profile)); }
        Ok(())
    }
    /// Both running and shutdown copied ingress use this pure filter.
    fn prepare_fixture_events(&mut self, events: impl IntoIterator<Item = TransferEvent<T>>) -> (Vec<TransferEvent<T>>, Vec<String>) {
        use crate::download_acceptance::FixtureVerdict;
        let mut accepted = Vec::new();
        let mut errors = Vec::new();
        for event in events {
            let verdict = self.fixture.as_ref().map(|a| a.borrow_mut().inspect(&event));
            match verdict {
                None | Some(FixtureVerdict::Forward) => accepted.push(event),
                Some(FixtureVerdict::ForwardWithFailure(error)) => { accepted.push(event); errors.push(error); },
                Some(FixtureVerdict::Quarantine(error)) => errors.push(error),
            }
        }
        // Both Running and quit pass through this exact pure filter. Staged
        // events never reach the legacy completion builder (which trusts CEF's
        // full_path); only validated PublishedPath may become a staged record.
        let mut direct = Vec::new();
        for event in accepted {
            // Staged observations are consumed here, before the direct router.
            // Their progress/terminal state still invalidates every presenter.
            self.ui_revision=self.ui_revision.wrapping_add(1);
            match self.staged.ingest(event) {
                Ok(staged_errors) => errors.extend(staged_errors),
                Err(direct_event) => direct.push(direct_event),
            }
        }
        (direct, errors)
    }
    fn has_pending_records(&self) -> bool {
        self.router.has_pending_records() || self.staged.has_pending()
    }
    pub(crate) fn attach_fixture(&mut self, audit: Rc<RefCell<crate::download_acceptance::FixtureAudit<T>>>) -> Result<(), String> {
        if self.fixture.is_some() || self.policy.is_some() || !self.router.ledger.is_empty() || !self.errors.is_empty() || self.router.owners.len() != 1 || self.staged.has_pending() {
            return Err("Fixture service is occupied".into());
        }
        {
            let a = audit.borrow();
            let owner = self.router.owners.get(&a.owner).ok_or("Fixture owner not registered")?;
            if owner.profile != a.profile || owner.kind != SourceKind::Normal { return Err("Fixture captured owner mismatch".into()); }
        }
        self.router.staged_only = audit.borrow().is_staged();
        self.fixture = Some(audit);
        Ok(())
    }
    /// Install only before the first transfer. Owners may already be captured;
    /// no admission or default folder is enabled by this explicit policy seam.
    pub(crate) fn set_staged_destination_policy(&mut self, policy: DestinationPolicy)
        -> Result<(), String>
    {
        if self.fixture.is_some() || self.policy.is_some() || !self.router.ledger.is_empty()
            || self.staged.has_pending() || !self.errors.is_empty()
        { return Err("Staged policy requires an unused download service".into()); }
        self.router.staged_only = true;
        self.policy = Some(policy);
        Ok(())
    }
    fn capture_staged_plans(&mut self, effects: Vec<DownloadEffect<T>>, direct: &mut Vec<DownloadEffect<T>>) {
        for effect in effects {
            if let DownloadEffect::Staging(token, plan) = effect {
                let transfer = &self.router.ledger[&token];
                let request = transfer.destination_request.as_ref().unwrap().clone();
                let owner = self.router.owners[&transfer.provenance.owner()].clone();
                if let Err(error) = self.staged.insert_planned(token, owner, request, plan) {
                    self.errors.push(error);
                }
            } else { direct.push(effect); }
        }
    }
    fn receive_copied(&mut self, events: impl IntoIterator<Item = TransferEvent<T>>) -> Vec<DownloadEffect<T>> {
        let (events, errors) = self.prepare_fixture_events(events);
        self.errors.extend(errors);
        let fixture = self.fixture.clone();
        let policy = self.policy.clone();
        let mut decide = |profile, request: &DestinationRequest| {
            if let Some(audit) = &fixture { audit.borrow_mut().decide(profile, request) }
            else { policy.as_ref().map_or(DestinationDecision::Defer, |p| p(profile, request)) }
        };
        let mut effects = Vec::new();
        // Capture a plan immediately after its exact Requested event, before
        // reducing later observations in this same copied batch. No I/O occurs.
        for event in events {
            let token=event.token;
            let event = match self.staged.ingest(event) {
                Ok(errors) => { self.errors.extend(errors); continue; },
                Err(event) => event,
            };
            let (next, errors) = self.router.reduce([event], &mut decide);
            self.errors.extend(errors);
            self.capture_staged_plans(next, &mut effects);
            if self.router.ledger.contains_key(&token)&&!self.ui_ids.contains_key(&token) {
                if let Some(id)=crate::downloads_view::TransferId::allocate(){self.ui_ids.insert(token,id);}
            }
        }
        let (next, errors) = self.router.reduce([], &mut decide);
        self.errors.extend(errors);
        self.capture_staged_plans(next, &mut effects);
        for (token, profile) in std::mem::take(&mut self.user_cancellations) {
            if let Ok(effect) = self.router.control(token, profile, DownloadControl::Cancel) { effects.push(effect); }
        }
        // Preserve batch coalescing for legacy effects: a later terminal must
        // suppress a previously planned Continue/Cancel before any native call.
        effects.retain(|effect| {
            let token = match effect { DownloadEffect::Destination(t, _) | DownloadEffect::Control(t, _) | DownloadEffect::Staging(t, _) => t };
            self.router.ledger.get(token).is_some_and(|transfer| !transfer.info.phase.is_terminal())
        });
        effects
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
    pub(crate) fn register_distinct_source(&mut self, source: &ProfileHandle, captured: UserProfileId) -> Result<u64,String> {
        let (handle, profile, blank) = source.download_normal_owner(captured).ok_or("No normal download owner")?;
        if blank { return Err("Blank download admission remains unavailable".into()); }
        self.router.register_normal(profile, handle, SourceKind::Normal)
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
        self.receive_copied(events.into_iter().map(TransferEvent::from))
    }
    pub(crate) fn fixture_status(&self) -> Option<crate::download_acceptance::FixtureTransferStatus> {
        use crate::download_acceptance::FixtureTransferStatus;
        let a = self.fixture.as_ref()?.borrow();
        let transfer = a.token.and_then(|token| self.router.ledger.get(&token));
        let staged_snapshot = if a.is_staged() {
            a.token.and_then(|token| self.staged.completed_snapshot(token))
        } else { None };
        let canonical_id = staged_snapshot.as_ref().map(|(id, _)| *id)
            .or_else(|| transfer.and_then(|t| t.committed));
        let inventory_match = canonical_id.and_then(|id| {
            let owner = self.router.owners.get(&a.owner)?;
            let (_, records) = owner.handle.download_inventory()?;
            let info = staged_snapshot.as_ref().map(|(_, info)| info)
                .or_else(|| transfer.map(|t| &t.info))?;
            let final_path = a.published_path.as_deref().unwrap_or(&a.config.output);
            records.iter().find(|r| r.id() == id).map(|r| {
                id.profile() == a.profile && r.final_path().as_os_str() == final_path.as_os_str()
                    && r.source_url() == a.config.fixture_url() && r.original_url() == a.config.fixture_url()
                    && r.mime_type() == info.mime_type && r.finished_at() == DownloadTime::from_cef_basetime(info.end_time)
            })
        }).unwrap_or(false);
        Some(FixtureTransferStatus { captured_profile: a.profile, owner: a.owner, sequence: a.sequence, provenance: a.provenance, phase: a.phase, received_bytes: a.received_bytes,
            requested_name_match: a.requested_name_match, resolved_path_match: a.resolved_path_match,
            canonical_id, terminal_updated_seen: a.terminal_updated_seen, failure: a.failure.clone(), service_errors: self.errors.len(),
            effect_ok: a.effect_ok, inventory_match, output: a.published_path.clone().unwrap_or_else(|| a.config.output.clone()),
            staged: a.is_staged(), transfer_path: a.stage_path.clone(), published_path: a.published_path.clone(),
            collision_preserved: a.collision_preserved,
            collision_required: a.mode == crate::download_acceptance::FixtureMode::StagedCollision,
            source: None, admission: None, consumer: None })
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
                DownloadEffect::Staging(token, _) => (token,
                    Err("Staging reservation requires the service consumer lease".into())),
                DownloadEffect::Control(token, command) => (token, control(token, command)),
            };
            result.err().map(|error| (token, error))
        })
        .collect()
}

/// Copied observations from one turn of the sole running consumer. Quiet fixture
/// startup/context Pending and gaps between callbacks are still unfinished work.
#[derive(Default)]
struct RunningTurn {
    drained_empty: bool,
    effects_empty: bool,
    records_pending: bool,
    retirement_pending: bool,
    engine_pending: bool,
}
impl<T: Copy + Eq + Hash, H: Clone> DownloadService<T, H> {
    fn fixture_unfinished(&self) -> bool {
        self.errors.is_empty() && self.fixture.as_ref().is_some_and(|audit| {
            let audit = audit.borrow();
            audit.failure.is_none() && !audit.phase.is_some_and(|phase| phase.is_terminal())
                && audit.admission.as_ref().is_none_or(|d| d.borrow().phase != crate::window::download_probe::admission::Phase::Failed)
        })
    }
    fn fixture_failed(&self) -> bool {
        self.fixture.as_ref().is_some_and(|audit| {
            let audit = audit.borrow();
            !self.errors.is_empty() || audit.failure.is_some()
                || audit.admission.as_ref().is_some_and(|d| d.borrow().phase == crate::window::download_probe::admission::Phase::Failed)
        })
    }
    fn continue_running(&self, turn: RunningTurn) -> bool {
        // Fail closed rather than automatically retrying genuine fixture failures.
        if self.fixture_failed() { return false; }
        let fixture_unfinished = self.fixture_unfinished();
        fixture_unfinished || !turn.drained_empty || !turn.effects_empty
            || turn.records_pending || turn.retirement_pending || turn.engine_pending
    }
}

fn continue_running<T: Copy + Eq + Hash, H: Clone>(gate: &ConsumerGate, quitting: bool, service: &DownloadService<T, H>, turn: RunningTurn) -> bool {
    !quitting && gate.is_running() && service.continue_running(turn)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsumerStatus {
    pub owner: shutdown::ConsumerOwner,
    pub busy: bool,
    pub scheduled: bool,
    pub running: bool,
    pub legacy_consumer_excluded: bool,
}

/// Replace only the native boundary in task-loop tests. Scheduling, the pump
/// predicate, lease, reducer, persistence, continuation and timer remain real.
#[cfg(test)]
trait RunningEngineDriver {
    fn download_work_pending(&self) -> bool;
    fn pump(&self);
    fn take_download_events(&self) -> Result<Vec<DownloadEvent>, String>;
    fn prepare_download_retirements(&self) -> Result<engine::DownloadRetirementPreparation, String>;
}

/// Retain as ONE app global, not in BrowserWindow or Pages. Starting is explicit;
/// no owner, admission, destination or UI route is enabled by construction.
#[derive(Clone, Default)]
pub(crate) struct AppDownloads {
    pub service: Rc<RefCell<DownloadService>>,
    running: Rc<Cell<bool>>,
    task_started: Rc<Cell<bool>>,
    consumer: ConsumerGate,
    retirement_failed: Rc<Cell<bool>>,
    #[cfg(test)]
    test_driver: Option<Rc<dyn RunningEngineDriver>>,
}
impl Global for AppDownloads {}
#[cfg(test)]
struct IdleDriver;
#[cfg(test)]
impl RunningEngineDriver for IdleDriver {
    fn download_work_pending(&self)->bool { false }
    fn pump(&self) {}
    fn take_download_events(&self)->Result<Vec<DownloadEvent>,String> { Ok(Vec::new()) }
    fn prepare_download_retirements(&self)->Result<engine::DownloadRetirementPreparation,String> { Ok(engine::DownloadRetirementPreparation::Ready) }
}
impl AppDownloads {
    #[cfg(test)]
    pub(crate) fn idle_for_tests()->Self { Self { test_driver: Some(Rc::new(IdleDriver)), ..Self::default() } }
    fn download_work_pending(&self) -> bool {
        #[cfg(test)]
        if let Some(driver) = &self.test_driver { return driver.download_work_pending(); }
        Engine::download_work_pending()
    }
    fn pump(&self) {
        #[cfg(test)]
        if let Some(driver) = &self.test_driver { return driver.pump(); }
        Engine::pump();
    }
    fn take_download_events(&self) -> Result<Vec<DownloadEvent>, String> {
        #[cfg(test)]
        if let Some(driver) = &self.test_driver { return driver.take_download_events(); }
        Engine::take_download_events()
    }
    fn prepare_download_retirements(&self) -> Result<engine::DownloadRetirementPreparation, String> {
        #[cfg(test)]
        if let Some(driver) = &self.test_driver { return driver.prepare_download_retirements(); }
        Engine::prepare_download_retirements()
    }
    pub(crate) fn consumer_status(&self, cx: &App) -> ConsumerStatus {
        let (owner, busy) = self.consumer.snapshot();
        ConsumerStatus {
            owner, busy, scheduled: self.running.get() && !self.task_started.get(),
            running: self.task_started.get(),
            // This is the exact installed-global guard used by the legacy loop
            // before and after pumping; never call probe init/can_arm here.
            legacy_consumer_excluded: cx.try_global::<Self>().is_some_and(|installed| Rc::ptr_eq(&installed.service, &self.service)),
        }
    }
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
            struct Running(Rc<Cell<bool>>, Rc<Cell<bool>>);
            impl Drop for Running {
                fn drop(&mut self) {
                    self.0.set(false);
                    self.1.set(false);
                }
            }
            let _running = Running(this.running.clone(), this.task_started.clone());
            this.task_started.set(true);
            loop {
                // Each check and CEF operation is outside App/model/engine borrows.
                // Quit owns CEF from this point; it must perform its final drain
                // and normal writer flush under its own shutdown ordering.
                if crate::quit::is_quitting() || !this.consumer.is_running() {
                    break;
                }
                if this.retirement_failed.get() || this.service.borrow().fixture_failed() {
                    break;
                }
                // The engine hint can be quiet before the first callback. Keep
                // progressing the armed run on this same awaited consumer task.
                let fixture_unfinished = { this.service.borrow().fixture_unfinished() };
                if this.download_work_pending() || fixture_unfinished {
                    this.pump();
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
                let events = match this.take_download_events() {
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
                        let audit = { this.service.borrow().fixture.clone() };
                        let check = if let Some(audit) = &audit {
                            let reservation = { audit.borrow().effect_check(token, &path) };
                            reservation.and_then(|output| output.validate_target_absent(&path))
                        } else { Ok(()) };
                        // Filesystem validation ran with no App/service/audit borrow.
                        if crate::quit::is_quitting() || !this.consumer.is_running() { return Ok(()); }
                        let result = check.and_then(|()| Engine::resolve_download_destination(token, path));
                        if let Some(audit) = audit {
                            let mut a = audit.borrow_mut();
                            a.effect_ok = result.is_ok();
                            if let Err(error) = &result { a.fail(error); }
                        }
                        result
                    },
                    |token, command| {
                        if crate::quit::is_quitting() || !this.consumer.is_running() {
                            return Ok(()); // Intentional suppression is not a native failure.
                        }
                        if command == DownloadControl::Cancel { Engine::cancel_download_if_active(token).map(|_| ()) }
                        else { Engine::control_download(token, command) }
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
                // Transfer THIS batch's capability into staged Work and back.
                // Reentrant quit observes InFlight until effects/state/errors return.
                let (lease, staged_result) = staged::drive(
                    &this.service, lease, Engine::resolve_download_destination,
                    staged::record_profile,
                );
                if staged_result.is_err() { break; }
                // Progress and cancellation do not mutate the canonical profile.
                // Redraw their presenters after reducing this copied batch rather
                // than waiting for unrelated page input or inventory changes.
                if !drained_empty { let _=cx.update(|cx|cx.refresh_windows()); }
                let intents={this.service.borrow_mut().take_completion_intents()};
                for (handle,profile,id,open,hide) in intents {
                    if crate::quit::is_quitting(){break;}
                    let path=handle.download_inventory().and_then(|(_,rows)|rows.into_iter().find(|r|r.id()==id&&r.id().profile()==profile).map(|r|r.final_path().to_owned()));
                    if hide {handle.hide_completed_download(id);}
                    if open {if let Some(path)=path.filter(|p|p.is_file()){let _=cx.update(|cx|cx.open_with_system(&path));}}
                }
                // Native parking must not run under App/service/model/audit borrows.
                // Keep THIS batch's lease until any reentrant quit sees its errors.
                if crate::quit::is_quitting() || !this.consumer.is_running() { break; }
                if this.retirement_failed.get() { break; }
                let retirement = this.prepare_download_retirements();
                if let Err(error) = &retirement {
                    this.retirement_failed.set(true);
                    lease.record_errors([error.clone()]);
                    this.service.borrow_mut().errors.push(error.clone());
                }
                if crate::quit::is_quitting() || !this.consumer.is_running() { break; }
                if retirement.is_err() { break; }
                let retirement_pending = retirement == Ok(engine::DownloadRetirementPreparation::Pending);
                drop(lease); // No old batch/effect can overtake the final drain/flush.
                if saved.is_err() {
                    break;
                } // Retain pending metadata; explicit wake retries.
                let engine_pending = this.download_work_pending();
                let keep_running = {
                    let service = this.service.borrow();
                    continue_running(&this.consumer, crate::quit::is_quitting(), &service, RunningTurn {
                        drained_empty, effects_empty, retirement_pending, engine_pending,
                        records_pending: service.has_pending_records(),
                    })
                };
                if !keep_running { break; }
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
