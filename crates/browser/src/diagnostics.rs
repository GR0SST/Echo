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
            "echoVersion": env!("CARGO_PKG_VERSION"),
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
