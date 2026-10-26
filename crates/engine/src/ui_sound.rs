//! Original short UI cues. These do not enter CEF's per-tab audio mixer.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub enum UiSound {
    Capture,
    /// The first-run welcome intro music (~21 s).
    Welcome,
}
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
/// Bumped to fade out a playing welcome cue.
static WELCOME_STOP: AtomicUsize = AtomicUsize::new(0);
/// When the welcome music actually reached the output, so visuals can follow
/// its beat instead of the moment playback was requested.
static WELCOME_STARTED: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
pub fn welcome_sound_started() -> Option<Instant> {
    *WELCOME_STARTED.lock().unwrap_or_else(|e| e.into_inner())
}
/// Fades out the welcome music if it is playing.
pub fn stop_welcome_sound() {
    WELCOME_STOP.fetch_add(1, Ordering::AcqRel);
}
struct Lease;
impl Drop for Lease {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}
/// A fresh default-output stream leaves timestamped media replay untouched.
/// Admission is bounded; devices without a supported output fail silently.
pub fn play_ui_sound(sound: UiSound) -> bool {
    if ACTIVE
        .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |count| {
            (count < 4).then_some(count + 1)
        })
        .is_err()
    {
        return false;
    }
    if let UiSound::Welcome = sound {
        *WELCOME_STARTED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    let lease = Lease;
    std::thread::Builder::new()
        .name("echo-ui-sound".into())
        .spawn(move || {
            let _lease = lease;
            if let Err(error) = play(sound) {
                if std::env::var_os("BROWSER_SCRIPT").is_some() {
                    eprintln!("[ui-sound] unavailable: {error}");
                }
            }
        })
        .is_ok()
}
fn waveform(sound: UiSound, rate: u32) -> Vec<f32> {
    if let UiSound::Welcome = sound {
        return welcome_samples(rate).unwrap_or_else(|_| crate::welcome_music::render(rate));
    }
    let length = (rate as usize * 140 / 1000).max(1);
    let mut seed = 0x71c3_a5d9u32;
    (0..length)
        .map(|index| {
            let t = index as f32 / rate.max(1) as f32;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed >> 8) as f32 / 0x00ff_ffff as f32 * 2. - 1.;
            match sound {
                UiSound::Welcome => 0.,
                UiSound::Capture => {
                    // Two softly decaying mechanical ticks, composed here from scratch.
                    let pulse = |start: f32| {
                        let age = t - start;
                        if age < 0. {
                            0.
                        } else {
                            (age / 0.001).min(1.) * (-age / 0.011).exp()
                        }
                    };
                    let envelope = pulse(0.) + 0.65 * pulse(0.045);
                    (noise * 0.6 + (t * std::f32::consts::TAU * 920.).sin() * 0.4) * envelope * 0.09
                }
            }
        })
        .collect()
}
/// The welcome cue, rendered once from `welcome_music` (see
/// `examples/render_welcome_music.rs`) and shipped as Ogg Vorbis.
const WELCOME_OGG: &[u8] = include_bytes!("../assets/welcome.ogg");
/// Mono samples of the shipped cue at the output's `rate`.
fn welcome_samples(rate: u32) -> Result<Vec<f32>, String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(WELCOME_OGG))
        .map_err(|e| e.to_string())?;
    let source = reader.ident_hdr.audio_sample_rate;
    let channels = usize::from(reader.ident_hdr.audio_channels.max(1));
    let mut decoded = Vec::with_capacity((crate::welcome_music::SECONDS * source as f32) as usize);
    while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| e.to_string())? {
        decoded.extend(packet.chunks_exact(channels).map(|frame| {
            frame.iter().map(|&s| f32::from(s) / 32768.).sum::<f32>() / channels as f32
        }));
    }
    Ok(resample(&decoded, source, rate))
}
/// Linear resampling keeps the cue's length, so the intro stays on the beat
/// at any device rate.
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() || from == 0 {
        return samples.to_vec();
    }
    let length = (samples.len() as u64 * u64::from(to) / u64::from(from)) as usize;
    let step = from as f64 / to as f64;
    (0..length)
        .map(|i| {
            let at = i as f64 * step;
            let index = at as usize;
            let next = samples.get(index + 1).copied().unwrap_or(0.);
            let current = samples.get(index).copied().unwrap_or(0.);
            current + (next - current) * (at - index as f64) as f32
        })
        .collect()
}
struct Cursor {
    samples: Vec<f32>,
    position: usize,
    channels: usize,
    rate: u32,
    done: Arc<AtomicBool>,
    rendered: Arc<AtomicUsize>,
    /// Stop generation to watch, and the fade gain once it changes.
    stop: Option<(usize, f32, f32)>,
}
impl Cursor {
    fn fill<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, buffer: &mut [T]) {
        if let Some((generation, _, step)) = &mut self.stop {
            if WELCOME_STOP.load(Ordering::Acquire) != *generation && *step == 0. {
                *step = 1. / (0.6 * self.rate.max(1) as f32);
            }
        }
        for frame in buffer.chunks_mut(self.channels) {
            let mut sample = self.samples.get(self.position).copied().unwrap_or(0.);
            if let Some((_, gain, step)) = &mut self.stop {
                *gain = (*gain - *step).max(0.);
                sample *= *gain;
                if *gain == 0. {
                    self.position = self.samples.len();
                }
            }
            for channel in frame {
                *channel = T::from_sample(sample);
            }
            if self.position < self.samples.len() {
                self.rendered.fetch_add(1, Ordering::Relaxed);
                self.position += 1;
            } else {
                self.done.store(true, Ordering::Release);
            }
        }
    }
}
fn stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    cursor: Cursor,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    let mut cursor = cursor;
    device
        .build_output_stream(
            config,
            move |buffer: &mut [T], _| cursor.fill(buffer),
            move |_| {
                failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|error| error.to_string())
}
fn play(sound: UiSound) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("No default output")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    if config.channels == 0 || config.sample_rate.0 == 0 {
        return Err("Invalid output format".into());
    }
    let done = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let rendered = Arc::new(AtomicUsize::new(0));
    let cursor = Cursor {
        samples: waveform(sound, config.sample_rate.0),
        stop: matches!(sound, UiSound::Welcome)
            .then(|| (WELCOME_STOP.load(Ordering::Acquire), 1., 0.)),
        rate: config.sample_rate.0,
        position: 0,
        channels: config.channels as usize,
        done: done.clone(),
        rendered: rendered.clone(),
    };
    let output = match format {
        cpal::SampleFormat::F32 => stream::<f32>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::F64 => stream::<f64>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::I8 => stream::<i8>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::I32 => stream::<i32>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::I64 => stream::<i64>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::U8 => stream::<u8>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::U32 => stream::<u32>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::U64 => stream::<u64>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::I16 => stream::<i16>(&device, &config, cursor, failed.clone()),
        cpal::SampleFormat::U16 => stream::<u16>(&device, &config, cursor, failed.clone()),
        _ => Err("Unsupported UI output format".into()),
    }?;
    output.play().map_err(|e| e.to_string())?;
    if let UiSound::Welcome = sound {
        *WELCOME_STARTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }
    let limit = match sound {
        UiSound::Capture => Duration::from_millis(900),
        UiSound::Welcome => Duration::from_secs_f32(crate::welcome_music::SECONDS + 2.),
    };
    let start = Instant::now();
    while !done.load(Ordering::Acquire)
        && !failed.load(Ordering::Acquire)
        && start.elapsed() < limit
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    // Keep the stream alive until its last submitted buffer reaches the device.
    if done.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(100));
    }
    if std::env::var_os("BROWSER_SCRIPT").is_some() {
        eprintln!(
            "[ui-sound] {} rendered_frames={} failed={}",
            match sound { UiSound::Capture => "capture", UiSound::Welcome => "welcome" },
            rendered.load(Ordering::Relaxed),
            failed.load(Ordering::Acquire)
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn welcome_music_is_finite_bounded_and_silent_at_its_edges() {
        let rate = 22050;
        let samples = waveform(UiSound::Welcome, rate);
        let expected = (crate::welcome_music::SECONDS * rate as f32) as usize;
        assert!(samples.len().abs_diff(expected) < rate as usize / 100, "{}", samples.len());
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 0.55));
        assert!(samples[0].abs() < 0.001 && samples.last().unwrap().abs() < 0.001);
        let rms = |from: f32, to: f32| {
            let part = &samples[(from * rate as f32) as usize..(to * rate as f32) as usize];
            (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
        };
        // Quiet swell, a louder body after the downbeat, then a fade-out.
        assert!(rms(0.2, 1.5) < rms(6., 12.));
        assert!(rms(20.6, 21.2) < rms(6., 12.) * 0.1);
    }
    /// The shipped file must be regenerated whenever the source changes.
    #[test]
    fn shipped_welcome_cue_matches_its_source() {
        // The asset's own rate: the synth's noise differs between rates.
        let rate = 48_000;
        let shipped = welcome_samples(rate).unwrap();
        let source = crate::welcome_music::render(rate);
        let envelope = |s: &[f32]| -> Vec<f32> {
            s.chunks(rate as usize / 20).map(|w| (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt()).collect()
        };
        let (a, b) = (envelope(&shipped), envelope(&source));
        let n = a.len().min(b.len());
        assert!(a.len().abs_diff(b.len()) <= 1);
        let dot: f32 = (0..n).map(|i| a[i] * b[i]).sum();
        let norm = |v: &[f32]| v[..n].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(dot / (norm(&a) * norm(&b)) > 0.99, "{}", dot / (norm(&a) * norm(&b)));
    }
    #[test]
    fn resampling_keeps_the_cue_length() {
        let samples = vec![0.25; 48_000];
        assert_eq!(resample(&samples, 48_000, 44_100).len(), 44_100);
        assert_eq!(resample(&samples, 48_000, 96_000).len(), 96_000);
    }
    #[test]
    fn stopping_welcome_fades_to_silence() {
        let done = Arc::new(AtomicBool::new(false));
        let mut cursor = Cursor {
            samples: vec![0.5; 4800],
            position: 0,
            channels: 1,
            rate: 1000,
            done: done.clone(),
            rendered: Arc::new(AtomicUsize::new(0)),
            stop: Some((WELCOME_STOP.load(Ordering::Acquire), 1., 0.)),
        };
        let mut output = [0f32; 100];
        cursor.fill(&mut output);
        assert_eq!(output[99], 0.5);
        stop_welcome_sound();
        let mut output = [0f32; 1000];
        cursor.fill(&mut output);
        assert!(output[0] < 0.5 && output[0] > 0.4);
        assert_eq!(output[999], 0.);
        assert!(done.load(Ordering::Acquire));
    }
    #[test]
    fn original_capture_cue_is_short_finite_quiet_and_silent_at_its_edges() {
        for rate in [16000, 44100, 48000, 96000] {
            let samples = waveform(UiSound::Capture, rate);
            assert_eq!(samples.len(), rate as usize * 140 / 1000);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() < 0.1));
            assert_eq!(samples[0], 0.);
            assert!(samples.last().unwrap().abs() < 0.001);
            assert!(samples.iter().any(|s| s.abs() > 0.01));
        }
    }
    #[test]
    fn device_callback_replicates_channels_and_finishes_with_zeroes() {
        let done = Arc::new(AtomicBool::new(false));
        let rendered = Arc::new(AtomicUsize::new(0));
        let mut cursor = Cursor {
            samples: vec![0.1, -0.1],
            position: 0,
            channels: 2,
            done: done.clone(),
            rendered: rendered.clone(),
            stop: None,
            rate: 48000,
        };
        let mut output = [9f32; 8];
        cursor.fill(&mut output);
        assert_eq!(output, [0.1, 0.1, -0.1, -0.1, 0., 0., 0., 0.]);
        assert!(done.load(Ordering::Acquire));
        assert_eq!(rendered.load(Ordering::Relaxed), 2);
    }
}
