//! Renderer-side edit focus is needed to distinguish sidebar Undo from web
//! editors, including contenteditable and editors inside cross-origin frames.
use cef::*;

pub(crate) const EDIT_FOCUS_MESSAGE: &str = "Echo.EditFocus";

fn report(frame: Option<&mut Frame>, editable: bool) {
    let Some(frame) = frame else {
        return;
    };
    let Some(mut message) = process_message_create(Some(&CefString::from(EDIT_FOCUS_MESSAGE)))
    else {
        return;
    };
    if let Some(arguments) = message.argument_list() {
        arguments.set_bool(0, i32::from(editable));
    }
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
}

wrap_render_process_handler! {
    struct EditingRenderHandler;
    impl RenderProcessHandler {
        fn on_browser_created(&self, browser: Option<&mut Browser>, extra_info: Option<&mut DictionaryValue>) { crate::boosts::created(browser.as_deref(), extra_info.as_deref()); crate::devtools_renderer::created(browser, extra_info); }
        fn on_browser_destroyed(&self, browser: Option<&mut Browser>) { crate::boosts::destroyed(browser.as_deref()); crate::devtools_renderer::destroyed(browser); }
        fn on_context_released(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, context: Option<&mut V8Context>) {
            if let (Some(browser), Some(frame), Some(context)) = (browser, frame, context) { crate::boost_picker::release(browser, frame); crate::devtools_renderer::context_released(browser, frame, context); }
        }
        fn on_process_message_received(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, source_process: ProcessId, message: Option<&mut ProcessMessage>) -> i32 { if crate::boost_picker::receive(browser.as_deref(), frame.as_deref(), source_process, message.as_deref()) { return 1; } if crate::boosts::receive(browser.as_deref(), frame.as_deref(), source_process, message.as_deref()) { return 1; } crate::devtools_renderer::receive(browser, frame, source_process, message) }
        fn on_context_created(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, context: Option<&mut V8Context>) {
            if let (Some(browser), Some(frame), Some(context)) = (browser.as_deref(), frame.as_deref(), context.as_deref()) { crate::boosts::context_created(browser, frame, context); crate::devtools_renderer::context_created(browser, frame, context); }
            if frame.as_ref().is_some_and(|frame| frame.is_main() != 0) { report(frame, false); }
        }
        fn on_focused_node_changed(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, node: Option<&mut Domnode>) {
            report(frame, node.is_some_and(|node| node.is_editable() != 0));
        }
    }
}

wrap_app! {
    struct EditingApp;
    impl App {
        fn render_process_handler(&self) -> Option<RenderProcessHandler> { Some(EditingRenderHandler::new()) }
    }
}

pub fn subprocess_app() -> App {
    EditingApp::new()
}
