//! `SpriteFile` — a sprite as a text file: its rows, palette, gait, scale and body motion in one place, so a game embeds it and `sprite-view` reviews it, with no game built.
//!
//! ```text
//! # A humpback. Comments start with #; blank lines are ignored.
//! name: whale
//! scale: 1.3                          # design units an art pixel
//! palette: b=16294f B=3a66b4 L=c3d6ee # char=RRGGBB; more palette: lines add more
//! gait: 0*1.3 1 2 3 4*1.3 5 6 7       # frames in cycle order, *hold (default 1)
//! period: 1.3                         # seconds a cycle takes
//! jitter: 0.12                        # each clone's rate strays this far either way
//! bob: 0.58                           # art px the body sinks with the stroke
//! surge: 0.77                         # art px it is pushed forward
//! backdrop: 0d1b2a                    # what the viewer draws it over
//!
//! frame flukes up                     # a frame; the rest of the line is a label
//! ....bbb....
//! ...bBBBb...
//!
//! frame level
//! ...
//! ```
//!
//! The header is flat `key: value`, every key optional but `name`. Rows
//! use the palette's characters and `.` for transparent; a row's leading
//! and trailing spaces are ignored, so rows may be indented. `#` is never a
//! palette character, so a comment between rows is a comment. A sprite with
//! several frames and no `gait:` plays them evenly.
//!
//! A game embeds the file with [`sprite_file!`](crate::sprite_file!), which
//! also records where it lives, so the viewer can write tuned values back to
//! the source ([`SpriteFile::retuned`]): only the header lines it changes,
//! the art and comments untouched. Pure: no window.

use crate::film::Body;
use crate::gait::Gait;
use crate::pixel::{Grid, Palette, PixelError};

/// Embed a sprite file from a path relative to the crate's manifest, and
/// record that path so the viewer can write tuned values back to it.
///
/// ```ignore
/// static WHALE: LazyLock<SpriteFile> = LazyLock::new(|| sprite_file!("sprites/whale.sprite"));
/// ```
#[macro_export]
macro_rules! sprite_file {
    ($path:literal) => {
        $crate::sprite_file::SpriteFile::embedded(
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)),
            concat!(env!("CARGO_MANIFEST_DIR"), "/", $path),
        )
    };
}

/// A sprite read from text.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteFile {
    pub name: String,
    /// Design units an art pixel is drawn at.
    pub scale: f32,
    /// `(char, 0xRRGGBB)` in the order given.
    pub colours: Vec<(char, u32)>,
    /// Each frame's label (the rest of its `frame` line) and rows.
    pub frames: Vec<(String, Vec<String>)>,
    /// `(frame, hold)` in cycle order, if a `gait:` line gave them.
    pub steps: Option<Vec<(usize, f32)>>,
    pub period: f32,
    pub jitter: f32,
    pub bob: f32,
    pub surge: f32,
    /// The viewer's background, `0xRRGGBB`.
    pub backdrop: Option<u32>,
    /// Where the file lives, when known: what the viewer writes back to.
    pub path: Option<String>,
}

/// Why a sprite file did not read, with the line it is about.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteFileError {
    /// 1-based; 0 when the problem is the whole file.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for SpriteFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for SpriteFileError {}

fn err(line: usize, message: impl Into<String>) -> SpriteFileError {
    SpriteFileError {
        line,
        message: message.into(),
    }
}

/// A line with any `#` comment removed, trimmed.
fn uncommented(line: &str) -> &str {
    line.split('#').next().unwrap_or("").trim()
}

