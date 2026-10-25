//! `PixelSprite` — pixel art written as text in code, one character a pixel, baked into nearest-filtered textures: sprites with no asset files, diffable and editable by anyone.
//!
//! A sprite is a few frames of rows, and a [`Palette`] mapping each
//! character to a colour. `.` and space are transparent. Every frame of a
//! sprite is the same size, and every row of a frame the same length; a
//! mistake is an error naming the frame, row and column, never a garbled
//! sprite.
//!
//! ```text
//! const CRAB: &[&str] = &[
//!     "..rr....rr..",
//!     ".rRRr..rRRr.",
//!     "..rrrrrrrr..",
//!     ".rrwkrrwkrr.",
//! ];
//! ```
//!
//! Variants are baked from the same rows, so a state change is a different
//! texture rather than drawing tricks: [`Grid::outlined`] rings the opaque
//! pixels in one colour (an angry NPC's red edge, or readability over busy
//! ground), and [`Grid::silhouette`] fills them with one (the white flash of
//! a hit). [`Grid::flipped`] mirrors a frame, for a walk facing the other way.
//!
//! [`Grid`] is pure (parse, variants, RGBA bytes) and tested without a
//! window; [`PixelSprite`] uploads the textures and draws them, anchored at
//! the feet and on whole pixels, so the art stays crisp on a pixel stage.

use macroquad::prelude::*;

/// Characters to colours. `.` and space are always transparent.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    entries: Vec<(char, [u8; 4])>,
}

impl Palette {
    pub fn new() -> Self {
        Self::default()
    }

    /// `c` is the opaque colour `rgb` (`0xRRGGBB`). A later entry for the same
    /// character replaces an earlier one, which is how a palette swap is made
    /// from a base palette.
    pub fn with(mut self, c: char, rgb: u32) -> Self {
        let colour = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        self.entries.retain(|(k, _)| *k != c);
        self.entries.push((c, colour));
        self
    }

    fn get(&self, c: char) -> Option<[u8; 4]> {
        self.entries.iter().find(|(k, _)| *k == c).map(|(_, v)| *v)
    }
}

/// Why some art did not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PixelError {
    Empty,
    /// A row is not as long as the frame's first.
    Ragged {
        frame: usize,
        row: usize,
        len: usize,
        expected: usize,
    },
    /// A frame is not the size of the sprite's first.
    Size {
        frame: usize,
        size: (usize, usize),
        expected: (usize, usize),
    },
    /// A character the palette does not name.
    Unknown {
        frame: usize,
        row: usize,
        col: usize,
        c: char,
    },
}

impl std::fmt::Display for PixelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "no rows"),
            Self::Ragged {
                frame,
                row,
                len,
                expected,
            } => write!(
                f,
                "frame {frame} row {row} is {len} wide, the frame's first is {expected}"
            ),
            Self::Size {
                frame,
                size,
                expected,
            } => write!(
                f,
                "frame {frame} is {}x{}, the first is {}x{}",
                size.0, size.1, expected.0, expected.1
            ),
            Self::Unknown { frame, row, col, c } => {
                write!(
                    f,
                    "frame {frame} row {row} col {col}: '{c}' is not in the palette"
                )
            }
        }
    }
}

impl std::error::Error for PixelError {}

/// One frame of pixel art: RGBA, transparent where there is no pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub w: usize,
    pub h: usize,
    /// Row-major, `w * h`, `None` where transparent.
    pixels: Vec<Option<[u8; 4]>>,
}

impl Grid {
    /// Parse one frame. `frame` only labels errors.
    pub fn parse(rows: &[&str], palette: &Palette, frame: usize) -> Result<Self, PixelError> {
        let expected = rows.first().ok_or(PixelError::Empty)?.chars().count();
        if expected == 0 {
            return Err(PixelError::Empty);
        }
        let mut pixels = Vec::with_capacity(expected * rows.len());
        for (row, line) in rows.iter().enumerate() {
            let len = line.chars().count();
            if len != expected {
                return Err(PixelError::Ragged {
                    frame,
                    row,
                    len,
                    expected,
                });
            }
            for (col, c) in line.chars().enumerate() {
                pixels.push(match c {
                    '.' | ' ' => None,
                    c => Some(
                        palette
                            .get(c)
                            .ok_or(PixelError::Unknown { frame, row, col, c })?,
                    ),
                });
            }
        }
        Ok(Self {
            w: expected,
            h: rows.len(),
            pixels,
        })
    }

