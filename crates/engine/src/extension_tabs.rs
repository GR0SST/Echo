//! Authoritative model snapshots for the source-built extension tabs bridge.
//! Model keys are opaque strings; native Chromium session IDs never escape as
//! Rust model IDs. Hidden service views and extension popups are not tabs.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionWindow {
    pub key: String,
    pub kind: ExtensionWindowKind,
    pub state: ExtensionWindowState,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub incognito: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionWindowKind {
    Normal,
    Popup,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionWindowState {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionTab {
    pub key: String,
    pub window: String,
    /// Present only for the current primary Alloy page of this model tab.
    /// CEF checks the browser belongs to the exact submitting profile.
    pub browser_id: Option<i32>,
    pub index: u32,
    pub active: bool,
    pub highlighted: bool,
    pub pinned: bool,
    pub muted: bool,
    pub audible: bool,
    pub auto_discardable: bool,
    pub discarded: bool,
    pub frozen: bool,
    pub group_id: i32,
    pub split_view_id: i32,
    pub opener_key: Option<String>,
    pub status: ExtensionTabStatus,
    pub url: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub last_accessed: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionTabStatus {
    Loading,
    Complete,
    Unloaded,
}

/// One complete profile, including dormant tabs and every surviving GPUI root.
/// Submit through an existing Alloy owner in that exact profile. Incognito is
/// a separate context and cannot be merged with normal-profile snapshots.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionTabsSnapshot {
    pub profile_key: String,
    pub windows: Vec<ExtensionWindow>,
    pub tabs: Vec<ExtensionTab>,
    pub focused_window: Option<String>,
    pub last_focused_window: Option<String>,
}

impl ExtensionTabsSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.profile_key.is_empty()
            || self.profile_key.len() > 128
            || self.profile_key.contains('\0')
        {
            return Err("Extension snapshots require an explicit profile key".into());
        }
        if self.windows.len() > 128 || self.tabs.len() > 10_000 {
            return Err("Extension model snapshot exceeds its bounds".into());
        }
        let key_valid = |key: &str| !key.is_empty() && key.len() <= 128 && !key.contains('\0');
        let mut windows = HashSet::new();
        let mut privacy = None;
        for window in &self.windows {
            if !key_valid(&window.key) || !windows.insert(window.key.as_str()) {
                return Err("Extension windows require unique nonempty model keys".into());
            }
            if privacy.is_some_and(|incognito| incognito != window.incognito) {
                return Err("Extension snapshots cannot mix normal and private contexts".into());
            }
            privacy = Some(window.incognito);
        }
        for selected in [&self.focused_window, &self.last_focused_window]
            .into_iter()
            .flatten()
        {
            if !windows.contains(selected.as_str()) {
                return Err("Extension focus references an absent model window".into());
            }
        }
        if self.focused_window.is_some() && self.focused_window != self.last_focused_window {
            return Err("The focused window must also be the last-focused window".into());
        }
        let mut tabs = HashSet::new();
        let mut positions = HashSet::new();
        let mut active = HashSet::new();
        let mut browsers = HashSet::new();
        for tab in &self.tabs {
            if !key_valid(&tab.key) || !tabs.insert(tab.key.as_str()) {
                return Err("Extension tabs require unique nonempty model keys".into());
            }
            if !windows.contains(tab.window.as_str()) || !positions.insert((&tab.window, tab.index))
            {
                return Err("Extension tabs require an existing window and unique index".into());
            }
            if tab.active && (!tab.highlighted || !active.insert(&tab.window)) {
                return Err(
                    "A model window has at most one active tab, which is highlighted".into(),
                );
            }
            if tab
                .browser_id
                .is_some_and(|id| id <= 0 || !browsers.insert(id))
            {
                return Err("An Alloy page may represent only one model tab".into());
            }
            if tab.url.len() > 2 * 1024 * 1024
                || tab.title.len() > 1024 * 1024
                || tab
                    .favicon_url
                    .as_ref()
                    .is_some_and(|url| url.len() > 2 * 1024 * 1024)
                || !tab.last_accessed.is_finite()
                || tab.last_accessed < 0.
            {
                return Err("Extension tab metadata exceeds its bounds".into());
            }
        }
        if self.tabs.iter().any(|tab| {
            tab.opener_key
                .as_ref()
                .is_some_and(|opener| opener == &tab.key || !tabs.contains(opener.as_str()))
        }) {
            return Err("Extension tab opener must be another tab in the same profile".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> ExtensionTabsSnapshot {
        ExtensionTabsSnapshot {
            profile_key: "default".into(),
            windows: vec![ExtensionWindow {
                key: "window:18446744073709551615".into(),
                kind: ExtensionWindowKind::Normal,
                state: ExtensionWindowState::Normal,
                left: 100,
                top: 100,
                width: 1000,
                height: 760,
                incognito: false,
            }],
            tabs: vec![ExtensionTab {
                key: "tab:18446744073709551615".into(),
                window: "window:18446744073709551615".into(),
                browser_id: None,
                index: 0,
                active: true,
                highlighted: true,
                pinned: false,
                muted: false,
                audible: false,
                auto_discardable: true,
                discarded: true,
                frozen: false,
                group_id: -1,
                split_view_id: -1,
                status: ExtensionTabStatus::Unloaded,
                opener_key: None,
                url: "https://example.com/".into(),
                title: "Example".into(),
                favicon_url: None,
                last_accessed: 1.,
            }],
            focused_window: Some("window:18446744073709551615".into()),
            last_focused_window: Some("window:18446744073709551615".into()),
        }
    }
    #[test]
    fn dormant_tabs_and_full_width_model_keys_round_trip_without_numeric_coercion() {
        let model = snapshot();
        model.validate().unwrap();
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("tab:18446744073709551615"));
        assert_eq!(
            serde_json::from_str::<ExtensionTabsSnapshot>(&json).unwrap(),
            model
        );
    }
    #[test]
    fn renderer_attachment_changes_do_not_change_model_identity() {
        let mut model = snapshot();
        let key = model.tabs[0].key.clone();
        for browser in [Some(17), None, Some(23)] {
            model.tabs[0].browser_id = browser;
            model.validate().unwrap();
            assert_eq!(model.tabs[0].key, key);
        }
    }
    #[test]
    fn rejects_ambiguous_identity_selection_and_privacy_boundaries() {
        let mut model = snapshot();
        model.tabs.push(model.tabs[0].clone());
        assert!(model.validate().is_err());
        model.tabs[1].key = "tab:second".into();
        model.tabs[1].index = 1;
        assert!(model.validate().is_err()); // Two active tabs.
        model.tabs[1].active = false;
        model.validate().unwrap();
        model.tabs[0].browser_id = Some(17);
        model.tabs[1].browser_id = Some(17);
        assert!(model.validate().is_err());
        model.tabs[1].browser_id = None;
        model.windows.push(ExtensionWindow {
            key: "private".into(),
            incognito: true,
            ..model.windows[0].clone()
        });
        assert!(model.validate().is_err());
    }
    #[test]
    fn focus_must_refer_to_a_surviving_window_and_survives_application_deactivation() {
        let mut model = snapshot();
        model.focused_window = None;
        model.validate().unwrap();
        model.windows.clear();
        model.tabs.clear();
        assert!(model.validate().is_err());
        model.last_focused_window = None;
        model.validate().unwrap();
    }
}
