//! Original cue for the first-run welcome intro, synthesized from scratch:
//! a dark, slightly unstable D drone over low rumble that swells into a
//! downbeat at 4.67 s, then a muffled, saturated G – C – D – Em progression
//! at 90 BPM with mechanical ticks, fading out by ~21 s.
use std::f32::consts::TAU;

pub(crate) const SECONDS: f32 = 21.3;
const BEAT: f32 = 60. / 90.;
const DOWNBEAT: f32 = 4.67;

fn hz(midi: f32) -> f32 {
    440. * 2f32.powf((midi - 69.) / 12.)
}
fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2. - 1.
    }
}

/// Pad chords as (start, MIDI notes, bass note).
const CHORDS: [(f32, [f32; 5], f32); 6] = [
    (0., [50., 57., 62., 65., 67.], 38.),          // D minor add 11 (intro, no bass until the downbeat)
    (DOWNBEAT, [55., 59., 62., 67., 71.], 43.),     // G
    (DOWNBEAT + 4. * BEAT, [60., 64., 67., 72., 74.], 36.), // C add9
    (DOWNBEAT + 11. * BEAT, [62., 66., 69., 74., 78.], 38.), // D
    (DOWNBEAT + 13. * BEAT, [64., 67., 71., 76., 79.], 40.), // Em
    (SECONDS, [64., 67., 71., 76., 79.], 40.),
];
const GLIDE: f32 = 0.6;

fn chord_at(t: f32) -> ([f32; 5], f32) {
    let mut index = 0;
    while index + 1 < CHORDS.len() - 1 && t >= CHORDS[index + 1].0 {
        index += 1;
    }
    let (_, notes, bass) = CHORDS[index];
    let (next_start, next_notes, next_bass) = CHORDS[index + 1];
    // The pad slides into the D and Em chords as short glissandi; elsewhere
    // notes change with a brief portamento.
    let glide = match index { 2 => GLIDE + 0.1, 3 => GLIDE, _ => 0.08 };
    let mix = smooth(next_start - glide, next_start, t);
    let mut out = [0.; 5];
    for i in 0..5 {
        out[i] = notes[i] + (next_notes[i] - notes[i]) * mix;
    }
    (out, if mix > 0.5 { next_bass } else { bass })
}

/// Bell plucks: (time, MIDI note, gain).
fn bells() -> Vec<(f32, f32, f32)> {
    let mut out = vec![
        (0.05, 74., 0.5),
        (0.88, 69., 0.45),
        (1.30, 76., 0.5),
        (1.88, 67., 0.5),
        (2.25, 74., 0.55),
        (2.58, 81., 0.4),
        (3.21, 79., 0.55),
        (3.76, 74., 0.6),
        (4.04, 76., 0.6),
    ];
    let arps: [(f32, [f32; 4]); 4] = [
        (DOWNBEAT, [79., 83., 86., 83.]),
        (DOWNBEAT + 4. * BEAT, [76., 79., 84., 86.]),
        (DOWNBEAT + 11. * BEAT, [78., 81., 86., 81.]),
        (DOWNBEAT + 13. * BEAT, [79., 83., 88., 86.]),
    ];
    for (index, (start, notes)) in arps.iter().enumerate() {
        let end = arps.get(index + 1).map_or(DOWNBEAT + 19. * BEAT, |a| a.0);
        let mut t = *start + BEAT;
        let mut n = 0;
        while t < end - 0.05 {
            out.push((t, notes[n % 4], 0.35));
            t += BEAT;
            n += 1;
        }
    }
    out
}

