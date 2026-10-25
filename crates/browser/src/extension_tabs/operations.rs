//! Validate the complete mutation before scheduling any GPUI model edit.
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    method: String,
    arguments: Vec<Value>,
    extension_id: String,
    profile_key: String,
    tabs: BTreeMap<String, String>,
    windows: BTreeMap<String, String>,
    current_window: String,
    active_tab_key: Option<String>,
}
#[derive(Clone, Debug)]
pub(super) struct Mutation {
    pub profile: String,
    pub extension: String,
    pub operation: Operation,
}
#[derive(Clone, Debug)]
pub(super) enum Operation {
    Create {
        window: String,
        url: String,
        active: bool,
        pinned: bool,
        index: Option<i32>,
        opener: Option<String>,
    },
    Update {
        tab: String,
        url: Option<String>,
        active: Option<bool>,
        highlighted: Option<bool>,
        pinned: Option<bool>,
        muted: Option<bool>,
        auto_discardable: Option<bool>,
        opener: Option<String>,
    },
    Remove {
        tabs: Vec<String>,
    },
    Reload {
        tab: String,
        bypass: bool,
    },
}
fn boolean(properties: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    properties
        .get(key)
        .map(|v| v.as_bool().ok_or_else(|| format!("Invalid {key}")))
        .transpose()
}
fn text(properties: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    properties
        .get(key)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("Invalid {key}"))
        })
        .transpose()
}
fn id(value: &Value, map: &BTreeMap<String, String>) -> Result<String, String> {
    value
        .as_i64()
        .filter(|id| *id > 0)
        .and_then(|id| map.get(&id.to_string()))
        .cloned()
        .ok_or_else(|| "Target does not belong to this Echo profile".into())
}
fn properties(args: &[Value]) -> Result<&Map<String, Value>, String> {
    args.iter()
        .find_map(Value::as_object)
        .ok_or_else(|| "Missing mutation properties".into())
}
fn keys(properties: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    if let Some(key) = properties
        .keys()
        .find(|key| !allowed.contains(&key.as_str()))
    {
        Err(format!("Echo does not support the {key} mutation"))
    } else {
        Ok(())
    }
}
impl Mutation {
    pub(super) fn parse(data: Value) -> Result<Self, String> {
        let request: Request = serde_json::from_value(data).map_err(|e| e.to_string())?;
        if request.extension_id.len() != 32
            || !request
                .extension_id
                .bytes()
                .all(|c| (b'a'..=b'p').contains(&c))
            || request.profile_key.is_empty()
            || request.arguments.len() > 3
        {
            return Err("Invalid extension model request".into());
        }
        let target_tab = || {
            request
                .arguments
                .first()
                .filter(|value| value.is_number())
                .map(|value| id(value, &request.tabs))
                .unwrap_or_else(|| {
                    request
                        .active_tab_key
                        .clone()
                        .ok_or_else(|| "No selected tab".into())
                })
        };
        let operation = match request.method.as_str() {
            "tabs.create" => {
                let p = properties(&request.arguments)?;
                keys(
                    p,
                    &[
                        "url",
                        "active",
                        "selected",
                        "pinned",
                        "index",
                        "windowId",
                        "openerTabId",
                    ],
                )?;
                let window = match p.get("windowId") {
                    Some(value) if value.as_i64() == Some(-2) => request.current_window.clone(),
                    Some(value) => id(value, &request.windows)?,
                    None => request.current_window.clone(),
                };
                if window.is_empty() {
                    return Err("No current Echo window".into());
                }
                let index = p
                    .get("index")
                    .map(|v| {
                        v.as_i64()
                            .and_then(|n| i32::try_from(n).ok())
                            .ok_or("Invalid index")
                    })
                    .transpose()?;
                Operation::Create {
                    window,
                    url: text(p, "url")?.unwrap_or_else(|| "about:blank".into()),
                    active: boolean(p, "active")?
                        .or(boolean(p, "selected")?)
                        .unwrap_or(true),
                    pinned: boolean(p, "pinned")?.unwrap_or(false),
                    index,
                    opener: p
                        .get("openerTabId")
                        .map(|v| id(v, &request.tabs))
                        .transpose()?,
                }
            }
            "tabs.update" => {
                let p = properties(&request.arguments)?;
                keys(
                    p,
                    &[
                        "url",
                        "active",
                        "selected",
                        "highlighted",
                        "pinned",
                        "muted",
                        "autoDiscardable",
                        "openerTabId",
                    ],
                )?;
                Operation::Update {
                    tab: target_tab()?,
                    url: text(p, "url")?,
                    active: boolean(p, "active")?.or(boolean(p, "selected")?),
                    highlighted: boolean(p, "highlighted")?,
                    pinned: boolean(p, "pinned")?,
                    muted: boolean(p, "muted")?,
                    auto_discardable: boolean(p, "autoDiscardable")?,
                    opener: p
                        .get("openerTabId")
                        .map(|v| id(v, &request.tabs))
                        .transpose()?,
                }
            }
            "tabs.remove" => {
                let Some(value) = request.arguments.first() else {
                    return Err("Missing tabs".into());
                };
                let values = if let Some(values) = value.as_array() {
                    values.clone()
                } else {
                    vec![value.clone()]
                };
                if values.is_empty() || values.len() > 10_000 {
                    return Err("Invalid removal bounds".into());
                }
                let mut tabs = Vec::new();
                for value in values {
                    let key = id(&value, &request.tabs)?;
                    if !tabs.contains(&key) {
                        tabs.push(key);
                    }
                }
                Operation::Remove { tabs }
            }
            "tabs.reload" => {
                let p = request.arguments.iter().find_map(Value::as_object);
                if let Some(p) = p {
                    keys(p, &["bypassCache"])?;
                }
                Operation::Reload {
                    tab: target_tab()?,
                    bypass: p
                        .map(|p| boolean(p, "bypassCache"))
                        .transpose()?
                        .flatten()
                        .unwrap_or(false),
                }
            }
            _ => return Err("Unknown extension model mutation".into()),
        };
        Ok(Self {
            profile: request.profile_key,
            extension: request.extension_id,
            operation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn request(method: &str, arguments: Value) -> Value {
        json!({"method":method,"arguments":arguments,"extensionId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "profileKey":"default","tabs":{"17":"model:one","23":"model:two"},
            "windows":{"31":"window:one"},"currentWindow":"window:one","activeTabKey":"model:one"})
    }
    #[test]
    fn removal_validates_every_id_before_any_action_and_deduplicates() {
        assert!(Mutation::parse(request("tabs.remove", json!([[17, 999]]))).is_err());
        let parsed = Mutation::parse(request("tabs.remove", json!([[17, 23, 17]]))).unwrap();
        assert!(
            matches!(parsed.operation,Operation::Remove{tabs} if tabs == ["model:one","model:two"])
        );
    }
    #[test]
    fn missing_optional_id_uses_the_native_model_selection() {
        let parsed =
            Mutation::parse(request("tabs.update", json!([null,{"active":true}]))).unwrap();
        assert!(
            matches!(parsed.operation,Operation::Update{tab,active:Some(true),..} if tab == "model:one")
        );
        let parsed =
            Mutation::parse(request("tabs.reload", json!([{"bypassCache":true}]))).unwrap();
        assert!(
            matches!(parsed.operation,Operation::Reload{tab,bypass:true} if tab == "model:one")
        );
    }
    #[test]
    fn invalid_properties_and_foreign_windows_are_rejected_before_creation() {
        assert!(Mutation::parse(request("tabs.create", json!([{"windowId":999}]))).is_err());
        assert!(Mutation::parse(request("tabs.create", json!([{"active":"yes"}]))).is_err());
        assert!(Mutation::parse(request(
            "tabs.update",
            json!([17,{"url":"https://example.com/","invented":true}])
        ))
        .is_err());
    }
}
