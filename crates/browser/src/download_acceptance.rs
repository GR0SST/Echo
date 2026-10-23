//! Explicit disposable automation fixture. No consumer, native calls or writer.
//! This is a same-user TMP check, not an adversarial no-overwrite guarantee.
use crate::{
    downloads::{DestinationDecision, DestinationRequest, TransferEvent},
    window::download_probe::{self, Config, admission},
};
use browser_core::{UserProfileId, downloads::CompletedDownloadId};
use engine::{DownloadDestination, DownloadPhase, DownloadProvenance};
use gpui::{App, Global};
use std::{
    cell::RefCell,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    created: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}
impl FileIdentity {
    fn read(path: &Path, directory: bool) -> Result<Self, String> {
        let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if m.file_type().is_symlink() || (if directory { !m.is_dir() } else { !m.is_file() }) {
            return Err("Fixture reservation type changed".into());
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            // Unsupported creation identity fails closed. No mtime fallback.
            created: m.created().map_err(|e| e.to_string())?,
            #[cfg(unix)]
            device: m.dev(),
            #[cfg(unix)]
            inode: m.ino(),
        })
    }
}

pub(crate) struct FixtureOutput {
    root: PathBuf,
    directory: PathBuf,
    directory_identity: FileIdentity,
    marker_identity: FileIdentity,
    nonce: String,
}
impl FixtureOutput {
    pub(crate) fn reserve_in_tmp() -> Result<Self, String> {
        Self::reserve_under(&std::env::temp_dir())
    }
    // Private implementation seam: callers cannot supply arbitrary automation paths.
    fn reserve_under(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        for _ in 0..32 {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let nonce = format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let directory = root.join(format!("echo-app-download-{nonce}"));
            match fs::create_dir(&directory) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
            let marker = directory.join(".echo-owner");
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)
                .map_err(|e| e.to_string())?;
            file.write_all(nonce.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            return Ok(Self {
                directory_identity: FileIdentity::read(&directory, true)?,
                marker_identity: FileIdentity::read(&marker, false)?,
                root,
                directory,
                nonce,
            });
        }
        Err("Cannot reserve fresh fixture TMP directory".into())
    }
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
    pub(crate) fn validate_directory(&self) -> Result<(), String> {
        if self.directory.parent() != Some(self.root.as_path()) {
            return Err("Fixture target escaped reservation".into());
        }
        if FileIdentity::read(&self.directory, true)? != self.directory_identity
            || self.directory.canonicalize().map_err(|e| e.to_string())? != self.directory
        {
            return Err("Fixture directory replaced".into());
        }
        let marker = self.directory.join(".echo-owner");
        if FileIdentity::read(&marker, false)? != self.marker_identity {
            return Err("Fixture marker replaced".into());
        }
        let mut contents = String::new();
        fs::File::open(&marker)
            .map_err(|e| e.to_string())?
            .take(256)
            .read_to_string(&mut contents)
            .map_err(|e| e.to_string())?;
        if contents != self.nonce {
            return Err("Fixture marker changed".into());
        }
        Ok(())
    }
    pub(crate) fn validate_target_absent(&self, path: &Path) -> Result<(), String> {
        if self.directory.parent() != Some(self.root.as_path())
            || path.parent() != Some(self.directory.as_path())
            || path.file_name().is_none()
        {
            return Err("Fixture target escaped reservation".into());
        }
        self.validate_directory()?;
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err("Fixture target already exists".into()),
            Err(e) => Err(format!("Fixture target absence unknown: {e}")),
        }
    }
}

/// Construct outside any App/entity/service borrow, before the window transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FixtureMode { Direct, Staged, StagedCollision }
impl FixtureMode { fn staged(self) -> bool { self != Self::Direct } }
const COLLISION_BYTES: &[u8] = b"Echo owned collision sentinel\n";
struct CollisionWitness { identity: FileIdentity, path: PathBuf }