/// Renders mono samples in [-1, 1] at `rate`.
pub(crate) fn render(rate: u32) -> Vec<f32> {
    let rate = rate.max(1) as f32;
    let length = (SECONDS * rate) as usize;
    let mut dry = vec![0f32; length];
    let mut rng = Rng(0x2f6e_2b1d);
    let bells = bells();
    let mut phases = [[0f32; 3]; 5];
    let mut bass_phase = 0f32;
    let mut lowpass = 0f32;
    let mut hiss_low = 0f32;
    let mut hiss_band = 0f32;
    let mut lowpass2 = 0f32;
    let mut rumble = 0f32;
    let mut rumble2 = 0f32;
    let mut drone_phase = [0f32; 2];
    let detune = [-0.07f32, 0., 0.06];
    for (index, sample) in dry.iter_mut().enumerate() {
        let t = index as f32 / rate;
        let (notes, bass) = chord_at(t);
        // A slow, uneven pitch drift keeps the harmony uneasy.
        let wow = 0.06 * (TAU * 0.23 * t).sin() + 0.03 * (TAU * 0.61 * t + 1.).sin();
        // Pad: detuned saws through a slowly opening one-pole lowpass.
        let mut pad = 0.;
        for (voice, note) in notes.iter().enumerate() {
            for (osc, cents) in detune.iter().enumerate() {
                let phase = &mut phases[voice][osc];
                *phase = (*phase + hz(note + cents + wow) / rate).fract();
                pad += *phase * 2. - 1.;
            }
        }
        pad /= 15.;
        let cutoff = 260. + 420. * smooth(0., DOWNBEAT, t) + 520. * smooth(DOWNBEAT, DOWNBEAT + 1.5, t) + 90. * (t * 0.7).sin();
        let k = 1. - (-TAU * cutoff / rate).exp();
        lowpass += (pad - lowpass) * k;
        lowpass2 += (lowpass - lowpass2) * k;
        let swell = 0.12 + 0.88 * smooth(0., DOWNBEAT, t).powf(1.6);
        let tremolo = 1. + 0.04 * (TAU * t / (BEAT * 2.)).sin();
        let fade = 1. - smooth(15.6, 20.6, t);
        let mut value = lowpass2 * 0.75 * swell * tremolo;
        // The intro's near-pure D drone, handing over to the band.
        let drone_level = (0.35 + 0.65 * smooth(0., 2.5, t)) * (1. - smooth(DOWNBEAT, DOWNBEAT + 2.5, t));
        for (index, (note, gain)) in [(62f32, 0.16f32), (50., 0.09)].iter().enumerate() {
            let phase = &mut drone_phase[index];
            *phase = (*phase + hz(note + wow * 0.5) / rate).fract();
            value += (TAU * *phase).sin() * gain * drone_level;
        }
        // Bass from the downbeat.
        bass_phase = (bass_phase + hz(bass) / rate).fract();
        let b = (TAU * bass_phase).sin() + 0.25 * (2. * TAU * bass_phase).sin();
        value += b * 0.12 * smooth(DOWNBEAT - 0.02, DOWNBEAT + 0.3, t);
        // Low rumble under everything, heaviest into the downbeat.
        let noise = rng.next();
        rumble += (noise - rumble) * (TAU * 90. / rate);
        rumble2 += (rumble - rumble2) * (TAU * 90. / rate);
        value += rumble2 * 1.6 * (0.4 + 0.6 * smooth(0., DOWNBEAT, t)) * (1. - 0.4 * smooth(DOWNBEAT, DOWNBEAT + 3., t));
        // Air: faint band-limited noise, swelling into the downbeat.
        hiss_low += (noise - hiss_low) * 0.35;
        hiss_band += ((noise - hiss_low) - hiss_band) * 0.5;
        let riser = smooth(DOWNBEAT - 1.2, DOWNBEAT, t) * (t < DOWNBEAT) as u8 as f32;
        value += hiss_band * (0.004 + 0.04 * riser + 0.02 * smooth(DOWNBEAT, DOWNBEAT + 0.2, t));
        // Gentle saturation for grit.
        *sample = (value * 1.8).tanh() / 1.8 * fade;
    }
    // Downbeat impact: a low thump with a noise bloom.
    add_hit(&mut dry, rate, DOWNBEAT, 0.55, 58., 0.7, &mut rng);
    // Soft percussion: accents every two beats, light ticks on the off-beats.
    let mut beat = 1;
    while DOWNBEAT + beat as f32 * BEAT < 18. {
        let at = DOWNBEAT + beat as f32 * BEAT;
        if beat % 2 == 0 {
            add_hit(&mut dry, rate, at, 0.22, 64., 0.25, &mut rng);
            add_tick(&mut dry, rate, at + BEAT / 2., 0.09, &mut rng);
        }
        add_tick(&mut dry, rate, at, 0.14, &mut rng);
        beat += 1;
    }
    for (at, note, gain) in bells {
        add_bell(&mut dry, rate, at, hz(note), gain * 0.05);
    }
    let mut out = reverb(&dry, rate);
    // Final edges and loudness: start/end at silence, peak at 0.5.
    let peak = out.iter().fold(0f32, |m, s| m.max(s.abs())).max(1e-6);
    let edge = (rate * 0.01) as usize;
    let len = out.len();
    for (index, sample) in out.iter_mut().enumerate() {
        let t = index as f32 / rate;
        let tail = 1. - smooth(SECONDS - 0.8, SECONDS, t);
        let attack = (index as f32 / edge as f32).min(1.);
        *sample = *sample / peak * 0.5 * tail * attack;
        if index + 1 == len {
            *sample = 0.;
        }
    }
    out
}

