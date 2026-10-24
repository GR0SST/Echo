//! Imported passwords use the OS credential store, never session persistence.
use importer::{Credential, CredentialStore};
pub(crate) struct SystemCredentialStore;
impl CredentialStore for SystemCredentialStore {
    fn store(&self, profile: &str, credential: &Credential) -> Result<(), String> {
        platform::store(profile, credential)
    }
}
#[cfg(all(target_os = "macos", not(test)))]
mod platform {
    use importer::Credential;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSString;
    use std::ffi::c_void;
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecClass: *const c_void;
        static kSecClassGenericPassword: *const c_void;
        static kSecAttrService: *const c_void;
        static kSecAttrAccount: *const c_void;
        static kSecValueData: *const c_void;
        static kSecAttrSynchronizable: *const c_void;
        static kSecAttrAccessible: *const c_void;
        static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: *const c_void;
        fn SecItemAdd(query: *const c_void, result: *mut *const c_void) -> i32;
        fn SecItemUpdate(query: *const c_void, attributes: *const c_void) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFBooleanFalse: *const c_void;
    }
    pub(super) fn store(profile: &str, c: &Credential) -> Result<(), String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let class = AnyClass::get(c"NSMutableDictionary")
                .ok_or("The local keychain is unavailable.")?;
            let query: *mut AnyObject = msg_send![class, dictionary];
            let service = NSString::from_str(&format!(
                "dev.echo.browser.passwords:{}",
                crate::profile_dir().display()
            ));
            let account = NSString::from_str(
                &serde_json::to_string(&(profile, &c.origin, &c.username))
                    .map_err(|_| "Cannot identify the password entry.")?,
            );
            let _: () = msg_send![query,setObject:kSecClassGenericPassword.cast::<AnyObject>(), forKey:kSecClass.cast::<AnyObject>()];
            let _: () =
                msg_send![query,setObject:&*service, forKey:kSecAttrService.cast::<AnyObject>()];
            let _: () =
                msg_send![query,setObject:&*account, forKey:kSecAttrAccount.cast::<AnyObject>()];
            let _: () = msg_send![query,setObject:kCFBooleanFalse.cast::<AnyObject>(), forKey:kSecAttrSynchronizable.cast::<AnyObject>()];
            let update: *mut AnyObject = msg_send![class, dictionary];
            let bytes = c.password.as_bytes();
            let data: *mut AnyObject = msg_send![AnyClass::get(c"NSData").ok_or("The local keychain is unavailable.")?,dataWithBytes:bytes.as_ptr().cast::<c_void>(), length:bytes.len()];
            let _: () = msg_send![update,setObject:data, forKey:kSecValueData.cast::<AnyObject>()];
            let status = SecItemUpdate(query.cast(), update.cast());
            let status = if status == -25300 {
                let _: () =
                    msg_send![query,setObject:data, forKey:kSecValueData.cast::<AnyObject>()];
                let _: () = msg_send![query,setObject:kSecAttrAccessibleWhenUnlockedThisDeviceOnly.cast::<AnyObject>(), forKey:kSecAttrAccessible.cast::<AnyObject>()];
                SecItemAdd(query.cast(), std::ptr::null_mut())
            } else {
                status
            };
            if status == 0 {
                Ok(())
            } else {
                Err(format!(
                    "The password could not be saved in the local keychain ({status})."
                ))
            }
        })
    }
}
#[cfg(any(not(target_os = "macos"), test))]
mod platform {
    pub(super) fn store(_profile: &str, _credential: &importer::Credential) -> Result<(), String> {
        Err("The local password store is unavailable on this platform.".into())
    }
}
