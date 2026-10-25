//! `Viewer` — a window for reviewing a game's sprites by eye, moving: one sprite looping large and at real size against guides, a crowd of its clones each on its own phase, and keys to tune the gait and body live and print the lines to paste back.
//!
//! `game --view` (or `game --view whale`) opens it in place of the game, on
//! the sprites the game lists in [`crate::Game::sprites`]. It runs through the
//! kit's loop like any game, so `--shot` and `--strip` capture it too: the
//! capture's note carries the sprite, its gait as tuned, and the motion
//! problems [`crate::motion::Cycle`] finds, so a review that reads the report
//! sees what the eye sees on screen.
//!
//! | Key | Does |
//! |---|---|
//! | Left / Right | the previous or next sprite |
//! | Space | pause; `,` and `.` step a display frame while paused |
//! | Z | quarter speed |
//! | F | face the other way |
//! | Up / Down | the cycle 10% slower or faster |
//! | `[` / `]` | select a step of the gait; `-` / `=` shorten or lengthen its hold |
//! | J / K | less or more jitter between clones |
//! | B / N | less or more bob; S / D less or more surge |
//! | R | back to the game's values |
//! | W | write the tuned values into the sprite's file (a sprite read from one) |
//! | P | print the tuned `Gait` and `.body` lines to paste into the game |
//!
//! A sprite read from a file ([`crate::sprite_file`]) is watched: save the
//! file in an editor and the viewer reloads it, art and all, so a sprite is
//! drawn and judged in one loop. `W` writes only the header values it
//! tunes; the art and comments are untouched.

use macroquad::prelude::*;

use crate::app::{Backdrop, Frame, Game};
use crate::gait::Gait;
use crate::harness::SheetSprite;
use crate::motion::Cycle;
use crate::pixel::PixelSprite;

/// The viewer's stage, in design units.
pub const SIZE: Vec2 = Vec2::new(1280.0, 800.0);
/// Clones in the crowd.
const CROWD: usize = 10;

/// A gait and body being tuned.
#[derive(Clone, Debug, PartialEq)]
struct Tune {
    steps: Vec<(usize, f32)>,
    period: f32,
    spread: f32,
    bob: f32,
    surge: f32,
}

impl Tune {
    fn of(s: &SheetSprite) -> Self {
        let gait = s.gait.clone().unwrap_or_else(|| Gait::even(1, 1.0));
        Self {
            steps: gait.steps().to_vec(),
            period: gait.period(),
            spread: gait.spread(),
            bob: s.body.bob,
            surge: s.body.surge,
        }
    }

    fn gait(&self) -> Gait {
        Gait::new(&self.steps, self.period).jitter(self.spread)
    }

    /// The lines to paste back into the game.
    fn lines(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|(f, w)| format!("({f}, {w:.2})"))
            .collect();
        format!(
            "Gait::new(&[{}], {:.2}).jitter({:.2})\n.body({:.2}, {:.2})",
            steps.join(", "),
            self.period,
            self.spread,
            self.bob,
            self.surge
        )
    }
}

/// The sprite viewer: a [`Game`] the loop runs in place of the game.
pub struct Viewer {
    sprites: Vec<SheetSprite>,
    tunes: Vec<Tune>,
    /// Uploaded on the first draw: every frame, then every frame flipped.
    baked: Vec<Option<PixelSprite>>,
    at: usize,
    step: usize,
    t: f32,
    paused: bool,
    slow: bool,
    left: bool,
    backdrop: Backdrop,
    /// Each sprite's file's last modified time, for reloading.
    stamps: Vec<Option<std::time::SystemTime>>,
    /// Seconds until the files are next checked.
    poll: f32,
    /// A message, and the seconds it stays up.
    status: Option<(String, f32)>,
}

/// A file's last modified time, if it has one (never on the web).
fn stamp(path: Option<&str>) -> Option<std::time::SystemTime> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    std::fs::metadata(path?).and_then(|m| m.modified()).ok()
}

impl Viewer {
    /// A viewer on `sprites`, starting at the one named `start` if given.
    pub fn new(sprites: Vec<SheetSprite>, backdrop: Backdrop, start: Option<&str>) -> Self {
        let at = start
            .and_then(|n| sprites.iter().position(|s| s.name == n))
            .unwrap_or(0);
        Self {
            tunes: sprites.iter().map(Tune::of).collect(),
            baked: sprites.iter().map(|_| None).collect(),
            stamps: sprites.iter().map(|s| stamp(s.source.as_deref())).collect(),
            sprites,
            at,
            step: 0,
            t: 0.0,
            paused: false,
            slow: false,
            left: false,
            backdrop,
            poll: 0.5,
            status: None,
        }
    }