    /// Whether the pixel at (`x`, `y`) is drawn. Off the grid is not.
    pub fn opaque(&self, x: i32, y: i32) -> bool {
        (0..self.w as i32).contains(&x)
            && (0..self.h as i32).contains(&y)
            && self.pixels[y as usize * self.w + x as usize].is_some()
    }

    /// One pixel larger on every side, the new border and every transparent
    /// pixel touching an opaque one (not diagonally) filled with `rgb`.
    pub fn outlined(&self, rgb: u32) -> Self {
        let colour = Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
        let (w, h) = (self.w + 2, self.h + 2);
        let mut pixels = vec![None; w * h];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (sx, sy) = (x - 1, y - 1);
                pixels[y as usize * w + x as usize] = if self.opaque(sx, sy) {
                    self.pixels[sy as usize * self.w + sx as usize]
                } else if [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| self.opaque(sx + dx, sy + dy))
                {
                    colour
                } else {
                    None
                };
            }
        }
        Self { w, h, pixels }
    }

    /// Every opaque pixel in `rgb`.
    pub fn silhouette(&self, rgb: u32) -> Self {
        let colour = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        Self {
            w: self.w,
            h: self.h,
            pixels: self.pixels.iter().map(|p| p.map(|_| colour)).collect(),
        }
    }

    /// Mirrored left to right.
    pub fn flipped(&self) -> Self {
        let mut pixels = Vec::with_capacity(self.pixels.len());
        for row in self.pixels.chunks(self.w) {
            pixels.extend(row.iter().rev());
        }
        Self {
            w: self.w,
            h: self.h,
            pixels,
        }
    }

    /// RGBA bytes, row-major, transparent pixels zero.
    pub fn rgba(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|p| p.unwrap_or([0; 4]))
            .collect()
    }

    /// The colour at (`x`, `y`), if drawn.
    pub fn at(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        (x < self.w && y < self.h)
            .then(|| self.pixels[y * self.w + x])
            .flatten()
    }

    /// How this frame becomes `next`, cell by cell: see [`Onion`]. Frames of
    /// different sizes are compared over the cells both have.
    pub fn onion(&self, next: &Grid) -> Onion {
        let (w, h) = (self.w.min(next.w), self.h.min(next.h));
        let mut rows = Vec::with_capacity(h);
        let (mut stayed, mut appeared, mut vanished, mut recoloured) = (0, 0, 0, 0);
        let mut change: Option<(usize, usize, usize, usize)> = None;
        for y in 0..h {
            let mut row = String::with_capacity(w);
            for x in 0..w {
                let c = match (self.at(x, y), next.at(x, y)) {
                    (Some(a), Some(b)) if a == b => {
                        stayed += 1;
                        '#'
                    }
                    (Some(_), Some(_)) => {
                        recoloured += 1;
                        '~'
                    }
                    (None, Some(_)) => {
                        appeared += 1;
                        '+'
                    }
                    (Some(_), None) => {
                        vanished += 1;
                        '-'
                    }
                    (None, None) => '.',
                };
                if !matches!(c, '#' | '.') {
                    change = Some(match change {
                        None => (x, y, x, y),
                        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    });
                }
                row.push(c);
            }
            rows.push(row);
        }
        Onion {
            w,
            h,
            rows,
            stayed,
            appeared,
            vanished,
            recoloured,
            change,
        }
    }

    /// Every pixel `zoom` times over, as an image to write with
    /// `export_png`: art reviewed at a size it can be read at, without a
    /// window. Transparent cells are `backdrop` (or a two-tone checker when
    /// `None`, so a sprite's edge shows against nothing).
    pub fn image(&self, zoom: usize, backdrop: Option<[u8; 4]>) -> Image {
        let zoom = zoom.max(1);
        let (w, h) = (self.w * zoom, self.h * zoom);
        let mut bytes = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let (gx, gy) = (x / zoom, y / zoom);
                let px = self.at(gx, gy).unwrap_or_else(|| {
                    backdrop.unwrap_or(if (gx + gy) % 2 == 0 {
                        [58, 58, 66, 255]
                    } else {
                        [44, 44, 52, 255]
                    })
                });
                bytes.extend_from_slice(&px);
            }
        }
        Image {
            bytes,
            width: w as u16,
            height: h as u16,
        }
    }
}

/// How one frame of a sprite becomes the next, as text: motion judged in the
/// raw art, before anything is drawn.
///
/// One character a cell: `#` drawn in both, `+` appears, `-` vanishes, `~`
/// drawn in both in another colour, `.` in neither. Good motion is change
/// confined to the part that moves (a tail, a claw) while the rest, the eye
/// above all, holds still; a frame that only shifts a thin line reads as a
/// flicker, and one that moves everything reads as a jump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Onion {
    pub w: usize,
    pub h: usize,
    pub rows: Vec<String>,
    pub stayed: usize,
    pub appeared: usize,
    pub vanished: usize,
    pub recoloured: usize,
    /// The cells the change spans, inclusive: `(x0, y0, x1, y1)`. `None`
    /// when the frames are the same.
    pub change: Option<(usize, usize, usize, usize)>,
}

