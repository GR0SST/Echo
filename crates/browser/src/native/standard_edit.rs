//! Bind GPUI menu definitions to AppKit's standard text responder selectors.
use crate::standard_edit::Command;
use objc2::{msg_send, sel, runtime::{AnyObject, Bool, Sel}};
use objc2_foundation::NSString;

const COMMANDS: [Command; 20] = [
    Command::ShowSpelling, Command::CheckSpelling, Command::ContinuousSpelling,
    Command::Grammar, Command::AutomaticCorrection, Command::ShowSubstitutions,
    Command::SmartInsertDelete, Command::SmartQuotes, Command::SmartDashes,
    Command::SmartLinks, Command::DataDetectors, Command::TextReplacement,
    Command::Uppercase, Command::Lowercase, Command::Capitalize,
    Command::StartSpeaking, Command::StopSpeaking, Command::Bold, Command::Italic,
    Command::Underline,
];
fn binding(command: Command) -> (&'static str, &'static str, Sel, &'static str, usize) {
    use Command::*;
    match command {
        ShowSpelling => ("Spelling and Grammar", "Show Spelling and Grammar", sel!(showGuessPanel:), ":", 0),
        CheckSpelling => ("Spelling and Grammar", "Check Document Now", sel!(checkSpelling:), ";", 0),
        ContinuousSpelling => ("Spelling and Grammar", "Check Spelling While Typing", sel!(toggleContinuousSpellChecking:), "", 0),
        Grammar => ("Spelling and Grammar", "Check Grammar With Spelling", sel!(toggleGrammarChecking:), "", 0),
        AutomaticCorrection => ("Spelling and Grammar", "Correct Spelling Automatically", sel!(toggleAutomaticSpellingCorrection:), "", 0),
        ShowSubstitutions => ("Substitutions", "Show Substitutions", sel!(orderFrontSubstitutionsPanel:), "", 0),
        SmartInsertDelete => ("Substitutions", "Smart Copy/Paste", sel!(toggleSmartInsertDelete:), "", 0),
        SmartQuotes => ("Substitutions", "Smart Quotes", sel!(toggleAutomaticQuoteSubstitution:), "", 0),
        SmartDashes => ("Substitutions", "Smart Dashes", sel!(toggleAutomaticDashSubstitution:), "", 0),
        SmartLinks => ("Substitutions", "Smart Links", sel!(toggleAutomaticLinkDetection:), "", 0),
        DataDetectors => ("Substitutions", "Data Detectors", sel!(toggleAutomaticDataDetection:), "", 0),
        TextReplacement => ("Substitutions", "Text Replacement", sel!(toggleAutomaticTextReplacement:), "", 0),
        Uppercase => ("Transformations", "Make Upper Case", sel!(uppercaseWord:), "", 0),
        Lowercase => ("Transformations", "Make Lower Case", sel!(lowercaseWord:), "", 0),
        Capitalize => ("Transformations", "Capitalize", sel!(capitalizeWord:), "", 0),
        StartSpeaking => ("Speech", "Start Speaking", sel!(startSpeaking:), "", 0),
        StopSpeaking => ("Speech", "Stop Speaking", sel!(stopSpeaking:), "", 0),
        Bold => ("Format", "Bold", sel!(addFontTrait:), "b", 2),
        Italic => ("Format", "Italic", sel!(addFontTrait:), "i", 1),
        Underline => ("Format", "Underline", sel!(underline:), "u", 0),
    }
}
unsafe fn app() -> *mut AnyObject {
    unsafe { msg_send![objc2::class!(NSApplication), sharedApplication] }
}
unsafe fn target(traits: usize) -> *mut AnyObject {
    if traits == 0 { std::ptr::null_mut() }
    else { unsafe { msg_send![objc2::class!(NSFontManager), sharedFontManager] } }
}

pub(super) fn configure() {
    unsafe {
        let root: *mut AnyObject = msg_send![app(), mainMenu];
        if root.is_null() { return; }
        let edit: *mut AnyObject = msg_send![root, itemWithTitle: &*NSString::from_str("Edit")];
        if edit.is_null() { return; }
        let edit: *mut AnyObject = msg_send![edit, submenu];
        if edit.is_null() { return; }
        for command in COMMANDS {
            let (group, title, selector, key, traits) = binding(command);
            let item = find_item(edit, group, title);
            if item.is_null() { continue; }
            let _: () = msg_send![item, setTarget: target(traits)];
            let _: () = msg_send![item, setAction: selector];
            let _: () = msg_send![item, setTag: traits as isize];
            let _: () = msg_send![item, setKeyEquivalent: &*NSString::from_str(key)];
            let _: () = msg_send![item, setKeyEquivalentModifierMask: if key.is_empty() { 0usize } else { 1usize << 20 }];
        }
    }
}

unsafe fn find_item(edit: *mut AnyObject, group: &str, title: &str) -> *mut AnyObject {
    unsafe {
        let item: *mut AnyObject = msg_send![edit, itemWithTitle: &*NSString::from_str(group)];
        if item.is_null() { return std::ptr::null_mut(); }
        let mut menu: *mut AnyObject = msg_send![item, submenu];
        if menu.is_null() { return std::ptr::null_mut(); }
        if group == "Format" {
            let font: *mut AnyObject = msg_send![menu, itemWithTitle: &*NSString::from_str("Font")];
            if font.is_null() { return std::ptr::null_mut(); }
            menu = msg_send![font, submenu];
            if menu.is_null() { return std::ptr::null_mut(); }
        }
        msg_send![menu, itemWithTitle: &*NSString::from_str(title)]
    }
}

pub(super) fn perform(command: Command, view: *mut std::ffi::c_void) -> bool {
    unsafe {
        let application = app();
        let key: *mut AnyObject = msg_send![application, keyWindow];
        let owner: *mut AnyObject = msg_send![view.cast::<AnyObject>(), window];
        if owner.is_null() || owner != key { return false; }
        let (group, title, selector, _, traits) = binding(command);
        let root: *mut AnyObject = msg_send![application, mainMenu];
        if root.is_null() { return false; }
        let edit: *mut AnyObject = msg_send![root, itemWithTitle: &*NSString::from_str("Edit")];
        if edit.is_null() { return false; }
        let edit: *mut AnyObject = msg_send![edit, submenu];
        if edit.is_null() { return false; }
        let sender = find_item(edit, group, title);
        if sender.is_null() { return false; }
        let accepted: Bool = msg_send![application, sendAction: selector, to: target(traits), from: sender];
        accepted.as_bool()
    }
}
