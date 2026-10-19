//! `Audio` — sound effects and one looping music track behind a single mute, plus `wav`, which wraps synthesised samples so macroquad can load them (feature `audio`).
//!
//! For placing sounds in a world: [`Panned`] bakes a sound at a few stereo
//! positions (macroquad has no live panning), and [`Voices`] caps how many
//! play at once.
//!
//! The `audio` feature turns on macroquad's own `audio` feature. Without it
//! every macroquad sound call compiles and is a silent stub, which is easy to
//! mistake for a quiet sound.
//!
//! Sounds can come from files (`macroquad::audio::load_sound`) or from
//! samples a game generates: [`load_pcm`] takes mono `f32` samples and a
//! rate, so a spike can have a groove and effects with no asset files.
//!
//! ## In the browser
//!
//! - The page must load quad-snd's `audio.js`, or every `audio_*` import is
//!   stubbed and the game is silent: stamp it with
//!   [`crate::web::stamp`]`(dir, &[Plugin::Audio])`.
//! - Browsers start audio suspended. `audio.js` resumes it on the first
//!   click, touch or key press, so music started at load is heard from then
//!   (the console's "AudioContext was not allowed to start" is that, and
//!   expected).
//!
//! Nothing here reads the clock.

use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, set_sound_volume, stop_sound,
};

/// Mono `f32` samples (clamped to -1..1) as a 16-bit PCM WAV file.
pub fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    wav_channels(samples, 1, rate)
}

/// Interleaved `f32` samples (left, right, left, ...) as a 16-bit stereo PCM
/// WAV file.
pub fn wav_stereo(interleaved: &[f32], rate: u32) -> Vec<u8> {
    wav_channels(interleaved, 2, rate)
}

fn wav_channels(samples: &[f32], channels: u16, rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let block = channels * 2;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(block)).to_le_bytes()); // byte rate
    out.extend_from_slice(&block.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// Load generated samples as a [`Sound`]. On the web this waits a frame or
/// more for the browser to decode it.
pub async fn load_pcm(samples: &[f32], rate: u32) -> Result<Sound, macroquad::Error> {
    load_sound_from_bytes(&wav(samples, rate)).await
}

/// Left and right gains for `pan` (-1 is hard left, 1 hard right), by the
/// equal-power law: a sound keeps its loudness as it moves across. The centre
/// is 1/sqrt(2) a side.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Mono samples panned to `pan`, interleaved for [`wav_stereo`].
pub fn pan_mono(samples: &[f32], pan: f32) -> Vec<f32> {
    let (l, r) = pan_gains(pan);
    samples.iter().flat_map(|s| [s * l, s * r]).collect()
}

/// The pans `positions` variants are baked at, evenly left to right. Always
/// odd, so one is the centre.
fn pans(positions: usize) -> Vec<f32> {
    let n = positions.max(1) | 1;
    if n == 1 {
        return vec![0.0];
    }
    (0..n)
        .map(|i| -1.0 + 2.0 * i as f32 / (n - 1) as f32)
        .collect()
}

/// The variant nearest `pan`, of `count` spread as [`pans`] spreads them.
fn nearest(pan: f32, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let at = (pan.clamp(-1.0, 1.0) + 1.0) / 2.0 * (count - 1) as f32;
    (at.round() as usize).min(count - 1)
}

/// A mono sound baked at a few stereo positions, for placing it left or
/// right of the listener.
///
/// macroquad cannot pan a sound as it plays (`PlaySoundParams` is looping and
/// volume, nothing else), but it plays stereo samples as they are, natively
/// and in the browser. So the panning is baked: [`Audio::play_panned`] picks
/// the variant nearest the pan asked for. A sound keeps its place once
/// started, which suits short effects; a long one that should follow its
/// source needs a real panner.
pub struct Panned {
    variants: Vec<Sound>,
    seconds: f32,
}

impl Panned {
    /// Bake `samples` at `positions` pans (made odd, so one is the centre).
    /// Five is plenty for a 2D screen.
    pub async fn load(
        samples: &[f32],
        rate: u32,
        positions: usize,
    ) -> Result<Self, macroquad::Error> {
        let mut variants = Vec::new();
        for pan in pans(positions) {
            variants.push(load_sound_from_bytes(&wav_stereo(&pan_mono(samples, pan), rate)).await?);
        }
        Ok(Self {
            variants,
            seconds: samples.len() as f32 / rate as f32,
        })
    }

    /// How long the sound plays.
    pub fn seconds(&self) -> f32 {
        self.seconds
    }
}

/// A cap on how many sounds play at once, so a crowd cannot flood the mix:
/// past the cap, a new sound is dropped rather than piled on. A sound counts
/// until its length has passed.
///
/// It reads no clock: `now` is any steadily rising time in seconds, the
/// frame's clock for one.
pub struct Voices {
    max: usize,
    ending: Vec<f32>,
}

impl Voices {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            ending: Vec::new(),
        }
    }

    /// Whether a sound `seconds` long may start at `now`, and if so, count it.
    pub fn admit(&mut self, now: f32, seconds: f32) -> bool {
        self.ending.retain(|end| *end > now);
        if self.ending.len() >= self.max {
            return false;
        }
        self.ending.push(now + seconds);
        true
    }

    /// Sounds counted as playing at `now`.
    pub fn playing(&mut self, now: f32) -> usize {
        self.ending.retain(|end| *end > now);
        self.ending.len()
    }
}

