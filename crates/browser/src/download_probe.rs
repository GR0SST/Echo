//! Disposable loopback-only acceptance harness, never a destination policy.
//! Guarded synchronous creation stays in Pages; all new pumping, download
//! effects and retirement preparation run in the App-owned task outside cx.update.
use engine::{
    DownloadControl, DownloadDestination, DownloadEvent, DownloadOwner, DownloadProvenance,
    DownloadSnapshot, DownloadToken, Engine,
};
use gpui::{App, AppContext, Global};
use std::{cell::RefCell, collections::HashSet, path::PathBuf, rc::Rc, time::Duration};

#[path = "download_probe_admission.rs"]
pub(crate) mod admission;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Policy {
    Baseline,
    ContinueNormal,
}
#[derive(Clone, Debug)]
pub(crate) struct Config {
    pub origin: String,
    pub output: PathBuf,
    pub filename: String,
    pub policy: Policy,
}
impl Config {
    pub(crate) fn parse(arg: &str) -> Result<Self, String> {
        let args: Vec<_> = arg.split_whitespace().collect();
        if args.len() != 3 {
            return Err(
                "Expected origin absolute-output-directory baseline|continue-normal (no spaces)"
                    .into(),
            );
        }
        let policy = match args[2] {
            "baseline" => Policy::Baseline,
            "continue-normal" => Policy::ContinueNormal,
            _ => return Err("Unsupported probe policy".into()),
        };
        Self::new(args[0], PathBuf::from(args[1]), policy)
    }
    pub(crate) fn new(origin: &str, directory: PathBuf, policy: Policy) -> Result<Self, String> {
        Self::validate_origin(origin)?;
        if !directory.is_absolute() || !directory.is_dir() {
            return Err("Output must be an existing absolute disposable directory".into());
        }
        let directory = directory
            .canonicalize()
            .map_err(|_| "Cannot resolve output directory")?;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let filename = format!(
            "echo-download-probe-{}-{}.bin",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let output = directory.join(&filename);
        // symlink_metadata also rejects dangling links, unlike exists().
        match output.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Ok(_) => return Err("Generated output already exists".into()),
            Err(e) => return Err(format!("Generated output absence unknown: {e}")),
        }
        Ok(Self {
            origin: origin.into(),
            output,
            filename,
            policy,
        })
    }
    pub(crate) fn validate_origin(origin: &str) -> Result<(), String> {
        let authority = origin
            .strip_prefix("http://")
            .ok_or("Only an HTTP loopback fixture origin is admitted")?;
        let port = authority
            .strip_prefix("127.0.0.1:")
            .or_else(|| authority.strip_prefix("[::1]:"))
            .ok_or("Use an explicit numeric loopback address and port")?;
        if port.is_empty()
            || !port.bytes().all(|b| b.is_ascii_digit())
            || port.parse::<u16>().ok().filter(|p| *p != 0).is_none()
        {
            return Err("Invalid loopback port or non-origin URL".into());
        }
        Ok(())
    }
    pub(crate) fn fixture_url(&self) -> String {
        format!("{}/download/{}", self.origin, self.filename)
    }
    pub(crate) fn owner(&self, captured_normal_owner: u64) -> DownloadOwner {
        match self.policy {
            Policy::Baseline => DownloadOwner::cancel_before_native_close(captured_normal_owner),
            Policy::ContinueNormal => {
                DownloadOwner::continue_normal_after_source_close(captured_normal_owner)
            }
        }
    }
    fn matches_source(&self, info: &engine::DownloadInfo, provenance: DownloadProvenance) -> bool {
        !provenance.is_private() && info.url == self.fixture_url() && info.original_url == self.fixture_url()
    }
    fn accepts(&self, info: &engine::DownloadInfo, destination: &DownloadDestination,
        provenance: DownloadProvenance, bound_item: bool) -> bool {
        if !self.matches_source(info, provenance)
            || (!info.suggested_file_name.is_empty() && info.suggested_file_name != self.filename) {
            return false;
        }
        match destination {
            // This is the authoritative copied OnBeforeDownload argument. CEF
            // may leave DownloadInfo::suggested_file_name empty on EVERY event.
            DownloadDestination::Requested { suggested_name } => suggested_name == &self.filename,
            // Empty later item names are safe only for the already admitted
            // token/full provenance and the exact resolved target. A filename
            // in info alone never grants an unknown item a destination.
            DownloadDestination::Resolved(path) => bound_item && path == &self.output,
            DownloadDestination::Unrequested | DownloadDestination::Rejected => false,
        }
    }

}

