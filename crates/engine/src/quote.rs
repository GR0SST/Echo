//! Read the explicitly requested selection through CEF DOM, without a page JS API.
use cef::*;
pub(crate) const REQUEST: &str = "Echo.QuoteSelection.v1";
pub(crate) const RESULT: &str = "Echo.QuoteSelectionResult.v1";
pub(crate) fn request(browser: &Browser, token: u64) -> bool {
    let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) else {
        return false;
    };
    let Some(mut message) = process_message_create(Some(&CefString::from(REQUEST))) else {
        return false;
    };
    let Some(args) = message.argument_list() else {
        return false;
    };
    args.set_string(0, Some(&CefString::from(token.to_string().as_str())));
    args.set_string(1, Some(&CefString::from(&frame.url())));
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    true
}
wrap_domvisitor! {
    struct Selection { frame: Frame, token: String, url: String }
    impl Domvisitor {
        fn visit(&self, document: Option<&mut Domdocument>) {
            if CefString::from(&self.frame.url()).to_string() != self.url { return; }
            let text = document.map(|document| CefString::from(&document.selection_as_text()).to_string()).unwrap_or_default();
            if text.len() > 65536 { return; }
            if let Some(mut message) = process_message_create(Some(&CefString::from(RESULT))) {
                if let Some(args) = message.argument_list() {
                    args.set_string(0, Some(&CefString::from(self.token.as_str())));
                    args.set_string(1, Some(&CefString::from(text.as_str())));
                    self.frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
                }
            }
        }
    }
}
pub(crate) fn receive(
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    if source != ProcessId::BROWSER {
        return false;
    }
    let (Some(frame), Some(message)) = (frame, message) else {
        return false;
    };
    if CefString::from(&message.name()).to_string() != REQUEST {
        return false;
    }
    let Some(args) = message.argument_list().filter(|args| {
        args.size() == 2
            && args.get_type(0) == ValueType::STRING
            && args.get_type(1) == ValueType::STRING
    }) else {
        return true;
    };
    let token = CefString::from(&args.string(0)).to_string();
    let url = CefString::from(&args.string(1)).to_string();
    if token.parse::<u64>().is_ok() && url == CefString::from(&frame.url()).to_string() {
        frame.visit_dom(Some(&mut Selection::new(frame.clone(), token, url)));
    }
    true
}
