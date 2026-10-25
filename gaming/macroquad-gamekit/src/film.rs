//! `Film` — consecutive frames laid out and measured, so motion (a sprite swimming through its gait, a card gliding, a tween easing) can be judged from one image and its report.
//!
//! A single capture is one moment; motion is the difference between
//! moments. A film is `count` frames, `every` display frames apart. Two
//! sources: [`of_sprite`], one sprite alone, played through its gait on a
//! plain canvas (`--sprites out.png --only whale --strip 24`), which is how
//! a sprite's motion is tested; and `--shot --strip`, the real scene or a
//! crop of it, for motion in place. [`Film::sheet`] tiles the frames in
//! reading order, then adds two panels:
//!
//! - **Trail**: every frame averaged. A thing moving smoothly leaves an even
//!   smear; one that stutters leaves clumps where it stopped.
//! - **Change map**: each pixel that changed between neighbouring frames,
//!   coloured by when (blue early, yellow late). Even spacing shows as an
//!   even ramp; a jump shows as a gap.
//!
//! [`Film::steps`] measures each step: how many pixels changed, and how far
//! the mover moved, its position being the centre of what differs from the
//! still background (each pixel's median over the film). With a crop around
//! one mover, its travel a step is its speed. [`Film::text`]
//! names the problems: a **stall** (a step where nothing moved between steps
//! that did, which reads as a hitch) and **uneven** steps (one step's travel
//! over [`UNEVEN`] times its neighbours'). Pure: RGBA bytes in, bytes out.

/// A step whose mover travels more than this many times its neighbours'
/// mean is uneven: eased motion changes speed by well under twice a step.
pub const UNEVEN: f32 = 2.0;
/// The summed RGB difference over which a pixel counts as changed.
const CHANGED: u32 = 36;

/// Frames of one size, as RGBA bytes.
#[derive(Clone, Debug, Default)]
pub struct Film {
    pub w: usize,
    pub h: usize,
    pub frames: Vec<Vec<u8>>,
    /// Display frames between captures.
    pub every: u32,
    /// What does not move, when known (a sprite's film: its empty canvas);
    /// otherwise each pixel's median over the film stands in.
    pub still: Option<Vec<u8>>,
}

/// How a sprite's body moves with its gait, in art pixels: it sinks by
/// `bob` times the stride's swing and is pushed forward by `surge` times its
/// surge, as the game moves it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub bob: f32,
    pub surge: f32,
}

/// A sprite alone, played: `count` frames `every` display frames apart at
/// [`crate::motion::HZ`], each the gait's pose at that moment (at the
/// nominal rate), moved by `body`, drawn at `zoom` over `backdrop` facing
/// right. Faint guides mark where it rests, so the body's travel shows
/// against something fixed.
pub fn of_sprite(
    frames: &[crate::pixel::Grid],
    gait: &crate::gait::Gait,
    body: Body,
    zoom: usize,
    count: usize,
    every: u32,
    backdrop: [u8; 4],
) -> Film {
    let Some(first) = frames.first() else {
        return Film::new(1, 1, every);
    };
    let zoom = zoom.max(1);
    let pad = body.bob.abs().max(body.surge.abs()).ceil() as usize + 2;
    let (cw, ch) = ((first.w + pad * 2) * zoom, (first.h + pad * 2) * zoom);
    let mut canvas = vec![0u8; cw * ch * 4];
    for px in canvas.as_chunks_mut::<4>().0 {
        *px = backdrop;
    }
    let guide = [
        backdrop[0].saturating_add(28),
        backdrop[1].saturating_add(28),
        backdrop[2].saturating_add(28),
        255,
    ];
    // Guides: the line it rests on and its resting nose.
    let (rest_y, rest_x) = ((pad + first.h) * zoom, (pad + first.w) * zoom);
    for x in 0..cw {
        let i = (rest_y.min(ch - 1) * cw + x) * 4;
        canvas[i..i + 4].copy_from_slice(&guide);
    }
    for y in 0..ch {
        let i = (y * cw + rest_x.min(cw - 1)) * 4;
        canvas[i..i + 4].copy_from_slice(&guide);
    }
    let mut film = Film::new(cw, ch, every);
    film.still = Some(canvas.clone());
    for k in 0..count {
        let t = k as f32 * every as f32 / crate::motion::HZ;
        let stride = gait.at_beat(t / gait.period());
        let g = &frames[stride.frame.min(frames.len() - 1)];
        let ox = ((pad as f32 + body.surge * stride.surge) * zoom as f32).round() as i64;
        let oy = ((pad as f32 + body.bob * stride.swing) * zoom as f32).round() as i64;
        let mut f = canvas.clone();
        for gy in 0..g.h {
            for gx in 0..g.w {
                let Some(c) = g.at(gx, gy) else { continue };
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        let (x, y) = (ox + (gx * zoom + dx) as i64, oy + (gy * zoom + dy) as i64);
                        if x < 0 || y < 0 || x >= cw as i64 || y >= ch as i64 {
                            continue;
                        }
                        let i = (y as usize * cw + x as usize) * 4;
                        f[i..i + 4].copy_from_slice(&c);
                    }
                }
            }
        }
        film.frames.push(f);
    }
    film
}

