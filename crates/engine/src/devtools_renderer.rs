//! Renderer capability installed only from native browser extra_info.
use crate::devtools::{self, BridgeMessage, MAX_MESSAGE};
use cef::*;
use std::{cell::RefCell, collections::HashMap};

pub(crate) const ROLE_KEY: &str = "Echo.InspectorGeneration";
pub(crate) const IPC: &str = "Echo.InspectorBridge.v1";
const HOST: &str = include_str!("devtools_host.js");
struct Document {
    epoch: u64,
    context: V8Context,
    delivery: Option<V8Value>,
}
struct Renderer {
    generation: u64,
    next: u64,
    document: Option<Document>,
}
thread_local! { static RENDERERS: RefCell<HashMap<i32, Renderer>> = RefCell::new(HashMap::new()); }

fn text(value: &CefStringUserfree) -> String {
    CefString::from(value).to_string()
}
fn valid_frame(browser: &Browser, frame: &Frame) -> bool {
    frame.is_main() != 0
        && devtools::expected_frontend(&text(&frame.url()))
        && browser
            .main_frame()
            .is_some_and(|main| text(&main.identifier()) == text(&frame.identifier()))
}

pub(crate) fn created(browser: Option<&mut Browser>, extra: Option<&mut DictionaryValue>) {
    let (Some(browser), Some(extra)) = (browser, extra) else {
        return;
    };
    let key = CefString::from(ROLE_KEY);
    if extra.get_type(Some(&key)) != ValueType::STRING {
        return;
    }
    let Some(generation) = text(&extra.string(Some(&key)))
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
    else {
        eprintln!("[inspector-bridge renderer] browser={} invalid native generation", browser.identifier());
        return;
    };
    RENDERERS.with_borrow_mut(|states| {
        states.insert(
            browser.identifier(),
            Renderer {
                generation,
                next: 0,
                document: None,
            },
        );
    });
}
pub(crate) fn destroyed(browser: Option<&mut Browser>) {
    if let Some(browser) = browser {
        RENDERERS.with_borrow_mut(|states| {
            states.remove(&browser.identifier());
        });
    }
}

pub(crate) fn context_created(browser: &Browser, frame: &Frame, context: &V8Context) {
    if !valid_frame(browser, frame) {
        return;
    }
    let identity = RENDERERS.with_borrow_mut(|states| {
        let state = states.get_mut(&browser.identifier())?;
        state.next = state.next.checked_add(1)?;
        state.document = Some(Document {
            epoch: state.next,
            context: context.clone(),
            delivery: None,
        });
        Some((state.generation, state.next))
    });
    let Some((generation, epoch)) = identity else {
        return;
    };
    send(frame, generation, epoch, "context", "");
    let mut handler = HostCall::new(browser.identifier(), generation, epoch);
    let Some(function) = v8_value_create_function(
        Some(&CefString::from("EchoInspectorSend")),
        Some(&mut handler),
    ) else {
        eprintln!("[inspector-bridge renderer] generation={generation} epoch={epoch} native function creation failed");
        return;
    };
    let (mut factory, mut exception) = (None, None);
    // Only this fixed source is evaluated. Protocol bytes are V8 string args.
    if context.eval(
        Some(&CefString::from(HOST)),
        Some(&CefString::from("echo-inspector-host")),
        1,
        Some(&mut factory),
        Some(&mut exception),
    ) == 0
    {
        log_exception("host eval failed", exception.as_ref());
        return;
    }
    let delivery = factory.and_then(|factory| {
        let delivery = factory.execute_function(None, Some(&[Some(function)]));
        if factory.has_exception() != 0 {
            log_exception("host factory failed", factory.exception().as_ref());
        }
        delivery.filter(|value| value.is_function() != 0)
    });
    if delivery.is_none() {
        eprintln!("[inspector-bridge renderer] host installation did not return a delivery function");
    }
    RENDERERS.with_borrow_mut(|states| {
        if let Some(document) = states
            .get_mut(&browser.identifier())
            .and_then(|s| s.document.as_mut())
            .filter(|d| d.epoch == epoch)
        {
            document.delivery = delivery;
        }
    });
}

fn log_exception(stage: &str, exception: Option<&V8Exception>) {
    if let Some(exception) = exception {
        eprintln!("[inspector-bridge renderer] {stage}: V8 exception at line {}", exception.line_number());
    } else {
        eprintln!("[inspector-bridge renderer] {stage}: no V8 exception (check context validity/script execution)");
    }
}