/// Effects and a music track behind one mute.
pub struct Audio {
    muted: bool,
    music: Option<(Sound, f32)>,
}

impl Audio {
    pub fn new(muted: bool) -> Self {
        Self { muted, music: None }
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    /// Play an effect once, unless muted.
    pub fn play(&self, sound: &Sound, volume: f32) {
        if !self.muted {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
        }
    }

    /// Play a [`Panned`] effect once at `pan` (-1 left to 1 right), unless
    /// muted. At the centre it is as loud as the mono sound at `volume`.
    pub fn play_panned(&self, sound: &Panned, volume: f32, pan: f32) {
        let i = nearest(pan, sound.variants.len());
        // The equal-power centre is 1/sqrt(2) a side; a mono sound plays at
        // full on both.
        self.play(&sound.variants[i], volume * std::f32::consts::SQRT_2);
    }

    /// Loop `sound` as the music, replacing any music playing. It starts even
    /// when muted (silently), so unmuting brings it in where it would be.
    pub fn play_music(&mut self, sound: Sound, volume: f32) {
        self.stop_music();
        play_sound(
            &sound,
            PlaySoundParams {
                looped: true,
                volume: if self.muted { 0.0 } else { volume },
            },
        );
        self.music = Some((sound, volume));
    }

    pub fn stop_music(&mut self) {
        if let Some((sound, _)) = self.music.take() {
            stop_sound(&sound);
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if let Some((sound, volume)) = &self.music {
            set_sound_volume(sound, if muted { 0.0 } else { *volume });
        }
    }

    pub fn toggle_mute(&mut self) {
        self.set_muted(!self.muted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_describes_the_data() {
        let w = wav(&[0.0; 100], 22_050);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(w.len(), 44 + 200);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 22_050);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 200);
    }

    #[test]
    fn wav_clamps_and_scales() {
        let w = wav(&[2.0, -2.0, 0.5], 8_000);
        let sample = |i: usize| i16::from_le_bytes([w[44 + i * 2], w[45 + i * 2]]);
        assert_eq!(sample(0), i16::MAX);
        assert_eq!(sample(1), -i16::MAX);
        assert_eq!(sample(2), i16::MAX / 2);
    }

    #[test]
    fn stereo_wav_describes_two_channels() {
        let w = wav_stereo(&[0.0; 100], 22_050);
        assert_eq!(u16::from_le_bytes(w[22..24].try_into().unwrap()), 2);
        assert_eq!(
            u32::from_le_bytes(w[28..32].try_into().unwrap()),
            22_050 * 4
        );
        assert_eq!(u16::from_le_bytes(w[32..34].try_into().unwrap()), 4);
        assert_eq!(w.len(), 44 + 200);
    }

    #[test]
    fn panning_keeps_the_power_and_puts_the_sound_on_its_side() {
        for pan in [-1.0, -0.5, 0.0, 0.3, 1.0] {
            let (l, r) = pan_gains(pan);
            assert!((l * l + r * r - 1.0).abs() < 1e-5, "{pan}: {l} {r}");
        }
        let (l, r) = pan_gains(-1.0);
        assert!(l > 0.999 && r < 1e-5);
        let (l, r) = pan_gains(0.0);
        assert!((l - r).abs() < 1e-6);
        assert_eq!(pan_mono(&[1.0], 1.0).len(), 2);
    }

    #[test]
    fn variants_spread_evenly_and_the_nearest_is_picked() {
        assert_eq!(pans(5), vec![-1.0, -0.5, 0.0, 0.5, 1.0]);
        assert_eq!(pans(4).len(), 5, "made odd, so one is the centre");
        assert_eq!(pans(0), vec![0.0]);
        assert_eq!(nearest(0.0, 5), 2);
        assert_eq!(nearest(-0.8, 5), 0);
        assert_eq!(nearest(0.3, 5), 3);
        assert_eq!(nearest(9.0, 5), 4);
        assert_eq!(nearest(0.7, 1), 0);
    }

    #[test]
    fn voices_cap_what_plays_at_once() {
        let mut v = Voices::new(2);
        assert!(v.admit(0.0, 1.0));
        assert!(v.admit(0.1, 0.5));
        assert!(!v.admit(0.2, 1.0), "full");
        assert_eq!(v.playing(0.2), 2);
        assert!(v.admit(0.7, 1.0), "the short one ended");
        assert_eq!(v.playing(1.05), 1);
    }
}
