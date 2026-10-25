//! Standard editing menu commands share the platform's current text responder.
use crate::window::BrowserWindow;
use gpui::{Context, Menu, MenuItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    ShowSpelling, CheckSpelling, ContinuousSpelling, Grammar, AutomaticCorrection,
    ShowSubstitutions, SmartInsertDelete, SmartQuotes, SmartDashes, SmartLinks,
    DataDetectors, TextReplacement, Uppercase, Lowercase, Capitalize,
    StartSpeaking, StopSpeaking, Bold, Italic, Underline,
}

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = standard_edit, no_json)]
pub(crate) struct Perform(pub Command);

pub(crate) fn append_submenus(items: &mut Vec<MenuItem>) {
    if !cfg!(target_os = "macos") { return; }
    use Command::*;
    let item = |title, command| MenuItem::action(title, Perform(command));
    let submenu = |name: &'static str, items| MenuItem::submenu(Menu { name: name.into(), items });
    items.extend([
        submenu("Spelling and Grammar", vec![
            item("Show Spelling and Grammar", ShowSpelling),
            item("Check Document Now", CheckSpelling), MenuItem::separator(),
            item("Check Spelling While Typing", ContinuousSpelling),
            item("Check Grammar With Spelling", Grammar),
            item("Correct Spelling Automatically", AutomaticCorrection),
        ]),
        submenu("Substitutions", vec![
            item("Show Substitutions", ShowSubstitutions), MenuItem::separator(),
            item("Smart Copy/Paste", SmartInsertDelete), item("Smart Quotes", SmartQuotes),
            item("Smart Dashes", SmartDashes), item("Smart Links", SmartLinks),
            item("Data Detectors", DataDetectors), item("Text Replacement", TextReplacement),
        ]),
        submenu("Transformations", vec![item("Make Upper Case", Uppercase),
            item("Make Lower Case", Lowercase), item("Capitalize", Capitalize)]),
        submenu("Speech", vec![item("Start Speaking", StartSpeaking), item("Stop Speaking", StopSpeaking)]),
        submenu("Format", vec![submenu("Font", vec![item("Bold", Bold),
            item("Italic", Italic), item("Underline", Underline)])]),
    ]);
}

impl BrowserWindow {
    /// Fallback action path for toolkit-dispatched menus. Native macOS menus
    /// bind directly to the first responder so AppKit owns enable/check state.
    pub(crate) fn perform_standard_edit(&mut self, command: Command, window: &gpui::Window, cx: &mut Context<Self>) {
        let target = window.window_handle();
        #[cfg(test)]
        let window_id = target.window_id();
        cx.spawn(async move |_, cx| {
            let Ok(view) = target.update(cx, |_, window, _| crate::native::view_of(window)) else { return; };
            // Native panels can re-enter AppKit. No GPUI window/application
            // borrow may remain while dispatching to the text responder.
            if crate::native::perform_standard_edit(command, view) {
                #[cfg(test)]
                let _ = cx.update(|cx| {
                    if !cx.has_global::<Requests>() { cx.set_global(Requests::default()); }
                    let requests = cx.global_mut::<Requests>();
                    requests.commands.push(command);
                    requests.windows.push(window_id);
                });
            }
        }).detach();
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct Requests {
    pub commands: Vec<Command>,
    pub windows: Vec<gpui::WindowId>,
}
#[cfg(test)]
impl gpui::Global for Requests {}