    fn say(&mut self, message: String) {
        println!("{message}");
        self.status = Some((message, 4.0));
    }

    /// Reload any sprite whose file changed on disk: its art, and its tune
    /// from the file's values.
    fn reload(&mut self) {
        for i in 0..self.sprites.len() {
            let path = self.sprites[i].source.clone();
            let now = stamp(path.as_deref());
            if now.is_none() || now == self.stamps[i] {
                continue;
            }
            self.stamps[i] = now;
            let path = path.unwrap_or_default();
            match crate::sprite_file::SpriteFile::load(&path) {
                Ok(file) => {
                    self.sprites[i] = SheetSprite::from_file(&file);
                    self.tunes[i] = Tune::of(&self.sprites[i]);
                    self.baked[i] = None;
                    self.step = 0;
                    self.say(format!("reloaded {path}"));
                }
                Err(e) => self.say(format!("not reloaded: {e}")),
            }
        }
    }

    /// Write the current sprite's tuned values into its file.
    fn write_back(&mut self) {
        let s = &self.sprites[self.at];
        let Some(path) = s.source.clone() else {
            let name = s.name.clone();
            self.say(format!("{name} was not read from a file: press P for the lines"));
            return;
        };
        let tune = &self.tunes[self.at];
        let written = std::fs::read_to_string(&path).and_then(|text| {
            let out =
                crate::sprite_file::SpriteFile::retuned(&text, &tune.gait(), tune.bob, tune.surge);
            std::fs::write(&path, out)
        });
        match written {
            Ok(()) => self.say(format!("wrote {path}")),
            Err(e) => self.say(format!("could not write {path}: {e}")),
        }
    }

    fn cycle(&self) -> Option<Cycle> {
        let s = self.sprites.get(self.at)?;
        (s.frames.len() > 1).then(|| Cycle::play(&s.frames, &self.tunes[self.at].gait(), s.scale))
    }

    fn keys(&mut self, frame: &Frame) {
        let pressed = |k: KeyCode| frame.input.keys_pressed.contains(&k);
        let n = self.sprites.len().max(1);
        if pressed(KeyCode::Right) {
            self.at = (self.at + 1) % n;
            self.step = 0;
        }
        if pressed(KeyCode::Left) {
            self.at = (self.at + n - 1) % n;
            self.step = 0;
        }
        if pressed(KeyCode::Space) {
            self.paused = !self.paused;
        }
        if pressed(KeyCode::Z) {
            self.slow = !self.slow;
        }
        if pressed(KeyCode::F) {
            self.left = !self.left;
        }
        if self.paused && pressed(KeyCode::Period) {
            self.t += 1.0 / 60.0;
        }
        if self.paused && pressed(KeyCode::Comma) {
            self.t -= 1.0 / 60.0;
        }
        let Some(tune) = self.tunes.get_mut(self.at) else {
            return;
        };
        if pressed(KeyCode::Up) {
            tune.period *= 1.1;
        }
        if pressed(KeyCode::Down) {
            tune.period = (tune.period / 1.1).max(0.05);
        }
        let steps = tune.steps.len().max(1);
        if pressed(KeyCode::RightBracket) {
            self.step = (self.step + 1) % steps;
        }
        if pressed(KeyCode::LeftBracket) {
            self.step = (self.step + steps - 1) % steps;
        }
        if let Some((_, w)) = tune.steps.get_mut(self.step) {
            if pressed(KeyCode::Equal) {
                *w += 0.1;
            }
            if pressed(KeyCode::Minus) {
                *w = (*w - 0.1).max(0.1);
            }
        }
        let nudge = |v: &mut f32, by: f32, lo: f32, hi: f32| *v = (*v + by).clamp(lo, hi);
        if pressed(KeyCode::J) {
            nudge(&mut tune.spread, -0.02, 0.0, 0.9);
        }
        if pressed(KeyCode::K) {
            nudge(&mut tune.spread, 0.02, 0.0, 0.9);
        }
        if pressed(KeyCode::B) {
            nudge(&mut tune.bob, -0.25, 0.0, 20.0);
        }
        if pressed(KeyCode::N) {
            nudge(&mut tune.bob, 0.25, 0.0, 20.0);
        }
        if pressed(KeyCode::S) {
            nudge(&mut tune.surge, -0.25, 0.0, 20.0);
        }
        if pressed(KeyCode::D) {
            nudge(&mut tune.surge, 0.25, 0.0, 20.0);
        }
        if pressed(KeyCode::R) {
            *tune = Tune::of(&self.sprites[self.at]);
        }
        if pressed(KeyCode::P) {
            println!("{}:\n{}\n", self.sprites[self.at].name, tune.lines());
        }
        if pressed(KeyCode::W) {
            self.write_back();
        }
    }

