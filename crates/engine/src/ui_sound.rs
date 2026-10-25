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
}
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
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
    let length = (rate as usize * 140 / 1000).max(1);
    let mut seed = 0x71c3_a5d9u32;
    (0..length)
        .map(|index| {
            let t = index as f32 / rate.max(1) as f32;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed >> 8) as f32 / 0x00ff_ffff as f32 * 2. - 1.;
            match sound {
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
struct Cursor {
    samples: Vec<f32>,
    position: usize,
    channels: usize,
    done: Arc<AtomicBool>,
    rendered: Arc<AtomicUsize>,
}
impl Cursor {
    fn fill<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, buffer: &mut [T]) {
        for frame in buffer.chunks_mut(self.channels) {
            let sample = self.samples.get(self.position).copied().unwrap_or(0.);
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
    let start = Instant::now();
    while !done.load(Ordering::Acquire)
        && !failed.load(Ordering::Acquire)
        && start.elapsed() < Duration::from_millis(900)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    // Keep the stream alive until its last submitted buffer reaches the device.
    if done.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(100));
    }
    if std::env::var_os("BROWSER_SCRIPT").is_some() {
        eprintln!(
            "[ui-sound] capture rendered_frames={} failed={}",
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
        };
        let mut output = [9f32; 8];
        cursor.fill(&mut output);
        assert_eq!(output, [0.1, 0.1, -0.1, -0.1, 0., 0., 0., 0.]);
        assert!(done.load(Ordering::Acquire));
        assert_eq!(rendered.load(Ordering::Relaxed), 2);
    }
}
