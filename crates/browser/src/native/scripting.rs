//! macOS AppleEvent transport only. Decoding queues owned Rust commands;
//! application/model/CEF work happens on a later GPUI turn.
use browser_core::scripting::{code, Command, Error, Object, Record, Value};
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
    sel,
};
use objc2_foundation::NSString;
use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::{c_void, CStr},
};
#[repr(C)]
struct Suspension {
    _opaque: [u8; 0],
}
unsafe impl objc2::RefEncode for Suspension {
    const ENCODING_REF: objc2::encode::Encoding = objc2::encode::Encoding::Pointer(
        &objc2::encode::Encoding::Struct("__NSAppleEventManagerSuspension", &[]),
    );
}
#[repr(C)]
pub(crate) struct OpaqueAEDataStorageType {
    _opaque: [u8; 0],
}
#[repr(C, packed(2))]
pub(crate) struct RawDesc {
    pub(crate) kind: u32,
    pub(crate) data: *mut *mut OpaqueAEDataStorageType,
}
unsafe impl objc2::RefEncode for RawDesc {
    const ENCODING_REF: objc2::encode::Encoding =
        objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct(
            "AEDesc",
            &[
                objc2::encode::Encoding::UInt,
                objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Pointer(
                    &objc2::encode::Encoding::Struct("OpaqueAEDataStorageType", &[]),
                )),
            ],
        ));
}
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn AEDuplicateDesc(source: *const RawDesc, copy: *mut RawDesc) -> i16;
    fn AEDisposeDesc(descriptor: *mut RawDesc) -> i16;
}