impl SpriteFile {
    /// Read a sprite from `text`.
    pub fn parse(text: &str) -> Result<Self, SpriteFileError> {
        let mut out = Self {
            name: String::new(),
            scale: 1.0,
            colours: Vec::new(),
            frames: Vec::new(),
            steps: None,
            period: 1.0,
            jitter: 0.0,
            bob: 0.0,
            surge: 0.0,
            backdrop: None,
            path: None,
        };
        let num = |n: usize, v: &str| {
            v.parse::<f32>()
                .map_err(|_| err(n, format!("`{v}` is not a number")))
        };
        let hex = |n: usize, v: &str| {
            u32::from_str_radix(v.trim_start_matches('#'), 16)
                .ok()
                .filter(|_| v.trim_start_matches('#').len() == 6)
                .ok_or_else(|| err(n, format!("`{v}` is not a colour (RRGGBB)")))
        };
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(label) = trimmed.strip_prefix("frame")
                && (label.is_empty() || label.starts_with(' '))
            {
                out.frames.push((uncommented(label).to_string(), Vec::new()));
                continue;
            }
            if let Some((_, rows)) = out.frames.last_mut() {
                rows.push(uncommented(raw).to_string());
                continue;
            }
            let line = uncommented(raw);
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| err(n, format!("expected `key: value` or `frame`, got `{line}`")))?;
            let value = value.trim();
            match key.trim() {
                "name" => out.name = value.to_string(),
                "scale" => out.scale = num(n, value)?,
                "period" => out.period = num(n, value)?.max(1e-3),
                "jitter" => out.jitter = num(n, value)?,
                "bob" => out.bob = num(n, value)?,
                "surge" => out.surge = num(n, value)?,
                "backdrop" => out.backdrop = Some(hex(n, value)?),
                "palette" => {
                    for entry in value.split_whitespace() {
                        let (c, rgb) = entry
                            .split_once('=')
                            .ok_or_else(|| err(n, format!("`{entry}`: expected char=RRGGBB")))?;
                        let mut chars = c.chars();
                        let (Some(c), None) = (chars.next(), chars.next()) else {
                            return Err(err(n, format!("`{entry}`: one character before =")));
                        };
                        if matches!(c, '.' | ' ' | '#') {
                            return Err(err(n, format!("`{c}` cannot be a palette character")));
                        }
                        out.colours.retain(|(k, _)| *k != c);
                        out.colours.push((c, hex(n, rgb)?));
                    }
                }
                "gait" => {
                    let mut steps = Vec::new();
                    for step in value.split_whitespace() {
                        let (f, w) = step.split_once('*').unwrap_or((step, "1"));
                        let f = f
                            .parse::<usize>()
                            .map_err(|_| err(n, format!("`{step}`: expected FRAME or FRAME*HOLD")))?;
                        steps.push((f, num(n, w)?));
                    }
                    out.steps = Some(steps);
                }
                other => return Err(err(n, format!("unknown key `{other}`"))),
            }
        }
        if out.name.is_empty() {
            return Err(err(0, "no `name:` line"));
        }
        if out.frames.is_empty() {
            return Err(err(0, "no frames"));
        }
        if let Some(steps) = &out.steps
            && let Some((f, _)) = steps.iter().find(|(f, _)| *f >= out.frames.len())
        {
            return Err(err(
                0,
                format!("the gait plays frame {f}, but there are {}", out.frames.len()),
            ));
        }
        // Drop blank rows a frame ends with (the gap before the next one).
        for (_, rows) in &mut out.frames {
            while rows.last().is_some_and(|r| r.is_empty()) {
                rows.pop();
            }
        }
        out.grids()
            .map_err(|e| err(0, format!("{}: {e}", out.name)))?;
        Ok(out)
    }

    /// Read a sprite embedded from `path`: see [`sprite_file!`](crate::sprite_file!).
    /// Panics on a bad file, which is a build-time mistake in the game.
    pub fn embedded(text: &str, path: &str) -> Self {
        let mut s = Self::parse(text).unwrap_or_else(|e| panic!("{path}: {e}"));
        s.path = Some(path.to_string());
        s
    }

    /// Read a sprite file from disk.
    pub fn load(path: &str) -> Result<Self, SpriteFileError> {
        let text = std::fs::read_to_string(path).map_err(|e| err(0, format!("{path}: {e}")))?;
        let mut s = Self::parse(&text).map_err(|e| SpriteFileError {
            line: e.line,
            message: format!("{path}: {}", e.message),
        })?;
        s.path = Some(path.to_string());
        Ok(s)
    }

    /// The palette the file gives.
    pub fn palette(&self) -> Palette {
        self.colours
            .iter()
            .fold(Palette::new(), |p, (c, rgb)| p.with(*c, *rgb))
    }

    /// The frames, in the file's palette.
    pub fn grids(&self) -> Result<Vec<Grid>, PixelError> {
        self.grids_in(&self.palette())
    }

    /// The frames with some colours swapped: `&[('f', flag)]` flies a
    /// ship's flag in its pool's colour.
    pub fn grids_with(&self, swaps: &[(char, u32)]) -> Result<Vec<Grid>, PixelError> {
        let palette = swaps.iter().fold(self.palette(), |p, (c, rgb)| p.with(*c, *rgb));
        self.grids_in(&palette)
    }

    fn grids_in(&self, palette: &Palette) -> Result<Vec<Grid>, PixelError> {
        let frames: Vec<Vec<&str>> = self
            .frames
            .iter()
            .map(|(_, rows)| rows.iter().map(String::as_str).collect())
            .collect();
        let refs: Vec<&[&str]> = frames.iter().map(Vec::as_slice).collect();
        crate::pixel::parse_frames(&refs, palette)
    }

    /// How the frames play: the file's gait, or every frame evenly.
    pub fn gait(&self) -> Gait {
        match &self.steps {
            Some(steps) => Gait::new(steps, self.period),
            None => Gait::even(self.frames.len(), self.period),
        }
        .jitter(self.jitter)
    }

    /// How the body moves with the gait, in art pixels.
    pub fn body(&self) -> Body {
        Body {
            bob: self.bob,
            surge: self.surge,
        }
    }

    /// `text` (this sprite's file) with its gait, period, jitter, bob and
    /// surge set to these: each header line replaced where it is, added
    /// after `name:` where it was missing, and nothing else touched.
    pub fn retuned(text: &str, gait: &Gait, bob: f32, surge: f32) -> String {
        let steps: Vec<String> = gait
            .steps()
            .iter()
            .map(|(f, w)| {
                if (*w - 1.0).abs() < 1e-4 {
                    format!("{f}")
                } else {
                    format!("{f}*{}", trim(*w))
                }
            })
            .collect();
        let values = [
            ("gait", steps.join(" ")),
            ("period", trim(gait.period())),
            ("jitter", trim(gait.spread())),
            ("bob", trim(bob)),
            ("surge", trim(surge)),
        ];
        let mut lines: Vec<String> = Vec::new();
        let mut done = [false; 5];
        let mut in_frames = false;
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("frame") {
                in_frames = true;
            }
            let key = (!in_frames)
                .then(|| uncommented(line).split_once(':').map(|(k, _)| k.trim()))
                .flatten();
            match values.iter().position(|(k, _)| Some(*k) == key) {
                Some(i) => {
                    let comment = line.find('#').map_or(String::new(), |at| {
                        format!("  {}", line[at..].trim_end())
                    });
                    lines.push(format!("{}: {}{comment}", values[i].0, values[i].1));
                    done[i] = true;
                }
                None => lines.push(line.to_string()),
            }
        }
        // Keys the file did not have go after `name:`.
        let at = lines
            .iter()
            .position(|l| uncommented(l).starts_with("name:"))
            .map_or(0, |i| i + 1);
        let missing: Vec<String> = values
            .iter()
            .zip(done)
            .filter(|(_, d)| !d)
            .map(|((k, v), _)| format!("{k}: {v}"))
            .collect();
        for (k, line) in missing.into_iter().enumerate() {
            lines.insert(at + k, line);
        }
        let mut out = lines.join("\n");
        if text.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// A number without trailing zeros: `1.30` is `1.3`, `2.00` is `2`.
