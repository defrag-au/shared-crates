//! `Audio` — sound effects and one looping music track behind a single mute, plus `wav`, which wraps synthesised samples so macroquad can load them (feature `audio`).
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
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
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
}
