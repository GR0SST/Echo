//! Renders the welcome cue to a mono 32-bit float WAV. The shipped asset is
//! made from it with libvorbis:
//!
//! ```sh
//! cargo run --release -p engine --example render_welcome_music /tmp/welcome.wav
//! sox /tmp/welcome.wav -C 6 crates/engine/assets/welcome.ogg
//! ```
use std::io::Write;

const RATE: u32 = 48_000;

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).expect("output .wav path");
    let samples = engine::render_welcome_music(RATE);
    let data = (samples.len() * 4) as u32;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&3u16.to_le_bytes())?; // IEEE float
    out.write_all(&1u16.to_le_bytes())?; // mono
    out.write_all(&RATE.to_le_bytes())?;
    out.write_all(&(RATE * 4).to_le_bytes())?;
    out.write_all(&4u16.to_le_bytes())?;
    out.write_all(&32u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data.to_le_bytes())?;
    for sample in samples {
        out.write_all(&sample.to_le_bytes())?;
    }
    out.flush()
}
