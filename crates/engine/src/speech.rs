//! Local platform speech for the inspected page-selection context menu.
pub fn available() -> bool {
    cfg!(target_os = "macos")
}

pub fn is_speaking() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::is_speaking()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Call on the UI thread outside application/window/engine borrows.
pub fn speak(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        mac::speak(text)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Stop only this application's speech synthesizer.
pub fn stop() {
    #[cfg(target_os = "macos")]
    mac::stop();
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    use std::cell::RefCell;

    thread_local! {
        static SYNTH: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    pub(super) fn is_speaking() -> bool {
        SYNTH.with_borrow(|value| {
            value.as_ref().is_some_and(|value| unsafe {
                let speaking: Bool = msg_send![&**value, isSpeaking];
                speaking.as_bool()
            })
        })
    }
    pub(super) fn speak(text: &str) -> bool {
        // Clone the native owner before dispatch: speech can process AppKit
        // callbacks, so no thread-local borrow crosses the native operation.
        let synth = SYNTH.with_borrow_mut(|value| {
            if value.is_none() {
                let class = AnyClass::get(c"NSSpeechSynthesizer")?;
                let raw: *mut AnyObject = unsafe { msg_send![class, alloc] };
                let raw: *mut AnyObject =
                    unsafe { msg_send![raw, initWithVoice: std::ptr::null::<AnyObject>()] };
                *value = unsafe { Retained::from_raw(raw) };
            }
            value.clone()
        });
        synth.is_some_and(|synth| unsafe {
            let _: () = msg_send![&*synth, stopSpeaking];
            let started: Bool = msg_send![&*synth, startSpeakingString: &*NSString::from_str(text)];
            started.as_bool()
        })
    }
    pub(super) fn stop() {
        let synth = SYNTH.with_borrow(|value| value.clone());
        if let Some(synth) = synth {
            unsafe {
                let _: () = msg_send![&*synth, stopSpeaking];
            }
        }
    }
}