impl Onion {
    /// The share of the drawn cells (in either frame) that changed.
    pub fn changed_share(&self) -> f32 {
        let changed = self.appeared + self.vanished + self.recoloured;
        let drawn = self.stayed + changed;
        if drawn == 0 {
            return 0.0;
        }
        changed as f32 / drawn as f32
    }

    /// Whether the cell at (`x`, `y`) is the same in both frames, drawn or not:
    /// an anchor (an eye) that must not move.
    pub fn held(&self, x: usize, y: usize) -> bool {
        self.rows
            .get(y)
            .and_then(|r| r.chars().nth(x))
            .is_some_and(|c| c == '#' || c == '.')
    }

    /// The rows, then a line of counts.
    pub fn text(&self) -> String {
        let mut out = self.rows.join("\n");
        let span = self.change.map_or("none".to_string(), |(x0, y0, x1, y1)| {
            format!("x {x0}-{x1}, y {y0}-{y1}")
        });
        out.push_str(&format!(
            "\n{} held, {} appear, {} vanish, {} recolour ({:.0}% changed), change at {span}",
            self.stayed,
            self.appeared,
            self.vanished,
            self.recoloured,
            self.changed_share() * 100.0
        ));
        out
    }
}

/// Parse every frame of a sprite, checking they are all one size.
pub fn parse_frames(frames: &[&[&str]], palette: &Palette) -> Result<Vec<Grid>, PixelError> {
    let grids = frames
        .iter()
        .enumerate()
        .map(|(i, rows)| Grid::parse(rows, palette, i))
        .collect::<Result<Vec<_>, _>>()?;
    let first = grids.first().ok_or(PixelError::Empty)?;
    let expected = (first.w, first.h);
    for (frame, g) in grids.iter().enumerate() {
        if (g.w, g.h) != expected {
            return Err(PixelError::Size {
                frame,
                size: (g.w, g.h),
                expected,
            });
        }
    }
    Ok(grids)
}

/// Baked frames of one sprite, ready to draw.
pub struct PixelSprite {
    frames: Vec<Texture2D>,
    pub w: f32,
    pub h: f32,
}

impl PixelSprite {
    /// Upload `grids` (all one size) as textures. Needs the window.
    pub fn new(grids: &[Grid]) -> Self {
        let frames = grids
            .iter()
            .map(|g| {
                let tex = Texture2D::from_rgba8(g.w as u16, g.h as u16, &g.rgba());
                tex.set_filter(FilterMode::Nearest);
                tex
            })
            .collect();
        let (w, h) = grids
            .first()
            .map_or((0.0, 0.0), |g| (g.w as f32, g.h as f32));
        Self { frames, w, h }
    }

    /// Parse and upload in one go: the usual way, for art that is a constant
    /// in the game and checked by its tests. Panics on bad art, naming where.
    pub fn from_rows(frames: &[&[&str]], palette: &Palette) -> Self {
        match parse_frames(frames, palette) {
            Ok(grids) => Self::new(&grids),
            Err(e) => panic!("pixel art: {e}"),
        }
    }

    pub fn frames(&self) -> usize {
        self.frames.len()
    }

    /// Draw frame `frame` (wrapping) with its bottom centre at `foot`, on
    /// whole pixels, `tint` multiplying its colours (white leaves them).
    pub fn draw(&self, frame: usize, foot: Vec2, tint: Color) {
        self.draw_scaled(frame, foot, vec2(1.0, 1.0), tint);
    }

