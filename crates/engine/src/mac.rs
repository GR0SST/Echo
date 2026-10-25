//! AppKit glue: CEF's NSApplication requirements and child NSView layout.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

use objc2::ffi;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::Bounds;

thread_local! {
    static HANDLING_SEND_EVENT: Cell<bool> = const { Cell::new(false) };
}

type SendEvent = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);
static ORIGINAL_SEND_EVENT: OnceLock<SendEvent> = OnceLock::new();

#[cfg(target_arch = "aarch64")]
const BOOL_ENCODING: &str = "B";
#[cfg(not(target_arch = "aarch64"))]
const BOOL_ENCODING: &str = "c";

extern "C-unwind" fn is_handling_send_event(_: *mut AnyObject, _: Sel) -> Bool {
    Bool::new(HANDLING_SEND_EVENT.get())
}

extern "C-unwind" fn set_handling_send_event(_: *mut AnyObject, _: Sel, value: Bool) {
    HANDLING_SEND_EVENT.set(value.as_bool());
}

extern "C-unwind" fn send_event(this: *mut AnyObject, sel: Sel, event: *mut AnyObject) {
    remember_pip_frame(event);
    observe_pip_return(event);
    let was_handling = HANDLING_SEND_EVENT.replace(true);
    if let Some(original) = ORIGINAL_SEND_EVENT.get() {
        unsafe { original(this, sel, event) };
    }
    remember_pip_frame(event);
    HANDLING_SEND_EVENT.set(was_handling);
}

/// Chromium requires `NSApp` to implement `CrAppControlProtocol`. The
/// NSApplication subclass belongs to the UI toolkit, so the methods are
/// added to whatever class `NSApp` already is.
pub fn install_app_protocols() {
    unsafe {
        let ns_application = AnyClass::get(c"NSApplication").expect("AppKit is not loaded");
        let app: *mut AnyObject = msg_send![ns_application, sharedApplication];
        let class: *const AnyClass = msg_send![app, class];
        let class = class as *mut AnyClass;

        let original = (*class)
            .instance_method(sel!(sendEvent:))
            .expect("NSApplication has no sendEvent:")
            .implementation();
        let _ = ORIGINAL_SEND_EVENT.set(std::mem::transmute::<Imp, SendEvent>(original));

        let getter_types = format!("{BOOL_ENCODING}@:\0");
        let setter_types = format!("v@:{BOOL_ENCODING}\0");
        add_method(
            class,
            sel!(isHandlingSendEvent),
            std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool, Imp>(
                is_handling_send_event,
            ),
            &getter_types,
        );
        add_method(
            class,
            sel!(setHandlingSendEvent:),
            std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel, Bool), Imp>(
                set_handling_send_event,
            ),
            &setter_types,
        );
        // Overrides NSApplication's implementation; the original is called above.
        add_method(
            class,
            sel!(sendEvent:),
            std::mem::transmute::<SendEvent, Imp>(send_event),
            "v@:@\0",
        );

        for name in [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"] {
            add_protocol(class, name);
        }
    }
}

unsafe fn add_method(class: *mut AnyClass, sel: Sel, imp: Imp, types: &str) {
    let added = unsafe { ffi::class_addMethod(class, sel, imp, types.as_ptr() as *const c_char) };
    if !added.as_bool() {
        // Already defined directly on the class: replace it.
        unsafe { ffi::class_replaceMethod(class, sel, imp, types.as_ptr() as *const c_char) };
    }
}

unsafe fn add_protocol(class: *mut AnyClass, name: &CStr) {
    unsafe {
        let protocol = ffi::objc_getProtocol(name.as_ptr());
        if !protocol.is_null() {
            ffi::class_addProtocol(class, protocol);
        }
    }
}

/// Whether the main run loop runs in AppKit's event tracking mode, i.e.
/// inside a mouse tracking loop such as a window resize.
pub fn is_tracking_events() -> bool {
    unsafe {
        let Some(class) = AnyClass::get(c"NSRunLoop") else {
            return false;
        };
        let run_loop: *mut AnyObject = msg_send![class, currentRunLoop];
        let mode: *mut AnyObject = msg_send![run_loop, currentMode];
        if mode.is_null() {
            return false;
        }
        let name: *const c_char = msg_send![mode, UTF8String];
        !name.is_null() && CStr::from_ptr(name) == c"NSEventTrackingRunLoopMode"
    }
}