fn is_bound_item<T: PartialEq>(bound: Option<DownloadProvenance>, known_token: Option<&T>,
    provenance: DownloadProvenance, token: &T) -> bool {
    bound == Some(provenance) && known_token.is_some_and(|known| known == token)
}

fn bind_provenance(
    owner: u64,
    bound: &mut Option<DownloadProvenance>,
    actual: DownloadProvenance,
) -> Result<(), &'static str> {
    if actual.owner() != owner || actual.is_private() {
        return Err("Probe owner/privacy mismatch");
    }
    if bound.is_some_and(|expected| expected != actual) {
        return Err("Probe provenance drift rejected");
    }
    if bound.is_none() {
        *bound = Some(actual);
    }
    Ok(())
}

struct Run {
    config: Config,
    admission: admission::Shared,
    owner: u64,
    provenance: Option<DownloadProvenance>,
    items: Vec<DownloadSnapshot>,
    resolved: HashSet<DownloadToken>,
    controls: Vec<(DownloadToken, DownloadControl)>,
    failure: Option<String>,
    sequence: u64,
    retirement_failed: bool,
}
#[derive(Default)]
struct State {
    run: Option<Run>,
    // Preserve other consumers' events; never print unrelated URLs or paths.
    unclaimed: Vec<DownloadEvent>,
}
struct Probe(Rc<RefCell<State>>);
impl Global for Probe {}

pub(crate) fn idle_for_app_downloads(cx: &App) -> bool {
    cx.try_global::<Probe>().is_none_or(|probe| {
        let state = probe.0.borrow();
        state.run.is_none() && state.unclaimed.is_empty()
    })
}
pub(crate) fn can_arm(cx: &App) -> bool {
    !cx.has_global::<crate::downloads::AppDownloads>() && !admission::occupied() && cx.try_global::<Probe>()
        .is_some_and(|probe| probe.0.borrow().run.is_none())
}

pub(crate) fn arm(config: Config, source: admission::Source, cx: &mut App) -> Result<(), String> {
    if crate::quit::is_quitting() || !can_arm(cx) {
        return Err("Probe cannot arm".into());
    }
    use std::sync::atomic::{AtomicU64, Ordering};
    // Separate opaque process-local namespace: never derive owner from core IDs.
    static NEXT_OWNER: AtomicU64 = AtomicU64::new(1 << 63);
    let owner = NEXT_OWNER.fetch_add(1, Ordering::Relaxed);
    let descriptor = admission::try_install(config.clone(), source, owner)?;
    eprintln!(
        "[download-probe] owner={owner} output={} filename={} policy={:?}",
        config.output.display(),
        config.filename,
        config.policy
    );
    cx.global::<Probe>().0.borrow_mut().run = Some(Run {
        config,
        admission: descriptor,
        owner,
        provenance: None,
        items: Vec::new(),
        resolved: HashSet::new(),
        controls: Vec::new(),
        failure: None,
        sequence: 0,
        retirement_failed: false,
    });
    Ok(())
}

/// Each engine entry can synchronously dispatch native quit. Callers must not
/// retain an App/model/probe borrow here and must recheck for the NEXT effect.
fn while_running<R>(effect: impl FnOnce() -> R) -> Option<R> {
    if crate::quit::is_quitting() { return None; }
    Some(effect())
}

/// One shared effect executor makes destination/control/preparation boundaries
/// explicit and testable with a driver that seals quit from INSIDE an effect.
fn apply_native_effects<D, C>(destinations: impl IntoIterator<Item = D>, controls: impl IntoIterator<Item = C>,
    mut resolve: impl FnMut(D), mut control: impl FnMut(C), prepare: impl FnOnce()) -> bool {
    for destination in destinations {
        if while_running(|| resolve(destination)).is_none() { return false; }
    }
    for command in controls {
        if while_running(|| control(command)).is_none() { return false; }
    }
    if while_running(prepare).is_none() { return false; }
    !crate::quit::is_quitting()
}