/// What changed from one frame of a film to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilmStep {
    /// Pixels changed.
    pub changed: usize,
    /// Where the mover is after the step: the centre of what differs from
    /// the still background.
    pub centre: Option<(f32, f32)>,
    /// How far the mover moved in the step, in pixels.
    pub travel: Option<f32>,
}

impl Film {
    pub fn new(w: usize, h: usize, every: u32) -> Self {
        Self {
            w,
            h,
            frames: Vec::new(),
            every,
            still: None,
        }
    }

    /// Add a frame; one of another size is cropped or padded to the film's.
    pub fn push(&mut self, bytes: &[u8], w: usize, h: usize) {
        let mut out = vec![0u8; self.w * self.h * 4];
        for y in 0..self.h.min(h) {
            for x in 0..self.w.min(w) {
                let (s, d) = ((y * w + x) * 4, (y * self.w + x) * 4);
                out[d..d + 4].copy_from_slice(&bytes[s..s + 4]);
            }
        }
        self.frames.push(out);
    }

    fn differs(a: &[u8], b: &[u8], i: usize) -> bool {
        let d: u32 = (0..3)
            .map(|c| (a[i * 4 + c] as i32 - b[i * 4 + c] as i32).unsigned_abs())
            .sum();
        d > CHANGED
    }

    /// The still background: each pixel's median across the film.
    fn background(&self) -> Vec<u8> {
        if let Some(still) = &self.still {
            return still.clone();
        }
        let n = self.frames.len();
        let mut out = vec![0u8; self.w * self.h * 4];
        let mut vals: Vec<u8> = Vec::with_capacity(n);
        for (i, px) in out.iter_mut().enumerate() {
            vals.clear();
            vals.extend(self.frames.iter().map(|f| f[i]));
            vals.sort_unstable();
            *px = vals.get(n / 2).copied().unwrap_or(0);
        }
        out
    }

    /// Where the mover is in each frame: the centre of the pixels that
    /// differ from the still background, or `None` where none do.
    pub fn positions(&self) -> Vec<Option<(f32, f32)>> {
        let bg = self.background();
        self.frames
            .iter()
            .map(|f| {
                let (mut n, mut sx, mut sy) = (0usize, 0.0f64, 0.0f64);
                for i in 0..self.w * self.h {
                    if Self::differs(f, &bg, i) {
                        n += 1;
                        sx += (i % self.w) as f64;
                        sy += (i / self.w) as f64;
                    }
                }
                (n > 0).then(|| ((sx / n as f64) as f32, (sy / n as f64) as f32))
            })
            .collect()
    }