    /// Draw sprite `i` with its feet at `foot`, `px` design units an art
    /// pixel, at time `t` as clone `seed`, moved by its body.
    fn draw_at(&self, i: usize, foot: Vec2, px: f32, t: f32, seed: u64, left: bool) {
        let (Some(s), Some(Some(baked))) = (self.sprites.get(i), self.baked.get(i)) else {
            return;
        };
        let tune = &self.tunes[i];
        let n = s.frames.len();
        let (frame, dx, dy) = if n > 1 {
            let stride = tune.gait().at(t, seed);
            (
                stride.frame.min(n - 1),
                tune.surge * stride.surge,
                tune.bob * stride.swing,
            )
        } else {
            (0, 0.0, 0.0)
        };
        let forward = if left { -dx } else { dx };
        let index = frame + if left { n } else { 0 };
        baked.draw_scaled(index, foot + vec2(forward, dy) * px, vec2(px, px), WHITE);
    }
}

impl Game for Viewer {
    fn update(&mut self, frame: &Frame) {
        self.keys(frame);
        if !self.paused {
            self.t += frame.dt * if self.slow { 0.25 } else { 1.0 };
        }
        self.poll -= frame.dt;
        if self.poll <= 0.0 {
            self.poll = 0.5;
            self.reload();
        }
        if let Some((_, left)) = self.status.as_mut() {
            *left -= frame.dt;
            if *left <= 0.0 {
                self.status = None;
            }
        }
    }

