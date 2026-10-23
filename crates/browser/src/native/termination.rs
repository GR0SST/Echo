//! Pre-terminate gate: return BEFORE AppKit can enter its modal quit loop.
//! No GPUI/CEF calls from native callbacks. The exact toolkit objects survive;
//! no-ivar subclasses inherit their existing methods, ivars and protocols.
use std::{cell::{Cell, RefCell}, collections::HashMap, ffi::c_void};
use objc2::{msg_send, sel, rc::Retained, runtime::{AnyClass, AnyObject, ClassBuilder, Sel}};
use crate::quit::{NATIVE_GATE as GATE, NativePrimaryOutcome, NativeTerminationHost, replay_native_termination, native_event_failure_code, native_event_result_code, dispatch_original_quit_event};

type Terminate = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);
thread_local! {
    static ORIGINAL: Cell<Option<Terminate>> = const { Cell::new(None) };
    static ORIGINAL_AE: Cell<Option<(AppleEventHandler, *mut c_void)>> = const { Cell::new(None) };
    static CONTEXTS: RefCell<HashMap<usize, EventContext>> = RefCell::new(HashMap::new());
    static NEXT_CONTEXT: Cell<usize> = const { Cell::new(1) };
    static AE_DISPATCHED: Cell<bool> = const { Cell::new(false) };
    // Outer Option distinguishes uncaptured from an explicitly nil first sender.
    static SENDER: RefCell<Option<Option<Retained<AnyObject>>>> = const { RefCell::new(None) };
}

fn apple_events() -> Result<*mut AnyObject, String> {
    let class = AnyClass::get(c"NSAppleEventManager").ok_or("NSAppleEventManager is unavailable")?;
    Ok(unsafe { msg_send![class, sharedAppleEventManager] })
}

unsafe extern "C-unwind" fn terminate(app: *mut AnyObject, selector: Sel, sender: *mut AnyObject) {
    // Claim the intent before any Objective-C call, releasing the gate borrow
    // immediately. Repeated native intents join; none enters AppKit yet.
    if GATE.with(|gate| gate.borrow().is_finalizing()) {
        // A preserved Cocoa AppleEvent handler may call terminate: itself.
        // Allow that single original continuation, never a second replay.
        if GATE.with(|gate| gate.borrow_mut().take_native_continuation()) {
            if let Some(original) = ORIGINAL.with(Cell::get) {
                eprintln!("[quit-native] Preserved AppleEvent handler continuing through original terminate");
                unsafe { original(app, selector, sender); }
                GATE.with(|gate| gate.borrow_mut().fail("Original native terminate continuation unexpectedly returned".into()));
            }
        } else {
            eprintln!("[quit-native] Ignoring reentrant terminate during final native call");
        }
        return;
    }
    let first = match GATE.with(|gate| gate.borrow_mut().capture()) {
        Ok(first) => first,
        Err(_) => {
            // This event is still current: reply with failure directly, without
            // creating a suspension which a failed coordinator cannot release.
            if let Ok(manager) = apple_events() {
                let reply: *mut AnyObject = unsafe { msg_send![manager, currentReplyAppleEvent] };
                unsafe { fail_reply(reply); }
            }
            eprintln!("[quit-native] Rejected additional quit after sticky failure; no suspension created");
            return;
        }
    };
    crate::quit::seal();
    if first {
        let sender = unsafe { Retained::retain(sender) };
        SENDER.with(|slot| *slot.borrow_mut() = Some(sender));
    }
    let captured = (|| -> Result<Option<usize>, String> {
        let manager = apple_events()?;
        unsafe {
            // Do not retain these descriptors: Foundation documents that as
            // undefined. A suspension ID owns the ORIGINAL event and reply.
            let event: *mut AnyObject = msg_send![manager, currentAppleEvent];
            if event.is_null() { return Ok(None); }
            let id = reserve_context()?;
            CONTEXTS.with(|contexts| contexts.borrow_mut().try_reserve(1)).map_err(|e| format!("Cannot track Foundation quit event: {e}"))?;
            let suspension: *mut c_void = msg_send![manager, suspendCurrentAppleEvent];
            if suspension.is_null() { return Err("Cannot suspend the original native quit AppleEvent".into()); }
            CONTEXTS.with(|contexts| contexts.borrow_mut().insert(id, EventContext::Foundation(suspension as usize)));
            Ok(Some(id))
        }
    })();
    match captured {
        Ok(suspension) => {
            GATE.with(|gate| gate.borrow_mut().record_context(first, suspension));
            eprintln!("[quit-native] terminate captured; first={first}; apple_event_suspended={}; returning before AppKit", suspension.is_some());
        }
        Err(error) => {
            eprintln!("[quit-native] Termination paused: {error}");
            GATE.with(|gate| gate.borrow_mut().fail(error));
        }
    }
    // In particular: no NSTerminateLater, no cancellation, no CEF polling.
}

