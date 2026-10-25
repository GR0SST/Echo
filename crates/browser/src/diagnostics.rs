//! Local, explicit clipboard diagnostics. No account or telemetry identifiers.
use crate::window::BrowserWindow;
use gpui::{ClipboardItem, Context};

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
