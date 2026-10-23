//! Renderer-side edit focus is needed to distinguish sidebar Undo from web
//! editors, including contenteditable and editors inside cross-origin frames.
use cef::*;

pub(crate) const EDIT_FOCUS_MESSAGE: &str = "Echo.EditFocus";

fn report(frame: Option<&mut Frame>, editable: bool) {
    let Some(frame) = frame else { return; };
    let Some(mut message) = process_message_create(Some(&CefString::from(EDIT_FOCUS_MESSAGE))) else { return; };
    if let Some(arguments) = message.argument_list() { arguments.set_bool(0, i32::from(editable)); }
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
}

wrap_render_process_handler! {
    struct EditingRenderHandler;
    impl RenderProcessHandler {
        fn on_context_created(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _context: Option<&mut V8Context>) {
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

pub fn subprocess_app() -> App { EditingApp::new() }