unsafe extern "C-unwind" fn should_terminate(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> usize {
    let complete = GATE.with(|gate| gate.borrow().is_finalizing());
    eprintln!("[quit-native] applicationShouldTerminate entered; engine_complete={complete}");
    if complete { return 1; } // NSTerminateNow: no nested modal wait.
    // This is not a normal route: terminate: must have intercepted it. Fail
    // closed rather than enter another modal loop, and never cancel/reissue.
    let error = "AppKit bypassed the pre-terminate gate before engine completion".to_owned();
    crate::quit::seal();
    GATE.with(|gate| { let mut gate = gate.borrow_mut(); let _ = gate.capture(); gate.fail(error); });
    eprintln!("[quit-native] Unexpected gate bypass; termination refused and no retry will be issued");
    0
}

pub(crate) fn install() -> Result<(), String> {
    unsafe {
        let class = AnyClass::get(c"NSApplication").ok_or("NSApplication is unavailable")?;
        let app: *mut AnyObject = msg_send![class, sharedApplication];
        let app = app.as_ref().ok_or("NSApplication instance is unavailable")?;
        let superclass = app.class();
        if superclass.name() == c"EchoPreTerminateApplication" { return Ok(()); }
        let original = superclass.instance_method(sel!(terminate:)).ok_or("NSApplication has no terminate:")?;
        let original = std::mem::transmute::<objc2::runtime::Imp, Terminate>(original.implementation());
        let delegate: *mut AnyObject = msg_send![app, delegate];
        let delegate = Retained::retain(delegate).ok_or("GPUI application delegate is unavailable")?;
        let delegate_class = delegate.class();
        // Installed GPUI 0.2.2 has no optional shouldTerminate policy. Refuse an
        // unknown policy instead of silently replacing its cancellation/Later.
        if delegate_class.instance_method(sel!(applicationShouldTerminate:)).is_some() {
            return Err("Pre-terminate installation requires GPUI's unmodified termination delegate".into());
        }
        let mut application = ClassBuilder::new(c"EchoPreTerminateApplication", superclass)
            .ok_or("Cannot register pre-terminate application subclass")?;
        application.add_method(sel!(terminate:), terminate as Terminate);
        let mut delegation = ClassBuilder::new(c"EchoPreTerminateDelegate", delegate_class)
            .ok_or("Cannot register pre-terminate delegate subclass")?;
        delegation.add_method(sel!(applicationShouldTerminate:), should_terminate as unsafe extern "C-unwind" fn(_, _, _) -> usize);
        let application = application.register();
        let delegation = delegation.register();
        ORIGINAL.with(|slot| slot.set(Some(original)));
        AnyObject::set_class(app, application);
        AnyObject::set_class(&delegate, delegation);
        // Refresh AppKit's optional-selector cache on the same retained object.
        let nil: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![app, setDelegate: nil];
        let _: () = msg_send![app, setDelegate: &*delegate];
        let mut handler = None;
        let mut refcon = std::ptr::null_mut();
        let error = AEGetEventHandler(fourcc(b"aevt"), fourcc(b"quit"), &mut handler, &mut refcon, 0);
        if error != 0 { return Err(format!("Cannot read original quit AppleEvent handler: {error}")); }
        let handler = handler.ok_or("Original quit AppleEvent handler is absent")?;
        ORIGINAL_AE.with(|slot| slot.set(Some((handler, refcon))));
        let error = AEInstallEventHandler(fourcc(b"aevt"), fourcc(b"quit"), Some(capture_quit_event), std::ptr::null_mut(), 0);
        if error != 0 { return Err(format!("Cannot install quit AppleEvent gate: {error}")); }
        eprintln!("[quit-native] Installed terminate and AppleEvent gates; original UPP/refcon and Cocoa mapping preserved");
    }
    Ok(())
}

pub(crate) fn take_request() -> Result<bool, String> {
    let result = GATE.with(|gate| gate.borrow_mut().take_request());
    if matches!(&result, Ok(true)) { eprintln!("[quit-native] Delivering captured termination request to coordinator"); }
    result
}

/// Reply while an event is current or suspended; never retain its descriptor.
unsafe fn fail_reply(reply: *mut AnyObject) {
    unsafe {
        if reply.is_null() { return; }
        let kind: u32 = msg_send![reply, descriptorType];
        if kind == u32::from_be_bytes(*b"null") { return; }
        let error_key = u32::from_be_bytes(*b"errn");
        let existing: *mut AnyObject = msg_send![reply, paramDescriptorForKeyword: error_key];
        let existing_error: i32 = if existing.is_null() { 0 } else { msg_send![existing, int32Value] };
        if existing_error == 0 {
            let error: *mut AnyObject = msg_send![AnyClass::get(c"NSAppleEventDescriptor").expect("Foundation is loaded"), descriptorWithInt32: -10000i32];
            let _: () = msg_send![reply, setParamDescriptor: error, forKeyword: error_key];
        }
    }
}

struct Replay {
    app: *mut AnyObject,
    manager: *mut AnyObject,
    original: Terminate,
    sender: Option<Retained<AnyObject>>,
    primary_raw: Option<(usize, RawEvent)>,
}
impl NativeTerminationHost for Replay {
    fn restore(&mut self, id: usize) {
        let context = CONTEXTS.with(|contexts| contexts.borrow_mut().remove(&id));
        match context {
            Some(EventContext::Foundation(suspension)) => {
                CONTEXTS.with(|contexts| contexts.borrow_mut().insert(id, EventContext::Foundation(suspension)));
                eprintln!("[quit-native] Restoring original Foundation AppleEvent/reply context after CEF Complete");
                unsafe { let _: () = msg_send![self.manager, setCurrentAppleEventAndReplyEventWithSuspensionID: suspension as *mut c_void]; }
            }
            Some(EventContext::Raw(raw)) => self.primary_raw = Some((id, raw)),
            None => GATE.with(|gate| gate.borrow_mut().fail("Original native event context is missing".into())),
        }
    }
    fn original_terminate(&mut self) -> NativePrimaryOutcome {
        if let Some((id, raw)) = self.primary_raw.take() {
            eprintln!("[quit-native] CEF Complete; resuming original quit AppleEvent UPP/refcon exactly once");
            AE_DISPATCHED.with(|flag| flag.set(false));
            let error = unsafe { AEResumeTheCurrentEvent(std::ptr::addr_of!(raw.event.0), std::ptr::addr_of!(raw.reply.0), Some(replay_quit_event), std::ptr::null_mut()) };
            let dispatched = AE_DISPATCHED.with(Cell::get);
            if dispatched {
                // Dispatcher preserved the original live result before the
                // manager sends its reply. Its copies can now be disposed.
                GATE.with(|gate| gate.borrow_mut().released(id));
            } else {
                // Resume rejected before dispatch: keep the suspended event
                // tracked, with no retry or fabricated success.
                CONTEXTS.with(|contexts| contexts.borrow_mut().insert(id, EventContext::Raw(raw)));
            }
            if dispatched && error == 0 && GATE.with(|gate| gate.borrow().is_finalizing()) {
                eprintln!("[quit-native] Original quit AppleEvent accepted; awaiting its single native terminate continuation");
                return NativePrimaryOutcome::AppleEventAccepted;
            }
            GATE.with(|gate| gate.borrow_mut().fail(format!("Original AppleEvent resume failed: {error}")));
            return if dispatched { NativePrimaryOutcome::Returned } else { NativePrimaryOutcome::ResumeRejected };
        }
        if GATE.with(|gate| !gate.borrow().is_finalizing()) { return NativePrimaryOutcome::ResumeRejected; }
        eprintln!("[quit-native] CEF Complete; invoking preserved original terminate exactly once");
        let sender = self.sender.as_ref().map_or(std::ptr::null_mut(), |sender| &**sender as *const AnyObject as *mut AnyObject);
        unsafe { (self.original)(self.app, sel!(terminate:), sender); }
        eprintln!("[quit-native] Original terminate unexpectedly returned; no native retry");
        NativePrimaryOutcome::Returned
    }
    fn resume_failed(&mut self, id: usize) {
        let context = CONTEXTS.with(|contexts| contexts.borrow_mut().remove(&id));
        match context {
            Some(EventContext::Foundation(suspension)) => unsafe {
                let reply: *mut AnyObject = msg_send![self.manager, replyAppleEventForSuspensionID: suspension as *mut c_void];
                fail_reply(reply);
                let _: () = msg_send![self.manager, resumeWithSuspensionID: suspension as *mut c_void];
                GATE.with(|gate| gate.borrow_mut().released(id));
            },
            Some(EventContext::Raw(mut raw)) => {
                let prepared = unsafe { fail_raw_reply(&mut raw.reply.0) };
                if let Err(error) = prepared {
                    GATE.with(|gate| gate.borrow_mut().fail(error));
                    CONTEXTS.with(|contexts| contexts.borrow_mut().insert(id, EventContext::Raw(raw)));
                    return;
                }
                let error = unsafe { AEResumeTheCurrentEvent(std::ptr::addr_of!(raw.event.0), std::ptr::addr_of!(raw.reply.0), None, std::ptr::null_mut()) };
                if error != 0 {
                    GATE.with(|gate| gate.borrow_mut().fail(format!("Cannot complete joined quit AppleEvent: {error}")));
                    CONTEXTS.with(|contexts| contexts.borrow_mut().insert(id, EventContext::Raw(raw)));
                    return;
                }
                GATE.with(|gate| gate.borrow_mut().released(id));
            }
            None => return, // primary raw event already completed in its dispatcher
        }
        eprintln!("[quit-native] Resumed captured AppleEvent with failure after original native return");
    }
}

/// Engine Complete is the sole permission to continue original native handling.
/// An accepted original AppleEvent can return while its terminate is still queued;
/// keep that exact-once continuation armed instead of synthesizing another quit.
pub(crate) fn complete(status: &engine::ShutdownStatus) -> Result<(), String> {
    let plan = GATE.with(|gate| gate.borrow_mut().claim_original(status))?
        .ok_or("Native termination was already claimed or engine shutdown is incomplete")?;
    let original = ORIGINAL.with(Cell::get).ok_or("Native pre-terminate hook is unavailable")?;
    let manager = apple_events()?;
    let app = unsafe { msg_send![AnyClass::get(c"NSApplication").ok_or("NSApplication is unavailable")?, sharedApplication] };
    let sender = SENDER.with(|slot| slot.borrow_mut().take()).flatten();
    let mut replay = Replay { app, manager, original, sender, primary_raw: None };
    // No App/window/page/gate/coordinator borrow survives any native call.
    let result = replay_native_termination(&plan, &mut replay);
    if let Err(error) = &result { GATE.with(|gate| gate.borrow_mut().fail(error.clone())); }
    result
}


const fn fourcc(value: &[u8; 4]) -> u32 { u32::from_be_bytes(*value) }

// AEDataModel.h uses #pragma pack(push, 2), including on 64-bit macOS.
#[repr(C, packed(2))]
struct RawDesc { kind: u32, data: *mut *mut c_void }
type AppleEventHandler = unsafe extern "C" fn(*const RawDesc, *mut RawDesc, *mut c_void) -> i16;
struct OwnedDesc(RawDesc);
impl OwnedDesc {
    unsafe fn duplicate(source: *const RawDesc) -> Result<Self, String> {
        let mut copy = Self(RawDesc { kind: fourcc(b"null"), data: std::ptr::null_mut() });
        let error = unsafe { AEDuplicateDesc(source, &mut copy.0) };
        if error != 0 { return Err(format!("Cannot duplicate native quit descriptor: {error}")); }
        Ok(copy)
    }
}
impl Drop for OwnedDesc {
    fn drop(&mut self) {
        let error = unsafe { AEDisposeDesc(&mut self.0) };
        if error != 0 {
            eprintln!("[quit-native] Cannot dispose quit descriptor copy: {error}");
            let _ = GATE.try_with(|gate| {
                if let Ok(mut gate) = gate.try_borrow_mut() { gate.fail(format!("Cannot dispose native quit descriptor: {error}")); }
            });
        }
    }
}
struct RawEvent { event: OwnedDesc, reply: OwnedDesc }
enum EventContext { Foundation(usize), Raw(RawEvent) }
fn reserve_context() -> Result<usize, String> {
    NEXT_CONTEXT.with(|next| {
        let id = next.get();
        next.set(id.checked_add(1).ok_or("Native quit context IDs exhausted")?);
        Ok(id)
    })
}

unsafe extern "C" fn capture_quit_event(event: *const RawDesc, reply: *mut RawDesc, _: *mut c_void) -> i16 {
    let first = match GATE.with(|gate| gate.borrow_mut().capture()) {
        Ok(first) => first,
        Err(_) => return unsafe { fail_raw_reply(reply) }.unwrap_or(-10000),
    };
    crate::quit::seal();
    if first { SENDER.with(|slot| *slot.borrow_mut() = Some(None)); }
    let captured = (|| -> Result<usize, String> {
        if event.is_null() || reply.is_null() { return Err("Quit AppleEvent descriptors are absent".into()); }
        let id = reserve_context()?;
        // All fallible copies and registry capacity are prepared BEFORE suspend.
        let raw = unsafe { RawEvent { event: OwnedDesc::duplicate(event)?, reply: OwnedDesc::duplicate(reply)? } };
        CONTEXTS.with(|contexts| contexts.borrow_mut().try_reserve(1)).map_err(|e| format!("Cannot track quit AppleEvent: {e}"))?;
        let error = unsafe { AESuspendTheCurrentEvent(event) };
        if error != 0 { return Err(format!("Cannot suspend native quit AppleEvent: {error}")); }
        // No further fallible operation occurs before registration.
        CONTEXTS.with(|contexts| contexts.borrow_mut().insert(id, EventContext::Raw(raw)));
        GATE.with(|gate| gate.borrow_mut().record_context(first, Some(id)));
        Ok(id)
    })();
    match captured {
        Ok(_) => { eprintln!("[quit-native] Original quit AppleEvent captured before Cocoa; first={first}; returning suspended"); 0 }
        Err(error) => {
            eprintln!("[quit-native] Quit AppleEvent rejected: {error}");
            GATE.with(|gate| gate.borrow_mut().fail(error));
            unsafe { fail_raw_reply(reply) }.unwrap_or(-10000)
        }
    }
}

unsafe extern "C" fn replay_quit_event(event: *const RawDesc, reply: *mut RawDesc, _: *mut c_void) -> i16 {
    AE_DISPATCHED.with(|flag| flag.set(true));
    let original = ORIGINAL_AE.with(Cell::get);
    dispatch_original_quit_event(|| {
        let result = if let Some((handler, refcon)) = original {
            GATE.with(|gate| gate.borrow_mut().begin_event_dispatch());
            unsafe { handler(event, reply, refcon) }
        } else { -10000 };
        eprintln!("[quit-native] Original quit UPP returned {result}; preserving its live reply before resume completes");
        result
    }, |result| {
        // AEResume may send the reply as soon as THIS dispatcher returns.
        let finished = unsafe { finish_raw_reply(reply, result) };
        match &finished {
            Ok(code) => GATE.with(|gate| gate.borrow_mut().finish_event_dispatch(*code)),
            Err(error) => GATE.with(|gate| gate.borrow_mut().fail(error.clone())),
        }
        finished
    }).unwrap_or(-10000)
}

unsafe fn finish_raw_reply(reply: *mut RawDesc, returned: i16) -> Result<i16, String> {
    unsafe {
        let mut existing = 0i32;
        let mut actual_type = 0u32;
        let mut actual_size = 0isize;
        let read = if reply.is_null() || (*reply).kind == fourcc(b"null") { -1701 } else {
            AEGetParamPtr(reply, fourcc(b"errn"), fourcc(b"long"), &mut actual_type,
                &mut existing as *mut i32 as *mut c_void, 4, &mut actual_size)
        };
        let result = native_event_result_code((read == 0).then_some(existing), returned);
        if result == 0 { return Ok(0); } // Leave the original successful reply untouched.
        fail_raw_reply_with_result(reply, i16::try_from(result).unwrap_or(-10000))
    }
}

unsafe fn fail_raw_reply(reply: *mut RawDesc) -> Result<i16, String> {
    unsafe { fail_raw_reply_with_result(reply, -10000) }
}
unsafe fn fail_raw_reply_with_result(reply: *mut RawDesc, returned: i16) -> Result<i16, String> {
    if reply.is_null() { return Ok(-10000); }
    unsafe {
        if (*reply).kind == fourcc(b"null") { return Ok(-10000); }
        let mut existing = 0i32;
        let mut actual_type = 0u32;
        let mut actual_size = 0isize;
        let read = AEGetParamPtr(reply, fourcc(b"errn"), fourcc(b"long"), &mut actual_type,
            &mut existing as *mut i32 as *mut c_void, 4, &mut actual_size);
        let error = native_event_failure_code((read == 0).then_some(existing), returned);
        let written = AEPutParamPtr(reply, fourcc(b"errn"), fourcc(b"long"), &error as *const i32 as *const c_void, 4);
        if written != 0 { return Err(format!("Cannot write native quit failure reply: {written}")); }
        Ok(i16::try_from(error).unwrap_or(-10000))
    }
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn AEGetEventHandler(class: u32, id: u32, handler: *mut Option<AppleEventHandler>, refcon: *mut *mut c_void, system: u8) -> i16;
    fn AEInstallEventHandler(class: u32, id: u32, handler: Option<AppleEventHandler>, refcon: *mut c_void, system: u8) -> i16;
    fn AEDuplicateDesc(source: *const RawDesc, result: *mut RawDesc) -> i16;
    fn AEDisposeDesc(desc: *mut RawDesc) -> i16;
    fn AEGetParamPtr(event: *const RawDesc, key: u32, desired: u32, actual: *mut u32, data: *mut c_void, max: isize, size: *mut isize) -> i16;
    fn AEPutParamPtr(event: *mut RawDesc, key: u32, kind: u32, data: *const c_void, size: isize) -> i16;
}
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn AESuspendTheCurrentEvent(event: *const RawDesc) -> i16;
    fn AEResumeTheCurrentEvent(event: *const RawDesc, reply: *const RawDesc, dispatcher: Option<AppleEventHandler>, refcon: *mut c_void) -> i16;
}

#[cfg(test)]
mod ffi_tests {
    use super::*;
    #[test]
    fn sdk_appleevent_descriptor_and_callback_abi_match_lp64_pack2() {
        assert_eq!(std::mem::size_of::<RawDesc>(), 12);
        assert_eq!(std::mem::align_of::<RawDesc>(), 2);
        assert_eq!(std::mem::size_of::<Option<AppleEventHandler>>(), std::mem::size_of::<usize>());
        assert_eq!(std::mem::size_of::<*mut c_void>(), 8);
    }
}