fn trim(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAIL: &str = "\
# a test creature
name: tail
scale: 1.5
palette: b=16294f B=3a66b4
gait: 0*1.3 1 0
period: 1.3   # a slow stroke
bob: 0.5

frame up
bB.
...

# between frames
frame down
...
bB.
";

    #[test]
    fn a_file_reads_into_frames_a_gait_and_a_body() {
        let s = SpriteFile::parse(TAIL).expect("reads");
        assert_eq!(s.name, "tail");
        assert_eq!(s.scale, 1.5);
        assert_eq!(s.frames.len(), 2);
        assert_eq!(s.frames[0].0, "up");
        assert_eq!(s.frames[0].1, vec!["bB.", "..."]);
        let grids = s.grids().expect("parses");
        assert_eq!((grids[0].w, grids[0].h), (3, 2));
        assert_eq!(grids[0].at(0, 0), Some([0x16, 0x29, 0x4f, 255]));
        assert_eq!(s.gait().steps(), &[(0, 1.3), (1, 1.0), (0, 1.0)]);
        assert_eq!(s.gait().period(), 1.3);
        assert_eq!(s.body().bob, 0.5);
        let swapped = s.grids_with(&[('b', 0xff0000)]).expect("parses");
        assert_eq!(swapped[0].at(0, 0), Some([255, 0, 0, 255]));
    }

    #[test]
    fn mistakes_name_their_line() {
        let bad = |t: &str| SpriteFile::parse(t).expect_err("should not read");
        assert_eq!(bad("name: x\nscale: big\nframe\nb\n").line, 2);
        assert_eq!(bad("name: x\nwobble: 1\nframe\nb\n").line, 2);
        assert!(bad("name: x\npalette: b=123\nframe\nb\n").message.contains("colour"));
        assert!(bad("name: x\nframe\nz\n").message.contains("not in the palette"));
        assert!(bad("name: x\npalette: b=000000\ngait: 0 3\nframe\nb\n").message.contains("frame 3"));
        assert!(bad("scale: 1\nframe\n.\n").message.contains("name"));
    }

    /// Writing tuned values back changes the header lines and nothing else.
    #[test]
    fn retuning_rewrites_only_the_header_values() {
        let gait = Gait::new(&[(0, 1.5), (1, 1.0)], 0.9).jitter(0.2);
        let out = SpriteFile::retuned(TAIL, &gait, 0.75, 1.0);
        assert!(out.contains("gait: 0*1.5 1\n"), "{out}");
        assert!(out.contains("period: 0.9  # a slow stroke\n"), "{out}");
        assert!(out.contains("bob: 0.75\n"), "{out}");
        // Missing keys go after the name.
        assert!(out.contains("name: tail\njitter: 0.2\nsurge: 1\n"), "{out}");
        // The art and comments are untouched.
        assert!(out.contains("# between frames\nframe down\n...\nbB.\n"), "{out}");
        let again = SpriteFile::parse(&out).expect("still reads");
        assert_eq!(again.gait().steps(), &[(0, 1.5), (1, 1.0)]);
        assert_eq!(again.surge, 1.0);
    }
}
