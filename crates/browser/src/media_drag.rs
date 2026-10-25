//! Outgoing local file dragging. Native tracking begins outside GPUI borrows.
use crate::library::media::{MediaAction, MediaView};
use browser_core::media::MediaStamp;
use gpui::{div, prelude::*, App, Context, Render, Window};
struct Drag;
impl Render for Drag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
pub(crate) fn attach(
    tile: gpui::Stateful<gpui::Div>,
    stamp: MediaStamp,
    cx: &mut Context<MediaView>,
) -> gpui::Stateful<gpui::Div> {
    let owner = cx.weak_entity();
    tile.on_drag(stamp, move |stamp, _, window, cx| {
        #[cfg(test)]
        {
            let _ = owner.update(cx, |view, cx| {
                view.media_action(stamp.clone(), MediaAction::Drag, cx)
            });
        }
        #[cfg(not(test))]
        {
            let handle = window.window_handle();
            #[cfg(target_os = "macos")]
            let prepared = mac::prepare(crate::native::view_of(window), stamp.clone());
            #[cfg(target_os = "macos")]
            cx.spawn(async move |cx| {
                if let Some(prepared) = prepared {
                    if let Some(session) = prepared.start() {
                        while !session.done() && !crate::quit::is_quitting() {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(16))
                                .await;
                        }
                    }
                }
                let _ = handle.update(cx, |_, window, cx| {
                    cx.stop_active_drag(window);
                });
            })
            .detach();
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (handle, owner.clone());
            }
        }
        let _ = window;
        cx.new(|_| Drag)
    })
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use objc2::{
        class, msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
        sel,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex, OnceLock,
        },
    };
    fn registry() -> &'static Mutex<HashMap<usize, Arc<AtomicBool>>> {
        static MAP: OnceLock<Mutex<HashMap<usize, Arc<AtomicBool>>>> = OnceLock::new();
        MAP.get_or_init(Default::default)
    }
    unsafe extern "C-unwind" fn operation(
        _: &AnyObject,
        _: Sel,
        _: *mut AnyObject,
        _: isize,
    ) -> usize {
        1
    }
    unsafe extern "C-unwind" fn ended(
        this: &AnyObject,
        _: Sel,
        _: *mut AnyObject,
        _: NSPoint,
        _: usize,
    ) {
        if let Some(done) = registry().lock().unwrap().get(&(this as *const _ as usize)) {
            done.store(true, Ordering::Release);
        }
    }
    fn source_class() -> &'static AnyClass {
        static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
        CLASS.get_or_init(|| unsafe {
            let mut builder =
                ClassBuilder::new(c"EchoLibraryMediaDragSource", class!(NSObject)).unwrap();
            if let Some(protocol) = objc2::runtime::AnyProtocol::get(c"NSDraggingSource") {
                builder.add_protocol(protocol);
            }
            builder.add_method(
                sel!(draggingSession:sourceOperationMaskForDraggingContext:),
                operation as unsafe extern "C-unwind" fn(_, _, _, _) -> usize,
            );
            builder.add_method(
                sel!(draggingSession:endedAtPoint:operation:),
                ended as unsafe extern "C-unwind" fn(_, _, _, _, _),
            );
            builder.register()
        })
    }
    pub struct Prepared {
        view: Retained<AnyObject>,
        event: Retained<AnyObject>,
        stamp: MediaStamp,
    }
    pub fn prepare(view: Option<*mut std::ffi::c_void>, stamp: MediaStamp) -> Option<Prepared> {
        unsafe {
            if echo_media::validate(&stamp).is_err() {
                return None;
            }
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let event = Retained::retain(msg_send![app, currentEvent])?;
            Some(Prepared {
                view: Retained::retain(view?.cast())?,
                event,
                stamp,
            })
        }
    }
    pub struct Session {
        source: Retained<AnyObject>,
        _session: Retained<AnyObject>,
        done: Arc<AtomicBool>,
    }
    impl Session {
        pub fn done(&self) -> bool {
            self.done.load(Ordering::Acquire)
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            registry()
                .lock()
                .unwrap()
                .remove(&(Retained::as_ptr(&self.source) as usize));
        }
    }
    impl Prepared {
        pub fn start(self) -> Option<Session> {
            unsafe {
                echo_media::validate(&self.stamp).ok()?;
                let path = NSString::from_str(self.stamp.path.to_str()?);
                let url: *mut AnyObject = msg_send![class!(NSURL),fileURLWithPath:&*path];
                let item: *mut AnyObject = msg_send![class!(NSDraggingItem), alloc];
                let item: Retained<AnyObject> =
                    Retained::from_raw(msg_send![item,initWithPasteboardWriter:url])?;
                let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
                let image: *mut AnyObject = msg_send![workspace,iconForFile:&*path];
                let point: NSPoint = msg_send![&*self.event, locationInWindow];
                let point: NSPoint = msg_send![&*self.view,convertPoint:point,fromView:std::ptr::null_mut::<AnyObject>()];
                let _: () = msg_send![&*item,setDraggingFrame:NSRect::new(NSPoint::new(point.x-24.,point.y-24.),NSSize::new(48.,48.)),contents:image];
                let items: *mut AnyObject = msg_send![class!(NSArray),arrayWithObject:&*item];
                let source: Retained<AnyObject> = msg_send![source_class(), new];
                let done = Arc::new(AtomicBool::new(false));
                registry()
                    .lock()
                    .unwrap()
                    .insert(Retained::as_ptr(&source) as usize, done.clone());
                let session = Retained::retain(
                    msg_send![&*self.view,beginDraggingSessionWithItems:items,event:&*self.event,source:&*source],
                );
                if let Some(session) = session {
                    Some(Session {
                        source,
                        _session: session,
                        done,
                    })
                } else {
                    registry()
                        .lock()
                        .unwrap()
                        .remove(&(Retained::as_ptr(&source) as usize));
                    None
                }
            }
        }
    }
}
