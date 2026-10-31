//! Document-start customizations. CEF and renderer IPC stay inside the engine.
use cef::*;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap};

const KEY: &str = "Echo.LocalBoosts.v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteBoost {
    pub domain: String,
    pub css: String,
    pub javascript: String,
}
thread_local! {
    static CONFIG: RefCell<HashMap<i32, Vec<SiteBoost>>> = RefCell::new(HashMap::new());
}
fn decode(value: &str) -> Vec<SiteBoost> {
    serde_json::from_str(value).unwrap_or_default()
}
pub(crate) fn created(browser: Option<&Browser>, extra: Option<&DictionaryValue>) {
    let (Some(browser), Some(extra)) = (browser, extra) else {
        return;
    };
    let key = CefString::from(KEY);
    if extra.get_type(Some(&key)) == ValueType::STRING {
        let boosts = decode(&CefString::from(&extra.string(Some(&key))).to_string());
        CONFIG.with_borrow_mut(|config| {
            config.insert(browser.identifier(), boosts);
        });
    }
}
pub(crate) fn destroyed(browser: Option<&Browser>) {
    if let Some(browser) = browser {
        CONFIG.with_borrow_mut(|config| {
            config.remove(&browser.identifier());
        });
    }
}
pub(crate) fn extra(boosts: &[SiteBoost]) -> Option<DictionaryValue> {
    let extra = dictionary_value_create()?;
    extra.set_string(
        Some(&CefString::from(KEY)),
        Some(&CefString::from(
            serde_json::to_string(boosts).ok()?.as_str(),
        )),
    );
    Some(extra)
}
fn sources(boosts: &[SiteBoost], url: &str, run_javascript: bool) -> Vec<String> {
    let host = url::Url::parse(url)
        .ok()
        .filter(|url| matches!(url.scheme(), "https" | "http"))
        .and_then(|url| url.host_str().map(str::to_owned));
    let matching: Vec<_> = boosts
        .iter()
        .filter(|boost| host.as_deref() == Some(boost.domain.as_str()))
        .collect();
    let css = matching
        .iter()
        .map(|boost| boost.css.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    // Constructed sheets apply before a root exists and do not require a
    // page's inline-style CSP permission. Preserve all page-owned sheets.
    let script = format!(
        r#"(() => {{
const key = Symbol.for('echo.local.boost.stylesheet');
const css = {};
let sheet = document[key];
if (!css) {{
 if (sheet) {{ document.adoptedStyleSheets = document.adoptedStyleSheets.filter(value => value !== sheet); delete document[key]; }}
 return;
}}
if (!sheet) {{ sheet = new CSSStyleSheet(); document[key] = sheet; }}
sheet.replaceSync(css);
if (!document.adoptedStyleSheets.includes(sheet)) {{ document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]; }}
}})();"#,
        serde_json::to_string(&css).unwrap()
    );
    let mut sources = vec![script];
    if run_javascript {
        sources.extend(
            matching
                .iter()
                .filter(|boost| !boost.javascript.trim().is_empty())
                .map(|boost| boost.javascript.clone()),
        );
    }
    sources
}

pub(crate) fn context_created(browser: &Browser, frame: &Frame, context: &V8Context) {
    let scripts = CONFIG.with_borrow(|configs| {
        configs
            .get(&browser.identifier())
            .map(|boosts| sources(boosts, &CefString::from(&frame.url()).to_string(), true))
            .unwrap_or_default()
    });
    for script in scripts {
        // User code may reenter CEF. Do not retain a renderer-state borrow.
        // Separate evaluations also isolate syntax errors in different Boosts.
        let (mut result, mut exception) = (None, None);
        let ok = context.eval(
            Some(&CefString::from(script.as_str())),
            Some(&CefString::from("echo-local-boost")),
            1,
            Some(&mut result),
            Some(&mut exception),
        );
        if ok == 0 {
            eprintln!("[boost] document-start script failed");
        }
    }
}

/// Update CSS immediately; edited JavaScript runs on the next document only.
pub(crate) fn update(browser: &Browser, boosts: &[SiteBoost]) {
    let mut identifiers = CefStringList::new();
    browser.frame_identifiers(Some(&mut identifiers));
    for identifier in identifiers {
        let Some(frame) = browser.frame_by_identifier(Some(&CefString::from(identifier.as_str())))
        else {
            continue;
        };
        let Some(mut message) = process_message_create(Some(&CefString::from(KEY))) else {
            continue;
        };
        if let Some(arguments) = message.argument_list() {
            arguments.set_string(
                0,
                Some(&CefString::from(
                    serde_json::to_string(boosts).unwrap().as_str(),
                )),
            );
            frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        }
    }
}
pub(crate) fn receive(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source_process: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    if source_process != ProcessId::BROWSER {
        return false;
    }
    let (Some(browser), Some(frame), Some(message)) = (browser, frame, message) else {
        return false;
    };
    if CefString::from(&message.name()).to_string() != KEY {
        return false;
    }
    let Some(arguments) = message.argument_list() else {
        return true;
    };
    if arguments.size() != 1 || arguments.get_type(0) != ValueType::STRING {
        return true;
    }
    let boosts = decode(&CefString::from(&arguments.string(0)).to_string());
    // Edited Boosts recolor the page; chrome takes its colors again right away.
    let script = sources(&boosts, &CefString::from(&frame.url()).to_string(), false).remove(0)
        + "\n;document[Symbol.for('echo.document.color.refresh')]?.();";
    CONFIG.with_borrow_mut(|config| {
        config.insert(browser.identifier(), boosts);
    });
    frame.execute_java_script(
        Some(&CefString::from(script.as_str())),
        Some(&CefString::from("echo-local-boost")),
        1,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_start_script_filters_domains_and_quotes_source() {
        let boosts = vec![SiteBoost {
            domain: "example.com".into(),
            css: "body { content: '\"'; }".into(),
            javascript: "window.boost = 'ok';".into(),
        }];
        let script = sources(&boosts, "https://example.com/path", true).join("\n");
        assert!(script.contains("adoptedStyleSheets"));
        assert!(script.contains("window.boost"));
        assert!(!sources(&boosts, "https://example.com.evil.test", true)
            .join("\n")
            .contains("window.boost"));
        assert!(!sources(&boosts, "file:///example.com", true)
            .join("\n")
            .contains("window.boost"));
        assert!(!sources(&boosts, "https://example.com", false)
            .join("\n")
            .contains("window.boost"));
    }
}
