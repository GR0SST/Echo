//! Rebind existing browser actions while retaining their original focus predicates.
use browser_core::shortcuts::Shortcuts;
use gpui::{Action, App, AsKeystroke, KeyBinding, Keystroke};
use std::sync::LazyLock;
pub(crate) static CATALOG: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/settings-shortcuts.json")).unwrap()
});
struct Installed {
    baseline: Vec<KeyBinding>,
    settings: Shortcuts,
    page: Vec<engine::Shortcut>,
    priorities: Vec<(engine::Shortcut, engine::ShortcutPriority)>,
    revision: u64,
}
impl gpui::Global for Installed {}

pub(crate) fn init(cx: &mut App) {
    let baseline: Vec<_> = cx.key_bindings().borrow().bindings().cloned().collect();
    let page = crate::window::page_shortcuts();
    let priorities = page_priorities(&Shortcuts::default(), &baseline, &baseline, &page);
    cx.set_global(Installed {
        baseline,
        settings: Shortcuts::default(),
        page,
        priorities,
        revision: 0,
    });
}
pub(crate) fn default_key(display: &str) -> Option<String> {
    if display == "---" {
        return None;
    }
    let mut result = String::new();
    for (symbol, modifier) in [
        ('⌃', "ctrl-"),
        ('⌥', "alt-"),
        ('⇧', "shift-"),
        ('⌘', "cmd-"),
    ] {
        if display.contains(symbol) {
            result.push_str(modifier);
        }
    }
    let key: String = display.chars().filter(|ch| !"⌃⌥⇧⌘".contains(*ch)).collect();
    let lower = key.to_lowercase();
    result.push_str(match key.as_str() {
        "←" => "left",
        "→" => "right",
        "↑" => "up",
        "↓" => "down",
        "⇥" => "tab",
        "⎋" => "escape",
        "⎵" => "space",
        _ => &lower,
    });
    Keystroke::parse(&result).ok().map(|key| key.unparse())
}
pub(crate) fn display(key: Option<&str>) -> String {
    let Some(key) = key.and_then(|key| Keystroke::parse(key).ok()) else {
        return "---".into();
    };
    let m = key.modifiers;
    format!(
        "{}{}{}{}{}",
        if m.control { "⌃" } else { "" },
        if m.alt { "⌥" } else { "" },
        if m.shift { "⇧" } else { "" },
        if m.platform { "⌘" } else { "" },
        match key.key.as_str() {
            "left" => "←".into(),
            "right" => "→".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            "tab" => "⇥".into(),
            "escape" => "⎋".into(),
            "space" => "⎵".into(),
            _ => key.key.to_uppercase(),
        }
    )
}
fn unbound_action(label: &str) -> Option<Box<dyn Action>> {
    use crate::{menus as m, window as w};
    Some(match label {
        "Duplicate" => Box::new(w::DuplicateTab),
        "New Folder…" => Box::new(w::NewFolder),
        "New Space…" => Box::new(w::NewSpace),
        "Manage Spaces…" => Box::new(m::ManageSpaces),
        "Copy URL as Quote" => Box::new(w::CopyUrlAsQuote),
        "Collapse All Folders" => Box::new(w::CollapseAllFolders),
        "Expand All Folders" => Box::new(w::ExpandAllFolders),
        "View Archive" => Box::new(m::ViewArchive),
        "Clear Archive" => Box::new(m::ClearArchive),
        "Rename Current Item" => Box::new(m::RenameTab),
        "Reset Tab to Original URL" => Box::new(m::ResetPinnedTab),
        "Clear Cookies and Refresh" => Box::new(m::ClearCookiesAndRefresh),
        _ => return None,
    })
}
fn anchor<'a>(baseline: &'a [KeyBinding], default: Option<&str>) -> Option<&'a KeyBinding> {
    let key = Keystroke::parse(default?).ok()?;
    baseline.iter().find(|binding| {
        binding.keystrokes().len() == 1
            && binding.keystrokes()[0].as_keystroke() == &key
            && (binding.predicate().is_none()
                || binding
                    .predicate()
                    .is_some_and(|p| p.to_string() == "BrowserWindow"))
    })
}
pub(crate) fn supported(id: &str, default: Option<&str>, cx: &App) -> bool {
    CATALOG.iter().any(|(label, _)| label == id)
        && (anchor(&cx.global::<Installed>().baseline, default).is_some()
            || unbound_action(id).is_some())
}
pub(crate) fn apply(settings: &Shortcuts, cx: &mut App) {
    let Some(installed) = cx.try_global::<Installed>() else {
        return;
    };
    if installed.settings == *settings {
        return;
    }
    let revision = installed.revision.wrapping_add(1);
    let baseline = installed.baseline.clone();
    let mut bindings = baseline.clone();
    let mut page = crate::window::page_shortcuts();
    let specs = settings
        .bindings
        .iter()
        .filter_map(|(id, value)| {
            if value
                .as_ref()
                .is_some_and(|value| Keystroke::parse(value).is_err())
            {
                return None;
            }
            let (_, notation) = CATALOG.iter().find(|(label, _)| label == id)?;
            let default = default_key(notation).and_then(|key| Keystroke::parse(&key).ok());
            let original = anchor(
                &baseline,
                default.as_ref().map(|key| key.unparse()).as_deref(),
            );
            let action = original
                .map(|b| b.action().boxed_clone())
                .or_else(|| unbound_action(id))?;
            Some((value, default, original, action))
        })
        .collect::<Vec<_>>();
    // Remove all changed defaults before adding custom keys. This permits swaps
    // and assignments to a key that another customization has released.
    bindings.retain(|binding| {
        !specs.iter().any(|(_, default, _, action)| {
            default.as_ref().is_some_and(|key| {
                binding.keystrokes().len() == 1
                    && binding.keystrokes()[0].as_keystroke() == key
                    && binding.action().partial_eq(action.as_ref())
            })
        })
    });
    page.retain(|key| {
        !specs
            .iter()
            .any(|(_, default, _, _)| default.as_ref() == Some(&to_keystroke(key)))
    });
    for (value, default, original, action) in specs {
        let Some(value) = value else {
            continue;
        };
        let key = Keystroke::parse(value).expect("validated custom shortcut");
        // Leave a still-conflicting browser binding in control. Text editing
        // bindings retain their separate predicates and never block assignment.
        if bindings.iter().any(|binding| {
            binding.keystrokes().len() == 1
                && binding.keystrokes()[0].as_keystroke() == &key
                && (binding.predicate().is_none()
                    || binding
                        .predicate()
                        .is_some_and(|p| p.to_string() == "BrowserWindow"))
        }) {
            continue;
        }
        let mut predicates = if let Some(default) = default.as_ref() {
            baseline
                .iter()
                .filter(|binding| {
                    binding.keystrokes().len() == 1
                        && binding.keystrokes()[0].as_keystroke() == default
                        && binding.action().partial_eq(action.as_ref())
                })
                .map(|binding| binding.predicate())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if predicates.is_empty() {
            predicates.push(
                original
                    .map(|binding| binding.predicate())
                    .unwrap_or_else(|| {
                        Some(std::rc::Rc::new(
                            gpui::KeyBindingContextPredicate::parse("BrowserWindow").unwrap(),
                        ))
                    }),
            );
        }
        for predicate in predicates {
            if let Ok(binding) = KeyBinding::load(
                value,
                action.boxed_clone(),
                predicate,
                false,
                None,
                &gpui::DummyKeyboardMapper,
            ) {
                bindings.push(binding);
            }
        }
        page.push(to_engine(&key));
    }
    let priorities = page_priorities(settings, &baseline, &bindings, &page);
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
    cx.set_global(Installed {
        baseline,
        settings: settings.clone(),
        page,
        priorities,
        revision,
    });
    crate::menus::refresh_shortcuts(cx);
}
pub(crate) type PageConfiguration = (
    u64,
    Vec<engine::Shortcut>,
    Vec<(engine::Shortcut, engine::ShortcutPriority)>,
);
pub(crate) fn page(cx: &App) -> Option<PageConfiguration> {
    cx.try_global::<Installed>()
        .map(|s| (s.revision, s.page.clone(), s.priorities.clone()))
}

pub(crate) fn default_priority(id: &str) -> browser_core::shortcuts::ShortcutPriority {
    use browser_core::shortcuts::ShortcutPriority::*;
    match id {
        "Refresh the Page"
        | "Force Refresh the Page"
        | "Find…"
        | "Find Next"
        | "Find Previous"
        | "Previous Tab"
        | "Go to Sidebar Item 1"
        | "Go to Sidebar Item 2"
        | "Go to Sidebar Item 3"
        | "Go to Sidebar Item 4"
        | "Go to Sidebar Item 5"
        | "Go to Sidebar Item 6"
        | "Go to Sidebar Item 7"
        | "Go to Sidebar Item 8"
        | "Go to last Sidebar Item" => PreferWebsites,
        "Show/Hide Sidebar" | "Print" | "Add Split View" | "Pin/Unpin Tab" => UseBoth,
        _ => PreferBrowser,
    }
}
fn page_priorities(
    settings: &Shortcuts,
    baseline: &[KeyBinding],
    bindings: &[KeyBinding],
    keys: &[engine::Shortcut],
) -> Vec<(engine::Shortcut, engine::ShortcutPriority)> {
    keys.iter()
        .map(|key| {
            let physical = to_keystroke(key);
            let priority = CATALOG
                .iter()
                .filter_map(|(id, notation)| {
                    let default = default_key(notation);
                    let effective = settings.effective(id, default.as_deref())?;
                    if Keystroke::parse(effective).ok()? != physical {
                        return None;
                    }
                    let action = anchor(baseline, default.as_deref())
                        .map(|binding| binding.action().boxed_clone())
                        .or_else(|| unbound_action(id))?;
                    if !bindings.iter().any(|binding| {
                        binding.keystrokes().len() == 1
                            && binding.keystrokes()[0].as_keystroke() == &physical
                            && binding.action().partial_eq(action.as_ref())
                    }) {
                        return None;
                    }
                    let rank = u8::from(settings.bindings.contains_key(id)) * 2
                        + u8::from(settings.priorities.contains_key(id));
                    Some((rank, settings.priority(id, default_priority(id))))
                })
                .max_by_key(|(rank, _)| *rank)
                .map(|(_, priority)| priority)
                .unwrap_or_default();
            use browser_core::shortcuts::ShortcutPriority as Model;
            let priority = match priority {
                Model::PreferBrowser => engine::ShortcutPriority::PreferBrowser,
                Model::PreferWebsites => engine::ShortcutPriority::PreferWebsites,
                Model::UseBoth => engine::ShortcutPriority::UseBoth,
            };
            (key.clone(), priority)
        })
        .collect()
}
pub(crate) fn to_engine(key: &Keystroke) -> engine::Shortcut {
    engine::Shortcut {
        cmd: key.modifiers.platform,
        ctrl: key.modifiers.control,
        alt: key.modifiers.alt,
        shift: key.modifiers.shift,
        key: key.key.clone(),
    }
}
fn to_keystroke(key: &engine::Shortcut) -> Keystroke {
    let mut result = Keystroke::parse(&key.key).unwrap();
    result.modifiers.platform = key.cmd;
    result.modifiers.control = key.ctrl;
    result.modifiers.alt = key.alt;
    result.modifiers.shift = key.shift;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_notation_has_a_canonical_representation_and_retains_minus_arrows_and_function_keys() {
        for (_, key) in CATALOG.iter().filter(|(_, key)| key != "---") {
            let canonical = default_key(key).expect("bundled shortcut must parse");
            assert_eq!(default_key(&display(Some(&canonical))), Some(canonical));
        }
        assert_eq!(
            default_key("⇧⌘C"),
            Some(Keystroke::parse("cmd-shift-c").unwrap().unparse())
        );
    }
}

#[gpui::test]
fn custom_global_shortcuts_retain_their_browser_and_global_focus_contexts(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(crate::init);
    cx.update(|cx| {
        let mut settings = Shortcuts::default();
        settings.set("Blank Window", Some("ctrl-alt-2".into()));
        apply(&settings, cx);
        let map = cx.key_bindings();
        let contexts = map
            .borrow()
            .bindings()
            .filter(|binding| {
                binding.action().partial_eq(&crate::menus::BlankWindow)
                    && binding.keystrokes().len() == 1
                    && binding.keystrokes()[0].as_keystroke()
                        == &Keystroke::parse("ctrl-alt-2").unwrap()
            })
            .map(|binding| binding.predicate().map(|predicate| predicate.to_string()))
            .collect::<Vec<_>>();
        assert!(
            contexts.contains(&None),
            "a Settings/auxiliary window still needs the global binding: {contexts:?}"
        );
        assert!(
            contexts.contains(&Some("BrowserWindow".into())),
            "browser-specific dispatch is retained: {contexts:?}"
        );
        apply(&Shortcuts::default(), cx);
        assert!(!cx
            .key_bindings()
            .borrow()
            .bindings()
            .any(|binding| binding.keystrokes().len() == 1
                && binding.keystrokes()[0].as_keystroke()
                    == &Keystroke::parse("ctrl-alt-2").unwrap()));
    });
}

#[gpui::test]
fn resetting_to_an_earlier_value_has_a_new_revision_for_covered_page_hosts(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(crate::init);
    cx.update(|cx| {
        let mut settings = Shortcuts::default();
        settings.set("Duplicate", Some("ctrl-alt-1".into()));
        apply(&settings, cx);
        let first = page(cx).unwrap();
        settings.set("Duplicate", Some("ctrl-alt-2".into()));
        apply(&settings, cx);
        let second = page(cx).unwrap();
        settings.set("Duplicate", Some("ctrl-alt-1".into()));
        apply(&settings, cx);
        let reset = page(cx).unwrap();
        assert!(first.0 < second.0 && second.0 < reset.0);
        assert_eq!(first.1, reset.1);
    });
}
