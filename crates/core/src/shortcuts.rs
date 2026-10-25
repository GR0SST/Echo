//! Install-local keyboard customizations. Display names never contain profile data.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How a browser binding competes with keyboard handlers in the focused page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutPriority {
    PreferWebsites,
    UseBoth,
    #[default]
    #[serde(other)]
    PreferBrowser,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    /// A missing entry inherits the default; None explicitly removes a binding.
    pub bindings: BTreeMap<String, Option<String>>,
    /// Missing policies inherit the action's default, independently of its key.
    pub priorities: BTreeMap<String, ShortcutPriority>,
}
impl Shortcuts {
    pub fn effective<'a>(&'a self, id: &str, default: Option<&'a str>) -> Option<&'a str> {
        self.bindings
            .get(id)
            .map_or(default, |value| value.as_deref())
    }
    pub fn set(&mut self, id: impl Into<String>, binding: Option<String>) {
        self.bindings.insert(id.into(), binding);
    }
    pub fn priority(&self, id: &str, default: ShortcutPriority) -> ShortcutPriority {
        self.priorities.get(id).copied().unwrap_or(default)
    }
    pub fn set_priority(&mut self, id: impl Into<String>, priority: ShortcutPriority) {
        self.priorities.insert(id.into(), priority);
    }
    pub fn reset(&mut self, id: &str) {
        self.bindings.remove(id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_bindings_inherit_policies_and_policy_changes_do_not_reassign_keys() {
        let mut value: Shortcuts =
            serde_json::from_str(r#"{"bindings":{"Copy URL":"cmd-r","Archive Tab":null}}"#)
                .unwrap();
        assert_eq!(
            value.priority("Copy URL", ShortcutPriority::PreferWebsites),
            ShortcutPriority::PreferWebsites
        );
        value.set_priority("Copy URL", ShortcutPriority::UseBoth);
        value.set_priority("future-action", ShortcutPriority::PreferWebsites);
        let round_trip: Shortcuts =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(value, round_trip);
        assert_eq!(round_trip.effective("Copy URL", None), Some("cmd-r"));
        assert_eq!(round_trip.effective("Archive Tab", Some("cmd-w")), None);
        assert_eq!(
            round_trip.priority("Copy URL", ShortcutPriority::PreferBrowser),
            ShortcutPriority::UseBoth
        );
        value.reset("Copy URL");
        assert_eq!(
            value.effective("Copy URL", Some("cmd-shift-c")),
            Some("cmd-shift-c")
        );
        assert_eq!(
            value.priority("Copy URL", ShortcutPriority::PreferBrowser),
            ShortcutPriority::UseBoth
        );
    }
    #[test]
    fn removal_reset_and_unknown_actions_round_trip_without_losing_intent() {
        let mut value = Shortcuts::default();
        value.set("Copy URL", Some("ctrl-alt-1".into()));
        value.set("Archive Tab", None);
        value.set("future-action", Some("f9".into()));
        let mut value: Shortcuts =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(
            value.effective("Copy URL", Some("cmd-shift-c")),
            Some("ctrl-alt-1")
        );
        assert_eq!(value.effective("Archive Tab", Some("cmd-w")), None);
        value.reset("Archive Tab");
        assert_eq!(value.effective("Archive Tab", Some("cmd-w")), Some("cmd-w"));
        assert_eq!(value.effective("future-action", None), Some("f9"));
    }
}
