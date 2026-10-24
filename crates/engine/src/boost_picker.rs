//! Trusted native picker capability, scoped to a live renderer context.
use cef::*;
use std::{cell::RefCell, collections::HashMap};
pub(crate) const IPC: &str = "Echo.BoostPicker.v1";
pub(crate) const RESULT: &str = "Echo.BoostZap.v1";
thread_local! { static PICKERS: RefCell<HashMap<(i32,String),(V8Context,V8Value)>> = RefCell::new(HashMap::new()); }
pub(crate) fn request(browser: &Browser, enabled: bool) {
    let mut ids = CefStringList::new();
    browser.frame_identifiers(Some(&mut ids));
    for id in ids {
        if let Some(frame) = browser.frame_by_identifier(Some(&CefString::from(id.as_str()))) {
            if let Some(mut message) = process_message_create(Some(&CefString::from(IPC))) {
                if let Some(args) = message.argument_list() {
                    args.set_bool(0, i32::from(enabled));
                    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
                }
            }
        }
    }
}
pub(crate) fn release(browser: &Browser, frame: &Frame) {
    PICKERS.with_borrow_mut(|pickers| {
        pickers.remove(&(
            browser.identifier(),
            CefString::from(&frame.identifier()).to_string(),
        ));
    });
}
wrap_v8_handler! {
    struct Pick { browser: i32, frame: String }
    impl V8Handler {
        fn execute(&self,_name:Option<&CefString>,_object:Option<&mut V8Value>,arguments:Option<&[Option<V8Value>]>,_retval:Option<&mut Option<V8Value>>,_exception:Option<&mut CefString>)->i32 {
            let Some(value)=arguments.filter(|args|args.len()==1).and_then(|args|args[0].as_ref()).filter(|value|value.is_string()!=0) else{return 0;};
            let value=CefString::from(&value.string_value()).to_string(); if value.len()>4096{return 0;}
            let Some(context)=v8_context_get_current_context() else{return 0;};
            let (Some(browser),Some(frame))=(context.browser(),context.frame()) else{return 0;};
            if browser.identifier()!=self.browser || CefString::from(&frame.identifier()).to_string()!=self.frame{return 0;}
            let valid=PICKERS.with_borrow(|pickers|pickers.get(&(self.browser,self.frame.clone())).is_some_and(|(current,_)|current.is_same(Some(&mut context.clone()))!=0));if !valid{return 0;}
            if let Some(mut message)=process_message_create(Some(&CefString::from(RESULT))){if let Some(args)=message.argument_list(){args.set_string(0,Some(&CefString::from(value.as_str())));frame.send_process_message(ProcessId::BROWSER,Some(&mut message));}}
            1
        }
    }
}
pub(crate) fn receive(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    if source != ProcessId::BROWSER {
        return false;
    }
    let (Some(browser), Some(frame), Some(message)) = (browser, frame, message) else {
        return false;
    };
    if CefString::from(&message.name()).to_string() != IPC {
        return false;
    }
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    let old = PICKERS.with_borrow_mut(|pickers| pickers.remove(&key));
    if let Some((context, cleanup)) = old {
        if context.enter() != 0 {
            cleanup.execute_function(None, None);
            context.exit();
        }
    }
    let enabled = message.argument_list().is_some_and(|args| {
        args.size() == 1 && args.get_type(0) == ValueType::BOOL && args.bool(0) != 0
    });
    if !enabled {
        return true;
    }
    let Some(context) = frame.v8_context() else {
        return true;
    };
    if context.enter() == 0 {
        return true;
    }
    let mut handler = Pick::new(browser.identifier(), key.1.clone());
    let callback = v8_value_create_function(Some(&CefString::from("EchoZap")), Some(&mut handler));
    let (mut factory, mut exception) = (None, None);
    context.eval(
        Some(&CefString::from(include_str!("boost_picker.js"))),
        Some(&CefString::from("echo-boost-picker")),
        1,
        Some(&mut factory),
        Some(&mut exception),
    );
    if let (Some(factory), Some(callback)) = (factory, callback) {
        let cleanup = factory.execute_function(None, Some(&[Some(callback)]));
        if let Some(cleanup) = cleanup {
            PICKERS.with_borrow_mut(|pickers| {
                pickers.insert(key, (context.clone(), cleanup));
            });
        }
    }
    context.exit();
    true
}
