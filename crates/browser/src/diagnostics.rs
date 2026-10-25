//! Local, explicit clipboard diagnostics. No account or telemetry identifiers.
use crate::window::BrowserWindow;
use gpui::{App, ClipboardItem, Context, Global};

/// Resolved once at startup, including an explicitly overridden data directory.
/// Utility actions must not re-run profile migration or consult changing env vars.
pub(crate) struct DataDirectory(pub std::path::PathBuf);
impl Global for DataDirectory {}

pub(crate) fn reveal_data(cx: &mut App) {
    if crate::quit::is_quitting() { return; }
    let Some(directory) = cx.try_global::<DataDirectory>() else { return; };
    if let Ok(url) = url::Url::from_directory_path(&directory.0) {
        cx.open_url(url.as_str());
    }
}

impl BrowserWindow {
    pub(crate) fn copy_browser_info(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        let inventory = self
            .pages
            .as_ref()
            .map(|pages| pages.borrow().diagnostic_inventory())
            .unwrap_or_default();
        let profiles = inventory
            .iter()
            .map(|(_, profile, _)| *profile)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let entries = inventory
            .into_iter()
            .map(|(tab, profile, visible)| {
                serde_json::json!({
                    "tabId": tab, "profileId": profile, "visible": visible,
                })
            })
            .collect::<Vec<_>>();
        let info = serde_json::json!({
            "echoVersion": option_env!("ECHO_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
            "build": option_env!("ECHO_BUILD_NUMBER").unwrap_or("1"),
            "localDiagnostics": cx.try_global::<DataDirectory>().map(|d| d.0.join("Diagnostics").display().to_string()),
            "lastUiHeartbeat": cx.try_global::<DataDirectory>().and_then(|d| std::fs::read_to_string(d.0.join("Diagnostics/heartbeat.txt")).ok()),
            "deviceInfo": { "osVersion": crate::native::operating_system_version() },
            "numProfiles": self.browser.user_profiles().count(),
            "engineInfo": {
                "chromiumVersion": engine::chromium_version(),
                "active_profiles_count": profiles,
                "browser_tab_count": self.browser.all_tabs().count(),
                "native_web_contents_count": entries.len(),
                "native_web_contents_inventory": entries,
            },
        });
        cx.write_to_clipboard(ClipboardItem::new_string(
            serde_json::to_string_pretty(&info).expect("Diagnostic values serialize"),
        ));
        self.close_command_bar(cx);
        self.show_compact_toast("Copied Echo Info!".into(), cx);
    }
}

/// Local metadata only: no browsing URLs, credentials, uploads or telemetry.
pub(crate) fn start(profile: &std::path::Path) {
    use std::io::Write;
    let directory = profile.join("Diagnostics");
    if std::fs::create_dir_all(&directory).is_err() { return; }
    // Bound historical startup/panic metadata; CEF keeps its own local log.
    let cef_log = profile.join("Chromium/cef.log");
    if std::fs::metadata(&cef_log).is_ok_and(|m| m.len() > 5_242_880) {
        let _ = std::fs::rename(&cef_log, profile.join("Chromium/cef.previous.log"));
    }
    let log = directory.join("events.log");
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > 1_048_576) {
        let _ = std::fs::rename(&log, directory.join("events.previous.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
        let _ = writeln!(file, "start {} pid={} version={} build={}", echo_updater::now(), std::process::id(), option_env!("ECHO_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")), option_env!("ECHO_BUILD_NUMBER").unwrap_or("1"));
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            // Panic payloads may contain site data; retain only source/backtrace.
            let _ = writeln!(file, "panic {} at {:?}\n{}", echo_updater::now(), info.location(), std::backtrace::Backtrace::force_capture());
        }
        previous(info);
    }));
}

pub(crate) fn heartbeat(cx: &mut App) {
    cx.spawn(async move |cx| loop {
        if cx.update(|cx| {
            if let Some(data) = cx.try_global::<DataDirectory>() {
                let _ = std::fs::write(data.0.join("Diagnostics/heartbeat.txt"), echo_updater::now().to_string());
            }
        }).is_err() { break; }
        cx.background_executor().timer(std::time::Duration::from_secs(10)).await;
    }).detach();
}