    fn draw(&mut self, frame: &Frame) {
        for (i, s) in self.sprites.iter().enumerate() {
            if self.baked[i].is_none() {
                let flipped: Vec<_> = s.frames.iter().map(|g| g.flipped()).collect();
                self.baked[i] = Some(PixelSprite::new(&[s.frames.clone(), flipped].concat()));
            }
        }
        clear_background(Color::from_rgba(20, 20, 26, 255));
        let Some(s) = self.sprites.get(self.at) else {
            return;
        };
        let Some(first) = s.frames.first() else {
            return;
        };
        let (w, h) = (first.w as f32, first.h as f32);
        let guide = Color::from_rgba(255, 255, 255, 40);

        // The sprite large, against where it rests.
        let stage = Rect::new(20.0, 70.0, 780.0, 440.0);
        draw_rectangle(stage.x, stage.y, stage.w, stage.h, self.backdrop.top);
        let zoom = (680.0 / w).min(340.0 / h).floor().max(1.0);
        let foot = vec2(stage.center().x, stage.center().y + h * zoom * 0.5);
        draw_line(stage.x, foot.y, stage.right(), foot.y, 1.0, guide);
        let nose = foot.x + w * zoom * 0.5 * if self.left { -1.0 } else { 1.0 };
        draw_line(nose, stage.y, nose, stage.bottom(), 1.0, guide);
        self.draw_at(self.at, foot, zoom, self.t, 0, self.left);

        // At real size, and twice it, as the game draws it.
        let side = Rect::new(820.0, 70.0, 440.0, 200.0);
        draw_rectangle(side.x, side.y, side.w, side.h, self.backdrop.top);
        let real = s.scale;
        self.draw_at(
            self.at,
            vec2(side.x + 70.0, side.y + 120.0),
            real,
            self.t,
            0,
            self.left,
        );
        self.draw_at(
            self.at,
            vec2(side.x + 280.0, side.y + 150.0),
            real * 2.0,
            self.t,
            0,
            self.left,
        );

        // A crowd of clones: one sprite's textures, each its own phase.
        let crowd = Rect::new(20.0, 530.0, 780.0, 250.0);
        draw_rectangle(crowd.x, crowd.y, crowd.w, crowd.h, self.backdrop.top);
        let cell = crowd.w / (CROWD / 2) as f32;
        for k in 0..CROWD {
            let (col, row) = (k % (CROWD / 2), k / (CROWD / 2));
            let foot = vec2(
                crowd.x + cell * (col as f32 + 0.5),
                crowd.y + 105.0 + row as f32 * 120.0,
            );
            self.draw_at(self.at, foot, real * 1.5, self.t, k as u64 + 1, row == 1);
        }

        if !frame.text {
            return;
        }
        let ink = Color::from_rgba(220, 224, 236, 255);
        let quiet = Color::from_rgba(150, 156, 170, 255);
        let warn = Color::from_rgba(255, 140, 120, 255);
        draw_text(
            format!(
                "{}  ({} of {}){}{}",
                s.name,
                self.at + 1,
                self.sprites.len(),
                if self.paused { "  PAUSED" } else { "" },
                if self.slow { "  1/4 SPEED" } else { "" }
            ),
            20.0,
            36.0,
            30.0,
            ink,
        );
        if let Some((message, _)) = &self.status {
            draw_text(message, 820.0, 780.0, 16.0, ink);
        }
        draw_text(
            "<- -> sprite   space pause  , . step   Z slow   F face   up/down period   [ ] step  - = hold   J K jitter   B N bob   S D surge   R reset   W write   P print",
            20.0,
            58.0,
            16.0,
            quiet,
        );
        draw_text(
            "large, with where it rests",
            stage.x + 8.0,
            stage.y + 18.0,
            16.0,
            quiet,
        );
        draw_text(
            "real size, and twice it",
            side.x + 8.0,
            side.y + 18.0,
            16.0,
            quiet,
        );
        draw_text(
            "clones, each on its own phase",
            crowd.x + 8.0,
            crowd.y + 18.0,
            16.0,
            quiet,
        );

        let mut y = 300.0;
        let mut line = |text: &str, color: Color, size: f32| {
            draw_text(text, 820.0, y, size, color);
            y += size + 4.0;
        };
        let tune = &self.tunes[self.at];
        line(
            &format!(
                "period {:.2}s   jitter ±{:.0}%   bob {:.2}   surge {:.2}",
                tune.period,
                tune.spread * 100.0,
                tune.bob,
                tune.surge
            ),
            ink,
            18.0,
        );
        let holds: Vec<String> = tune
            .steps
            .iter()
            .enumerate()
            .map(|(i, (f, w))| {
                if i == self.step {
                    format!("[f{f} {w:.1}]")
                } else {
                    format!("f{f} {w:.1}")
                }
            })
            .collect();
        for chunk in holds.chunks(6) {
            line(&chunk.join("  "), ink, 18.0);
        }
        match self.cycle() {
            Some(c) if c.problems.is_empty() => line("no motion problems", quiet, 18.0),
            Some(c) => {
                for p in &c.problems {
                    for part in wrap(p, 52) {
                        line(&part, warn, 16.0);
                    }
                }
            }
            None => line("one frame: nothing to play", quiet, 18.0),
        }
    }

    fn backdrop(&self) -> Backdrop {
        self.backdrop
    }

    fn capture_note(&self, frame: &Frame) -> String {
        let Some(s) = self.sprites.get(self.at) else {
            return "viewer: no sprites".to_string();
        };
        let tune = &self.tunes[self.at];
        let problems = self.cycle().map_or("one frame".to_string(), |c| c.text());
        format!(
            "viewer: {} at t={:.2}s\n{}\n{problems}",
            s.name,
            frame.clock,
            tune.lines()
        )
    }
}

/// `text` broken into lines of at most `width` characters, at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The printed lines are the tuned values, ready to paste.
    #[test]
    fn a_tune_prints_what_to_paste() {
        let t = Tune {
            steps: vec![(0, 1.3), (1, 1.0)],
            period: 1.3,
            spread: 0.12,
            bob: 1.2,
            surge: 1.5,
        };
        assert_eq!(
            t.lines(),
            "Gait::new(&[(0, 1.30), (1, 1.00)], 1.30).jitter(0.12)\n.body(1.20, 1.50)"
        );
        assert_eq!(t.gait().steps(), &[(0, 1.3), (1, 1.0)]);
    }

    #[test]
    fn long_problems_wrap_at_spaces() {
        assert_eq!(wrap("aa bb cc", 5), vec!["aa bb", "cc"]);
    }
}