pub(crate) struct PreparedArm {
    pub config: Config,
    pub output: Rc<FixtureOutput>,
    mode: FixtureMode,
    collision: Option<CollisionWitness>,
}
impl PreparedArm {
    pub(crate) fn parse(arg: &str) -> Result<Self, String> {
        let args: Vec<_> = arg.split_whitespace().collect();
        if args.len() != 2 {
            return Err("Expected numeric-loopback-origin fixture-mode".into());
        }
        let mode = match args[1] {
            "continue-normal" => FixtureMode::Direct,
            "stage-normal" => FixtureMode::Staged,
            "stage-collision-normal" => FixtureMode::StagedCollision,
            _ => return Err("Unknown app download fixture mode".into()),
        };
        // Validate origin before any filesystem side effect; reuse Config grammar.
        Config::validate_origin(args[0])?;
        let output = Rc::new(FixtureOutput::reserve_in_tmp()?);
        Self::with_output_mode(args[0], output, mode)
    }
    fn with_output(origin: &str, output: Rc<FixtureOutput>) -> Result<Self, String> {
        Self::with_output_mode(origin, output, FixtureMode::Direct)
    }
    fn with_output_mode(origin: &str, output: Rc<FixtureOutput>, mode: FixtureMode) -> Result<Self, String> {
        let config = Config::new(origin, output.directory().to_path_buf(), download_probe::Policy::ContinueNormal)?;
        output.validate_target_absent(&config.output)?;
        let collision = if mode == FixtureMode::StagedCollision {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&config.output)
                .map_err(|error| error.to_string())?;
            file.write_all(COLLISION_BYTES).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            Some(CollisionWitness { identity: FileIdentity::read(&config.output, false)?, path: config.output.clone() })
        } else { None };
        Ok(Self { config, output, mode, collision })
    }
}

