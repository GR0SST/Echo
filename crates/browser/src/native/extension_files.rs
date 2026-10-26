//! Native file-panel options required by the reference's developer workflow.
use gpui::{App, Window};
use std::{future::Future, path::PathBuf, pin::Pin};

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Directory,
    PrivateKey,
}
pub(crate) fn choose(
    kind: Kind,
    window: &Window,
    cx: &mut App,
) -> Pin<Box<dyn Future<Output = Option<PathBuf>>>> {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use objc2::{msg_send, rc::Retained, runtime::AnyObject};
        let (sender, receiver) = futures::channel::oneshot::channel();
        let native = super::view_of(window).and_then(|view| unsafe {
            let native: *mut AnyObject = msg_send![view as *mut AnyObject, window];
            Retained::retain(native)
        });
        cx.foreground_executor()
            .spawn(async move {
                if let Some(parent) = native {
                    unsafe {
                        show(kind, parent, sender);
                    }
                } else {
                    let _ = sender.send(None);
                }
            })
            .detach();
        return Box::pin(async move { receiver.await.ok().flatten() });
    }
    #[cfg(test)]
    {
        let _ = (kind, window);
        let (sender, receiver) = futures::channel::oneshot::channel();
        assert!(
            !cx.has_global::<TestSelection>(),
            "A file chooser is already pending"
        );
        cx.set_global(TestSelection(sender));
        return Box::pin(async move { receiver.await.ok().flatten() });
    }
    #[cfg(all(not(target_os = "macos"), not(test)))]
    {
        let _ = window;
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            directories: matches!(kind, Kind::Directory),
            files: matches!(kind, Kind::PrivateKey),
            multiple: false,
            prompt: Some("Select".into()),
        });
        Box::pin(async move { chosen.await.ok()?.ok()??.into_iter().next() })
    }
}

#[cfg(all(target_os = "macos", not(test)))]
unsafe fn show(
    kind: Kind,
    parent: objc2::rc::Retained<objc2::runtime::AnyObject>,
    sender: futures::channel::oneshot::Sender<Option<PathBuf>>,
) {
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    use std::{cell::RefCell, ffi::CStr, os::unix::ffi::OsStrExt};
    let visible: Bool = unsafe { msg_send![&*parent, isVisible] };
    if !visible.as_bool() {
        let _ = sender.send(None);
        return;
    }
    let raw: *mut AnyObject = unsafe {
        msg_send![
            AnyClass::get(c"NSOpenPanel").expect("AppKit is loaded"),
            openPanel
        ]
    };
    let Some(panel) = (unsafe { Retained::retain(raw) }) else {
        let _ = sender.send(None);
        return;
    };
    let directory = matches!(kind, Kind::Directory);
    let message = NSString::from_str(if directory {
        "Select the extension directory."
    } else {
        "Select the private key file."
    });
    unsafe {
        let _: () = msg_send![&*panel, setCanChooseDirectories: Bool::new(directory)];
        let _: () = msg_send![&*panel, setCanChooseFiles: Bool::new(!directory)];
        let _: () = msg_send![&*panel, setAllowsMultipleSelection: Bool::NO];
        let _: () = msg_send![&*panel, setCanCreateDirectories: Bool::NO];
        let _: () = msg_send![&*panel, setResolvesAliases: Bool::NO];
        let _: () = msg_send![&*panel, setMessage: &*message];
        let _: () = msg_send![&*panel, setPrompt: &*NSString::from_str("Select")];
        if !directory {
            let types: *mut AnyObject = msg_send![AnyClass::get(c"NSArray").unwrap(),
                arrayWithObject: &*NSString::from_str("pem")];
            let _: () = msg_send![&*panel, setAllowedFileTypes: types];
        }
    }
    let sender = RefCell::new(Some(sender));
    let retained_panel = panel.clone();
    let sheet_parent = parent.clone();
    let completed = block2::RcBlock::new(move |response: isize| {
        let _ = &sheet_parent;
        let path = if response == 1 {
            unsafe {
                let url: *mut AnyObject = msg_send![&*retained_panel, URL];
                if url.is_null() {
                    None
                } else {
                    let file: Bool = msg_send![url, isFileURL];
                    let bytes: *const std::ffi::c_char = msg_send![url, fileSystemRepresentation];
                    if !file.as_bool() || bytes.is_null() {
                        None
                    } else {
                        Some(PathBuf::from(std::ffi::OsStr::from_bytes(
                            CStr::from_ptr(bytes).to_bytes(),
                        )))
                    }
                }
            }
        } else {
            None
        };
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(path);
        }
    });
    // The completion retains the native owner, so closing a GPUI entity cannot
    // leave this sheet with a dangling NSWindow during asynchronous dismissal.
    unsafe {
        let _: () =
            msg_send![&*panel, beginSheetModalForWindow: &*parent, completionHandler: &*completed];
    }
}

#[cfg(test)]
struct TestSelection(futures::channel::oneshot::Sender<Option<PathBuf>>);
#[cfg(test)]
impl gpui::Global for TestSelection {}
/// Supply the native chooser's asynchronous reply on GPUI's test platform,
/// whose stock existing-path chooser is unimplemented.
#[cfg(test)]
pub(crate) fn respond(cx: &mut App, path: Option<PathBuf>) {
    let selection = cx.remove_global::<TestSelection>();
    let _ = selection.0.send(path);
}