pub(crate) fn context_released(browser: &Browser, frame: &Frame, context: &V8Context) {
    let identity = RENDERERS.with_borrow_mut(|states| {
        let state = states.get_mut(&browser.identifier())?;
        let document = state.document.as_ref()?;
        if document.context.is_same(Some(&mut context.clone())) == 0 {
            return None;
        }
        let epoch = document.epoch;
        state.document = None;
        Some((state.generation, epoch))
    });
    if let Some((generation, epoch)) = identity {
        send(frame, generation, epoch, "release", "");
    }
}

fn send(frame: &Frame, generation: u64, epoch: u64, kind: &str, payload: &str) {
    let Some(mut message) =
        devtools::bridge_message(generation, epoch, kind, payload, &text(&frame.identifier()))
    else {
        return;
    };
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
}

wrap_v8_handler! {
    struct HostCall { browser_id: i32, generation: u64, epoch: u64 }
    impl V8Handler {
        fn execute(&self, _name: Option<&CefString>, _object: Option<&mut V8Value>, arguments: Option<&[Option<V8Value>]>, _retval: Option<&mut Option<V8Value>>, _exception: Option<&mut CefString>) -> i32 {
            let Some(args) = arguments.filter(|args| args.len() == 2) else { return 0; };
            let (Some(kind), Some(payload)) = (&args[0], &args[1]) else { return 0; };
            if kind.is_string() == 0 || payload.is_string() == 0 { return 0; }
            let (kind, payload) = (text(&kind.string_value()), text(&payload.string_value()));
            if payload.len() > MAX_MESSAGE || !devtools::renderer_kind(&kind, &payload) { return 0; }
            let Some(context) = v8_context_get_current_context() else { return 0; };
            let (Some(browser), Some(frame)) = (context.browser(), context.frame()) else { return 0; };
            if browser.identifier() != self.browser_id || !valid_frame(&browser, &frame) { return 0; }
            let valid = RENDERERS.with_borrow(|states| states.get(&self.browser_id).is_some_and(|state| state.generation == self.generation && state.document.as_ref().is_some_and(|d| d.epoch == self.epoch && d.context.is_same(Some(&mut context.clone())) != 0)));
            if !valid { return 0; }
            send(&frame, self.generation, self.epoch, &kind, &payload);
            1
        }
    }
}

pub(crate) fn receive(
    browser: Option<&mut Browser>,
    frame: Option<&mut Frame>,
    source: ProcessId,
    message: Option<&mut ProcessMessage>,
) -> i32 {
    if source != ProcessId::BROWSER {
        return 0;
    }
    let (Some(browser), Some(frame), Some(message)) = (browser, frame, message) else {
        return 0;
    };
    if !valid_frame(browser, frame) {
        return 0;
    }
    let Some(input) = BridgeMessage::parse(message) else {
        return 0;
    };
    if input.frame != text(&frame.identifier())
        || !devtools::browser_kind(&input.kind, &input.payload)
    {
        return 0;
    }
    let target = RENDERERS.with_borrow(|states| {
        let state = states.get(&browser.identifier())?;
        let document = state.document.as_ref()?;
        if state.generation != input.generation || document.epoch != input.epoch {
            return None;
        }
        Some((document.context.clone(), document.delivery.clone()?))
    });
    let Some((context, function)) = target else {
        return 0;
    };
    if context.is_valid() == 0 || context.enter() == 0 {
        eprintln!("[inspector-bridge renderer] delivery context invalid or could not enter");
        return 0;
    }
    let args = [
        v8_value_create_string(Some(&CefString::from(input.kind.as_str()))),
        v8_value_create_string(Some(&CefString::from(input.payload.as_str()))),
    ];
    let delivered = function
        .execute_function_with_context(Some(&mut context.clone()), None, Some(&args))
        .is_some();
    if function.has_exception() != 0 {
        log_exception("host delivery failed", function.exception().as_ref());
    }
    context.exit();
    if input.kind == "revoke" {
        RENDERERS.with_borrow_mut(|states| {
            if let Some(state) = states.get_mut(&browser.identifier()) {
                state.document = None;
            }
        });
    }
    i32::from(delivered)
}
