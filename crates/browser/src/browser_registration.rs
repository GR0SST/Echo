//! Browser registration is an OS boundary; browser flows depend on this interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefaultState {
    Ours,
    Other,
    Unavailable,
}
pub(crate) trait BrowserRegistration: Send + Sync {
    fn state(&self) -> DefaultState;
    fn request(&self) -> Result<DefaultState, String>;
}
pub(crate) struct SystemRegistration;
impl BrowserRegistration for SystemRegistration {
    fn state(&self) -> DefaultState {
        platform::state()
    }
    fn request(&self) -> Result<DefaultState, String> {
        platform::request()
    }
}
#[cfg(all(target_os = "macos", not(test)))]
mod platform {
    use super::DefaultState;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSString;
    use std::ffi::c_void;
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultHandlerForURLScheme(scheme: *const c_void) -> *const c_void;
        fn LSRegisterURL(url: *const c_void, update: u8) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: *const c_void);
    }
    unsafe fn bundle() -> Option<(*mut AnyObject, *mut NSString)> {
        let bundle: *mut AnyObject = msg_send![AnyClass::get(c"NSBundle")?, mainBundle];
        if bundle.is_null() {
            return None;
        }
        let id: *mut NSString = msg_send![bundle, bundleIdentifier];
        if id.is_null() {
            return None;
        }
        Some((bundle, id))
    }
    pub(super) fn state() -> DefaultState {
        unsafe {
            let Some((_, id)) = bundle() else {
                return DefaultState::Unavailable;
            };
            for scheme in ["http", "https"] {
                let scheme = NSString::from_str(scheme);
                let handler =
                    LSCopyDefaultHandlerForURLScheme((&*scheme as *const NSString).cast());
                if handler.is_null() {
                    return DefaultState::Other;
                }
                let same = (&*handler.cast::<NSString>()).to_string() == (&*id).to_string();
                CFRelease(handler);
                if !same {
                    return DefaultState::Other;
                }
            }
            DefaultState::Ours
        }
    }
    pub(super) fn request() -> Result<DefaultState, String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let Some((bundle, _)) = bundle() else {
                return Err(
                    "Open Echo from its application bundle to set it as your default browser."
                        .into(),
                );
            };
            let url: *mut AnyObject = msg_send![bundle, bundleURL];
            if url.is_null() {
                return Err("The Echo application could not be registered.".into());
            }
            let registered = LSRegisterURL(url.cast(), 1);
            if registered != 0 {
                return Err(format!(
                    "The Echo application could not be registered ({registered})."
                ));
            }
            let workspace: *mut AnyObject = msg_send![
                AnyClass::get(c"NSWorkspace").ok_or("Default browser settings are unavailable.")?,
                sharedWorkspace
            ];
            // The system owns consent. Do not send the HTTPS request after an
            // HTTP cancellation: that would display a second permission prompt.
            for scheme in ["http", "https"] {
                if state() == DefaultState::Ours {
                    break;
                }
                let (send, receive) = std::sync::mpsc::sync_channel(1);
                let callback = block2::RcBlock::new(move |error: *mut AnyObject| {
                    let result = if error.is_null() {
                        Ok(())
                    } else {
                        let description: *mut NSString = msg_send![error, localizedDescription];
                        Err(description
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                "The default browser change was not confirmed.".into()
                            }))
                    };
                    let _ = send.send(result);
                });
                let scheme = NSString::from_str(scheme);
                let _: () = msg_send![workspace,setDefaultApplicationAtURL:url, toOpenURLsWithScheme:&*scheme, completionHandler:&*callback];
                receive
                    .recv_timeout(std::time::Duration::from_secs(120))
                    .map_err(|_| "The default browser change was not confirmed.".to_owned())??;
            }
            match state() {
                DefaultState::Ours => Ok(DefaultState::Ours),
                _ => Err("The default browser change was not confirmed.".into()),
            }
        })
    }
}
#[cfg(any(not(target_os = "macos"), test))]
mod platform {
    use super::DefaultState;
    pub(super) fn state() -> DefaultState {
        DefaultState::Unavailable
    }
    pub(super) fn request() -> Result<DefaultState, String> {
        Err("Choose Echo in your system’s default apps settings.".into())
    }
}