struct Pending {
    pub suspension: usize,
    pub command: Command,
}
thread_local! {static QUEUE:RefCell<VecDeque<Pending>>=const{RefCell::new(VecDeque::new())}; static RECEIVER:RefCell<Option<Retained<AnyObject>>>=const{RefCell::new(None)};}
unsafe fn manager() -> *mut AnyObject {
    unsafe {
        msg_send![
            AnyClass::get(c"NSAppleEventManager").unwrap(),
            sharedAppleEventManager
        ]
    }
}
unsafe fn descriptor() -> &'static AnyClass {
    AnyClass::get(c"NSAppleEventDescriptor").unwrap()
}
unsafe fn string(object: *mut AnyObject) -> String {
    if object.is_null() {
        return String::new();
    }
    let bytes: *const std::ffi::c_char = unsafe { msg_send![object, UTF8String] };
    if bytes.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(bytes) }
            .to_string_lossy()
            .into_owned()
    }
}
unsafe fn decode(d: *mut AnyObject, depth: usize) -> Result<Value, Error> {
    if d.is_null() {
        return Ok(Value::Null);
    }
    if depth > 32 {
        return Err(Error::invalid("AppleEvent nesting exceeds 32 levels."));
    }
    let kind: u32 = unsafe { msg_send![d, descriptorType] };
    if kind == code(b"null") {
        return Ok(Value::Null);
    }
    if kind == code(b"type") || kind == code(b"enum") {
        let c: u32 = if kind == code(b"enum") {
            unsafe { msg_send![d, enumCodeValue] }
        } else {
            unsafe { msg_send![d, typeCodeValue] }
        };
        return Ok(Value::Code(c));
    }
    if kind == code(b"abso") {
        let data: *mut AnyObject = unsafe { msg_send![d, data] };
        let length: usize = unsafe { msg_send![data, length] };
        if length != 4 {
            return Err(Error::invalid("Invalid absolute ordinal."));
        }
        let bytes: *const c_void = unsafe { msg_send![data, bytes] };
        if bytes.is_null() {
            return Err(Error::invalid("Empty absolute ordinal."));
        }
        return Ok(Value::Code(unsafe {
            std::ptr::read_unaligned(bytes.cast::<u32>())
        }));
    }
    if kind == code(b"bool") {
        let b: u8 = unsafe { msg_send![d, booleanValue] };
        return Ok(Value::Bool(b != 0));
    }
    if [code(b"long"), code(b"shor"), code(b"comp")].contains(&kind) {
        let n: i32 = unsafe { msg_send![d, int32Value] };
        return Ok(Value::Int(n.into()));
    }
    let record: Bool = unsafe { msg_send![d, isRecordDescriptor] };
    if record.as_bool() || [code(b"list"), code(b"obj "), code(b"insl")].contains(&kind) {
        let count: isize = unsafe { msg_send![d, numberOfItems] };
        if !(0..=4096).contains(&count) {
            return Err(Error::invalid("AppleEvent collection is too large."));
        }
        let mut r = Record::new();
        let mut list = Vec::new();
        for i in 1..=count {
            let child: *mut AnyObject = unsafe { msg_send![d,descriptorAtIndex:i] };
            let value = unsafe { decode(child, depth + 1) }?;
            if kind == code(b"list") {
                list.push(value)
            } else {
                let key: u32 = unsafe { msg_send![d,keywordForDescriptorAtIndex:i] };
                r.insert(key, value);
            }
        }
        return Ok(if kind == code(b"list") {
            Value::List(list)
        } else {
            Value::Record(kind, r)
        });
    }
    let text: *mut AnyObject = unsafe { msg_send![d, stringValue] };
    if !text.is_null() {
        let s = unsafe { string(text) };
        if s.len() > 1024 * 1024 {
            return Err(Error::invalid("AppleEvent text is too large."));
        }
        return Ok(Value::Text(s));
    }
    Err(Error::invalid(format!(
        "Unsupported AppleEvent descriptor {kind:08x}."
    )))
}
unsafe fn encode(v: &Value) -> Result<*mut AnyObject, Error> {
    unsafe {
        let class = descriptor();
        let result = match v {
            Value::Null => msg_send![class,descriptorWithTypeCode:code(b"msng")],
            Value::Text(s) => msg_send![class,descriptorWithString:&*NSString::from_str(s)],
            Value::Bool(b) => msg_send![class,descriptorWithBoolean:u8::from(*b)],
            Value::Int(n) => msg_send![class,descriptorWithInt32:*n as i32],
            Value::Code(c) => msg_send![class,descriptorWithTypeCode:*c],
            Value::List(values) => {
                let list: *mut AnyObject = msg_send![class, listDescriptor];
                for (i, v) in values.iter().enumerate() {
                    let _: () = msg_send![list,insertDescriptor:encode(v)?, atIndex:i as isize+1];
                }
                list
            }
            Value::Record(kind, values) => {
                let record: *mut AnyObject = msg_send![class, recordDescriptor];
                for (key, v) in values {
                    let child = if *kind == code(b"obj ") && *key == code(b"form") {
                        msg_send![class,descriptorWithEnumCode:v.type_code().unwrap_or(0)]
                    } else if *kind == code(b"obj ") && *key == code(b"from") && *v == Value::Null {
                        msg_send![class, nullDescriptor]
                    } else {
                        encode(v)?
                    };
                    let _: () = msg_send![record,setDescriptor:child, forKeyword:*key];
                }
                if *kind == code(b"reco") {
                    record
                } else {
                    // `data` flattens record descriptors; using it as raw bytes
                    // loses their record flag. Duplicate the actual AEDesc and
                    // change only its type, preserving the nested descriptors.
                    let source: *const RawDesc = msg_send![record, aeDesc];
                    let mut copy = RawDesc {
                        kind: code(b"null"),
                        data: std::ptr::null_mut(),
                    };
                    if AEDuplicateDesc(source, &mut copy) != 0 {
                        return Err(Error::invalid("Cannot duplicate an object reference."));
                    }
                    let allocated: *mut AnyObject = msg_send![class, alloc];
                    if allocated.is_null() {
                        AEDisposeDesc(&mut copy);
                        return Err(Error::invalid("Cannot allocate an object reference."));
                    }
                    copy.kind = *kind;
                    let created = Retained::<AnyObject>::from_raw(
                        msg_send![allocated,initWithAEDescNoCopy:&copy as *const RawDesc],
                    )
                    .ok_or_else(|| Error::invalid("Cannot construct an object reference."))?;
                    Retained::autorelease_ptr(created)
                }
            }
            Value::Object(object) => {
                let (kind, id, container) = match object {
                    Object::Application => return Ok(msg_send![class, nullDescriptor]),
                    Object::Window(id) => (code(b"WiND"), id, Value::Null),
                    Object::Tab { window, id } => (
                        code(b"tAbB"),
                        id,
                        Value::Object(Object::Window(window.clone())),
                    ),
                    Object::Space { window, id } => (
                        code(b"sPaC"),
                        id,
                        Value::Object(Object::Window(window.clone())),
                    ),
                };
                return encode(&Value::Record(
                    code(b"obj "),
                    Record::from([
                        (code(b"want"), Value::Code(kind)),
                        (code(b"from"), container),
                        (code(b"form"), Value::Code(code(b"ID  "))),
                        (code(b"seld"), Value::Text(id.clone())),
                    ]),
                ));
            }
        };
        if result.is_null() {
            Err(Error::invalid("Cannot encode the scripting result."))
        } else {
            Ok(result)
        }
    }
}
unsafe fn write(reply: *mut AnyObject, result: Result<Value, Error>) {
    unsafe {
        match result.and_then(|v| encode(&v)) {
            Ok(value) => {
                let _: () = msg_send![reply,setParamDescriptor:value,forKeyword:code(b"----")];
            }
            Err(error) => {
                let class = descriptor();
                let number: *mut AnyObject = msg_send![class,descriptorWithInt32:error.number];
                let text: *mut AnyObject =
                    msg_send![class,descriptorWithString:&*NSString::from_str(&error.message)];
                let _: () = msg_send![reply,setParamDescriptor:number,forKeyword:code(b"errn")];
                let _: () = msg_send![reply,setParamDescriptor:text,forKeyword:code(b"errs")];
            }
        }
    }
}
unsafe extern "C-unwind" fn receive(
    _: *mut AnyObject,
    _: Sel,
    event: *mut AnyObject,
    reply: *mut AnyObject,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let class: u32 = msg_send![event, eventClass];
        let id: u32 = msg_send![event, eventID];
        if cfg!(debug_assertions) && std::env::var_os("BROWSER_SCRIPT").is_some() {
            eprintln!("[scripting-received] {class:08x}/{id:08x}");
        }
        let count: isize = msg_send![event, numberOfItems];
        if !(0..=64).contains(&count) {
            return Err(Error::invalid("Too many AppleEvent parameters."));
        }
        let mut params = Record::new();
        for i in 1..=count {
            let key: u32 = msg_send![event,keywordForDescriptorAtIndex:i];
            let child: *mut AnyObject = msg_send![event,descriptorAtIndex:i];
            params.insert(key, decode(child, 0)?);
        }
        // 'subj' is an attribute for commands inside an AppleScript tell block.
        let subject: *mut AnyObject = msg_send![event,attributeDescriptorForKeyword:code(b"subj")];
        if !subject.is_null() {
            params.insert(code(b"subj"), decode(subject, 0)?);
        }
        if cfg!(debug_assertions) && std::env::var_os("BROWSER_SCRIPT").is_some() {
            eprintln!("[scripting-event] {class:08x}/{id:08x} {params:?}");
        }
        let command = Command::decode(class, id, &params)?;
        if crate::quit::is_quitting() {
            return Err(Error {
                number: -600,
                message: "Echo is quitting.".into(),
            });
        }
        QUEUE.with(|queue| {
            let mut queue = queue.borrow_mut();
            if queue.len() >= 64 {
                return Err(Error::invalid("Too many pending scripting commands."));
            }
            let suspension: *mut Suspension = msg_send![manager(), suspendCurrentAppleEvent];
            if suspension.is_null() {
                return Err(Error::invalid("Cannot suspend this AppleEvent."));
            }
            queue.push_back(Pending {
                suspension: suspension as usize,
                command,
            });
            Ok(())
        })
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => unsafe { write(reply, Err(e)) },
        Err(_) => unsafe {
            write(
                reply,
                Err(Error::invalid("Scripting request could not be decoded.")),
            )
        },
    }
}
pub(crate) fn install() -> Result<(), String> {
    unsafe {
        let class = if let Some(c) = AnyClass::get(c"EchoScriptingReceiver") {
            c
        } else {
            let mut c = ClassBuilder::new(
                c"EchoScriptingReceiver",
                AnyClass::get(c"NSObject").ok_or("NSObject is unavailable")?,
            )
            .ok_or("Cannot create scripting receiver")?;
            c.add_method(
                sel!(handleEvent:withReply:),
                receive as unsafe extern "C-unwind" fn(_, _, _, _),
            );
            c.register()
        };
        let receiver = Retained::<AnyObject>::from_raw(msg_send![class, new])
            .ok_or("Cannot allocate scripting receiver")?;
        for (suite, commands) in [
            (
                code(b"core"),
                &[
                    code(b"getd"),
                    code(b"setd"),
                    code(b"cnte"),
                    code(b"crel"),
                    code(b"clos"),
                ][..],
            ),
            (
                code(b"CrSu"),
                &[
                    code(b"Back"),
                    code(b"Fwd "),
                    code(b"Rlod"),
                    code(b"stop"),
                    code(b"ExJa"),
                ][..],
            ),
            (code(b"CrSe"), &[code(b"lect")][..]),
            (code(b"SpaF"), &[code(b"ocus")][..]),
        ] {
            for command in commands {
                let _: () = msg_send![manager(),setEventHandler:&*receiver, andSelector:sel!(handleEvent:withReply:), forEventClass:suite, andEventID:*command];
            }
        }
        RECEIVER.with(|r| *r.borrow_mut() = Some(receiver));
        if cfg!(debug_assertions) && std::env::var_os("BROWSER_SCRIPT").is_some() {
            eprintln!("[scripting] Installed transport");
        }
        Ok(())
    }
}
pub(crate) fn take() -> Vec<(usize, Command)> {
    QUEUE.with(|q| {
        q.borrow_mut()
            .drain(..)
            .map(|p| (p.suspension, p.command))
            .collect()
    })
}
pub(crate) fn finish(suspension: usize, result: Result<Value, Error>) {
    unsafe {
        let manager = manager();
        let reply: *mut AnyObject =
            msg_send![manager,replyAppleEventForSuspensionID:suspension as *mut Suspension];
        if !reply.is_null() {
            write(reply, result)
        }
        let _: () = msg_send![manager,resumeWithSuspensionID:suspension as *mut Suspension];
    }
}
pub(crate) fn state(view: Option<*mut c_void>) -> (usize, bool, bool, bool) {
    let Some(view) = view else {
        return (usize::MAX, false, true, false);
    };
    unsafe {
        let w: *mut AnyObject = msg_send![view as *mut AnyObject, window];
        if w.is_null() {
            return (usize::MAX, false, true, false);
        }
        let app: *mut AnyObject =
            msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let windows: *mut AnyObject = msg_send![app, orderedWindows];
        let index: usize = msg_send![windows,indexOfObjectIdenticalTo:w];
        let mini: Bool = msg_send![w, isMiniaturized];
        let visible: Bool = msg_send![w, isVisible];
        let zoom: Bool = msg_send![w, isZoomed];
        (index, mini.as_bool(), visible.as_bool(), zoom.as_bool())
    }
}
pub(crate) fn set_window(
    view: Option<*mut c_void>,
    property: u32,
    value: &Value,
) -> Result<(), Error> {
    let view = view.ok_or_else(Error::missing)?;
    let Value::Bool(value) = value else {
        return Err(Error::invalid("Expected a boolean window state."));
    };
    unsafe {
        let w: *mut AnyObject = msg_send![view as *mut AnyObject, window];
        if w.is_null() {
            return Err(Error::missing());
        }
        if property == code(b"pmnd") {
            let current: Bool = msg_send![w, isMiniaturized];
            if current.as_bool() != *value {
                if *value {
                    let _: () = msg_send![w,miniaturize:std::ptr::null_mut::<AnyObject>()];
                } else {
                    let _: () = msg_send![w,deminiaturize:std::ptr::null_mut::<AnyObject>()];
                }
            }
        } else if property == code(b"pvis") {
            if *value {
                let _: () = msg_send![w,orderFront:std::ptr::null_mut::<AnyObject>()];
            } else {
                let _: () = msg_send![w,orderOut:std::ptr::null_mut::<AnyObject>()];
            }
        } else if property == code(b"pzum") {
            let current: Bool = msg_send![w, isZoomed];
            if current.as_bool() != *value {
                let _: () = msg_send![w,zoom:std::ptr::null_mut::<AnyObject>()];
            }
        } else {
            return Err(Error {
                number: -10006,
                message: "This window property cannot be changed after creation.".into(),
            });
        }
    }
    Ok(())
}