pub(crate) fn init(cx: &mut App) {
    if cx.has_global::<Probe>() {
        return;
    }
    let state = Rc::new(RefCell::new(State::default()));
    cx.set_global(Probe(state.clone()));
    cx.spawn(async move |cx| {
        let start = std::time::Instant::now();
        loop {
            cx.background_executor().timer(Duration::from_millis(8)).await;
            if crate::quit::is_quitting() { break; }
            // A preinstalled app service owns the ONE running event queue.
            // Inspection only: do not create or wake it from the probe.
            if cx.update(|cx| cx.has_global::<crate::downloads::AppDownloads>()).unwrap_or(true) { break; }
            let armed = state.borrow().run.is_some();
            if !armed { continue; }
            if Engine::download_work_pending() && while_running(Engine::pump).is_none() { break; }
            if crate::quit::is_quitting() { break; }
            // pump can dispatch app work reentrantly; check ownership again.
            if cx.update(|cx| cx.has_global::<crate::downloads::AppDownloads>()).unwrap_or(true) { break; }
            let Some(drained) = while_running(Engine::take_download_events) else { break; };
            // Draining can run CEF/native effects and seal quit reentrantly.
            // Preserve copied events without entering the next native stage.
            if crate::quit::is_quitting() {
                if let Ok(events) = drained { state.borrow_mut().unclaimed.extend(events); }
                break;
            }
            let events = match drained {
                Ok(events) => events,
                Err(_) => { state.borrow_mut().run.as_mut().unwrap().failure = Some("Central download drain failed".into()); continue; }
            };
            let mut destinations = Vec::new();
            {
                let mut state = state.borrow_mut();
                for event in events {
                    let snapshot = match &event { DownloadEvent::DestinationRequested(s) | DownloadEvent::Updated(s) => s };
                    let run = state.run.as_mut().unwrap();
                    if snapshot.provenance.owner() != run.owner {
                        state.unclaimed.push(event);
                        continue;
                    }
                    let bound_item = is_bound_item(run.provenance, run.items.first().map(|item| &item.token),
                        snapshot.provenance, &snapshot.token);
                    if !run.config.accepts(&snapshot.info, &snapshot.destination, snapshot.provenance, bound_item) {
                        // Updated may precede BeforeDownload/target-name selection.
                        // Preserve this copied event without granting a destination
                        // or binding provenance until the exact name is available.
                        if run.provenance.is_none()
                            && run.config.matches_source(&snapshot.info, snapshot.provenance)
                            && matches!(snapshot.destination, DownloadDestination::Unrequested)
                            && (snapshot.info.suggested_file_name.is_empty() || snapshot.info.suggested_file_name == run.config.filename) {
                            eprintln!("[download-probe] awaiting-before-download provenance={:?} phase={:?} bytes={}",
                                snapshot.provenance, snapshot.info.phase, snapshot.info.received_bytes);
                            state.unclaimed.push(event);
                            continue;
                        }
                        // Keep diagnostics bounded; never log the rejected URL.
                        run.failure = Some("Fixture provenance matched but exact URL/filename did not; destination denied".into());
                        continue;
                    }
                    // First copied exact fixture callback binds the full opaque
                    // engine provenance; no immediate creation result is needed.
                    if let Err(error) = bind_provenance(run.owner, &mut run.provenance, snapshot.provenance) {
                        run.failure = Some(error.into());
                        continue;
                    }
                    if !run.items.is_empty() && !run.items.iter().any(|item| item.token == snapshot.token) {
                        run.failure = Some("Only one generated fixture attachment is admitted per run".into());
                        continue;
                    }
                    run.sequence += 1;
                    eprintln!("[download-probe] seq={} ms={} token={:?} provenance={:?} phase={:?} bytes={} total={} destination_resolved={} expected_path={}",
                        run.sequence, start.elapsed().as_millis(), snapshot.token, snapshot.provenance,
                        snapshot.info.phase, snapshot.info.received_bytes, snapshot.info.total_bytes, matches!(snapshot.destination, DownloadDestination::Resolved(_)), run.config.output.display());
                    if matches!(snapshot.destination, DownloadDestination::Requested { .. }) && run.resolved.insert(snapshot.token) {
                        destinations.push((snapshot.token, run.config.output.clone()));
                    }
                    if let Some(item) = run.items.iter_mut().find(|s| s.token == snapshot.token) { *item = snapshot.clone(); }
                    else { run.items.push(snapshot.clone()); }
                }
            }
            let controls = { let mut state = state.borrow_mut(); std::mem::take(&mut state.run.as_mut().unwrap().controls) };
            let retry_retirement = !state.borrow().run.as_ref().unwrap().retirement_failed;
            // Every closure releases its native call before touching probe state.
            // The executor checks quit before EACH next effect, including across
            // destination -> control -> parking/preparation stage boundaries.
            let running = apply_native_effects(destinations, controls,
                |(token, path)| {
                    let result = if path.symlink_metadata().is_ok() { Err("Test output collision".into()) }
                        else { Engine::resolve_download_destination(token, path) };
                    if result.is_err() { state.borrow_mut().run.as_mut().unwrap().failure = Some("Destination resolution failed".into()); }
                },
                |(token, command)| {
                    if Engine::control_download(token, command).is_err() {
                        state.borrow_mut().run.as_mut().unwrap().failure = Some("Control failed".into());
                    }
                },
                || {
                    if retry_retirement && Engine::prepare_download_retirements().is_err() {
                        let mut state = state.borrow_mut();
                        let run = state.run.as_mut().unwrap();
                        run.retirement_failed = true;
                        run.failure = Some("Retirement preparation failed; parent must remain retained".into());
                    }
                });
            if !running { break; }

        }
    }).detach();
}

