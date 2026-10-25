//! User-initiated native folder approval. Preparing a panel never scans files.
use browser_core::media::MediaFolder;
use futures::channel::oneshot;
use gpui::{App, AsyncApp};
use std::path::PathBuf;
pub(crate) enum Prepared {
    #[cfg(target_os = "macos")]
    Native(mac::Prepared),
    Reply(oneshot::Receiver<Result<Option<PathBuf>, String>>),
}
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestPicker {
    pub pending: Vec<(
        MediaFolder,
        oneshot::Sender<Result<Option<PathBuf>, String>>,
    )>,
}
#[cfg(test)]
impl gpui::Global for TestPicker {}
pub(crate) fn prepare(
    parent: Option<*mut std::ffi::c_void>,
    folder: MediaFolder,
    cx: &mut App,
) -> Prepared {
    #[cfg(test)]
    {
        let (send, reply) = oneshot::channel();
        cx.default_global::<TestPicker>()
            .pending
            .push((folder, send));
        let _ = parent;
        return Prepared::Reply(reply);
    }
    #[cfg(not(test))]
    {
        #[cfg(target_os = "macos")]
        if let Some(native) = mac::prepare(parent, folder) {
            return Prepared::Native(native);
        }
        let _ = parent;
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(format!("Allow {}", folder.label()).into()),
        });
        let (send, reply) = oneshot::channel();
        cx.spawn(async move |_| {
            let value = chosen
                .await
                .map_err(|_| "Folder panel closed".to_string())
                .and_then(|v| v.map_err(|e| e.to_string()))
                .map(|paths| paths.and_then(|paths| paths.into_iter().next()));
            let _ = send.send(value);
        })
        .detach();
        Prepared::Reply(reply)
    }
}
impl Prepared {
    pub(crate) async fn choose(self, cx: &mut AsyncApp) -> Result<Option<PathBuf>, String> {
        match self {
            Self::Reply(reply) => reply.await.map_err(|_| "Folder panel closed".to_string())?,
            #[cfg(target_os = "macos")]
            Self::Native(prepared) => prepared.choose(cx).await,
        }
    }
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::{
        class, msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    pub(crate) struct Prepared {
        parent: Retained<AnyObject>,
        folder: MediaFolder,
    }
    pub(crate) fn prepare(
        view: Option<*mut std::ffi::c_void>,
        folder: MediaFolder,
    ) -> Option<Prepared> {
        if cfg!(test) {
            return None;
        }
        unsafe {
            let view = view? as *mut AnyObject;
            Some(Prepared {
                parent: Retained::retain(msg_send![view, window])?,
                folder,
            })
        }
    }
    impl Prepared {
        pub(crate) async fn choose(self, cx: &mut AsyncApp) -> Result<Option<PathBuf>, String> {
            let (send, mut receive) = oneshot::channel();
            let panel = unsafe {
                let visible: Bool = msg_send![&*self.parent, isVisible];
                if !visible.as_bool() {
                    return Ok(None);
                }
                let panel: Retained<AnyObject> =
                    Retained::retain(msg_send![class!(NSOpenPanel), openPanel])
                        .ok_or("Folder panel unavailable")?;
                let _: () = msg_send![&*panel,setCanChooseDirectories:Bool::YES];
                let _: () = msg_send![&*panel,setCanChooseFiles:Bool::NO];
                let _: () = msg_send![&*panel,setAllowsMultipleSelection:Bool::NO];
                let _: () = msg_send![&*panel,setCanCreateDirectories:Bool::NO];
                let _: () = msg_send![&*panel,setResolvesAliases:Bool::YES];
                let _: () = msg_send![&*panel,setMessage:&*NSString::from_str(&format!("Allow Echo to show your {} folder in Media. Media stays on your computer.",self.folder.label()))];
                let _: () = msg_send![&*panel,setPrompt:&*NSString::from_str("Allow")];
                let directory = match self.folder {
                    MediaFolder::Documents => dirs::document_dir(),
                    MediaFolder::Desktop => dirs::desktop_dir(),
                    MediaFolder::Downloads => dirs::download_dir(),
                };
                if let Some(directory) = directory {
                    let url: *mut AnyObject = msg_send![class!(NSURL),fileURLWithPath:&*NSString::from_str(&directory.to_string_lossy())];
                    let _: () = msg_send![&*panel,setDirectoryURL:url];
                }
                let sender = std::cell::RefCell::new(Some(send));
                let raw = Retained::as_ptr(&panel) as usize;
                let block = RcBlock::new(move |response: isize| {
                    let path = if response == 1 {
                        let panel = raw as *mut AnyObject;
                        let url: *mut AnyObject = msg_send![panel, URL];
                        if url.is_null() {
                            None
                        } else {
                            let path: *mut NSString = msg_send![url, path];
                            (!path.is_null()).then(|| PathBuf::from((&*path).to_string()))
                        }
                    } else {
                        None
                    };
                    if let Some(send) = sender.borrow_mut().take() {
                        let _ = send.send(Ok(path));
                    }
                });
                let _: () = msg_send![&*panel,beginSheetModalForWindow:&*self.parent,completionHandler:&*block];
                panel
            };
            loop {
                if let Some(result) = receive
                    .try_recv()
                    .map_err(|_| "Folder panel closed".to_string())?
                {
                    return result;
                }
                let visible: Bool = unsafe { msg_send![&*self.parent, isVisible] };
                if !visible.as_bool() || crate::quit::is_quitting() {
                    unsafe {
                        let _: () = msg_send![&*panel,cancel:std::ptr::null_mut::<AnyObject>()];
                    }
                    return Ok(None);
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
            }
        }
    }
}
