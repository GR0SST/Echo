//! Minimal native thumbnail shim; discovery, caching and scheduling are shared.
use crate::thumbnails::Thumbnail;
use block2::RcBlock;
use objc2::{
    class, msg_send,
    rc::{autoreleasepool, Retained},
    runtime::AnyObject,
};
use objc2_foundation::{NSSize, NSString};
use std::{ffi::c_void, path::Path, sync::mpsc, time::Duration};
#[link(name = "QuickLookThumbnailing", kind = "framework")]
unsafe extern "C" {}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGImageGetWidth(image: *const c_void) -> usize;
    fn CGImageGetHeight(image: *const c_void) -> usize;
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
    static kCGColorSpaceSRGB: *const c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits: usize,
        stride: usize,
        space: *mut c_void,
        info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(
        context: *mut c_void,
        rect: objc2_foundation::NSRect,
        image: *const c_void,
    );
    fn CGContextRelease(context: *mut c_void);
}
unsafe fn pixels(image: *const c_void, edge: u32) -> Option<Thumbnail> {
    if image.is_null() {
        return None;
    }
    let (w, h) = (unsafe { CGImageGetWidth(image) }, unsafe {
        CGImageGetHeight(image)
    });
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (edge as f64 / w.max(h) as f64).min(1.);
    let (width, height) = (
        ((w as f64 * scale).round() as usize).max(1),
        ((h as f64 * scale).round() as usize).max(1),
    );
    let mut rgba = vec![0u8; width.checked_mul(height)?.checked_mul(4)?];
    let space = unsafe { CGColorSpaceCreateWithName(kCGColorSpaceSRGB) };
    if space.is_null() {
        return None;
    }
    // RGBA, premultiplied alpha, explicit byte order. Convert back to straight
    // alpha before returning the shared decoder's portable pixel format.
    let context = unsafe {
        CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            space,
            (1 << 14) | 1,
        )
    };
    unsafe { CGColorSpaceRelease(space) };
    if context.is_null() {
        return None;
    }
    unsafe {
        CGContextDrawImage(
            context,
            objc2_foundation::NSRect::new(
                objc2_foundation::NSPoint::new(0., 0.),
                NSSize::new(width as f64, height as f64),
            ),
            image,
        );
        CGContextRelease(context);
    }
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(Thumbnail {
        width: width as u32,
        height: height as u32,
        rgba: rgba.into(),
    })
}
pub(crate) fn thumbnail(path: &Path, edge: u32) -> Option<Thumbnail> {
    if edge == 0 || edge > 512 {
        return None;
    }
    autoreleasepool(|_| unsafe {
        let path = NSString::from_str(path.to_str()?);
        let url: *mut AnyObject = msg_send![class!(NSURL),fileURLWithPath:&*path];
        if url.is_null() {
            return None;
        }
        let alloc: *mut AnyObject = msg_send![class!(QLThumbnailGenerationRequest), alloc];
        let request: Retained<AnyObject> = Retained::from_raw(
            msg_send![alloc,initWithFileAtURL:url, size:NSSize::new(edge as f64,edge as f64), scale:1f64, representationTypes:4usize],
        )?;
        let generator: *mut AnyObject = msg_send![class!(QLThumbnailGenerator), sharedGenerator];
        if generator.is_null() {
            return None;
        }
        let (send, receive) = mpsc::sync_channel(1);
        let completion = RcBlock::new(
            move |representation: *mut AnyObject, _error: *mut AnyObject| {
                let result = if representation.is_null() {
                    None
                } else {
                    let image: *const objc2_core_graphics::CGImage =
                        msg_send![representation, CGImage];
                    pixels(image.cast(), edge)
                };
                let _ = send.try_send(result);
            },
        );
        let _: () = msg_send![generator,generateBestRepresentationForRequest:&*request, completionHandler:&*completion];
        match receive.recv_timeout(Duration::from_secs(3)) {
            Ok(result) => result,
            Err(_) => {
                let _: () = msg_send![generator,cancelRequest:&*request];
                None
            }
        }
    })
}

pub(crate) fn trash(path: &Path) -> Option<std::path::PathBuf> {
    autoreleasepool(|_| unsafe {
        let path = NSString::from_str(path.to_str()?);
        let url: *mut AnyObject = msg_send![class!(NSURL),fileURLWithPath:&*path];
        let manager: *mut AnyObject = msg_send![class!(NSFileManager), defaultManager];
        let mut result: *mut AnyObject = std::ptr::null_mut();
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let success: objc2::runtime::Bool =
            msg_send![manager,trashItemAtURL:url,resultingItemURL:&mut result,error:&mut error];
        if !success.as_bool() || result.is_null() {
            return None;
        }
        let path: *mut NSString = msg_send![result, path];
        (!path.is_null()).then(|| std::path::PathBuf::from((&*path).to_string()))
    })
}