    /// Each step's change, from frame `i` to `i + 1`.
    pub fn steps(&self) -> Vec<FilmStep> {
        let at = self.positions();
        self.frames
            .windows(2)
            .enumerate()
            .map(|(k, pair)| {
                let changed = (0..self.w * self.h)
                    .filter(|i| Self::differs(&pair[0], &pair[1], *i))
                    .count();
                let travel = match (at[k], at[k + 1]) {
                    (Some(a), Some(b)) => Some(((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()),
                    _ => None,
                };
                FilmStep {
                    changed,
                    centre: at[k + 1],
                    travel,
                }
            })
            .collect()
    }

    /// What looks wrong in the motion, each a sentence.
    pub fn problems(&self) -> Vec<String> {
        let steps = self.steps();
        let mut out = Vec::new();
        let floor = (self.w * self.h / 4000).max(2);
        // An eased motion slows to nothing at its ends, so a still step
        // between slow ones is easing; between steps at full speed (the
        // film's median travel or more) it is a hitch.
        let mut sorted: Vec<f32> = steps.iter().filter_map(|s| s.travel).collect();
        sorted.sort_by(f32::total_cmp);
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        let fast = |s: &FilmStep| s.changed >= floor * 4 && s.travel.unwrap_or(0.0) >= median;
        for i in 1..steps.len().saturating_sub(1) {
            let (a, b, c) = (steps[i - 1], steps[i], steps[i + 1]);
            if b.changed < floor && fast(&a) && fast(&c) {
                out.push(format!(
                    "stall: nothing moved from frame {i} to {} between steps that changed {} and {} px; a hitch",
                    i + 1,
                    a.changed,
                    c.changed
                ));
            }
        }
        let travels: Vec<Option<f32>> = steps.iter().map(|s| s.travel).collect();
        for i in 1..travels.len().saturating_sub(1) {
            if let (Some(a), Some(b), Some(c)) = (travels[i - 1], travels[i], travels[i + 1]) {
                let around = (a + c) / 2.0;
                if b > 2.0 && b > around.max(0.5) * UNEVEN {
                    out.push(format!(
                        "uneven: the mover jumps {b:.1} px from frame {i} to {} against {around:.1} around it; a pop or a lurch",
                        i + 1
                    ));
                }
            }
        }
        out
    }

    /// The report: problems, then each step.
    pub fn text(&self) -> String {
        let mut out = String::from("## Problems\n\n");
        let problems = self.problems();
        if problems.is_empty() {
            out.push_str("none\n");
        }
        for p in &problems {
            out.push_str(&format!("- {p}\n"));
        }
        out.push_str(&format!(
            "\n## Steps ({} frames, {} display frames apart, {}x{} px)\n\n",
            self.frames.len(),
            self.every,
            self.w,
            self.h
        ));
        for (i, s) in self.steps().iter().enumerate() {
            let centre = s
                .centre
                .map_or("-".to_string(), |(x, y)| format!("({x:.0}, {y:.0})"));
            let travel = s.travel.map_or("-".to_string(), |t| format!("{t:.1} px"));
            out.push_str(&format!(
                "{i} to {}: {} px changed, centre {centre}, moved {travel}\n",
                i + 1,
                s.changed
            ));
        }
        out
    }

    /// The film on one image: frames in reading order, `cols` a row, then a
    /// row with the trail and the change map. Returns `(bytes, w, h)`.
    pub fn sheet(&self, cols: usize) -> (Vec<u8>, usize, usize) {
        const GAP: usize = 6;
        let n = self.frames.len();
        let cols = cols.clamp(1, n.max(1));
        let rows = n.div_ceil(cols) + 1;
        let (sw, sh) = (
            cols.max(2) * (self.w + GAP) + GAP,
            rows * (self.h + GAP) + GAP,
        );
        let mut out = vec![0u8; sw * sh * 4];
        for px in out.as_chunks_mut::<4>().0 {
            *px = [20, 20, 26, 255];
        }
        let mut blit = |bytes: &[u8], cx: usize, cy: usize| {
            for y in 0..self.h {
                for x in 0..self.w {
                    let s = (y * self.w + x) * 4;
                    let d = ((cy + y) * sw + cx + x) * 4;
                    out[d..d + 3].copy_from_slice(&bytes[s..s + 3]);
                    out[d + 3] = 255;
                }
            }
        };
        for (i, f) in self.frames.iter().enumerate() {
            blit(
                f,
                GAP + (i % cols) * (self.w + GAP),
                GAP + (i / cols) * (self.h + GAP),
            );
        }
        let last_row = GAP + (rows - 1) * (self.h + GAP);
        // The trail: every frame averaged.
        let mut trail = vec![0u8; self.w * self.h * 4];
        if n > 0 {
            for i in 0..self.w * self.h * 4 {
                let sum: u32 = self.frames.iter().map(|f| f[i] as u32).sum();
                trail[i] = (sum / n as u32) as u8;
            }
        }
        blit(&trail, GAP, last_row);
        // The change map: the first frame dimmed, each change coloured by
        // when it happened.
        let mut map: Vec<u8> = self.frames.first().map_or_else(
            || vec![0u8; self.w * self.h * 4],
            |f| f.iter().map(|b| b / 4).collect(),
        );
        let steps = n.saturating_sub(1).max(1) as f32;
        for (k, pair) in self.frames.windows(2).enumerate() {
            let t = k as f32 / steps;
            let colour = [
                (60.0 + 195.0 * t) as u8,
                (120.0 + 95.0 * t) as u8,
                (255.0 - 160.0 * t) as u8,
            ];
            for i in 0..self.w * self.h {
                if Self::differs(&pair[0], &pair[1], i) {
                    map[i * 4..i * 4 + 3].copy_from_slice(&colour);
                }
            }
        }
        blit(&map, GAP + self.w + GAP, last_row);
        (out, sw, sh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of `w`x`h` black with a white 2x2 block at `x`.
    fn block(w: usize, h: usize, x: usize) -> Vec<u8> {
        let mut f = vec![0u8; w * h * 4];
        for y in 0..2 {
            for dx in 0..2 {
                let i = (y * w + x + dx) * 4;
                f[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        f
    }

    /// Steady motion has no problems; a stop mid-way is a stall and a
    /// sudden jump uneven.
    #[test]
    fn a_film_names_stalls_and_lurches() {
        let (w, h) = (80, 4);
        let mut steady = Film::new(w, h, 1);
        for x in [0, 3, 6, 9, 12, 15] {
            steady.push(&block(w, h, x), w, h);
        }
        assert!(steady.problems().is_empty(), "{}", steady.text());

        let mut stall = Film::new(w, h, 1);
        for x in [0, 3, 6, 6, 9, 12] {
            stall.push(&block(w, h, x), w, h);
        }
        assert!(
            stall.problems().iter().any(|p| p.starts_with("stall")),
            "{}",
            stall.text()
        );

        let mut lurch = Film::new(w, h, 1);
        for x in [0, 3, 6, 40, 43, 46] {
            lurch.push(&block(w, h, x), w, h);
        }
        assert!(
            lurch.problems().iter().any(|p| p.starts_with("uneven")),
            "{}",
            lurch.text()
        );
    }

    /// A sprite's film draws its poses in the gait's order, moved by its
    /// body, and the still canvas is all that is behind it.
    #[test]
    fn a_sprite_plays_alone_through_its_gait() {
        let p = crate::pixel::Palette::new().with('a', 0xffffff);
        let frames = [
            crate::pixel::Grid::parse(&["a.", ".."], &p, 0).expect("parses"),
            crate::pixel::Grid::parse(&["..", ".a"], &p, 1).expect("parses"),
        ];
        let gait = crate::gait::Gait::even(2, 0.5);
        let film = of_sprite(
            &frames,
            &gait,
            Body {
                bob: 1.0,
                surge: 0.0,
            },
            2,
            6,
            5,
            [0, 0, 0, 255],
        );
        assert_eq!(film.frames.len(), 6);
        assert!(film.still.is_some());
        let at = film.positions();
        assert!(
            at.iter().all(Option::is_some),
            "the sprite is in every frame"
        );
        // Frames 0..2 are pose 0 (0 to 10 display frames), 3..5 pose 1.
        assert_ne!(at[0], at[3]);
    }

    /// The sheet holds every frame and the two panels.
    #[test]
    fn the_sheet_tiles_frames_and_panels() {
        let (w, h) = (10, 4);
        let mut film = Film::new(w, h, 2);
        for x in [0, 2, 4, 6, 8] {
            film.push(&block(w, h, x), w, h);
        }
        let (bytes, sw, sh) = film.sheet(3);
        assert_eq!(bytes.len(), sw * sh * 4);
        assert_eq!(sw, 3 * (w + 6) + 6);
        assert_eq!(sh, 3 * (h + 6) + 6);
    }
}
