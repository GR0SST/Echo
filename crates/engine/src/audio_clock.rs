//! CEF audio_capturer.cc supplies TimeTicks milliseconds, not Unix time.
//! https://github.com/chromiumembedded/cef/blob/master/libcef/browser/audio_capturer.cc
//! Keep the tiny clock-origin shims here; playback scheduling stays portable.
use std::time::{Duration, Instant};

pub(crate) fn packet_time(pts: i64, now: Instant) -> Option<Instant> {
    let delta = pts as i128 * 1000 - monotonic_micros()? as i128;
    // Reject an unsupported/different epoch instead of scheduling years away.
    if delta.abs() > 5_000_000 { return None; }
    let offset = Duration::from_micros(delta.unsigned_abs().try_into().ok()?);
    if delta >= 0 { now.checked_add(offset) } else { now.checked_sub(offset) }
}
#[cfg(target_os = "macos")]
fn monotonic_micros() -> Option<u64> {
    #[repr(C)] struct Timebase { numer: u32, denom: u32 }
    unsafe extern "C" { fn mach_absolute_time() -> u64; fn mach_timebase_info(info: *mut Timebase) -> i32; }
    static SCALE: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
    let &(numer,denom) = SCALE.get_or_init(|| {
        let mut info = Timebase { numer: 0, denom: 0 };
        unsafe { mach_timebase_info(&mut info); }
        (info.numer,info.denom)
    });
    if denom == 0 { return None; }
    Some((unsafe { mach_absolute_time() } as u128 * numer as u128 / denom as u128 / 1000).try_into().ok()?)
}
#[cfg(target_os = "linux")]
fn monotonic_micros() -> Option<u64> {
    #[repr(C)] struct Timespec { seconds: std::os::raw::c_long, nanos: std::os::raw::c_long }
    unsafe extern "C" { fn clock_gettime(clock: i32, time: *mut Timespec) -> i32; }
    let mut time = Timespec { seconds: 0, nanos: 0 };
    if unsafe { clock_gettime(1, &mut time) } != 0 { return None; }
    u64::try_from(time.seconds).ok()?.checked_mul(1_000_000)?.checked_add(u64::try_from(time.nanos).ok()? / 1000)
}
#[cfg(target_os = "windows")]
fn monotonic_micros() -> Option<u64> {
    #[link(name = "kernel32")]
    unsafe extern "system" { fn QueryPerformanceCounter(value: *mut i64) -> i32; fn QueryPerformanceFrequency(value: *mut i64) -> i32; }
    let (mut count,mut frequency)=(0,0);
    if unsafe { QueryPerformanceCounter(&mut count) } == 0 || unsafe { QueryPerformanceFrequency(&mut frequency) } == 0 || frequency<=0 || count<0 { return None; }
    (count as u128 * 1_000_000 / frequency as u128).try_into().ok()
}
#[cfg(not(any(target_os="macos",target_os="windows",target_os="linux")))]
fn monotonic_micros() -> Option<u64> { None }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_time_maps_to_the_current_instant_without_a_wall_clock_epoch() {
        let now=Instant::now();
        let pts=(monotonic_micros().unwrap()/1000) as i64;
        let mapped=packet_time(pts,now).unwrap();
        assert!(now.saturating_duration_since(mapped)<Duration::from_millis(100));
        assert!(mapped.saturating_duration_since(now)<Duration::from_millis(100));
        assert!(packet_time(i64::MIN,now).is_none());
        assert!(packet_time(i64::MAX,now).is_none());
    }
}
