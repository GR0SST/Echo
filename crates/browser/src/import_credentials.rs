//! Imported passwords share the login/autofill vault, never session persistence.
use importer::{Credential, CredentialStore};
pub(crate) struct SystemCredentialStore;
impl CredentialStore for SystemCredentialStore {
    fn store(&self, profile: &str, credential: &Credential) -> Result<(), String> {
        vault(profile)?.upsert(credential, chrono::Utc::now().timestamp())
    }
}
pub(crate) fn vault(profile: &str) -> Result<crate::password_store::Vault, String> {
    let vault = crate::password_store::Vault::open(&crate::profile_dir(), profile)?;
    vault.migrate_if_absent(|| platform::read(profile))?;
    Ok(vault)
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
        static kSecAttrSynchronizable: *const c_void;
        static kSecReturnAttributes: *const c_void;
        static kSecReturnData: *const c_void;
        static kSecMatchLimit: *const c_void;
        static kSecMatchLimitAll: *const c_void;
        static kSecMatchLimitOne: *const c_void;
        fn SecItemCopyMatching(query: *const c_void, result: *mut *const c_void) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFBooleanFalse: *const c_void;
        static kCFBooleanTrue: *const c_void;
        fn CFRelease(value: *const c_void);
    }
    pub(super) fn read(profile: &str) -> Result<Vec<Credential>, String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let class = AnyClass::get(c"NSMutableDictionary")
                .ok_or("The local keychain is unavailable.")?;
            let query: *mut AnyObject = msg_send![class, dictionary];
            let service = NSString::from_str(&format!(
                "dev.echo.browser.passwords:{}",
                crate::profile_dir().display()
            ));
            let _: () = msg_send![query,setObject:kSecClassGenericPassword.cast::<AnyObject>(), forKey:kSecClass.cast::<AnyObject>()];
            let _: () =
                msg_send![query,setObject:&*service, forKey:kSecAttrService.cast::<AnyObject>()];
            let _: () = msg_send![query,setObject:kCFBooleanFalse.cast::<AnyObject>(), forKey:kSecAttrSynchronizable.cast::<AnyObject>()];
            let _: () = msg_send![query,setObject:kCFBooleanTrue.cast::<AnyObject>(), forKey:kSecReturnAttributes.cast::<AnyObject>()];
            // macOS rejects ReturnData + MatchLimitAll. Enumerate attributes,
            // filter the immutable profile key, then read one exact account.
            let _: () = msg_send![query,setObject:kSecMatchLimitAll.cast::<AnyObject>(), forKey:kSecMatchLimit.cast::<AnyObject>()];
            let mut result = std::ptr::null();
            let status = SecItemCopyMatching(query.cast(), &mut result);
            if status == -25300 {
                return Ok(Vec::new());
            }
            if status != 0 || result.is_null() {
                return Err(format!(
                    "The previous Echo password store could not be read ({status})."
                ));
            }
            struct Release(*const c_void);
            impl Drop for Release {
                fn drop(&mut self) {
                    unsafe { CFRelease(self.0) };
                }
            }
            let _release = Release(result);
            let array = result.cast::<AnyObject>();
            let count: usize = msg_send![array, count];
            if count > 10_000 {
                return Err("The previous Echo password store is too large.".into());
            }
            let mut rows = Vec::new();
            for index in 0..count {
                let item: *mut AnyObject = msg_send![array,objectAtIndex:index];
                let account: *mut AnyObject =
                    msg_send![item,objectForKey:kSecAttrAccount.cast::<AnyObject>()];
                if account.is_null() {
                    return Err("An Echo password entry is invalid.".into());
                }
                let utf8: *const std::ffi::c_char = msg_send![account, UTF8String];
                if utf8.is_null() {
                    return Err("An Echo password entry is invalid.".into());
                }
                let (owner, origin, username): (String, String, String) =
                    serde_json::from_slice(std::ffi::CStr::from_ptr(utf8).to_bytes())
                        .map_err(|_| "An Echo password entry is invalid.")?;
                if owner != profile {
                    continue;
                }
                let single: *mut AnyObject = msg_send![class,dictionaryWithDictionary:query];
                let _: () =
                    msg_send![single,removeObjectForKey:kSecReturnAttributes.cast::<AnyObject>()];
                let _: () = msg_send![single,setObject:kSecMatchLimitOne.cast::<AnyObject>(), forKey:kSecMatchLimit.cast::<AnyObject>()];
                let _: () = msg_send![single,setObject:kCFBooleanTrue.cast::<AnyObject>(), forKey:kSecReturnData.cast::<AnyObject>()];
                let _: () =
                    msg_send![single,setObject:account, forKey:kSecAttrAccount.cast::<AnyObject>()];
                let mut secret = std::ptr::null();
                let status = SecItemCopyMatching(single.cast(), &mut secret);
                if status != 0 || secret.is_null() {
                    return Err(format!(
                        "An Echo password entry could not be read ({status})."
                    ));
                }
                let _secret = Release(secret);
                let data = secret.cast::<AnyObject>();
                let size: usize = msg_send![data, length];
                if size == 0 || size > 1024 * 1024 {
                    return Err("An Echo password entry is invalid.".into());
                }
                let bytes: *const c_void = msg_send![data, bytes];
                if bytes.is_null() {
                    return Err("An Echo password entry is invalid.".into());
                }
                let password = std::str::from_utf8(std::slice::from_raw_parts(bytes.cast::<u8>(), size))
                    .map_err(|_| "An Echo password entry is invalid.")?
                    .to_owned();
                rows.push(Credential {
                    origin,
                    username,
                    password,
                });
            }
            Ok(rows)
        })
    }
}

#[cfg(any(not(target_os = "macos"), test))]
mod platform {
    pub(super) fn read(_profile: &str) -> Result<Vec<importer::Credential>, String> {
        Ok(Vec::new())
    }
}