fn add_hit(buffer: &mut [f32], rate: f32, at: f32, gain: f32, pitch: f32, decay: f32, rng: &mut Rng) {
    let start = (at * rate) as usize;
    let mut phase = 0f32;
    let mut low = 0f32;
    for i in 0..(decay * 2.5 * rate) as usize {
        let Some(sample) = buffer.get_mut(start + i) else { break };
        let age = i as f32 / rate;
        let f = pitch * (1. + 1.2 * (-age / 0.03).exp());
        phase = (phase + f / rate).fract();
        low += (rng.next() - low) * 0.08;
        let env = (age / 0.004).min(1.) * (-age / (decay * 0.45)).exp();
        *sample += ((TAU * phase).sin() * 0.8 + low * 0.9) * env * gain;
    }
}

fn add_tick(buffer: &mut [f32], rate: f32, at: f32, gain: f32, rng: &mut Rng) {
    let start = (at * rate) as usize;
    let mut previous = 0f32;
    for i in 0..(0.08 * rate) as usize {
        let Some(sample) = buffer.get_mut(start + i) else { break };
        let age = i as f32 / rate;
        let noise = rng.next();
        let high = noise - previous;
        previous = noise;
        let env = (age / 0.001).min(1.) * (-age / 0.012).exp();
        *sample += high * env * gain;
    }
}

fn add_bell(buffer: &mut [f32], rate: f32, at: f32, frequency: f32, gain: f32) {
    let start = (at * rate) as usize;
    for i in 0..(2.4 * rate) as usize {
        let Some(sample) = buffer.get_mut(start + i) else { break };
        let age = i as f32 / rate;
        let env = (age / 0.003).min(1.) * (-age / 0.7).exp();
        let partials = (TAU * frequency * age).sin()
            + 0.35 * (TAU * frequency * 2.01 * age).sin() * (-age / 0.25).exp()
            + 0.08 * (TAU * frequency * 2.76 * age).sin() * (-age / 0.08).exp();
        *sample += partials * env * gain;
    }
}

/// A small Schroeder reverb: parallel combs into series allpasses.
fn reverb(dry: &[f32], rate: f32) -> Vec<f32> {
    let scale = rate / 44100.;
    let combs: Vec<usize> = [1557, 1617, 1491, 1422, 1277, 1356]
        .iter()
        .map(|d| ((*d as f32 * scale) as usize).max(1))
        .collect();
    let allpasses: Vec<usize> = [556, 441, 341]
        .iter()
        .map(|d| ((*d as f32 * scale) as usize).max(1))
        .collect();
    let mut comb_buffers: Vec<Vec<f32>> = combs.iter().map(|d| vec![0.; *d]).collect();
    let mut comb_damp = vec![0f32; combs.len()];
    let mut ap_buffers: Vec<Vec<f32>> = allpasses.iter().map(|d| vec![0.; *d]).collect();
    let mut out = Vec::with_capacity(dry.len());
    for (index, input) in dry.iter().enumerate() {
        let mut wet = 0.;
        for (c, buffer) in comb_buffers.iter_mut().enumerate() {
            let slot = index % buffer.len();
            let delayed = buffer[slot];
            comb_damp[c] = delayed * 0.6 + comb_damp[c] * 0.4;
            buffer[slot] = input * 0.3 + comb_damp[c] * 0.86;
            wet += delayed;
        }
        wet /= combs.len() as f32;
        for buffer in ap_buffers.iter_mut() {
            let slot = index % buffer.len();
            let delayed = buffer[slot];
            buffer[slot] = wet + delayed * 0.5;
            wet = delayed - wet * 0.5;
        }
        out.push(input * 0.7 + wet * 1.6);
    }
    out
}