pub(crate) fn frontmost() -> bool {
    unsafe {
        let app: *mut AnyObject =
            msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let active: Bool = msg_send![app, isActive];
        active.as_bool()
    }
}
pub(crate) fn sync_javascript_checkmark(enabled: bool) {
    unsafe {
        let app: *mut AnyObject =
            msg_send![AnyClass::get(c"NSApplication").unwrap(), sharedApplication];
        let menu: *mut AnyObject = msg_send![app, mainMenu];
        if menu.is_null() {
            return;
        }
        let view: *mut AnyObject = msg_send![menu,itemWithTitle:&*NSString::from_str("View")];
        if view.is_null() {
            return;
        }
        let submenu: *mut AnyObject = msg_send![view, submenu];
        let developer: *mut AnyObject =
            msg_send![submenu,itemWithTitle:&*NSString::from_str("Developer")];
        if developer.is_null() {
            return;
        }
        let submenu: *mut AnyObject = msg_send![developer, submenu];
        let item: *mut AnyObject = msg_send![submenu,itemWithTitle:&*NSString::from_str("Allow JavaScript from Apple Events")];
        if item.is_null() {
            return;
        }
        let current: isize = msg_send![item, state];
        let wanted = if enabled { 1isize } else { 0 };
        if current != wanted {
            let _: () = msg_send![item,setState:wanted];
        }
    }
}