/// Copied acceptance barrier for automation: never consults a source view.
pub(crate) fn active(cx: &App) -> bool {
    cx.try_global::<Probe>().is_some_and(|probe| {
        let state = probe.0.borrow();
        state.run.as_ref().is_some_and(|run| run.failure.is_none() && run.provenance.is_some()
            && run.items.iter().any(|item| item.info.phase == engine::DownloadPhase::InProgress
                && item.info.received_bytes > 0 && item.info.received_bytes < 33_554_432
                && matches!(&item.destination, DownloadDestination::Resolved(path) if path == &run.config.output)))
    })
}

pub(crate) fn control(arg: &str, cx: &mut App) {
    if crate::quit::is_quitting() { return; }
    let Some(probe) = cx.try_global::<Probe>() else {
        return;
    };
    let mut state = probe.0.borrow_mut();
    let Some(run) = state.run.as_mut() else {
        return;
    };
    let args: Vec<_> = arg.split_whitespace().collect();
    if args.len() != 2 {
        return;
    }
    let command = match args[0] {
        "pause" => DownloadControl::Pause,
        "resume" => DownloadControl::Resume,
        "cancel" => DownloadControl::Cancel,
        _ => return,
    };
    let Some(item) = args[1]
        .parse::<usize>()
        .ok()
        .and_then(|index| run.items.get(index))
    else {
        return;
    };
    run.controls.push((item.token, command)); // No CEF under the state/App borrow.
}
pub(crate) fn status(cx: &mut App) {
    let Some(probe) = cx.try_global::<Probe>() else {
        eprintln!("[download-probe] unarmed");
        return;
    };
    let (provenance, count, failure) = {
        let state = probe.0.borrow();
        let Some(run) = &state.run else {
            eprintln!("[download-probe] unarmed");
            return;
        };
        let descriptor = run.admission.borrow();
        eprintln!(
            "[download-probe-source] source={:?} owner={} admission={:?} bound_generation={:?}",
            descriptor.source, run.owner, descriptor.phase, descriptor.generation
        );
        for (index, item) in run.items.iter().enumerate() {
            eprintln!("[download-probe-item] index={index} token={:?} phase={:?} bytes={} total={} expected_path_match={}",
                item.token, item.info.phase, item.info.received_bytes, item.info.total_bytes,
                item.info.full_path == run.config.output);
        }
        (run.provenance, run.items.len(), run.failure.clone())
    };
    eprintln!("[download-probe] provenance={provenance:?} items={count} failure={failure:?}");
    // Pure copied diagnostics. No source entity or native view is accessed.
    for status in Engine::download_retirement_status()
        .into_iter()
        .filter(|s| Some(s.provenance) == provenance)
    {
        eprintln!("[download-probe-retirement] {status:?}");
    }
}