/// Whether native events wait in the queue for the toolkit to dispatch.
pub fn has_pending_events() -> bool {
    const ANY: u64 = u64::MAX;
    unsafe {
        let (Some(app_class), Some(date_class), Some(string_class)) = (
            AnyClass::get(c"NSApplication"),
            AnyClass::get(c"NSDate"),
            AnyClass::get(c"NSString"),
        ) else {
            return false;
        };
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        let past: *mut AnyObject = msg_send![date_class, distantPast];
        let mode: *mut AnyObject = msg_send![string_class, stringWithUTF8String: c"kCFRunLoopDefaultMode".as_ptr()];
        let event: *mut AnyObject = msg_send![
            app,
            nextEventMatchingMask: ANY,
            untilDate: past,
            inMode: mode,
            dequeue: Bool::NO
        ];
        !event.is_null()
    }
}

/// Positions `view` inside `parent` using top-left-origin coordinates.
pub fn set_frame(view: *mut c_void, parent: *mut c_void, frame: Bounds) {
    if view.is_null() || parent.is_null() {
        return;
    }
    unsafe {
        let view = view as *mut AnyObject;
        let parent = parent as *mut AnyObject;
        let flipped: Bool = msg_send![parent, isFlipped];
        let bounds: NSRect = msg_send![parent, bounds];
        let y = if flipped.as_bool() {
            frame.y
        } else {
            bounds.size.height - frame.y - frame.height
        };
        let rect = NSRect::new(NSPoint::new(frame.x, y), NSSize::new(frame.width, frame.height));
        let _: () = msg_send![view, setFrame: rect];
    }
}

pub fn set_hidden(view: *mut c_void, hidden: bool) {
    if view.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![view as *mut AnyObject, setHidden: Bool::new(hidden)];
    }
}

/// A plain view inside `parent` that clips its subviews to rounded corners.
/// Pages live in one, since AppKit manages the layers of CEF's own views.
/// The engine owns the returned reference until `release_clip_view`: the
/// window's superview may be destroyed before the engine is dropped.
pub fn create_clip_view(parent: *mut c_void, radius: f64) -> *mut c_void {
    unsafe {
        let class = AnyClass::get(c"NSView").expect("AppKit is not loaded");
        let view: *mut AnyObject = msg_send![class, alloc];
        let view: *mut AnyObject = msg_send![view, initWithFrame: NSRect::ZERO];
        let _: () = msg_send![view, setWantsLayer: Bool::YES];
        let layer: *mut AnyObject = msg_send![view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: radius];
            let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
        }
        let _: () = msg_send![parent as *mut AnyObject, addSubview: view];
        view as *mut c_void
    }
}

/// Moves the retained clip host, keeping CEF's browser and renderer alive.
pub fn move_clip_view(view: *mut c_void, parent: *mut c_void) {
    unsafe {
        let _: () = msg_send![view as *mut AnyObject, removeFromSuperview];
        let _: () = msg_send![parent as *mut AnyObject, addSubview: view as *mut AnyObject];
    }
}

#[repr(transparent)]
struct LayerColor(*const c_void);
unsafe impl objc2::Encode for LayerColor {
    const ENCODING: objc2::Encoding = objc2::Encoding::Pointer(&objc2::Encoding::Struct("CGColor", &[]));
}

/// CALayer retains its mask. The CEF child and its autoresizing stay untouched.
pub fn set_viewport_clip(view: *mut c_void, clip: Option<Bounds>) {
    unsafe {
        let layer: *mut AnyObject = msg_send![view as *mut AnyObject, layer];
        if layer.is_null() { return; }
        let Some(clip) = clip else {
            let _: () = msg_send![layer, setMask: std::ptr::null_mut::<AnyObject>()];
            return;
        };
        let bounds: NSRect = msg_send![view as *mut AnyObject, bounds];
        let class = AnyClass::get(c"CALayer").expect("AppKit requires QuartzCore");
        let mask: *mut AnyObject = msg_send![class, layer];
        let frame = NSRect::new(NSPoint::new(clip.x, bounds.size.height - clip.y - clip.height), NSSize::new(clip.width, clip.height));
        let _: () = msg_send![mask, setFrame: frame];
        let _: () = msg_send![mask, setCornerRadius: 6_f64];
        let color_class = AnyClass::get(c"NSColor").expect("AppKit is loaded");
        let black: *mut AnyObject = msg_send![color_class, blackColor];
        let color: LayerColor = msg_send![black, CGColor];
        let _: () = msg_send![mask, setBackgroundColor: color];
        let _: () = msg_send![layer, setMask: mask];
    }
}

