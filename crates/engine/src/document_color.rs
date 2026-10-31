//! The main document reports its composited canvas color; chrome never samples
//! arbitrary screen pixels or infers a page's appearance from the browser theme.
use cef::*;
pub(crate) const MESSAGE: &str = "Echo.DocumentColor.v1";
wrap_v8_handler! {
    struct Report { browser: i32, frame: String }
    impl V8Handler {
        fn execute(&self, _name: Option<&CefString>, _object: Option<&mut V8Value>, arguments: Option<&[Option<V8Value>]>, _result: Option<&mut Option<V8Value>>, _exception: Option<&mut CefString>) -> i32 {
            let Some(args) = arguments.filter(|args| args.len() == 7) else { return 0; };
            let Some(context) = v8_context_get_current_context() else { return 0; };
            let (Some(browser), Some(frame)) = (context.browser(), context.frame()) else { return 0; };
            if frame.is_main() == 0 || browser.identifier() != self.browser || CefString::from(&frame.identifier()).to_string() != self.frame { return 0; }
            if !frame.v8_context().is_some_and(|current| current.is_same(Some(&mut context.clone())) != 0) { return 0; }
            let Some(url) = args[0].as_ref().filter(|value| value.is_string() != 0).map(|value| CefString::from(&value.string_value()).to_string()) else { return 0; };
            if url.len() > 16384 { return 0; }
            let mut color = [0; 6];
            for index in 0..6 {
                let Some(value) = args[index + 1].as_ref().filter(|value| value.is_int() != 0).map(|value| value.int_value()) else { return 0; };
                let Ok(value) = u8::try_from(value) else { return 0; }; color[index] = value;
            }
            if let Some(mut message) = process_message_create(Some(&CefString::from(MESSAGE))) {
                if let Some(values) = message.argument_list() {
                    values.set_string(0, Some(&CefString::from(url.as_str())));
                    for index in 0..6 { values.set_int(index + 1, color[index] as i32); }
                    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
                }
            }
            1
        }
    }
}
pub(crate) fn created(browser: &Browser, frame: &Frame, context: &V8Context) {
    if frame.is_main() == 0 {
        return;
    }
    let mut handler = Report::new(
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    let Some(callback) = v8_value_create_function(
        Some(&CefString::from("reportDocumentColor")),
        Some(&mut handler),
    ) else {
        return;
    };
    let (mut factory, mut exception) = (None, None);
    context.eval(
        Some(&CefString::from(include_str!("document_color.js"))),
        Some(&CefString::from("echo-document-color")),
        1,
        Some(&mut factory),
        Some(&mut exception),
    );
    if let Some(factory) = factory {
        factory.execute_function(None, Some(&[Some(callback)]));
    }
}
