//! CEF loopback capture silences its source. Keep output and audible detection
//! together: capture is enabled only after a real output stream is running.
use super::{EventQueue, RawEvent};
use cef::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{cell::{Cell, RefCell}, collections::HashMap, sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant}};

const HOLD: Duration = Duration::from_millis(700);
const MAX_LATE: Duration = Duration::from_millis(26);
const LOOKAHEAD: Duration = Duration::from_millis(2);
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame { samples: [f32; 2], at: Instant }
#[derive(Default)]
struct Clock { anchor: Option<(i64, Instant)> }
impl Clock {
    fn packet(&mut self, pts: i64, now: Instant) -> Instant {
        if let Some(at) = crate::audio_clock::packet_time(pts, now) {
            self.anchor.get_or_insert((pts, now));
            return at;
        }
        self.relative_packet(pts, now)
    }
    fn relative_packet(&mut self, pts: i64, now: Instant) -> Instant {
        let (first, origin) = *self.anchor.get_or_insert((pts, now));
        let at = if pts >= first { origin.checked_add(Duration::from_millis((pts-first) as u64)) }
            else { origin.checked_sub(Duration::from_millis((first-pts) as u64)) };
        // A restarted capture/device can establish a different clock origin.
        let Some(at) = at.filter(|at| now.saturating_duration_since(*at)<Duration::from_millis(100)
            && at.saturating_duration_since(now)<Duration::from_millis(100)) else {
            self.anchor = Some((pts, now)); return now;
        };
        at
    }
}
#[derive(Default)]
struct Track {
    output: Option<Arc<Playback>>,
    audible: bool,
    last_sound: Option<Instant>,
    muted: bool,
    clock: Clock,
}
impl Track {
    fn activity(&mut self, sound: bool, now: Instant) -> Option<bool> {
        if sound { self.last_sound = Some(now); }
        let audible = self.last_sound.is_some_and(|last| now.saturating_duration_since(last) < HOLD);
        if audible == self.audible { None } else { self.audible = audible; Some(audible) }
    }
}
struct Playback {
    frames: crossbeam_queue::ArrayQueue<Frame>,
    muted: AtomicBool,
    active: AtomicBool,
    generation: AtomicU64,
}
impl Playback {
    fn clear(&self) { self.generation.fetch_add(1, Ordering::Relaxed); while self.frames.pop().is_some() {} }
}
#[derive(Default)]
struct State {
    tracks: HashMap<i32, Track>,
    outputs: Arc<Vec<Arc<Playback>>>,
}
impl State {
    fn register(&mut self, browser: i32, rate: u32) {
        self.tracks.insert(browser, Track {
            output: Some(Arc::new(Playback { frames: crossbeam_queue::ArrayQueue::new((rate as usize / 4).max(2)),
                muted: AtomicBool::new(false), active: AtomicBool::new(true), generation: AtomicU64::new(0) })), ..Track::default()
        });
        self.refresh();
    }
    fn mute(&mut self, browser: i32, muted: bool) {
        let Some(track) = self.tracks.get_mut(&browser) else { return; };
        if let Some(output) = &track.output {
            output.muted.store(muted, Ordering::Relaxed);
            if track.muted != muted { output.clear(); }
        }
        track.muted = muted;
    }
    fn close(&mut self, browser: i32) {
        if let Some(track) = self.tracks.remove(&browser) {
            if let Some(output) = track.output { output.active.store(false, Ordering::Relaxed); }
        }
        self.refresh();
    }
    fn refresh(&mut self) {
        self.outputs = Arc::new(self.tracks.values().filter_map(|track| track.output.clone()).collect());
    }
}
#[derive(Clone)]
pub(crate) struct Capture {
    state: Arc<Mutex<State>>,
    rate: u32,
}
pub(crate) struct Output {
    capture: Capture,
    stream: RefCell<Option<cpal::Stream>>,
    device: RefCell<String>,
    failed: Arc<AtomicBool>,
    checked: Cell<Instant>,
}