pub(crate) enum FixtureVerdict {
    Forward,
    ForwardWithFailure(String),
    Quarantine(String),
}
pub(crate) struct FixtureAudit<T> {
    pub config: Config,
    pub output: Rc<FixtureOutput>,
    pub profile: UserProfileId,
    pub owner: u64,
    pub token: Option<T>,
    pub provenance: Option<DownloadProvenance>,
    pub sequence: u64,
    pub phase: Option<DownloadPhase>,
    pub received_bytes: i64,
    pub requested_name_match: bool,
    pub resolved_path_match: bool,
    pub terminal_updated_seen: bool,
    pub failure: Option<String>,
    pub effect_ok: bool,
    pub admission: Option<admission::Shared>,
    pub mode: FixtureMode,
    pub stage_path: Option<PathBuf>,
    pub published_path: Option<PathBuf>,
    pub collision_preserved: bool,
    collision: Option<CollisionWitness>,
    quarantined: Vec<TransferEvent<T>>,
}
impl<T: Copy + Eq> FixtureAudit<T> {
    pub(crate) fn new(arm: PreparedArm, profile: UserProfileId, owner: u64) -> Self {
        Self {
            config: arm.config,
            output: arm.output,
            mode: arm.mode,
            collision: arm.collision,
            stage_path: None,
            published_path: None,
            collision_preserved: false,
            profile,
            owner,
            token: None,
            provenance: None,
            sequence: 0,
            phase: None,
            received_bytes: 0,
            requested_name_match: false,
            resolved_path_match: false,
            terminal_updated_seen: false,
            failure: None,
            effect_ok: false,
            admission: None,
            quarantined: Vec::new(),
        }
    }
    pub(crate) fn is_staged(&self) -> bool { self.mode.staged() }
    fn transfer_path(&self) -> &Path {
        self.stage_path.as_deref().unwrap_or(&self.config.output)
    }
    fn transfer_matches(&self, path: &Path) -> bool {
        path.as_os_str() == self.transfer_path().as_os_str()
    }
    fn candidates(&self) -> Result<Vec<PathBuf>, String> {
        let mut paths = vec![self.config.output.clone()];
        if self.mode == FixtureMode::StagedCollision {
            // This disposable fixture always mints one ASCII .bin filename.
            // It is not the production filename/suffix policy.
            let stem = self.config.filename.strip_suffix(".bin").ok_or("Fixture filename is not .bin")?;
            paths.push(self.output.directory().join(format!("{stem} (1).bin")));
        }
        Ok(paths)
    }
    /// Called only from the service's validated staged handoff preflight.
    /// Returns an owned guard; all filesystem checks run after this borrow ends.
    pub(crate) fn bind_stage(&mut self, token: T, path: &Path) -> Result<Rc<FixtureOutput>, String> {
        if !self.is_staged() || self.failure.is_some() || self.token != Some(token)
            || !self.requested_name_match || self.provenance.is_none()
            || path.file_name() != Some(std::ffi::OsStr::new("payload"))
            || path.parent().and_then(Path::parent) != Some(self.output.directory())
        { return Err("Fixture stage lacks captured request authority".into()); }
        if self.stage_path.as_ref().is_some_and(|old| old.as_os_str() != path.as_os_str()) {
            return Err("Fixture stage identity changed".into());
        }
        self.stage_path = Some(path.to_owned());
        Ok(self.output.clone())
    }
    pub(crate) fn publication_guard(&self, token: T, profile: UserProfileId, path: &Path)
        -> Result<Rc<FixtureOutput>, String>
    {
        if !self.is_staged() || self.failure.is_some() || self.token != Some(token)
            || self.profile != profile || self.stage_path.is_none()
            || self.phase != Some(DownloadPhase::Complete) || !self.terminal_updated_seen
            || !self.resolved_path_match
            || !self.candidates()?.iter().any(|candidate| candidate.as_os_str() == path.as_os_str())
            || self.published_path.as_ref().is_some_and(|old| old.as_os_str() != path.as_os_str())
        { return Err("Fixture publication lacks genuine bound Complete".into()); }
        Ok(self.output.clone())
    }
    /// Filesystem observation, invoked only outside every state borrow.
    pub(crate) fn collision_check(&self) -> Option<(PathBuf, FileIdentity)> {
        self.collision.as_ref().map(|witness| (witness.path.clone(), witness.identity.clone()))
    }
    pub(crate) fn verify_collision(witness: &(PathBuf, FileIdentity)) -> Result<(), String> {
        if FileIdentity::read(&witness.0, false)? != witness.1
            || fs::read(&witness.0).map_err(|error| error.to_string())? != COLLISION_BYTES
        { return Err("Owned collision sentinel changed".into()); }
        Ok(())
    }
    pub(crate) fn published(&mut self, path: &Path, collision_preserved: bool) {
        self.published_path = Some(path.to_owned());
        self.collision_preserved = collision_preserved;
    }
    pub(crate) fn fail(&mut self, error: &str) {
        if self.failure.is_none() {
            self.failure = Some(error.into());
        }
    }
    /// Report only this disposable fixture's paths, never unrelated URLs,
    /// requested names, paths or opaque engine tokens from a rejected event.
    fn conflict_diagnostic(&self, event: &TransferEvent<T>, reason: &str) -> String {
        let path = |path: &Path| {
            if path.as_os_str().is_empty() { return "Empty".to_owned(); }
            if path == self.config.output { return format!("Target({path:?})"); }
            if path.strip_prefix(self.output.directory()).is_ok_and(|relative| {
                relative.components().all(|c| matches!(c, std::path::Component::Normal(_)))
            }) {
                return format!("OwnedReservation({path:?})");
            }
            "OutsideOwnedReservation(redacted)".to_owned()
        };
        let destination = match &event.destination {
            DownloadDestination::Unrequested => "Unrequested".to_owned(),
            DownloadDestination::Requested { suggested_name } => format!(
                "Requested(name_match={})", suggested_name == &self.config.filename,
            ),
            DownloadDestination::Resolved(p) => format!("Resolved({})", path(p)),
            DownloadDestination::Rejected => "Rejected".to_owned(),
        };
        format!(
            "{reason}: sequence={} destination_requested={} destination={} phase={:?} full_path={} full_path_match={} url_match={} original_url_match={} identity_match={} token_bound={} provenance_bound={} provenance={:?} expected_provenance={:?}",
            self.sequence, event.destination_requested, destination, event.info.phase,
            path(&event.info.full_path), event.info.full_path == self.config.output,
            event.info.url == self.config.fixture_url(),
            event.info.original_url == self.config.fixture_url(),
            !event.provenance.is_private() && event.provenance.owner() == self.owner,
            self.token == Some(event.token), self.provenance == Some(event.provenance),
            event.provenance, self.provenance,
        )
    }
    pub(crate) fn inspect(&mut self, event: &TransferEvent<T>) -> FixtureVerdict {
        self.sequence = self.sequence.saturating_add(1);
        let identity = !event.provenance.is_private() && event.provenance.owner() == self.owner;
        let bound = self.token == Some(event.token) && self.provenance == Some(event.provenance);
        let urls = event.info.url == self.config.fixture_url()
            && event.info.original_url == self.config.fixture_url();
        let request = event.destination_requested
            && matches!(&event.destination, DownloadDestination::Requested { suggested_name } if suggested_name == &self.config.filename);
        let early = !event.destination_requested
            && event.info.phase == DownloadPhase::InProgress
            && matches!(&event.destination, DownloadDestination::Unrequested);
        let destination_ok = match &event.destination {
            DownloadDestination::Requested { suggested_name } => {
                suggested_name == &self.config.filename
            }
            DownloadDestination::Resolved(path) => {
                // Engine::resolve queues the authorized destination with the
                // previous copied Info, before CEF reports its actual item path.
                // This transition is not evidence of a resolved on-disk path.
                let authorized_empty_path = bound && self.requested_name_match
                    && !event.destination_requested
                    && event.info.full_path.as_os_str().is_empty();
                let awaiting_item_path = authorized_empty_path && !event.info.phase.is_terminal();
                // CEF may clear the item path on cancellation. Retain that
                // cancellation fact without claiming a path or completion.
                let canceled_without_path = authorized_empty_path
                    && event.info.phase == DownloadPhase::Canceled;
                self.transfer_matches(path)
                    && (self.transfer_matches(&event.info.full_path) || awaiting_item_path || canceled_without_path)
            }
            DownloadDestination::Unrequested => !event.destination_requested,
            DownloadDestination::Rejected => {
                !event.destination_requested && event.info.phase.is_terminal()
            }
        };
        let valid = identity
            && urls
            && destination_ok
            && (!event.destination_requested || request)
            && (bound
                || (self.token.is_none() && !event.info.phase.is_terminal() && (request || early)));
        // Preserve the genuine same-bound terminal even on metadata conflict.
        let terminal =
            identity && bound && !event.destination_requested && event.info.phase.is_terminal();
        if !valid && !terminal {
            let error = self.conflict_diagnostic(event, "Fixture event quarantined (identity/request/path conflict)");
            self.fail(&error);
            if self.quarantined.len() < 64 {
                self.quarantined.push(event.clone());
            } else {
                self.fail("Fixture quarantine overflow; further events not retained");
            }
            return FixtureVerdict::Quarantine(if self.quarantined.len() == 64 {
                "Fixture quarantine limit reached; further payload retention stops".into()
            } else {
                error
            });
        }
        if self.token.is_none() {
            self.token = Some(event.token);
            self.provenance = Some(event.provenance);
        }
        // Terminal tombstone status must not regress on a later progress replay.
        if !self.phase.is_some_and(|p| p.is_terminal()) {
            self.phase = Some(event.info.phase);
            self.received_bytes = event.info.received_bytes;
            self.resolved_path_match = matches!(&event.destination, DownloadDestination::Resolved(p) if self.transfer_matches(p))
                && self.transfer_matches(&event.info.full_path);
        }
        self.requested_name_match |= request;
        self.terminal_updated_seen |=
            !event.destination_requested && event.info.phase == DownloadPhase::Complete;
        if !valid {
            let error = self.conflict_diagnostic(event, "Fixture bound terminal metadata conflict");
            self.fail(&error);
            FixtureVerdict::ForwardWithFailure(error)
        } else {
            FixtureVerdict::Forward
        }
    }
    pub(crate) fn decide(
        &mut self,
        profile: UserProfileId,
        request: &DestinationRequest,
    ) -> DestinationDecision {
        if self.failure.is_none()
            && self.requested_name_match
            && profile == self.profile
            && Some(request.provenance) == self.provenance
            && request.suggested_name == self.config.filename
            && request.info.url == self.config.fixture_url()
            && request.info.original_url == self.config.fixture_url()
        {
            if self.is_staged() {
                match self.candidates() {
                    Ok(candidates) => DestinationDecision::Staged(crate::downloads::StagedDestination {
                        folder: self.output.directory().to_owned(), candidates,
                    }),
                    Err(error) => { self.fail(&error); DestinationDecision::Reject },
                }
            } else { DestinationDecision::Path(self.config.output.clone()) }
        } else {
            DestinationDecision::Reject
        }
    }
    /// Clone under audit borrow; execute the filesystem check AFTER releasing it.
    pub(crate) fn effect_check(&self, token: T, path: &Path) -> Result<Rc<FixtureOutput>, String> {
        if self.failure.is_some()
            || self.token != Some(token)
            || !self.requested_name_match
            || path != self.config.output.as_path()
        {
            return Err("Fixture effect lacks exact request authority".into());
        }
        Ok(self.output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FixtureTransferStatus {
    pub captured_profile: UserProfileId,
    pub owner: u64,
    pub sequence: u64,
    pub provenance: Option<DownloadProvenance>,
    pub phase: Option<DownloadPhase>,
    pub received_bytes: i64,
    pub requested_name_match: bool,
    pub resolved_path_match: bool,
    pub canonical_id: Option<CompletedDownloadId>,
    pub terminal_updated_seen: bool,
    pub failure: Option<String>,
    pub service_errors: usize,
    pub effect_ok: bool,
    pub inventory_match: bool,
    pub output: PathBuf,
    pub staged: bool,
    pub transfer_path: Option<PathBuf>,
    pub published_path: Option<PathBuf>,
    pub collision_preserved: bool,
    pub collision_required: bool,
    pub source: Option<admission::Source>,
    pub admission: Option<admission::Phase>,
    pub consumer: Option<crate::downloads::ConsumerStatus>,
}
impl FixtureTransferStatus {
    pub(crate) fn active(&self) -> bool {
        self.failure.is_none()
            && self.service_errors == 0
            && self.effect_ok
            && self.phase == Some(DownloadPhase::InProgress)
            && self.received_bytes > 0
            && self.resolved_path_match
    }
    pub(crate) fn completed(&self) -> bool {
        self.failure.is_none()
            && self.service_errors == 0
            && self.effect_ok
            && self.phase == Some(DownloadPhase::Complete)
            && self.terminal_updated_seen
            && self.requested_name_match
            && self.resolved_path_match
            && self.canonical_id.is_some()
            && self.inventory_match
            && (!self.staged || self.published_path.is_some())
            && (!self.collision_required || self.collision_preserved)
    }
}
#[derive(Clone, Default)]
pub(crate) struct AppDownloadAcceptance {
    pub descriptor: Option<admission::Shared>,
}
impl Global for AppDownloadAcceptance {}
pub(crate) fn preflight(cx: &App) -> Result<(), String> {
    if crate::quit::is_quitting()
        || cx.has_global::<AppDownloadAcceptance>()
        || cx.has_global::<crate::downloads::AppDownloads>()
        || admission::occupied()
        || !download_probe::idle_for_app_downloads(cx)
    {
        return Err("App download fixture requires a fresh unoccupied normal process".into());
    }
    Ok(())
}
pub(crate) fn status(cx: &App) -> Result<FixtureTransferStatus, String> {
    let run = cx
        .try_global::<AppDownloadAcceptance>()
        .ok_or("App download fixture unarmed")?;
    let downloads = cx.try_global::<crate::downloads::AppDownloads>().ok_or("App downloads absent")?;
    let service = downloads.service.clone();
    let mut status = service
        .borrow()
        .fixture_status()
        .ok_or("App download audit absent")?;
    status.consumer = Some(downloads.consumer_status(cx));
    if let Some(descriptor) = &run.descriptor {
        let d = descriptor.borrow();
        status.source = Some(d.source.clone());
        status.admission = Some(d.phase);
        if d.phase == admission::Phase::Failed && status.failure.is_none() {
            status.failure = Some("Fixture source admission failed".into());
        }
    }
    Ok(status)
}
pub(crate) fn active(cx: &App) -> bool {
    status(cx).is_ok_and(|s| s.active())
}
pub(crate) fn completed(cx: &App) -> bool {
    status(cx).is_ok_and(|s| s.completed())
}


#[cfg(test)]
mod typed_path_tests {
    use super::*;
    #[test]
    fn prepared_arm_retains_owned_tmp_space_path_without_environment_mutation() {
        // Fresh owned TMP ancestor; simulate a TMP root containing a user name.
        let parent = FixtureOutput::reserve_in_tmp().unwrap();
        let root = parent.directory().join("User Name TMP");
        fs::create_dir(&root).unwrap();
        let output = Rc::new(FixtureOutput::reserve_under(&root).unwrap());
        let arm = PreparedArm::with_output("http://127.0.0.1:18473", output).unwrap();
        assert_eq!(arm.config.output.parent(), Some(arm.output.directory()));
        assert_eq!(arm.output.directory().parent(), Some(root.as_path()));
        assert!(arm.config.output.to_string_lossy().contains("User Name TMP"));
        assert!(arm.config.filename.starts_with("echo-download-probe-"));
        assert!(arm.config.filename.ends_with(".bin"));
        arm.output.validate_target_absent(&arm.config.output).unwrap();
        let next = Config::new("http://127.0.0.1:18473", arm.output.directory().to_path_buf(), download_probe::Policy::ContinueNormal).unwrap();
        assert_ne!(arm.config.filename, next.filename);
        assert!(Config::new("http://localhost:18473", root, download_probe::Policy::ContinueNormal).is_err());
    }
}

#[cfg(test)]
mod conflict_diagnostic_tests {
    use super::*;

    fn audit_and_request() -> (FixtureAudit<u64>, TransferEvent<u64>) {
        let profile = browser_core::Browser::new().default_user_profile_id();
        let arm = PreparedArm::parse("http://127.0.0.1:18473 continue-normal").unwrap();
        let audit = FixtureAudit::new(arm, profile, 1);
        let event = TransferEvent {
            token: 7,
            provenance: DownloadProvenance::new(1, 3, 5, false),
            destination_requested: true,
            destination: DownloadDestination::Requested { suggested_name: audit.config.filename.clone() },
            info: engine::DownloadInfo {
                phase: DownloadPhase::InProgress, interrupt_reason: 0,
                current_speed: 0, percent_complete: 0, total_bytes: 10, received_bytes: 0,
                start_time: 0, end_time: 0, full_path: PathBuf::new(),
                url: audit.config.fixture_url(), original_url: audit.config.fixture_url(),
                suggested_file_name: String::new(), content_disposition: String::new(),
                mime_type: "application/octet-stream".into(),
            },
        };
        (audit, event)
    }

    fn transfer_status(audit: &FixtureAudit<u64>) -> FixtureTransferStatus {
        FixtureTransferStatus {
            captured_profile: audit.profile, owner: audit.owner, sequence: audit.sequence,
            provenance: audit.provenance, phase: audit.phase, received_bytes: audit.received_bytes,
            requested_name_match: audit.requested_name_match, resolved_path_match: audit.resolved_path_match,
            canonical_id: None, terminal_updated_seen: audit.terminal_updated_seen,
            failure: audit.failure.clone(), service_errors: 0,
            // Isolate the path/phase predicate with the effect-success input
            // satisfied. This pure status test asserts no native acceptance.
            effect_ok: true, inventory_match: false, output: audit.config.output.clone(),
            staged: audit.is_staged(), transfer_path: audit.stage_path.clone(),
            published_path: audit.published_path.clone(), collision_preserved: audit.collision_preserved,
            collision_required: audit.mode == FixtureMode::StagedCollision,
            source: None, admission: None, consumer: None,
        }
    }

    #[test]
    fn authorized_resolve_with_empty_info_waits_for_actual_path_and_bytes() {
        let (mut audit, mut event) = audit_and_request();
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        let request = DestinationRequest {
            provenance: event.provenance, suggested_name: audit.config.filename.clone(),
            info: event.info.clone(),
        };
        assert!(matches!(audit.decide(audit.profile, &request), DestinationDecision::Path(p) if p == audit.config.output));
        audit.effect_check(event.token, &audit.config.output).unwrap();

        // The exact production audit seam sees the engine's Resolved snapshot
        // before the next CEF callback fills in DownloadInfo.full_path.
        event.destination_requested = false;
        event.destination = DownloadDestination::Resolved(audit.config.output.clone());
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert!(event.info.full_path.as_os_str().is_empty()); // Never normalize genuine Info.
        assert!(!audit.resolved_path_match);
        assert!(!transfer_status(&audit).active());
        assert!(!transfer_status(&audit).completed());

        // Bytes alone do not turn the authorized destination into an item path.
        event.info.received_bytes = 5;
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert!(!audit.resolved_path_match);
        assert!(!transfer_status(&audit).active());
        assert!(!transfer_status(&audit).completed());

        event.info.full_path = audit.config.output.clone();
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert!(audit.resolved_path_match);
        assert!(transfer_status(&audit).active());
        assert!(!transfer_status(&audit).completed());
        assert!(audit.failure.is_none() && audit.quarantined.is_empty());
        assert!(!audit.terminal_updated_seen);
        assert_eq!(audit.sequence, 4);
    }

    #[test]
    fn empty_resolve_exception_never_accepts_wrong_destination_or_nonempty_item_path() {
        for conflict in 0..4 {
            let (mut audit, mut event) = audit_and_request();
            assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
            event.destination_requested = false;
            let wrong_destination = conflict % 2 == 1;
            let wrong = if conflict < 2 {
                audit.config.output.with_extension("bin.crdownload")
            } else { std::env::temp_dir().join("outside-owned-fixture.bin") };
            event.destination = DownloadDestination::Resolved(if wrong_destination {
                wrong.clone()
            } else { audit.config.output.clone() });
            if !wrong_destination { event.info.full_path = wrong; }
            assert!(matches!(audit.inspect(&event), FixtureVerdict::Quarantine(_)));
            assert!(audit.failure.is_some());
            assert!(!audit.resolved_path_match);
            assert!(!transfer_status(&audit).active());
        }
    }

    #[test]
    fn empty_resolve_exception_requires_genuine_request_and_same_token_provenance() {
        for conflict in 0..5 {
            let (mut audit, mut event) = audit_and_request();
            if conflict == 0 {
                event.destination_requested = false;
                event.destination = DownloadDestination::Unrequested;
            }
            assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
            event.destination_requested = false;
            event.destination = DownloadDestination::Resolved(audit.config.output.clone());
            match conflict {
                0 => assert!(!audit.requested_name_match),
                1 => event.token = 8,
                2 => event.provenance = DownloadProvenance::new(1, 3, 6, false),
                3 => event.info.url = "http://127.0.0.1:18473/other".into(),
                _ => event.info.original_url = "http://127.0.0.1:18473/other".into(),
            }
            assert!(matches!(audit.inspect(&event), FixtureVerdict::Quarantine(_)));
            assert!(audit.failure.is_some());
            assert!(!audit.resolved_path_match);
        }
    }

    #[test]
    fn canceled_empty_path_is_only_a_bound_cancellation_fact_even_at_full_bytes() {
        let (mut audit, mut event) = audit_and_request();
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        event.destination_requested = false;
        event.destination = DownloadDestination::Resolved(audit.config.output.clone());
        event.info.full_path = audit.config.output.clone();
        event.info.received_bytes = 5;
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert!(transfer_status(&audit).active());
        event.info.phase = DownloadPhase::Canceled;
        event.info.full_path = PathBuf::new();
        event.info.received_bytes = event.info.total_bytes;
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert_eq!(audit.phase, Some(DownloadPhase::Canceled));
        assert!(audit.failure.is_none() && audit.quarantined.is_empty());
        assert!(!audit.resolved_path_match && !audit.terminal_updated_seen);
        assert!(!transfer_status(&audit).active());
        assert!(!transfer_status(&audit).completed());
    }

    #[test]
    fn canceled_empty_path_preserves_request_identity_url_and_destination_guards() {
        for conflict in 0..8 {
            let (mut audit, mut event) = audit_and_request();
            if conflict == 0 {
                event.destination_requested = false;
                event.destination = DownloadDestination::Unrequested;
            }
            assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
            event.destination_requested = false;
            event.destination = DownloadDestination::Resolved(audit.config.output.clone());
            event.info.phase = DownloadPhase::Canceled;
            match conflict {
                0 => assert!(!audit.requested_name_match),
                1 => event.token = 8,
                2 => event.provenance = DownloadProvenance::new(1, 3, 6, false),
                3 => event.info.url = "http://127.0.0.1:18473/other".into(),
                4 => event.info.original_url = "http://127.0.0.1:18473/other".into(),
                5 => event.destination = DownloadDestination::Resolved(audit.config.output.with_extension("wrong")),
                6 => event.info.full_path = audit.config.output.with_extension("wrong"),
                _ => event.destination_requested = true,
            }
            assert!(matches!(audit.inspect(&event), FixtureVerdict::Quarantine(_) | FixtureVerdict::ForwardWithFailure(_)));
            assert!(audit.failure.is_some());
            assert!(!transfer_status(&audit).completed());
        }
    }

    #[test]
    fn resolved_terminal_with_empty_item_path_still_fails_with_exact_diagnostic() {
        let (mut audit, mut event) = audit_and_request();
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        // Empty item paths remain invalid on a genuine bound terminal.
        event.destination_requested = false;
        event.info.phase = DownloadPhase::Complete;
        event.destination = DownloadDestination::Resolved(audit.config.output.clone());
        let FixtureVerdict::ForwardWithFailure(error) = audit.inspect(&event) else { panic!("bound terminal must forward with failure"); };
        for fragment in ["sequence=2", "destination_requested=false", "destination=Resolved(Target(",
            "phase=Complete", "full_path=Empty", "full_path_match=false", "url_match=true",
            "original_url_match=true", "identity_match=true", "token_bound=true", "provenance_bound=true"] {
            assert!(error.contains(fragment), "missing {fragment}: {error}");
        }
        assert_eq!(audit.failure.as_deref(), Some(error.as_str()));
        assert!(audit.quarantined.is_empty());
        assert_eq!(audit.token, Some(7));
        assert!(audit.terminal_updated_seen);
        assert!(!audit.resolved_path_match);
    }

    #[test]
    fn conflict_diagnostic_redacts_unrelated_payloads_and_distinguishes_binding_failures() {
        let (mut audit, mut event) = audit_and_request();
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        event.token = 9;
        event.provenance = DownloadProvenance::new(1, 3, 6, false);
        event.info.url = "https://unrelated.invalid/private-url".into();
        event.info.original_url = "https://unrelated.invalid/private-origin".into();
        event.info.full_path = std::env::temp_dir().join("unrelated-private-file.bin");
        event.destination = DownloadDestination::Requested { suggested_name: "unrelated-private-name.bin".into() };
        let FixtureVerdict::Quarantine(error) = audit.inspect(&event) else { panic!("must remain rejected"); };
        for fragment in ["Requested(name_match=false)", "full_path=OutsideOwnedReservation(redacted)",
            "url_match=false", "original_url_match=false", "identity_match=true", "token_bound=false",
            "provenance_bound=false", "browser: 6", "browser: 5"] {
            assert!(error.contains(fragment), "missing {fragment}: {error}");
        }
        assert!(!error.contains("unrelated"));
        assert_eq!(audit.token, Some(7));
    }

    #[test]
    fn bound_terminal_conflict_keeps_forwarding_and_owned_path_diagnostic() {
        let (mut audit, mut event) = audit_and_request();
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        event.destination_requested = false;
        event.info.phase = DownloadPhase::Complete;
        event.info.full_path = audit.config.output.with_extension("bin.crdownload");
        event.destination = DownloadDestination::Resolved(audit.config.output.clone());
        let FixtureVerdict::ForwardWithFailure(error) = audit.inspect(&event) else { panic!("bound terminal must forward"); };
        assert!(error.contains("Fixture bound terminal metadata conflict"));
        assert!(error.contains("full_path=OwnedReservation("));
        assert!(error.contains("bin.crdownload"));
        assert!(audit.terminal_updated_seen);
        assert_eq!(audit.phase, Some(DownloadPhase::Complete));
        assert!(audit.quarantined.is_empty());
    }

    fn staged_audit_and_request(collision: bool) -> (FixtureAudit<u64>, TransferEvent<u64>) {
        let profile = browser_core::Browser::new().default_user_profile_id();
        let mode = if collision { "stage-collision-normal" } else { "stage-normal" };
        let arm = PreparedArm::parse(&format!("http://127.0.0.1:18473 {mode}")).unwrap();
        let audit = FixtureAudit::new(arm, profile, 1);
        let event = TransferEvent { token: 7, provenance: DownloadProvenance::new(1, 3, 5, false),
            destination_requested: true,
            destination: DownloadDestination::Requested { suggested_name: audit.config.filename.clone() },
            info: engine::DownloadInfo { phase: DownloadPhase::InProgress, interrupt_reason: 0,
                current_speed: 0, percent_complete: 0, total_bytes: 8, received_bytes: 0,
                start_time: 0, end_time: 0, full_path: PathBuf::new(),
                url: audit.config.fixture_url(), original_url: audit.config.fixture_url(),
                suggested_file_name: String::new(), content_disposition: String::new(),
                mime_type: "application/octet-stream".into() } };
        (audit, event)
    }

    #[test]
    fn staged_transfer_facts_require_the_actual_stage_not_the_existing_final_file() {
        let (mut audit, mut event) = staged_audit_and_request(true);
        assert!(audit.config.output.is_file()); // sentinel is never completion
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        let plan = match audit.decide(audit.profile, &DestinationRequest {
            provenance: event.provenance, suggested_name: audit.config.filename.clone(), info: event.info.clone(),
        }) { DestinationDecision::Staged(plan) => plan, _ => panic!("staged mode did not plan") };
        let stage = crate::downloads_publication::Staging::reserve((event.token, event.provenance), &plan.folder, plan.candidates).unwrap();
        let path = stage.validate_handoff().unwrap().to_owned();
        audit.bind_stage(event.token, &path).unwrap().validate_directory().unwrap();
        event.destination_requested = false;
        event.destination = DownloadDestination::Resolved(path.clone());
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert!(!audit.resolved_path_match);
        assert!(!transfer_status(&audit).active());
        assert!(!transfer_status(&audit).completed());
        event.info.full_path = path;
        event.info.received_bytes = 3;
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        assert!(transfer_status(&audit).active());
        assert!(!transfer_status(&audit).completed());
        assert!(audit.published_path.is_none());
        FixtureAudit::<u64>::verify_collision(&audit.collision_check().unwrap()).unwrap();
    }

    #[test]
    fn staged_binding_rejects_replacement_targets_and_publication_without_complete() {
        let (mut audit, event) = staged_audit_and_request(false);
        assert!(matches!(audit.inspect(&event), FixtureVerdict::Forward));
        let stage = crate::downloads_publication::Staging::reserve((event.token, event.provenance),
            audit.output.directory(), vec![audit.config.output.clone()]).unwrap();
        let path = stage.validate_handoff().unwrap().to_owned();
        audit.bind_stage(event.token, &path).unwrap();
        assert!(audit.bind_stage(event.token + 1, &path).is_err());
        assert!(audit.bind_stage(event.token, &audit.config.output.clone()).is_err());
        let replacement = audit.output.directory().join("another-stage/payload");
        assert!(audit.bind_stage(event.token, &replacement).is_err());
        assert!(audit.publication_guard(event.token, audit.profile, &audit.config.output).is_err());
        assert!(audit.published_path.is_none());
    }

}
