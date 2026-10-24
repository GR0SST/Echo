//! Platform-owned service menus and delivery for copied links.
use std::ffi::c_void;

pub(crate) struct SharePicker {
    #[cfg(all(target_os = "macos", not(test)))]
    _menu: objc2::rc::Retained<objc2::runtime::AnyObject>,
    #[cfg(test)]
    pub(crate) urls: Vec<String>,
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe extern "C-unwind" fn perform(
    _: &objc2::runtime::AnyObject,
    _: objc2::runtime::Sel,
    sender: *mut objc2::runtime::AnyObject,
) {
    use objc2::{msg_send, runtime::AnyObject};
    unsafe {
        let pair: *mut AnyObject = msg_send![sender, representedObject];
        let service: *mut AnyObject = msg_send![pair, objectAtIndex: 0usize];
        let items: *mut AnyObject = msg_send![pair, objectAtIndex: 1usize];
        let _: () = msg_send![service, performWithItems: items];
    }
}

/// Called on the main thread outside any GPUI application/entity borrow:
/// NSMenu runs a nested AppKit tracking loop until selection or dismissal.
pub(crate) fn show(
    view: Option<*mut c_void>,
    anchor: [f32; 4],
    urls: &[String],
) -> Option<SharePicker> {
    #[cfg(test)]
    {
        let _ = (view, anchor);
        return Some(SharePicker {
            urls: urls.to_vec(),
        });
    }
    #[cfg(all(target_os = "macos", not(test)))]
    unsafe {
        use objc2::{
            msg_send,
            rc::Retained,
            runtime::{AnyClass, AnyObject, Bool, ClassBuilder},
            sel,
        };
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
        let view = view? as *mut AnyObject;
        let root: Retained<AnyObject> = Retained::retain(msg_send![view, window])?;
        let app: *mut AnyObject = msg_send![AnyClass::get(c"NSApplication")?, sharedApplication];
        let key: *mut AnyObject = msg_send![app, keyWindow];
        let parent: *mut AnyObject = msg_send![key, parentWindow];
        if parent == (&*root as *const AnyObject).cast_mut() {
            // A clicked child must not keep keyboard input after the menu.
            // This is outside the GPUI application borrow and precedes native
            // tracking, so selecting a service can freely activate its app.
            let _: () = msg_send![&*root, makeKeyWindow];
        }
        let array = AnyClass::get(c"NSMutableArray")?;
        let items: Retained<AnyObject> = Retained::from_raw(msg_send![array, new])?;
        let url_class = AnyClass::get(c"NSURL")?;
        // Let system service discovery supply availability, names and icons.
        for url in urls {
            let value = NSString::from_str(url);
            let url: *mut AnyObject = msg_send![url_class, URLWithString: &*value];
            if !url.is_null() {
                let _: () = msg_send![&*items, addObject: url];
            }
        }
        let service_class = AnyClass::get(c"NSSharingService")?;
        let services: *mut AnyObject = msg_send![service_class, sharingServicesForItems: &*items];
        #[link(name = "AppKit", kind = "framework")]
        unsafe extern "C" {
            static NSSharingServiceNameAddToSafariReadingList: *mut AnyObject;
        }
        let reading_list: *mut AnyObject = msg_send![service_class, sharingServiceNamed: NSSharingServiceNameAddToSafariReadingList];
        let reading_title: *mut AnyObject = msg_send![reading_list, title];
        let count: usize = msg_send![services, count];
        if count == 0 {
            return None;
        }
        let target_class = if let Some(class) = AnyClass::get(c"EchoShareServiceTarget") {
            class
        } else {
            let mut builder =
                ClassBuilder::new(c"EchoShareServiceTarget", AnyClass::get(c"NSObject")?)?;
            builder.add_method(
                sel!(performShare:),
                perform as unsafe extern "C-unwind" fn(_, _, _),
            );
            builder.register()
        };
        let target: Retained<AnyObject> = Retained::from_raw(msg_send![target_class, new])?;
        let menu: Retained<AnyObject> =
            Retained::from_raw(msg_send![AnyClass::get(c"NSMenu")?, new])?;
        let _: () = msg_send![&*menu, setAutoenablesItems: Bool::NO];
        for index in 0..count {
            let service: *mut AnyObject = msg_send![services, objectAtIndex: index];
            let title: *mut AnyObject = msg_send![service, title];
            if !reading_title.is_null() {
                let reading: Bool = msg_send![title, isEqualToString: reading_title];
                if reading.as_bool() {
                    continue;
                }
            }
            let item: *mut AnyObject = msg_send![AnyClass::get(c"NSMenuItem")?, alloc];
            let item: Retained<AnyObject> = Retained::from_raw(
                msg_send![item, initWithTitle: title action: sel!(performShare:) keyEquivalent: &*NSString::from_str("")],
            )?;
            let pair: Retained<AnyObject> = Retained::from_raw(msg_send![array, new])?;
            let _: () = msg_send![&*pair, addObject: service];
            let _: () = msg_send![&*pair, addObject: &*items];
            let _: () = msg_send![&*item, setRepresentedObject: &*pair];
            let _: () = msg_send![&*item, setTarget: &*target];
            let image: *mut AnyObject = msg_send![service, image];
            if !image.is_null() {
                let image: Retained<AnyObject> = Retained::from_raw(msg_send![image, copy])?;
                let _: () = msg_send![&*image, setSize: NSSize::new(16., 16.)];
                let _: () = msg_send![&*item, setImage: &*image];
                // macOS 27 hides images automatically for newly linked apps.
                let visibility: Bool =
                    msg_send![&*item, respondsToSelector: sel!(setPreferredImageVisibility:)];
                if visibility.as_bool() {
                    let _: () = msg_send![&*item, setPreferredImageVisibility: 1isize];
                }
            }
            let _: () = msg_send![&*menu, addItem: &*item];
        }
        let [x, mut y, _, height] = anchor.map(f64::from);
        let flipped: Bool = msg_send![view, isFlipped];
        if flipped.as_bool() {
            y += height;
        } else {
            let bounds: NSRect = msg_send![view, bounds];
            y = bounds.size.height - y - height;
        }
        let _: Bool = msg_send![&*menu, popUpMenuPositioningItem: std::ptr::null_mut::<AnyObject>() atLocation: NSPoint::new(x, y) inView: view];
        Some(SharePicker { _menu: menu })
    }
    #[cfg(all(not(target_os = "macos"), not(test)))]
    {
        let _ = (view, anchor, urls);
        None
    }
}