/// Native automation diagnostic: run AppleScript on a worker, targeting this
/// process. It does not require another application's Automation permission.
pub(crate) fn execute_file(path: &str) -> Result<String, String> {
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    objc2::rc::autoreleasepool(|_| unsafe {
        let class = AnyClass::get(c"NSAppleScript").ok_or("NSAppleScript is unavailable")?;
        let allocated: *mut AnyObject = msg_send![class, alloc];
        let script = Retained::<AnyObject>::from_raw(
            msg_send![allocated,initWithSource:&*NSString::from_str(&source)],
        )
        .ok_or("Cannot compile AppleScript")?;
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let result: *mut AnyObject = msg_send![&*script,executeAndReturnError:&mut error];
        if result.is_null() {
            let description: *mut AnyObject = msg_send![error, description];
            Err(string(description))
        } else {
            let text: *mut AnyObject = msg_send![result, stringValue];
            Ok(string(text))
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn returned_object_references_keep_their_record_structure() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let expected = Object::Tab {
                window: "window-a".into(),
                id: "tab-9".into(),
            };
            let desc = encode(&Value::Object(expected.clone())).unwrap();
            let kind: u32 = msg_send![desc, descriptorType];
            assert_eq!(kind, code(b"obj "));
            let count: isize = msg_send![desc, numberOfItems];
            assert_eq!(count, 4);
            let decoded = decode(desc, 0).unwrap();
            assert_eq!(
                browser_core::scripting::query(&decoded).unwrap(),
                expected.query()
            );
        })
    }
    #[test]
    fn absolute_ordinals_use_apple_event_codes_instead_of_text_coercion() {
        objc2::rc::autoreleasepool(|_| unsafe {
            for value in [code(b"all "), code(b"last"), code(b"midd")] {
                let desc: *mut AnyObject = msg_send![descriptor(),descriptorWithDescriptorType:code(b"abso"),bytes:&value as *const u32 as *const c_void,length:4usize];
                assert_eq!(decode(desc, 0), Ok(Value::Code(value)));
            }
        })
    }
}
#[cfg(test)]
mod scalar_tests {
    use super::*;
    #[test]
    fn apple_event_scalars_use_the_sdk_abi_including_carbon_boolean_bytes() {
        objc2::rc::autoreleasepool(|_| unsafe {
            for value in [
                Value::Bool(false),
                Value::Bool(true),
                Value::Int(-1700),
                Value::Text("Echo 💜".into()),
                Value::Code(code(b"WiND")),
                Value::List(vec![Value::Bool(true), Value::Text("value".into())]),
            ] {
                let native = encode(&value).unwrap();
                assert_eq!(decode(native, 0).unwrap(), value);
            }
        })
    }
}