pub fn set_corner_radius(view: *mut c_void, radius: f64) {
    unsafe {
        let layer: *mut AnyObject = msg_send![view as *mut AnyObject, layer];
        if !layer.is_null() { let _: () = msg_send![layer, setCornerRadius: radius]; }
    }
}

/// The clip host is an unflipped NSView: minimum-Y corners are its bottom edge.
/// Keep those rounded when the page joins a GPUI split header along its top edge.
pub fn set_top_corners_rounded(view: *mut c_void, rounded: bool) {
    unsafe {
        let layer: *mut AnyObject = msg_send![view as *mut AnyObject, layer];
        if !layer.is_null() {
            let corners: usize = if rounded { 0b1111 } else { 0b0011 };
            let _: () = msg_send![layer, setMaskedCorners: corners];
        }
    }
}

/// Makes `view` fill its superview and follow its size.
pub fn fill_superview(view: *mut c_void) {
    const WIDTH_SIZABLE: usize = 1 << 1;
    const HEIGHT_SIZABLE: usize = 1 << 4;
    if view.is_null() {
        return;
    }
    unsafe {
        let view = view as *mut AnyObject;
        let superview: *mut AnyObject = msg_send![view, superview];
        if superview.is_null() {
            return;
        }
        let bounds: NSRect = msg_send![superview, bounds];
        let _: () = msg_send![view, setFrame: bounds];
        let _: () = msg_send![view, setAutoresizingMask: WIDTH_SIZABLE | HEIGHT_SIZABLE];
    }
}

pub fn release_clip_view(view: *mut c_void) {
    if !view.is_null() {
        unsafe {
            let _: () = msg_send![view as *mut AnyObject, removeFromSuperview];
            ffi::objc_release(view as *mut _);
        }
    }
}

/// Gives keyboard focus back to the toolkit's view (e.g. for a text field
/// drawn over the page).
pub fn make_first_responder(view: *mut c_void) {
    if view.is_null() {
        return;
    }
    unsafe {
        let view = view as *mut AnyObject;
        let window: *mut AnyObject = msg_send![view, window];
        if !window.is_null() {
            let _: Bool = msg_send![window, makeFirstResponder: view];
        }
    }
}

/// The window's content view that contains `view`. Pages are added there,
/// next to the toolkit's view: AppKit does not composite subviews of a
/// layer-hosting view such as GPUI's Metal view.
pub fn content_view_of(view: *mut c_void) -> *mut c_void {
    if view.is_null() {
        return view;
    }
    unsafe {
        let window: *mut AnyObject = msg_send![view as *mut AnyObject, window];
        if window.is_null() {
            return view;
        }
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() { view } else { content as *mut c_void }
    }
}

/// CEF154 CefBrowserHostView::dealloc calls WindowDestroyed(). The owning
/// superview supplies the native child's strong reference; detach ONLY this
/// child, never the clip host or NSWindow. Do not retain the child ourselves:
/// that would prevent dealloc and therefore OnBeforeClose. The clip remains
/// retained by the runtime lease until the callback has returned.
pub(crate) fn destroy_browser_child(child: *mut c_void) {
    unsafe {
        let _: () = msg_send![child as *mut AnyObject, removeFromSuperview];
    }
}

/// Independently owned, never ordered front. The existing clip remains CEF's
/// original parent_view; only its superview changes. No GPUI owner is retained.
pub(crate) fn create_retired_window() -> Result<(*mut c_void, *mut c_void), String> {
    unsafe {
        let class = AnyClass::get(c"NSWindow").ok_or("AppKit NSWindow unavailable")?;
        let window: *mut AnyObject = msg_send![class, alloc];
        let rect = NSRect::new(NSPoint::new(0., 0.), NSSize::new(16., 16.));
        let window: *mut AnyObject = msg_send![window, initWithContentRect: rect, styleMask: 0usize, backing: 2usize, defer: Bool::NO];
        if window.is_null() { return Err("Hidden NSWindow creation failed".into()); }
        let _: () = msg_send![window, setReleasedWhenClosed: Bool::NO];
        let _: () = msg_send![window, setExcludedFromWindowsMenu: Bool::YES];
        let _: () = msg_send![window, orderOut: std::ptr::null_mut::<AnyObject>()];
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() {
            ffi::objc_release(window);
            return Err("Hidden NSWindow has no content view".into());
        }
        Ok((window.cast(), content.cast()))
    }
}