    /// As [`draw`](Self::draw), scaled about the feet: a squash, a shrink as
    /// something falls away.
    pub fn draw_scaled(&self, frame: usize, foot: Vec2, scale: Vec2, tint: Color) {
        if self.frames.is_empty() {
            return;
        }
        let tex = &self.frames[frame % self.frames.len()];
        let size = vec2(self.w * scale.x, self.h * scale.y);
        let x = (foot.x - size.x / 2.0).round();
        let y = (foot.y - size.y).round();
        draw_texture_ex(
            tex,
            x,
            y,
            tint,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette::new().with('r', 0xff0000).with('k', 0x000000)
    }

    #[test]
    fn rows_parse_to_pixels() {
        let g = Grid::parse(&["r.", ".k"], &palette(), 0).expect("parses");
        assert_eq!((g.w, g.h), (2, 2));
        assert!(g.opaque(0, 0) && !g.opaque(1, 0) && g.opaque(1, 1));
        assert_eq!(&g.rgba()[0..8], &[255, 0, 0, 255, 0, 0, 0, 0]);
        assert!(!g.opaque(-1, 0) && !g.opaque(2, 0), "off the grid");
    }

    #[test]
    fn mistakes_say_where_they_are() {
        assert_eq!(
            Grid::parse(&["rr", "r"], &palette(), 3),
            Err(PixelError::Ragged {
                frame: 3,
                row: 1,
                len: 1,
                expected: 2
            })
        );
        assert_eq!(
            Grid::parse(&["rz"], &palette(), 0),
            Err(PixelError::Unknown {
                frame: 0,
                row: 0,
                col: 1,
                c: 'z'
            })
        );
        assert_eq!(Grid::parse(&[], &palette(), 0), Err(PixelError::Empty));
        let frames: [&[&str]; 2] = [&["rr"], &["r"]];
        assert!(matches!(
            parse_frames(&frames, &palette()),
            Err(PixelError::Size { frame: 1, .. })
        ));
    }

    #[test]
    fn a_later_palette_entry_replaces_an_earlier() {
        let p = palette().with('r', 0x00ff00);
        let g = Grid::parse(&["r"], &p, 0).expect("parses");
        assert_eq!(g.rgba(), [0, 255, 0, 255]);
    }

    #[test]
    fn an_outline_rings_the_art_and_grows_it_by_one_a_side() {
        let g = Grid::parse(&["r"], &palette(), 0).expect("parses");
        let o = g.outlined(0x0000ff);
        assert_eq!((o.w, o.h), (3, 3));
        assert!(o.opaque(1, 1), "the art");
        assert!(o.opaque(0, 1) && o.opaque(2, 1) && o.opaque(1, 0) && o.opaque(1, 2));
        assert!(!o.opaque(0, 0), "no diagonal corners");
        let blue = &o.rgba()[4..8];
        assert_eq!(blue, &[0, 0, 255, 255]);
    }

    /// A tail that flicks: the change sits in the tail's columns, the eye
    /// holds, and each kind of change is marked and counted.
    #[test]
    fn an_onion_skin_shows_what_moves() {
        let p = palette().with('w', 0xffffff);
        let a = Grid::parse(&["r...", "rrrk", "...."], &p, 0).expect("parses");
        let b = Grid::parse(&["....", "rrrk", "r..."], &p, 1).expect("parses");
        let onion = a.onion(&b);
        assert_eq!(onion.rows, vec!["-...", "####", "+..."]);
        assert_eq!((onion.stayed, onion.appeared, onion.vanished), (4, 1, 1));
        assert_eq!(onion.change, Some((0, 0, 0, 2)), "only the tail column");
        assert!(onion.held(3, 1), "the eye");
        assert!(!onion.held(0, 0));
        assert!((onion.changed_share() - 2.0 / 6.0).abs() < 1e-6);
        assert!(onion.text().ends_with("change at x 0-0, y 0-2"));

        let c = Grid::parse(&["r...", "rrrw", "...."], &p, 2).expect("parses");
        assert_eq!(a.onion(&c).rows[1], "###~", "a recoloured eye");
        assert_eq!(a.onion(&a).change, None);
    }

    /// A frame enlarged for review: each pixel a block, the rest a checker.
    #[test]
    fn a_frame_enlarges_to_an_image() {
        let g = Grid::parse(&["r."], &palette(), 0).expect("parses");
        let img = g.image(3, None);
        assert_eq!((img.width, img.height), (6, 3));
        assert_eq!(&img.bytes[0..4], &[255, 0, 0, 255]);
        assert_eq!(&img.bytes[8..12], &[255, 0, 0, 255], "the block's last");
        assert_eq!(&img.bytes[12..16], &[44, 44, 52, 255], "the checker");
        let flat = g.image(1, Some([1, 2, 3, 255]));
        assert_eq!(&flat.bytes[4..8], &[1, 2, 3, 255]);
    }

    #[test]
    fn silhouettes_and_flips() {
        let g = Grid::parse(&["rk."], &palette(), 0).expect("parses");
        let s = g.silhouette(0xffffff);
        assert_eq!(&s.rgba()[0..8], &[255; 8]);
        assert!(!s.opaque(2, 0));
        let f = g.flipped();
        assert!(!f.opaque(0, 0) && f.opaque(2, 0));
        assert_eq!(&f.rgba()[8..12], &[255, 0, 0, 255]);
    }
}
