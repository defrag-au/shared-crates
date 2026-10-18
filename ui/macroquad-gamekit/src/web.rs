//! `web` — the miniquad JS plugins the kit's `audio` and `gamepads` features need in the page, shipped with the kit and stamped by a game's `build.rs`.
//!
//! macroquad reaches the browser only through miniquad's plugin protocol: the
//! wasm imports plain functions, and a JS file loaded before `load(...)`
//! provides them. Leave one out and nothing fails to load — `gl.js` stubs
//! each missing import and logs "No audio_init function in gl.js" on every
//! call, and the game is silent or deaf to its pads. So the kit carries the
//! files, and a test (`tests/web_plugins.rs`) pins each one byte for byte to
//! the version of its crate that Cargo resolves.
//!
//! The JS needs no feature: a game's build script stamps what it uses,
//! without compiling quad-snd or gilrs for the host.
//!
//! ```ignore
//! // build.rs — build-dependency: macroquad-gamekit, no features
//! use macroquad_gamekit::web::{Plugin, stamp};
//! stamp(std::path::Path::new("web"), &[Plugin::Audio, Plugin::Gamepads])
//!     .expect("stamp web plugins");
//! ```
//!
//! Then load every file in `web/` after `gl.js` (augminted-bots:
//! `scripts/stage-macroquad-client.sh --web <crate>/web`).
//!
//! The gamepads plugin declares its version as the string `"0.1.7"`, while
//! `gl.js` compares it with the packed integer the crate exports, so the
//! verbatim file always logs a "version mismatch" error. [`Plugin::js`]
//! writes the packed number instead; nothing else changes.

use std::path::{Path, PathBuf};

/// A miniquad plugin one of the kit's features needs in the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plugin {
    /// quad-snd's `audio.js`, for the `audio` feature.
    Audio,
    /// The `gamepads` crate's macroquad plugin, for the `gamepads` feature.
    Gamepads,
}

impl Plugin {
    pub const ALL: [Plugin; 2] = [Plugin::Audio, Plugin::Gamepads];

    /// The file name it is stamped as.
    pub fn file_name(self) -> &'static str {
        match self {
            Plugin::Audio => "gamekit-audio.js",
            Plugin::Gamepads => "gamekit-gamepads.js",
        }
    }

    /// The crate the file comes from, and its path inside that crate.
    pub fn origin(self) -> (&'static str, &'static str) {
        match self {
            Plugin::Audio => ("quad-snd", "js/audio.js"),
            Plugin::Gamepads => ("gamepads", "js/macroquad-gamepads-0.1.js"),
        }
    }

    /// The file exactly as its crate ships it.
    pub fn source(self) -> &'static str {
        match self {
            Plugin::Audio => include_str!("../js/audio.js"),
            Plugin::Gamepads => include_str!("../js/gamepads.js"),
        }
    }

    /// What to put in the page: [`Plugin::source`], with the gamepads
    /// version written as the packed number `gl.js` compares it with.
    pub fn js(self) -> String {
        let src = self.source();
        match self {
            Plugin::Audio => src.to_owned(),
            Plugin::Gamepads => pack_version_field(src),
        }
    }
}

/// `major.minor.patch` packed the way miniquad's plugin version check reads
/// a crate's `<name>_crate_version` export: `major << 24 | minor << 16 | patch`.
pub fn packed_version(version: &str) -> Option<u32> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    (parts.next().is_none() && major < 256 && minor < 256 && patch < 65536)
        .then_some((major << 24) + (minor << 16) + patch)
}

/// Replace the first `version:"x.y.z"` with its packed number. Unchanged if
/// there is no such field, or it does not parse.
fn pack_version_field(src: &str) -> String {
    const KEY: &str = "version:\"";
    let Some(start) = src.find(KEY) else {
        return src.to_owned();
    };
    let value_at = start + KEY.len();
    let Some(len) = src[value_at..].find('"') else {
        return src.to_owned();
    };
    match packed_version(&src[value_at..value_at + len]) {
        Some(n) => format!("{}version:{n}{}", &src[..start], &src[value_at + len + 1..]),
        None => src.to_owned(),
    }
}

/// Write each plugin into `dir` (created if missing), returning the paths.
/// Rewrites a file only when its content changed, so a build script calling
/// this does not dirty the tree on every build.
pub fn stamp(dir: &Path, plugins: &[Plugin]) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    plugins
        .iter()
        .map(|p| {
            let path = dir.join(p.file_name());
            let js = p.js();
            if std::fs::read_to_string(&path).ok().as_deref() != Some(js.as_str()) {
                std::fs::write(&path, js)?;
            }
            Ok(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_like_the_crate_export() {
        // gamepads 0.1.7 exports 65543 from `gamepads_crate_version`.
        assert_eq!(packed_version("0.1.7"), Some(65543));
        assert_eq!(packed_version("1.2.3"), Some((1 << 24) + (2 << 16) + 3));
        assert_eq!(packed_version("0.1"), None);
        assert_eq!(packed_version("0.1.x"), None);
    }

    #[test]
    fn the_gamepads_plugin_is_stamped_with_a_packed_version() {
        let js = Plugin::Gamepads.js();
        assert!(js.contains("version:65543"), "{}", &js[..80]);
        assert!(!js.contains("version:\"0.1.7\""));
        // Only that field changes.
        assert_eq!(
            js.len() + "\"0.1.7\"".len() - "65543".len(),
            Plugin::Gamepads.source().len()
        );
    }

    #[test]
    fn the_audio_plugin_is_stamped_verbatim() {
        assert_eq!(Plugin::Audio.js(), Plugin::Audio.source());
    }

    #[test]
    fn stamp_writes_every_plugin_once() {
        let dir = std::env::temp_dir().join(format!("gamekit-web-{}", std::process::id()));
        let paths = stamp(&dir, &Plugin::ALL).expect("stamp");
        assert_eq!(paths.len(), 2);
        for (p, plugin) in paths.iter().zip(Plugin::ALL) {
            assert_eq!(std::fs::read_to_string(p).expect("read"), plugin.js());
        }
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}