pub(crate) fn clip_has_parent(clip: *mut c_void, parent: *mut c_void) -> bool {
    unsafe {
        let actual: *mut AnyObject = msg_send![clip as *mut AnyObject, superview];
        actual.cast::<c_void>() == parent
    }
}

pub(crate) fn release_retired_window(window: *mut c_void) {
    unsafe {
        let window = window as *mut AnyObject;
        let _: () = msg_send![window, close];
        ffi::objc_release(window);
    }
}

/// Adjust only Chromium's verified native video overlay. Chromium retains
/// ownership, drag/resize handling, transport controls and remembered location.
/// The localized title comes from the pinned CEF resource bundle, not web text.
#[derive(Clone)]
struct PipReturn {
    window: isize,
    label: String,
    callback: std::rc::Rc<dyn Fn()>,
}
type PipStyle = (String, String, std::rc::Rc<dyn Fn()>, std::time::Instant);
thread_local! {
    static PIP_STYLE: std::cell::RefCell<Option<PipStyle>> = const { std::cell::RefCell::new(None) };
    static PIP_RETURN: std::cell::RefCell<Option<PipReturn>> = const { std::cell::RefCell::new(None) };
    static PIP_OWNER: Cell<Option<i32>> = const { Cell::new(None) };
    static PIP_STYLED: Cell<isize> = const { Cell::new(0) };
    static PIP_ARMED: Cell<Option<isize>> = const { Cell::new(None) };
    static PIP_FRAME: Cell<Option<NSRect>> = const { Cell::new(None) };
}

// Sampling after native dispatch also catches AppKit header-drag tracking,
// which can consume its release event inside the original sendEvent call.
fn remember_pip_frame(event: *mut AnyObject) {
    let Some(number)=PIP_RETURN.with_borrow(|binding|binding.as_ref().map(|binding|binding.window)) else { return; };
    unsafe {
        let window:*mut AnyObject=msg_send![event,window];
        if window.is_null() { return; }
        let actual:isize=msg_send![window,windowNumber];
        let visible:Bool=msg_send![window,isVisible];
        if actual==number && visible.as_bool() { PIP_FRAME.set(Some(msg_send![window,frame])); }
    }
}

// Alloy's WebContents activation does not select the embedding app's tab.
// Observe the existing native control semantically, without replacing its UI,
// consuming its event, or guessing its hit rectangle after a user resize.
fn observe_pip_return(event: *mut AnyObject) {
    unsafe {
        let kind:usize=msg_send![event,type];
        if kind!=1 && kind!=2 { return; } // left mouse down/up
        let armed=PIP_ARMED.replace(None);
        let Some(binding)=PIP_RETURN.with_borrow(Clone::clone) else { return; };
        if kind==2 && armed!=Some(binding.window) { return; }
        let window:*mut AnyObject=msg_send![event,window];
        if window.is_null() { return; }
        let number:isize=msg_send![window,windowNumber];
        if number!=binding.window { return; }
        let local:NSPoint=msg_send![event,locationInWindow];
        let point:NSPoint=msg_send![window,convertPointToScreen:local];
        let mut element:*mut AnyObject=msg_send![window,accessibilityHitTest:point];
        for _ in 0..5 {
            if element.is_null() { break; }
            let role_supported:Bool=msg_send![element,respondsToSelector:sel!(accessibilityRole)];
            let is_button=if role_supported.as_bool() {
                let role:*mut AnyObject=msg_send![element,accessibilityRole];
                let role_name:*const c_char=if role.is_null(){std::ptr::null()}else{msg_send![role,UTF8String]};
                !role_name.is_null() && CStr::from_ptr(role_name).to_bytes()==b"AXButton"
            }else{false};
            if is_button {
                for selector in [sel!(accessibilityLabel),sel!(accessibilityTitle)] {
                    let supported:Bool=msg_send![element,respondsToSelector:selector];
                    if !supported.as_bool() { continue; }
                    let text:*mut AnyObject=match selector {
                        value if value==sel!(accessibilityLabel)=>msg_send![element,accessibilityLabel],
                        _=>msg_send![element,accessibilityTitle],
                    };
                    if text.is_null() { continue; }
                    let bytes:*const c_char=msg_send![text,UTF8String];
                    if !bytes.is_null() && CStr::from_ptr(bytes).to_bytes()==binding.label.as_bytes() {
                        if kind==1 { PIP_ARMED.set(Some(number)); } else { (binding.callback)(); }
                        return;
                    }
                }
            }
            let supported:Bool=msg_send![element,respondsToSelector:sel!(accessibilityParent)];
            if !supported.as_bool() { break; }
            element=msg_send![element,accessibilityParent];
        }
    }
}

