//! Install-local keyboard customizations. Display names never contain profile data.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    /// A missing entry inherits the default; None explicitly removes a binding.
    pub bindings: BTreeMap<String, Option<String>>,
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
    pub fn reset(&mut self, id: &str) {
        self.bindings.remove(id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
