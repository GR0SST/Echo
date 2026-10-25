//! The OS save panel is a platform boundary; export logic sees only a path and
//! the three reference formats. Native dispatch runs outside application borrows.
use browser_core::save_page::Format;
use gpui::{App, AsyncApp};
use std::{future::Future, path::PathBuf, pin::Pin};

#[derive(Debug)]
pub(crate) struct Choice {
    pub path: PathBuf,
    pub format: Format,
}
type ToolkitReply = Pin<Box<dyn Future<Output = Result<Option<Choice>, String>>>>;
pub(crate) enum Prepared {
    #[cfg(target_os = "macos")]
    Native(mac::Prepared),
    Toolkit(ToolkitReply),
}
pub(crate) fn prepare(parent: Option<*mut std::ffi::c_void>, name: &str, cx: &mut App) -> Prepared {
    let directory = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    if let Some(request) = mac::prepare(parent, name, directory.clone()) {
        return Prepared::Native(request);
    }
    let _ = parent;
    let reply = cx.prompt_for_new_path(&directory, Some(name));
    Prepared::Toolkit(Box::pin(async move {
        let chosen = reply
            .await
            .map_err(|_| "The save panel was closed.".to_owned())?
            .map_err(|error| error.to_string())?;
        Ok(chosen.map(|path| Choice {
            path,
            format: Format::Complete,
        }))
    }))
}
impl Prepared {
    pub(crate) async fn choose(self, cx: &mut AsyncApp) -> Result<Option<Choice>, String> {
        match self {
            Self::Toolkit(reply) => reply.await,
            #[cfg(target_os = "macos")]
            Self::Native(prepared) => {
                let Some(mut dialog) = prepared.start()? else {
                    return Ok(None);
                };
                loop {
                    if let Some(result) = dialog.poll() {
                        return result;
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
        runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
        sel,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use std::{
        cell::RefCell,
        ffi::{c_void, CStr},
        rc::Rc,
    };
    pub(crate) struct Prepared {
        parent: Retained<AnyObject>,
        name: String,
        directory: PathBuf,
    }
    struct Controls {
        panel: Retained<AnyObject>,
        _parent: Retained<AnyObject>,
        popup: Retained<AnyObject>,
        _target: Retained<AnyObject>,
    }
    pub(crate) struct Dialog {
        controls: Rc<Controls>,
        result: Rc<RefCell<Option<Result<Option<Choice>, String>>>>,
        done: bool,
    }
    pub(crate) fn prepare(
        view: Option<*mut c_void>,
        name: &str,
        directory: PathBuf,
    ) -> Option<Prepared> {
        if cfg!(test) {
            return None;
        }
        let view = view? as *mut AnyObject;
        unsafe {
            Some(Prepared {
                parent: Retained::retain(msg_send![view, window])?,
                name: name.into(),
                directory,
            })
        }
    }
    unsafe fn allowed(panel: &AnyObject, format: Format) {
        let extension = NSString::from_str(format.extension());
        let array: *mut AnyObject = msg_send![objc2::class!(NSArray),arrayWithObject:&*extension];
        let _: () = msg_send![panel,setAllowedFileTypes:array];
    }
    unsafe extern "C-unwind" fn changed(_target: &AnyObject, _selector: Sel, sender: &AnyObject) {
        let index: isize = msg_send![sender, indexOfSelectedItem];
        let format = Format::ALL.get(index as usize).copied().unwrap_or_default();
        let item: *mut AnyObject = msg_send![sender, selectedItem];
        let value: *mut AnyObject = msg_send![item, representedObject];
        let panel: *mut AnyObject = msg_send![value, nonretainedObjectValue];
        if panel.is_null() {
            return;
        }
        allowed(&*panel, format);
        let name: *mut NSString = msg_send![panel, nameFieldStringValue];
        if !name.is_null() {
            let mut path = PathBuf::from((&*name).to_string());
            path.set_extension(format.extension());
            let value = NSString::from_str(&path.to_string_lossy());
            let _: () = msg_send![panel,setNameFieldStringValue:&*value];
        }
    }
    impl Prepared {
        pub(crate) fn start(self) -> Result<Option<Dialog>, String> {
            unsafe {
                let visible: Bool = msg_send![&*self.parent, isVisible];
                let key: Bool = msg_send![&*self.parent, isKeyWindow];
                if std::env::var_os("BROWSER_SCRIPT").is_some() {
                    eprintln!(
                        "[save-dialog] parent visible={} key={}",
                        visible.as_bool(),
                        key.as_bool()
                    );
                }
                if !visible.as_bool() || !key.as_bool() {
                    return Ok(None);
                }
                let panel: Retained<AnyObject> =
                    Retained::retain(msg_send![objc2::class!(NSSavePanel), savePanel])
                        .ok_or("The save panel is unavailable.")?;
                let title = NSString::from_str("Save");
                let _: () = msg_send![&*panel,setTitle:&*title];
                let _: () = msg_send![&*panel,setCanCreateDirectories:Bool::YES];
                let _: () = msg_send![&*panel,setExtensionHidden:Bool::NO];
                let _: () = msg_send![&*panel,setCanSelectHiddenExtension:Bool::YES];
                allowed(&panel, Format::Complete);
                let name = NSString::from_str(&self.name);
                let _: () = msg_send![&*panel,setNameFieldStringValue:&*name];
                let directory = NSString::from_str(&self.directory.to_string_lossy());
                let url: *mut AnyObject = msg_send![objc2::class!(NSURL),fileURLWithPath:&*directory isDirectory:Bool::YES];
                if !url.is_null() {
                    let _: () = msg_send![&*panel,setDirectoryURL:url];
                }
                let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(236., 28.));
                let view: *mut AnyObject = msg_send![objc2::class!(NSView), alloc];
                let accessory: Retained<AnyObject> =
                    Retained::from_raw(msg_send![view,initWithFrame:frame])
                        .ok_or("The save format control is unavailable.")?;
                let label: *mut AnyObject = msg_send![objc2::class!(NSTextField),labelWithString:&*NSString::from_str("Format:")];
                let _: () = msg_send![label,setAlignment:2isize];
                let font: *mut AnyObject = msg_send![objc2::class!(NSFont),systemFontOfSize:13f64];
                let _: () = msg_send![label,setFont:font];
                let _: () = msg_send![label, sizeToFit];
                let measured: NSRect = msg_send![label, frame];
                let label_width = measured.size.width;
                let _: () = msg_send![label,setFrame:NSRect::new(NSPoint::new(0.,5.),NSSize::new(label_width,17.))];
                let ink: *mut AnyObject = msg_send![objc2::class!(NSColor), secondaryLabelColor];
                let _: () = msg_send![label,setTextColor:ink];
                let _: () =
                    msg_send![&*accessory,setFrameSize:NSSize::new(label_width+8.+182.,28.)];
                let _: () = msg_send![&*accessory,addSubview:label];
                let popup: *mut AnyObject = msg_send![objc2::class!(NSPopUpButton), alloc];
                let popup:Retained<AnyObject>=Retained::from_raw(msg_send![popup,initWithFrame:NSRect::new(NSPoint::new(label_width+8.,2.),NSSize::new(182.,24.)) pullsDown:Bool::NO]).ok_or("The save format control is unavailable.")?;
                let _: () = msg_send![&*popup,setFont:font];
                let represented: *mut AnyObject =
                    msg_send![objc2::class!(NSValue),valueWithNonretainedObject:&*panel];
                for (index, format) in Format::ALL.into_iter().enumerate() {
                    let label = NSString::from_str(format.label());
                    let _: () = msg_send![&*popup,addItemWithTitle:&*label];
                    let item: *mut AnyObject = msg_send![&*popup,itemAtIndex:index as isize];
                    let _: () = msg_send![item,setRepresentedObject:represented];
                }
                let _: () = msg_send![&*popup,selectItemAtIndex:2isize];
                let class = if let Some(class) = AnyClass::get(c"EchoSaveFormatTarget") {
                    class
                } else {
                    let mut builder =
                        ClassBuilder::new(c"EchoSaveFormatTarget", objc2::class!(NSObject))
                            .ok_or("The save format control is unavailable.")?;
                    builder.add_method(
                        sel!(formatChanged:),
                        changed as unsafe extern "C-unwind" fn(_, _, _),
                    );
                    builder.register()
                };
                let target: Retained<AnyObject> = Retained::from_raw(msg_send![class, new])
                    .ok_or("The save format control is unavailable.")?;
                let _: () = msg_send![&*popup,setTarget:&*target];
                let _: () = msg_send![&*popup,setAction:sel!(formatChanged:)];
                let _: () = msg_send![&*accessory,addSubview:&*popup];
                let _: () = msg_send![&*panel,setAccessoryView:&*accessory];
                let controls = Rc::new(Controls {
                    panel,
                    _parent: self.parent,
                    popup,
                    _target: target,
                });
                let result = Rc::new(RefCell::new(None));
                let output = result.clone();
                let retained = controls.clone();
                let completion = block2::RcBlock::new(move |response: isize| {
                    let choice = if response == 1 {
                        let index: isize = msg_send![&*retained.popup, indexOfSelectedItem];
                        let format = Format::ALL.get(index as usize).copied().unwrap_or_default();
                        let url: *mut AnyObject = msg_send![&*retained.panel, URL];
                        let path: *const std::ffi::c_char = if url.is_null() {
                            std::ptr::null()
                        } else {
                            msg_send![url, fileSystemRepresentation]
                        };
                        if path.is_null() {
                            Err("The save path is unavailable.".into())
                        } else {
                            use std::os::unix::ffi::OsStrExt;
                            Ok(Some(Choice {
                                path: PathBuf::from(std::ffi::OsStr::from_bytes(
                                    CStr::from_ptr(path).to_bytes(),
                                )),
                                format,
                            }))
                        }
                    } else {
                        Ok(None)
                    };
                    *output.borrow_mut() = Some(choice);
                });
                let _: () = msg_send![&*controls.panel,beginSheetModalForWindow:&*controls._parent completionHandler:&*completion];
                Ok(Some(Dialog {
                    controls,
                    result,
                    done: false,
                }))
            }
        }
    }
    impl Dialog {
        pub(crate) fn poll(&mut self) -> Option<Result<Option<Choice>, String>> {
            let value = self.result.borrow_mut().take();
            if value.is_some() {
                self.done = true;
            }
            value
        }
    }
    impl Drop for Dialog {
        fn drop(&mut self) {
            if !self.done {
                unsafe {
                    let _: () =
                        msg_send![&*self.controls.panel,cancel:std::ptr::null_mut::<AnyObject>()];
                    let _: () =
                        msg_send![&*self.controls.panel,orderOut:std::ptr::null_mut::<AnyObject>()];
                }
            }
        }
    }
}