pub fn video_pip_exited(owner: i32) {
    if PIP_OWNER.get()!=Some(owner) { return; }
    PIP_OWNER.set(None);PIP_STYLED.set(0);PIP_ARMED.set(None);
    PIP_STYLE.with_borrow_mut(|pending|*pending=None);
    PIP_RETURN.with_borrow_mut(|binding|*binding=None);
}

pub fn style_video_pip(owner: i32, title: &str, return_label: &str, on_return: impl Fn()+'static) {
    if PIP_OWNER.get()!=Some(owner) { PIP_OWNER.set(Some(owner));PIP_STYLED.set(0); }
    if !title.is_empty() && !return_label.is_empty() {
        PIP_STYLE.with_borrow_mut(|pending|*pending=Some((title.into(),return_label.into(),std::rc::Rc::new(on_return),std::time::Instant::now())));
    }
}

pub fn poll_video_pip_style() {
    // The DOM event can precede native widget creation. Retry even for a paused
    // video whose deduplicated media report will not otherwise change again.
    let Some((title,label,callback,started))=PIP_STYLE.with_borrow_mut(Option::take) else { return; };
    if !apply_video_pip_style(&title,&label,&callback) && started.elapsed()<std::time::Duration::from_secs(2) {
        PIP_STYLE.with_borrow_mut(|pending|*pending=Some((title,label,callback,started)));
    }
}

fn apply_video_pip_style(title: &str, label: &str, callback: &std::rc::Rc<dyn Fn()>) -> bool {
    unsafe {
        let Some(app_class)=AnyClass::get(c"NSApplication") else { return false; };
        let Some(pip_class)=AnyClass::get(c"NativeWidgetMacFramelessNSWindow") else { return false; };
        let app: *mut AnyObject=msg_send![app_class,sharedApplication];
        let windows:*mut AnyObject=msg_send![app,windows];
        let count:usize=msg_send![windows,count];
        for i in 0..count {
            let window:*mut AnyObject=msg_send![windows,objectAtIndex:i];
            let native:Bool=msg_send![window,isKindOfClass:pip_class];
            let level:isize=msg_send![window,level];
            let visible:Bool=msg_send![window,isVisible];
            if !native.as_bool() || level!=3 || !visible.as_bool() { continue; }
            let name:*mut AnyObject=msg_send![window,title];
            let text:*const c_char=msg_send![name,UTF8String];
            if text.is_null() || CStr::from_ptr(text).to_bytes()!=title.as_bytes() { continue; }
            let number:isize=msg_send![window,windowNumber];
            PIP_RETURN.with_borrow_mut(|binding|*binding=Some(PipReturn { window:number,label:label.into(),callback:callback.clone() }));
            let frame:NSRect=msg_send![window,frame];
            if PIP_STYLED.get()==number { PIP_FRAME.set(Some(frame));return true; }
            if frame.size.width<=0. || frame.size.height<=0. { continue; }
            let saved=PIP_FRAME.get();
            let width=saved.map_or(224.,|saved|saved.size.width);
            let size=NSSize::new(width,(width*frame.size.height/frame.size.width).round());
            let _:()=msg_send![window,setMinSize:size];
            let _:()=msg_send![window,setContentMinSize:size];
            // Preserve Chromium's chosen bottom/right corner and screen margin.
            let origin=saved.map_or_else(||NSPoint::new(frame.origin.x+frame.size.width-size.width,frame.origin.y),|saved|saved.origin);
            let bounds=NSRect::new(origin,size);
            let _:()=msg_send![window,setFrame:bounds display:true];
            let content:*mut AnyObject=msg_send![window,contentView];
            let _:()=msg_send![content,setWantsLayer:true];
            let layer:*mut AnyObject=msg_send![content,layer];
            if !layer.is_null() {
                let _:()=msg_send![layer,setCornerRadius:9f64];
                let _:()=msg_send![layer,setMasksToBounds:true];
            }
            PIP_STYLED.set(number);PIP_FRAME.set(Some(bounds));
            return true;
        }
    }
    false
}