/// Parent's central model can reclaim events if another explicit consumer exists.
#[allow(dead_code)]
pub(crate) fn take_unclaimed(cx: &mut App) -> Vec<DownloadEvent> {
    cx.try_global::<Probe>()
        .map(|probe| std::mem::take(&mut probe.0.borrow_mut().unclaimed))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_exact_callback_binds_full_provenance_and_rejects_private_or_drift() {
        let mut bound = None;
        let accepted = DownloadProvenance::new(901, 20, 30, false);
        assert!(bind_provenance(901, &mut bound, accepted).is_ok());
        assert_eq!(bound, Some(accepted));
        assert!(bind_provenance(901, &mut bound, accepted).is_ok());
        for bad in [
            DownloadProvenance::new(902, 20, 30, false),
            DownloadProvenance::new(901, 21, 30, false),
            DownloadProvenance::new(901, 20, 31, false),
            DownloadProvenance::new(901, 20, 30, true),
        ] {
            assert!(bind_provenance(901, &mut bound, bad).is_err());
            assert_eq!(bound, Some(accepted));
        }
        let mut private = None;
        assert!(bind_provenance(
            901,
            &mut private,
            DownloadProvenance::new(901, 20, 30, true)
        )
        .is_err());
        assert_eq!(private, None);
    }
    #[test]
    fn admits_only_numeric_loopback_origin_and_explicit_policy() {
        let directory = std::env::temp_dir();
        for origin in [
            "https://127.0.0.1:8080",
            "http://localhost:8080",
            "http://127.0.0.1:8080/path",
            "http://127.0.0.1:0",
            "http://127.0.0.1:99999",
            "http://user@127.0.0.1:8080",
            "http://[::1]:8080?x",
        ] {
            assert!(
                Config::parse(&format!("{origin} {} continue-normal", directory.display()))
                    .is_err()
            );
        }
        assert!(Config::parse(&format!(
            "http://127.0.0.1:8080 {} private",
            directory.display()
        ))
        .is_err());
        assert!(Config::parse("http://127.0.0.1:8080 relative continue-normal").is_err());
        let a = Config::parse(&format!(
            "http://127.0.0.1:8080 {} continue-normal",
            directory.display()
        ))
        .unwrap();
        let b = Config::parse(&format!(
            "http://[::1]:8080 {} baseline",
            directory.display()
        ))
        .unwrap();
        assert_ne!(a.filename, b.filename);
        assert!(a.output.is_absolute());
        assert_eq!(
            a.fixture_url(),
            format!("{}/download/{}", a.origin, a.filename)
        );
        assert_eq!(a.output.file_name().unwrap().to_str().unwrap(), a.filename);
    }
}

#[cfg(test)]
#[path = "download_probe_quit_tests.rs"]
mod quit_tests;

#[cfg(test)]
#[path = "download_probe_filename_tests.rs"]
mod filename_tests;


#[cfg(test)]
#[gpui::test]
fn installed_download_service_excludes_probe_without_waking_or_creating_service(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::quit::reset_for_test(cx);
        // A synthetic idle probe global avoids starting any task in this test.
        cx.set_global(Probe(Rc::new(RefCell::new(State::default()))));
        assert!(can_arm(cx));
        assert!(!cx.has_global::<crate::downloads::AppDownloads>());
        cx.set_global(crate::downloads::AppDownloads::default());
        assert!(!can_arm(cx));
        assert!(cx.global::<Probe>().0.borrow().run.is_none());
        crate::quit::reset_for_test(cx);
    });
}

#[cfg(test)]
#[gpui::test]
fn used_legacy_probe_stays_ineligible_even_without_descriptor(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::quit::reset_for_test(cx);
        admission::reset();
        assert!(idle_for_app_downloads(cx));
        let mut browser = browser_core::Browser::new();
        let config = Config::parse(&format!("http://127.0.0.1:18473 {} continue-normal", std::env::temp_dir().display())).unwrap();
        let tab = browser.new_tab(config.fixture_url(), std::time::SystemTime::UNIX_EPOCH);
        let source = admission::Source {
            identity: crate::pages::PageIdentity { tab, profile: browser.active_user_profile_id(), generation: 1 },
            space: browser.space().id,
            storage: crate::pages::StorageSelection::for_profile(browser.active_user_profile_id()), parent: 1,
        };
        let descriptor = admission::try_install(config.clone(), source.clone(), 90).unwrap();
        assert!(admission::try_install(config.clone(), source, 91).is_err());
        admission::fail(&descriptor);
        cx.set_global(Probe(Rc::new(RefCell::new(State { run: Some(Run {
            config, admission: descriptor, owner: 90, provenance: None, items: Vec::new(), resolved: HashSet::new(), controls: Vec::new(), failure: Some("previous failed run".into()), sequence: 0, retirement_failed: true,
        }), unclaimed: Vec::new() }))));
        admission::reset(); // Only test code can remove a descriptor.
        assert!(!idle_for_app_downloads(cx));
        assert!(crate::download_acceptance::preflight(cx).is_err());
        assert!(!cx.has_global::<crate::downloads::AppDownloads>());
    });
}
