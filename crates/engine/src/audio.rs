//! CEF loopback capture silences its source. Keep output and audible detection
//! together: capture is enabled only after a real output stream is running.
use super::{EventQueue, RawEvent};
use cef::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{collections::HashMap, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

const HOLD: Duration = Duration::from_millis(700);
#[derive(Default)]
struct Track {
    output: Option<Arc<Playback>>,
    audible: bool,
    last_sound: Option<Instant>,
    muted: bool,
}
impl Track {
    fn activity(&mut self, sound: bool, now: Instant) -> Option<bool> {
        if sound { self.last_sound = Some(now); }
        let audible = self.last_sound.is_some_and(|last| now.saturating_duration_since(last) < HOLD);
        if audible == self.audible { None } else { self.audible = audible; Some(audible) }
    }
}
struct Playback {
    frames: crossbeam_queue::ArrayQueue<[f32; 2]>,
    muted: AtomicBool,
    active: AtomicBool,
}
impl Playback {
    fn clear(&self) { while self.frames.pop().is_some() {} }
}
#[derive(Default)]
struct State {
    tracks: HashMap<i32, Track>,
    outputs: Arc<Vec<Arc<Playback>>>,
}
impl State {
    fn register(&mut self, browser: i32, rate: u32) {
        self.tracks.insert(browser, Track {
            output: Some(Arc::new(Playback { frames: crossbeam_queue::ArrayQueue::new((rate as usize / 4).max(1)),
                muted: AtomicBool::new(false), active: AtomicBool::new(true) })), ..Track::default()
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
    // Never accessed from a CEF callback; its closure retains the queues.
    _stream: cpal::Stream,
}
impl Output {
    pub(crate) fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        if channels == 0 { return None; }
        let state = Arc::new(Mutex::new(State::default()));
        let format = config.sample_format();
        let config: cpal::StreamConfig = config.into();
        fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(device: &cpal::Device,
            config: &cpal::StreamConfig, state: Arc<Mutex<State>>, channels: usize) -> Option<cpal::Stream> {
            let mut outputs: Arc<Vec<Arc<Playback>>> = Arc::default();
            device.build_output_stream(config, move |out: &mut [T], _| {
                // Registry changes are rare. Retain the last snapshot instead
                // of blocking or dropping a device buffer during CEF capture.
                if let Ok(state) = state.try_lock() { outputs = state.outputs.clone(); }
                for frame in out.chunks_mut(channels) {
                    let (mut left, mut right) = (0.0_f32, 0.0_f32);
                    for output in outputs.iter() {
                        let [l, r] = output.frames.pop().unwrap_or([0., 0.]);
                        if output.active.load(Ordering::Relaxed) && !output.muted.load(Ordering::Relaxed) { left += l; right += r; }
                    }
                    for (index, sample) in frame.iter_mut().enumerate() {
                        let value = if channels == 1 { (left + right) * 0.5 } else if index == 0 { left } else if index == 1 { right } else { 0. };
                        *sample = T::from_sample(value.clamp(-1., 1.));
                    }
                }
            }, |error| eprintln!("[audio] output error: {error}"), None).ok()
        }
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::F64 => build::<f64>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::I8 => build::<i8>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::I64 => build::<i64>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::U8 => build::<u8>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::U32 => build::<u32>(&device, &config, state.clone(), channels),
            cpal::SampleFormat::U64 => build::<u64>(&device, &config, state.clone(), channels),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Self { capture: Capture { state, rate }, _stream: stream })
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
            params.frames_per_buffer = (self.capture.rate / 100).clamp(128, 4096) as i32;
            1
        }
        fn on_audio_stream_packet(&self, browser: Option<&mut Browser>, data: *mut *const f32, frames: i32, _pts: i64) {
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
                let mut sound = false;
                for (&l, &r) in left.iter().zip(right) {
                    let l = if l.is_finite() { l } else { 0. };
                    let r = if r.is_finite() { r } else { 0. };
                    sound |= l.abs() > 0.0001 || r.abs() > 0.0001;
                    if !track.muted { if let Some(output) = &track.output { output.frames.force_push([l, r]); } }
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
        output.frames.force_push([0.2, 0.4]);
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
    fn bounded_queue_drops_whole_stereo_frames() {
        let mut state = State::default(); state.register(1, 8);
        let output = state.tracks[&1].output.as_ref().unwrap();
        output.frames.force_push([1., -1.]);
        output.frames.force_push([2., -2.]);
        assert_eq!(output.frames.force_push([3., -3.]), Some([1., -1.]));
        assert_eq!(output.frames.pop(), Some([2., -2.]));
        assert_eq!(output.frames.pop(), Some([3., -3.]));
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
