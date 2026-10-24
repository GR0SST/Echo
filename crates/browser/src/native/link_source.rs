//! The sender is captured synchronously inside AppKit's open-URL callback.
//! Frontmost-application heuristics would mistake Echo itself for the sender.
#[cfg(all(target_os = "macos", not(test)))]
pub(crate) fn current() -> Option<String> {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSString;
    unsafe {
        let manager: *mut AnyObject = msg_send![
            AnyClass::get(c"NSAppleEventManager")?,
            sharedAppleEventManager
        ];
        let event: *mut AnyObject = msg_send![manager, currentAppleEvent];
        if event.is_null() {
            return None;
        }
        // AEDataModel.h: keySenderPIDAttr, a read-only typeSInt32 attribute.
        let sender: *mut AnyObject =
            msg_send![event, attributeDescriptorForKeyword: u32::from_be_bytes(*b"spid")];
        if sender.is_null() {
            return None;
        }
        let pid: i32 = msg_send![sender, int32Value];
        if pid <= 0 {
            return None;
        }
        let application: *mut AnyObject = msg_send![AnyClass::get(c"NSRunningApplication")?, runningApplicationWithProcessIdentifier: pid];
        if application.is_null() {
            return None;
        }
        let identifier: *mut NSString = msg_send![application, bundleIdentifier];
        identifier.as_ref().map(|identifier| identifier.to_string())
    }
}
#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn current() -> Option<String> {
    None
}