// Only native event decoding belongs here; gesture and history policy are shared.
thread_local! {
 static SCROLL_ROOTS: std::cell::RefCell<std::collections::HashMap<i32,usize>>=std::cell::RefCell::default();
 static SCROLL_TARGETS: std::cell::RefCell<std::collections::HashMap<usize,i32>>=std::cell::RefCell::default();
 static SCROLL_ORIGINALS: std::cell::RefCell<std::collections::HashMap<usize,Imp>>=std::cell::RefCell::default();
}
pub(crate) fn register_page_scroll(id:i32,root:*mut c_void){SCROLL_ROOTS.with_borrow_mut(|all|{all.insert(id,root as usize);});}
pub(crate) fn forget_page_scroll(id:i32){cancel_pending_scroll(id);SCROLL_ROOTS.with_borrow_mut(|all|{all.remove(&id);});SCROLL_TARGETS.with_borrow_mut(|all|all.retain(|_,browser|*browser!=id));}
pub(crate) fn poll_page_scroll(){
 expire_pending_scroll();
 let roots:Vec<_>=SCROLL_ROOTS.with_borrow(|all|all.iter().map(|(id,root)|(*id,*root)).collect());
 for(id,root)in roots{if root!=0{unsafe{bind_scroll_descendants(root as *mut AnyObject,id);}}}
}
unsafe fn bind_scroll_descendants(view:*mut AnyObject,id:i32){unsafe{
 let class=(*view).class();let name=class.name().to_string_lossy();
 if name == "RenderWidgetHostViewCocoa"{
  let key=class as *const AnyClass as usize;
  if !SCROLL_ORIGINALS.with_borrow(|all|all.contains_key(&key)){
   if let Some(method)=class.instance_method(sel!(scrollWheel:)){
    SCROLL_ORIGINALS.with_borrow_mut(|all|{all.insert(key,method.implementation());});
    add_method(class as *const AnyClass as *mut AnyClass,sel!(scrollWheel:),std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject,Sel,*mut AnyObject),Imp>(page_scroll_event),"v@:@\0");
   }
  }
  SCROLL_TARGETS.with_borrow_mut(|all|{all.insert(view as usize,id);});return;
 }
 let children:*mut AnyObject=msg_send![view,subviews];let count:usize=msg_send![children,count];
 for index in 0..count{let child:*mut AnyObject=msg_send![children,objectAtIndex:index];bind_scroll_descendants(child,id);}
}}
extern "C-unwind" fn page_scroll_event(view:*mut AnyObject,sel:Sel,event:*mut AnyObject){unsafe{
 let id=SCROLL_TARGETS.with_borrow(|all|all.get(&(view as usize)).copied());
 if let Some(id)=id{
  let precise:Bool=msg_send![event,hasPreciseScrollingDeltas];
  let momentum:usize=msg_send![event,momentumPhase];let flags:usize=msg_send![event,modifierFlags];
  let phase:usize=msg_send![event,phase];
  // NSEventPhaseMayBegin has no movement; only explicit finger begin/end counts.
  let phase=if phase&16!=0{8}else if phase&8!=0{4}else if phase&1!=0{1}else if phase&4!=0{2}else{0};
  if precise.as_bool()&&momentum==0&&flags&((1<<17)|(1<<18)|(1<<19)|(1<<20))==0&&phase!=0{
   let point:NSPoint=msg_send![event,locationInWindow];let nil:*mut AnyObject=std::ptr::null_mut();
   let point:NSPoint=msg_send![view,convertPoint:point, fromView:nil];let flipped:Bool=msg_send![view,isFlipped];let bounds:NSRect=msg_send![view,bounds];
   let y=if flipped.as_bool(){point.y}else{bounds.size.height-point.y};
   let dx:f64=msg_send![event,scrollingDeltaX];let dy:f64=msg_send![event,scrollingDeltaY];
   if phase==4||phase==8 {
    if let Some(token)=crate::page_swipe::active_token(id){if queue_page_scroll(id,token,view,sel,event,phase,point.x,y,dx,dy){return;}}
    original_page_scroll(view,sel,event);crate::page_swipe::scroll(id,point.x,y,dx,dy,phase);return;
   }
   let eligible=crate::page_swipe::scroll(id,point.x,y,dx,dy,phase);
   if let Some(token)=crate::page_swipe::active_token(id){
    if (phase!=1||crate::page_swipe::awaiting_policy(id))&&queue_page_scroll(id,token,view,sel,event,phase,point.x,y,dx,dy){return;}
   }
   if eligible{return;}
  } else if phase!=0 {crate::page_swipe::cancel(id);}
 }
 original_page_scroll(view,sel,event);
}}
unsafe fn original_page_scroll(view:*mut AnyObject,sel:Sel,event:*mut AnyObject){unsafe{
 let class=(*view).class() as *const AnyClass as usize;
 if let Some(original)=SCROLL_ORIGINALS.with_borrow(|all|all.get(&class).copied().or_else(||all.values().next().copied())){
  let original:unsafe extern "C-unwind" fn(*mut AnyObject,Sel,*mut AnyObject)=std::mem::transmute(original);original(view,sel,event);
 }
}}

