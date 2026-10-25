//! Plain native save chooser. Preparation only retains the parent; AppKit
//! presentation and completion run after all application/model borrows end.
use gpui::{App, AsyncApp};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};
type Reply = Pin<Box<dyn Future<Output = Result<Option<PathBuf>, String>>>>;
pub(crate) enum Prepared {
    #[cfg(target_os = "macos")]
    Native(mac::Prepared),
    Toolkit(Reply),
}
pub(crate) fn prepare(
    parent: Option<*mut std::ffi::c_void>,
    directory: &Path,
    name: &str,
    cx: &mut App,
) -> Prepared {
    #[cfg(target_os = "macos")]
    if let Some(prepared) = mac::prepare(parent, directory, name) {
        return Prepared::Native(prepared);
    }
    let _ = parent;
    let reply = cx.prompt_for_new_path(directory, Some(name));
    Prepared::Toolkit(Box::pin(async move {
        reply
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }))
}
fn available_name(directory: &Path, suggested: &str) -> String {
    let Some(plan) = crate::ordinary_downloads::plan(directory.to_owned(), suggested) else {
        return suggested.into();
    };
    for leaf in plan.candidates {
        match directory.join(&leaf).try_exists() {
            Ok(false) => return leaf.to_string_lossy().into_owned(),
            Ok(true) => (),
            Err(_) => break,
        }
    }
    suggested.into()
}
impl Prepared {
    pub(crate) async fn choose(self, cx: &mut AsyncApp) -> Result<Option<PathBuf>, String> {
        match self {
            Self::Toolkit(reply) => reply.await,
            #[cfg(target_os = "macos")]
            Self::Native(request) => {
                let Some(mut dialog) = request.start()? else {
                    return Ok(None);
                };
                loop {
                    if crate::quit::is_quitting() {
                        return Ok(None);
                    }
                    if let Some(reply) = dialog.result.borrow_mut().take() {
                        dialog.done = true;
                        return reply;
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(16))
                        .await;
                }
            }
        }
    }
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    use std::{
        cell::RefCell,
        ffi::{c_void, CStr},
        rc::Rc,
    };
    pub(crate) struct Prepared {
        parent: Retained<AnyObject>,
        directory: PathBuf,
        name: String,
    }
    pub(super) struct Dialog {
        panel: Retained<AnyObject>,
        parent: Retained<AnyObject>,
        pub(super) result: Rc<RefCell<Option<Result<Option<PathBuf>, String>>>>,
        pub(super) done: bool,
    }
    pub(super) fn prepare(
        view: Option<*mut c_void>,
        directory: &Path,
        name: &str,
    ) -> Option<Prepared> {
        if cfg!(test) {
            return None;
        }
        let view = view? as *mut AnyObject;
        unsafe {
            Some(Prepared {
                parent: Retained::retain(msg_send![view, window])?,
                directory: directory.to_owned(),
                name: name.into(),
            })
        }
    }
    impl Prepared {
        pub(super) fn start(self) -> Result<Option<Dialog>, String> {
            unsafe {
                let visible: Bool = msg_send![&*self.parent, isVisible];
                let key: Bool = msg_send![&*self.parent, isKeyWindow];
                if !visible.as_bool() || !key.as_bool() {
                    return Ok(None);
                }
                let panel: Retained<AnyObject> =
                    Retained::retain(msg_send![objc2::class!(NSSavePanel), savePanel])
                        .ok_or("Save chooser is unavailable")?;
                let _: () = msg_send![&*panel,setCanCreateDirectories:Bool::YES];
                let _: () = msg_send![&*panel,setExtensionHidden:Bool::NO];
                let _: () = msg_send![&*panel,setCanSelectHiddenExtension:Bool::YES];
                let _: () = msg_send![&*panel,setNameFieldStringValue:&*NSString::from_str(&available_name(&self.directory, &self.name))];
                let directory = NSString::from_str(&self.directory.to_string_lossy());
                let url: *mut AnyObject = msg_send![objc2::class!(NSURL),fileURLWithPath:&*directory,isDirectory:Bool::YES];
                if !url.is_null() {
                    let _: () = msg_send![&*panel,setDirectoryURL:url];
                }
                let result = Rc::new(RefCell::new(None));
                let output = result.clone();
                let retained = panel.clone();
                let completion = block2::RcBlock::new(move |response: isize| {
                    let selected = if response == 1 {
                        let url: *mut AnyObject = msg_send![&*retained, URL];
                        let path: *const std::ffi::c_char = if url.is_null() {
                            std::ptr::null()
                        } else {
                            msg_send![url, fileSystemRepresentation]
                        };
                        if path.is_null() {
                            Err("Save path is unavailable".into())
                        } else {
                            use std::os::unix::ffi::OsStrExt;
                            Ok(Some(PathBuf::from(std::ffi::OsStr::from_bytes(
                                CStr::from_ptr(path).to_bytes(),
                            ))))
                        }
                    } else {
                        Ok(None)
                    };
                    *output.borrow_mut() = Some(selected);
                });
                let _: () = msg_send![&*panel,beginSheetModalForWindow:&*self.parent,completionHandler:&*completion];
                Ok(Some(Dialog {
                    panel,
                    parent: self.parent,
                    result,
                    done: false,
                }))
            }
        }
    }
    impl Drop for Dialog {
        fn drop(&mut self) {
            if !self.done {
                unsafe {
                    let _: () = msg_send![&*self.panel,cancel:std::ptr::null::<AnyObject>()];
                    let _: () = msg_send![&*self.panel,orderOut:std::ptr::null::<AnyObject>()];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chooser_proposes_the_next_free_name_without_modifying_existing_files() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("file.tar.gz"), b"first").unwrap();
        std::fs::write(folder.path().join("file.tar (1).gz"), b"second").unwrap();
        assert_eq!(
            available_name(folder.path(), "file.tar.gz"),
            "file.tar (2).gz"
        );
        assert_eq!(
            std::fs::read(folder.path().join("file.tar.gz")).unwrap(),
            b"first"
        );
        assert!(!folder.path().join("file.tar (2).gz").exists());
    }
}