/// One rate conversion for the mixed signal preserves pitch when the new
/// default device uses a different rate from already-running CEF streams.
struct Source {
    output: Arc<Playback>,
    pending: Option<Frame>,
    generation: u64,
}
impl Source {
    fn sample(&mut self, at: Instant, late_limit: Duration) -> [f32; 2] {
        let generation = self.output.generation.load(Ordering::Relaxed);
        if generation != self.generation { self.pending = None; self.generation = generation; }
        if !self.output.active.load(Ordering::Relaxed) || self.output.muted.load(Ordering::Relaxed) {
            self.pending = None; return [0.; 2];
        }
        loop {
            let Some(frame) = self.pending.take().or_else(|| self.output.frames.pop()) else { return [0.; 2]; };
            if frame.at > at + LOOKAHEAD { self.pending = Some(frame); return [0.; 2]; }
            if at.saturating_duration_since(frame.at) <= late_limit { return frame.samples; }
        }
    }
}
struct Mixer {
    registry: Arc<Vec<Arc<Playback>>>,
    sources: Vec<Source>,
    phase: f64,
    step: f64,
    current: [f32; 2],
    next: [f32; 2],
    primed: bool,
    late_limit: Duration,
}
impl Mixer {
    fn new(step: f64) -> Self {
        Self { registry: Arc::default(), sources: Vec::new(), phase: 0., step,
            current: [0.; 2], next: [0.; 2], primed: false, late_limit: MAX_LATE }
    }
    fn refresh(&mut self, outputs: Arc<Vec<Arc<Playback>>>) {
        if Arc::ptr_eq(&self.registry, &outputs) { return; }
        let mut old = std::mem::take(&mut self.sources);
        self.sources = outputs.iter().map(|output| {
            old.iter().position(|source| Arc::ptr_eq(&source.output, output))
                .map(|index| old.swap_remove(index))
                .unwrap_or_else(|| Source { output: output.clone(), pending: None, generation: output.generation.load(Ordering::Relaxed) })
        }).collect();
        self.registry = outputs;
    }
    fn source(&mut self, at: Instant) -> [f32; 2] {
        let mut mixed = [0.; 2];
        for source in &mut self.sources {
            let frame = source.sample(at, self.late_limit);
            mixed[0] += frame[0]; mixed[1] += frame[1];
        }
        mixed
    }
    fn sample(&mut self, at: Instant) -> [f32; 2] {
        if self.step == 1. { return self.source(at); }
        if !self.primed { self.current = self.source(at); self.next = self.source(at); self.primed = true; }
        let value = std::array::from_fn(|i| self.current[i] + (self.next[i] - self.current[i]) * self.phase as f32);
        self.phase += self.step;
        while self.phase >= 1. {
            self.phase -= 1.; self.current = self.next; self.next = self.source(at);
        }
        value
    }
}
impl Output {
    pub(crate) fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let rate = device.default_output_config().ok()?.sample_rate().0;
        let output = Self {
            capture: Capture { state: Arc::new(Mutex::new(State::default())), rate },
            stream: RefCell::new(None), device: RefCell::new(String::new()),
            failed: Arc::new(AtomicBool::new(false)), checked: Cell::new(Instant::now()),
        };
        output.rebuild(&device)?;
        Some(output)
    }
    fn rebuild(&self, device: &cpal::Device) -> Option<()> {
        let supported = device.default_output_config().ok()?;
        let channels = supported.channels() as usize;
        if channels == 0 { return None; }
        let format = supported.sample_format();
        let mut config: cpal::StreamConfig = supported.clone().into();
        // Bound replay latency rather than accepting a device's potentially
        // large default buffer. Respect the device's advertised range.
        if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
            config.buffer_size = cpal::BufferSize::Fixed(128u32.clamp(min, max));
        }
        self.stream.borrow_mut().take();
        {
            let mut state = self.capture.state.lock().unwrap();
            for track in state.tracks.values_mut() {
                if let Some(output) = &track.output { output.clear(); }
                track.clock.anchor = None;
            }
        }
        self.failed.store(false, Ordering::Relaxed);
        fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(device: &cpal::Device,
            config: &cpal::StreamConfig, capture: &Capture, failed: Arc<AtomicBool>) -> Option<cpal::Stream> {
            let state = capture.state.clone();
            let channels = config.channels as usize;
            let mut mixer = Mixer::new(capture.rate as f64 / config.sample_rate.0 as f64);
            let rate = config.sample_rate.0 as f64;
            device.build_output_stream(config, move |out: &mut [T], info| {
                if let Ok(state) = state.try_lock() { mixer.refresh(state.outputs.clone()); }
                let timestamp = info.timestamp();
                let latency = timestamp.playback.duration_since(&timestamp.callback).unwrap_or_default();
                // Large hardware buffers need enough source history to fill
                // one buffer; their predicted playback instant is another
                // buffer ahead. Never turn a slower device into silence.
                mixer.late_limit = MAX_LATE.saturating_add(latency.saturating_mul(2));
                let start = Instant::now() + latency;
                for (index, frame) in out.chunks_mut(channels).enumerate() {
                    let [left, right] = mixer.sample(start + Duration::from_secs_f64(index as f64 / rate));
                    for (index, sample) in frame.iter_mut().enumerate() {
                        let value = if channels == 1 { (left + right) * 0.5 } else if index == 0 { left } else if index == 1 { right } else { 0. };
                        *sample = T::from_sample(value.clamp(-1., 1.));
                    }
                }
            }, move |error| {
                eprintln!("[audio] output error: {error}");
                failed.store(true, Ordering::Relaxed);
            }, None).ok()
        }
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::F64 => build::<f64>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::I8 => build::<i8>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::I16 => build::<i16>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::I32 => build::<i32>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::I64 => build::<i64>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::U8 => build::<u8>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::U16 => build::<u16>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::U32 => build::<u32>(device, &config, &self.capture, self.failed.clone()),
            cpal::SampleFormat::U64 => build::<u64>(device, &config, &self.capture, self.failed.clone()),
            _ => None,
        }?;
        stream.play().ok()?;
        let name = device.name().ok()?;
        eprintln!("[audio] output {name}, {} Hz, {:?}", config.sample_rate.0, config.buffer_size);
        *self.device.borrow_mut() = name;
        *self.stream.borrow_mut() = Some(stream);
        Some(())
    }
    /// Run on the engine thread, never in a CEF or real-time audio callback.
    /// Reapply the small output buffer on a route change: a default-following
    /// audio unit can otherwise adopt the new device's larger buffer.
    pub(crate) fn poll(&self) {
        let now = Instant::now();
        if now.duration_since(self.checked.get()) < Duration::from_millis(250) { return; }
        self.checked.set(now);
        let Some(device) = cpal::default_host().default_output_device() else { return; };
        if self.stream.borrow().is_none() || self.failed.load(Ordering::Relaxed)
            || device.name().ok().as_ref() != Some(&*self.device.borrow()) {
            if self.rebuild(&device).is_none() { self.failed.store(true, Ordering::Relaxed); }
        }
    }
    pub(crate) fn capture(&self) -> Capture { self.capture.clone() }
    pub(crate) fn register(&self, browser: i32) { self.capture.state.lock().unwrap().register(browser, self.capture.rate); }
    pub(crate) fn mute(&self, browser: i32, muted: bool) { self.capture.state.lock().unwrap().mute(browser, muted); }
    pub(crate) fn audible(&self, browser: i32) -> Option<bool> { self.capture.state.lock().unwrap().tracks.get(&browser).map(|track| track.audible) }
    pub(crate) fn close(&self, browser: i32) { self.capture.state.lock().unwrap().close(browser); }
}