struct PendingScrollEvent {
 view:objc2::rc::Retained<AnyObject>, event:objc2::rc::Retained<AnyObject>, selector:Sel,
 phase:u8,x:f64,y:f64,dx:f64,dy:f64,
}
struct PendingScroll {token:i32,opened:std::time::Instant,events:Vec<PendingScrollEvent>}
thread_local!{static SCROLL_PENDING:std::cell::RefCell<std::collections::HashMap<i32,PendingScroll>>=std::cell::RefCell::default();}
pub(crate) fn cancel_pending_scroll(id:i32){let old=SCROLL_PENDING.with_borrow_mut(|all|all.remove(&id));drop(old);}
pub(crate) fn flush_pending_scroll(id:i32,token:i32){
 let pending=SCROLL_PENDING.with_borrow_mut(|all|if all.get(&id).is_some_and(|p|p.token==token){all.remove(&id)}else{None});
 let Some(pending)=pending else{return;};
 for packet in pending.events {
  if crate::page_swipe::active_token(id)!=Some(token){break;}
  if (packet.phase!=1&&packet.phase!=2)||!crate::page_swipe::eligible(id){unsafe{original_page_scroll(objc2::rc::Retained::as_ptr(&packet.view) as *mut AnyObject,packet.selector,objc2::rc::Retained::as_ptr(&packet.event) as *mut AnyObject);}}
  if packet.phase==4||packet.phase==8{crate::page_swipe::scroll(id,packet.x,packet.y,packet.dx,packet.dy,packet.phase);}
 }
}
unsafe fn queue_page_scroll(id:i32,token:i32,view:*mut AnyObject,selector:Sel,event:*mut AnyObject,phase:u8,x:f64,y:f64,dx:f64,dy:f64)->bool{unsafe{
 if phase==1{
  let old=SCROLL_PENDING.with_borrow_mut(|all|all.insert(id,PendingScroll{token,opened:std::time::Instant::now(),events:Vec::new()}));drop(old);
 }
 if !SCROLL_PENDING.with_borrow(|all|all.get(&id).is_some_and(|p|p.token==token)){return false;}
 let(Some(view),Some(event))=(objc2::rc::Retained::retain(view),objc2::rc::Retained::retain(event))else{return false;};
 let overflow=SCROLL_PENDING.with_borrow_mut(|all|{let pending=all.get_mut(&id).unwrap();pending.events.push(PendingScrollEvent{view,event,selector,phase,x,y,dx,dy});pending.events.len()>=512});
 if overflow{crate::page_swipe::deny(id,token);flush_pending_scroll(id,token);}
 true
}}
fn expire_pending_scroll(){
 let expired:Vec<_>=SCROLL_PENDING.with_borrow(|all|all.iter().filter(|(_,p)|p.opened.elapsed()>=std::time::Duration::from_millis(500)).map(|(id,p)|(*id,p.token)).collect());
 for(id,token)in expired{crate::page_swipe::deny(id,token);flush_pending_scroll(id,token);}
}
