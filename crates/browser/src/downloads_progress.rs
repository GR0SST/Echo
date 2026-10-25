//! Display copied metrics only. Byte counts and percentages never authorize completion.
use engine::{DownloadInfo, DownloadPhase};
pub(crate) fn subtitle(info: &DownloadInfo, opening: bool) -> String {
    if info.phase == DownloadPhase::Canceled {
        return "Cancelled".into();
    }
    if opening {
        return if info.percent_complete >= 0 {
            format!("Opening… ({}%)", info.percent_complete.clamp(0, 100))
        } else {
            "Opening…".into()
        };
    }
    let mut text = bytes(info.received_bytes.max(0));
    if info.total_bytes > 0 {
        text.push('/');
        text.push_str(&bytes(info.total_bytes));
    }
    if info.current_speed > 0
        && info.total_bytes > info.received_bytes
        && info.phase == DownloadPhase::InProgress
    {
        let remaining = info.total_bytes.saturating_sub(info.received_bytes).max(0) as u64;
        let seconds = remaining.div_ceil(info.current_speed as u64);
        let eta = if seconds >= 3600 {
            format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
        } else if seconds >= 60 {
            format!("{}m {}s", seconds / 60, seconds % 60)
        } else {
            format!("{seconds}s")
        };
        text.push_str(" · ");
        text.push_str(&eta);
    }
    text
}
pub(crate) fn percent(info: &DownloadInfo) -> Option<f32> {
    (info.percent_complete >= 0).then(|| info.percent_complete.clamp(0, 100) as f32 / 100.)
}
fn bytes(count: i64) -> String {
    #[cfg(target_os = "macos")]
    if let Some(value) = native::bytes(count) {
        return value;
    }
    let (value, unit) = if count >= 1_000_000_000 {
        (count as f64 / 1_000_000_000., "GB")
    } else if count >= 1_000_000 {
        (count as f64 / 1_000_000., "MB")
    } else {
        (count as f64 / 1000., "KB")
    };
    if value >= 100. || value.fract() == 0. {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}
#[cfg(target_os = "macos")]
mod native {
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    pub(super) fn bytes(count: i64) -> Option<String> {
        objc2::rc::autoreleasepool(|_| unsafe {
            let allocated: *mut AnyObject = msg_send![objc2::class!(NSByteCountFormatter), alloc];
            let formatter: Retained<AnyObject> = Retained::from_raw(msg_send![allocated, init])?;
            let _: () = msg_send![&*formatter,setCountStyle:0isize];
            let _: () = msg_send![&*formatter,setAllowsNonnumericFormatting:Bool::NO];
            let value: *mut NSString = msg_send![&*formatter,stringFromByteCount:count];
            Some(Retained::retain(value)?.to_string())
        })
    }
}