wrap_audio_handler! {
    pub(crate) struct Handler { capture: Capture, events: EventQueue }
    impl AudioHandler {
        fn audio_parameters(&self, _browser: Option<&mut Browser>, params: Option<&mut AudioParameters>) -> i32 {
            let Some(params) = params else { return 0; };
            params.sample_rate = self.capture.rate as i32;
            params.channel_layout = ChannelLayout::LAYOUT_STEREO;
            params.frames_per_buffer = 128;
            1
        }
        fn on_audio_stream_packet(&self, browser: Option<&mut Browser>, data: *mut *const f32, frames: i32, pts: i64) {
            let Some(browser) = browser else { return; };
            if data.is_null() || frames <= 0 || frames > 16384 { return; }
            // CEF owns two planar channels for this callback only; copy them
            // before returning. The requested layout is always stereo.
            let (left, right) = unsafe {
                let l = *data; let r = *data.add(1);
                if l.is_null() || r.is_null() { return; }
                (std::slice::from_raw_parts(l, frames as usize), std::slice::from_raw_parts(r, frames as usize))
            };
            let id = browser.identifier();
            let transition = {
                let mut state = self.capture.state.lock().unwrap();
                let Some(track) = state.tracks.get_mut(&id) else { return; };
                let now = Instant::now();
                let at = track.clock.packet(pts, now);
                let mut sound = false;
                for (index, (&l, &r)) in left.iter().zip(right).enumerate() {
                    let l = if l.is_finite() { l } else { 0. };
                    let r = if r.is_finite() { r } else { 0. };
                    sound |= l.abs() > 0.0001 || r.abs() > 0.0001;
                    if !track.muted { if let Some(output) = &track.output { output.frames.force_push(Frame { samples: [l, r], at: at + Duration::from_secs_f64(index as f64 / self.capture.rate as f64) }); } }
                }
                track.activity(sound, Instant::now())
            };
            if let Some(audible) = transition { self.events.push((id, RawEvent::Audio(audible))); }
        }
        fn on_audio_stream_stopped(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let mut state = self.capture.state.lock().unwrap();
                if let Some(track) = state.tracks.get_mut(&browser.identifier()) {
                    if let Some(output) = &track.output { output.clear(); }
                    track.last_sound = None;
                    track.clock.anchor = None;
                    if std::mem::take(&mut track.audible) { self.events.push((browser.identifier(), RawEvent::Audio(false))); }
                }
            }
        }
        fn on_audio_stream_error(&self, browser: Option<&mut Browser>, _message: Option<&CefString>) {
            self.on_audio_stream_stopped(browser);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mute_flushes_queued_sound_and_close_revokes_retained_device_snapshot() {
        let mut state = State::default();
        state.register(7, 48000);
        state.register(8, 48000);
        let snapshot = state.outputs.clone();
        let output = state.tracks[&7].output.as_ref().unwrap().clone();
        output.frames.force_push(Frame { samples: [0.2, 0.4], at: Instant::now() });
        state.mute(7, true);
        assert!(output.muted.load(Ordering::Relaxed));
        assert!(output.frames.is_empty());
        assert!(!state.tracks[&8].muted);
        state.mute(7, false);
        assert!(!output.muted.load(Ordering::Relaxed));
        state.close(7);
        assert!(!output.active.load(Ordering::Relaxed));
        assert_eq!(snapshot.len(), 2);
        assert_eq!(state.outputs.len(), 1);
        assert!(!state.tracks.contains_key(&7));
        state.mute(7, true);
        assert!(!state.tracks.contains_key(&7), "late controls cannot revive a closed source");
    }
    #[test]
    fn timestamps_preserve_packet_gaps_despite_callback_jitter() {
        let now=Instant::now(); let mut clock=Clock::default();
        assert_eq!(clock.relative_packet(1000,now),now);
        assert_eq!(clock.relative_packet(1010,now+Duration::from_millis(18)),now+Duration::from_millis(10));
        assert_eq!(clock.relative_packet(1040,now+Duration::from_millis(39)),now+Duration::from_millis(40));
        assert_eq!(clock.relative_packet(500,now+Duration::from_secs(1)),now+Duration::from_secs(1));
    }
    #[test]
    fn scheduled_source_waits_for_future_samples_and_discards_stale_backlog() {
        let now=Instant::now(); let mut state=State::default();state.register(1,48000);
        let output=state.tracks[&1].output.as_ref().unwrap().clone();
        output.frames.force_push(Frame{samples:[1.;2],at:now});
        output.frames.force_push(Frame{samples:[2.;2],at:now+Duration::from_millis(60)});
        let mut source=Source{output,pending:None,generation:0};
        assert_eq!(source.sample(now+Duration::from_millis(40),MAX_LATE),[0.;2]);
        assert_eq!(source.sample(now+Duration::from_millis(55),MAX_LATE),[0.;2]);
        assert_eq!(source.sample(now+Duration::from_millis(60),MAX_LATE),[2.;2]);
    }
    #[test]
    fn mute_flush_invalidates_a_consumers_cached_future_frame() {
        let now=Instant::now();let mut state=State::default();state.register(1,48000);
        let output=state.tracks[&1].output.as_ref().unwrap().clone();
        output.frames.force_push(Frame{samples:[1.;2],at:now+Duration::from_millis(5)});
        let mut source=Source{output,pending:None,generation:0};
        assert_eq!(source.sample(now,MAX_LATE),[0.;2]);
        state.mute(1,true);state.mute(1,false);
        assert_eq!(source.sample(now+Duration::from_millis(5),MAX_LATE),[0.;2]);
    }
    #[test]
    fn a_large_output_buffer_does_not_drop_every_sample_as_stale() {
        let now=Instant::now();let mut state=State::default();state.register(1,48000);
        let output=state.tracks[&1].output.as_ref().unwrap().clone();
        output.frames.force_push(Frame{samples:[0.5;2],at:now});
        let mut mixer=Mixer::new(1.);mixer.refresh(state.outputs.clone());
        mixer.late_limit=MAX_LATE+Duration::from_millis(80);
        assert_eq!(mixer.sample(now+Duration::from_millis(40)),[0.5;2]);
    }
    #[test]
    fn output_rate_changes_preserve_duration_and_stereo() {
        for (source,device) in [(48000,44100),(44100,48000),(48000,96000),(48000,48000)] {
            let now=Instant::now();let mut state=State::default();state.register(1,source);
            let output=state.tracks[&1].output.as_ref().unwrap();
            for i in 0..100 { output.frames.force_push(Frame{samples:[i as f32,-(i as f32)],at:now+Duration::from_secs_f64(i as f64/source as f64)}); }
            let mut mixer=Mixer::new(source as f64/device as f64);mixer.refresh(state.outputs.clone());
            for i in 0..50 {
                let [l,r]=mixer.sample(now+Duration::from_secs_f64(i as f64/device as f64));
                assert!((l-i as f32*source as f32/device as f32).abs()<0.001);
                assert_eq!(l,-r);
            }
        }
    }
    #[test]
    fn silence_expires_activity_and_restart_is_reported_once() {
        let now = Instant::now(); let mut track = Track::default();
        assert_eq!(track.activity(false, now), None);
        assert_eq!(track.activity(true, now), Some(true));
        assert_eq!(track.activity(true, now), None);
        assert_eq!(track.activity(false, now + HOLD / 2), None);
        assert_eq!(track.activity(false, now + HOLD), Some(false));
        assert_eq!(track.activity(false, now + HOLD), None);
        assert_eq!(track.activity(true, now + HOLD), Some(true));
    }
}
