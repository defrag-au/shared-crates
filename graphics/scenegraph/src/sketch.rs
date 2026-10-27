//! The frame as text: a coarse grid of pick rays, each cell the name of
//! what the view sees there. What a glance at a screenshot tells you about
//! composition — what fills the frame, what is in front of what, what is
//! hidden — read from the [`DrawList`] alone, with no raster.
//!
//! ```text
//! sketch 48x16 aspect=1.5
//! ................................................
//! .........................TTTTTTTTTTTT...........
//! ....dddd.....ccc....ff...TTTTTTTTTTTT...........
//! aaaaddddaaaaacccaaaaffaaaaTTaaaaaTTaaaaaaaaaaaaa
//! key a=floor:310 T=truck:120 d=duck:22 c=crate:9 f=fox:4
//! ```
//!
//! Each cell is one ray through the cell's centre, so a thing smaller than
//! a cell can fall between rays: the key counts cells, and a node in the
//! scene that no cell saw is listed under `unseen`. A Label is the
//! exception: one that a ray at its own centre hits claims that cell, so
//! small text still shows where it is.

use crate::draw::DrawList;
use crate::images::Images;

/// Cells across and down: a text cell is about twice as tall as wide, so
/// 48×16 is the 1.5 aspect of the reference stage.
pub const SKETCH_COLS: usize = 48;
pub const SKETCH_ROWS: usize = 16;

const SYMBOLS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// What each cell of a window of the view sees, row by row from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Sketch {
    pub cols: usize,
    pub rows: usize,
    pub aspect: f32,
    /// The ndc window the grid covers, `[x0, y0, x1, y1]` (y up): the whole
    /// view, or a close-up ([`close_up`]).
    pub window: [f32; 4],
    /// What the close-up is of; `None` for the whole view.
    pub of: Option<String>,
    /// `None` is nothing drawn there (the clear colour).
    pub cells: Vec<Option<String>>,
    /// Words after a name in the key (`faces=toward`), in order added.
    pub notes: Vec<(String, String)>,
}

/// The whole view.
pub const WHOLE: [f32; 4] = [-1.0, -1.0, 1.0, 1.0];

fn pick<'a>(
    list: &'a DrawList,
    images: Option<&Images>,
    x: f32,
    y: f32,
    aspect: f32,
) -> Option<crate::draw::Hit<'a>> {
    match images {
        Some(img) => list.hit_ndc_with(x, y, aspect, img),
        None => list.hit_ndc(x, y, aspect),
    }
}

/// Cast one ray through the centre of each of `cols`×`rows` cells at
/// `aspect`, through the same pick as a tap ([`DrawList::hit_ndc_with`]):
/// transparent texels let the ray through when `images` are given.
pub fn sketch(
    list: &DrawList,
    aspect: f32,
    images: Option<&Images>,
    cols: usize,
    rows: usize,
) -> Sketch {
    sketch_window(list, aspect, images, cols, rows, WHOLE)
}

/// [`sketch`] over an ndc `window` of the view. Every Label a ray at its
/// own centre hits claims the cell it is in: text is what a reader most
/// needs to find, and a label is often smaller than a cell.
pub fn sketch_window(
    list: &DrawList,
    aspect: f32,
    images: Option<&Images>,
    cols: usize,
    rows: usize,
    window: [f32; 4],
) -> Sketch {
    let [x0, y0, x1, y1] = window;
    let mut cells = Vec::with_capacity(cols * rows);
    for r in 0..rows {
        let y = y1 - (r as f32 + 0.5) * (y1 - y0) / rows as f32;
        for c in 0..cols {
            let x = x0 + (c as f32 + 0.5) * (x1 - x0) / cols as f32;
            cells.push(pick(list, images, x, y, aspect).map(|h| h.name.to_string()));
        }
    }
    let mut s = Sketch {
        cols,
        rows,
        aspect,
        window,
        of: None,
        cells,
        notes: Vec::new(),
    };
    for lab in &list.labels {
        s.claim(list, images, &lab.name, lab.centre);
    }
    s
}

/// A close-up of `name`: the window around everything it draws (its
/// meshes, quads and labels), padded, and sketched at the same grid. `None`
/// if nothing of it is in front of the eye and on screen.
pub fn close_up(
    list: &DrawList,
    aspect: f32,
    images: Option<&Images>,
    name: &str,
    cols: usize,
    rows: usize,
) -> Option<Sketch> {
    let mut points: Vec<crate::math::Vec3> = Vec::new();
    for m in list.meshes.iter().filter(|m| m.name == name) {
        points.extend(m.verts.iter().map(|v| v.0));
    }
    for q in list.quads.iter().filter(|q| q.name == name) {
        points.extend(q.corners());
    }
    for l in list.labels.iter().filter(|l| l.name == name) {
        points.extend(l.corners());
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points {
        if let Some((x, y)) = list.project_ndc(p, aspect) {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 > x1 || x1 < -1.0 || x0 > 1.0 || y1 < -1.0 || y0 > 1.0 {
        return None;
    }
    // A quarter of its size around it, so it fills the middle and its
    // neighbours show at the edges; never under a hundredth of the view.
    let pad_x = ((x1 - x0) * 0.25).max(0.01);
    let pad_y = ((y1 - y0) * 0.25).max(0.01);
    let window = [
        (x0 - pad_x).max(-1.0),
        (y0 - pad_y).max(-1.0),
        (x1 + pad_x).min(1.0),
        (y1 + pad_y).min(1.0),
    ];
    let mut s = sketch_window(list, aspect, images, cols, rows, window);
    s.of = Some(name.to_string());
    // Past an edge of the view, part of it is not drawn at all.
    for (past, edge) in [
        (x0 < -1.0, "left"),
        (x1 > 1.0, "right"),
        (y1 > 1.0, "top"),
        (y0 < -1.0, "bottom"),
    ] {
        if past {
            s.note(name, format!("cut={edge}"));
        }
    }
    Some(s)
}

/// Which way a node with `front=` faces, as seen from the view: `toward`
/// the eye, `away`, or across it to the screen's `left` or `right`.
pub fn facing(scene: &crate::Scene, list: &DrawList, name: &str) -> Option<&'static str> {
    use crate::math::{transform_dir, transform_point, Vec3};
    let crate::Kind::Mesh {
        front: Some(front), ..
    } = &scene.get(name)?.kind
    else {
        return None;
    };
    let w = scene.world_matrix(name).ok()?;
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let fwd = flat(transform_dir(w, front.local()));
    let to_eye = flat(list.camera.eye - transform_point(w, Vec3::ZERO));
    if fwd.length() < 1e-6 || to_eye.length() < 1e-6 {
        return None;
    }
    let (fwd, to_eye) = (fwd.normalized(), to_eye.normalized());
    let cos = fwd.dot(to_eye);
    const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;
    if cos > DIAGONAL {
        return Some("toward");
    }
    if cos < -DIAGONAL {
        return Some("away");
    }
    let right = flat(list.camera.at - list.camera.eye).cross(Vec3::Y);
    Some(if fwd.dot(right) > 0.0 {
        "right"
    } else {
        "left"
    })
}

impl Sketch {
    /// `name` claims the cell where `point` is drawn, if a ray there hits
    /// it: a thing smaller than a cell, that rays at cell centres miss, is
    /// still placed. Returns whether it did.
    pub fn claim(
        &mut self,
        list: &DrawList,
        images: Option<&Images>,
        name: &str,
        point: crate::math::Vec3,
    ) -> bool {
        let Some((x, y)) = list.project_ndc(point, self.aspect) else {
            return false;
        };
        let [x0, y0, x1, y1] = self.window;
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return false;
        }
        if pick(list, images, x, y, self.aspect).is_none_or(|h| h.name != name) {
            return false;
        }
        let c = (((x - x0) / (x1 - x0) * self.cols as f32) as usize).min(self.cols - 1);
        let r = (((y1 - y) / (y1 - y0) * self.rows as f32) as usize).min(self.rows - 1);
        self.cells[r * self.cols + c] = Some(name.to_string());
        true
    }

    /// A word after `name` in the key (`faces=toward`).
    pub fn note(&mut self, name: &str, word: impl Into<String>) {
        self.notes.push((name.to_string(), word.into()));
    }

    /// Names by how many cells they own, most first (ties by first seen,
    /// reading order).
    pub fn counts(&self) -> Vec<(&str, usize)> {
        let mut out: Vec<(&str, usize)> = Vec::new();
        for name in self.cells.iter().flatten() {
            match out.iter_mut().find(|(n, _)| n == name) {
                Some((_, k)) => *k += 1,
                None => out.push((name, 1)),
            }
        }
        // Stable: ties keep first-seen order.
        out.sort_by_key(|&(_, k)| std::cmp::Reverse(k));
        out
    }

    /// The grid, then `key` (symbol=name:cells, most cells first). The
    /// first 62 names get a letter or digit, any more share `#`; nothing is
    /// `.`. `unseen` lists `expected` names no cell saw.
    pub fn dump(&self, expected: &[&str]) -> String {
        let counts = self.counts();
        let symbol = |name: &str| {
            counts
                .iter()
                .position(|(n, _)| *n == name)
                .and_then(|i| SYMBOLS.get(i))
                .map_or('#', |b| *b as char)
        };
        let f = crate::math::compact_f;
        let mut out = match &self.of {
            None => format!(
                "sketch {}x{} aspect={}\n",
                self.cols,
                self.rows,
                f(self.aspect)
            ),
            Some(of) => {
                let [x0, y0, x1, y1] = self.window;
                format!(
                    "sketch {of} {}x{} window={},{}:{},{}\n",
                    self.cols,
                    self.rows,
                    f(x0),
                    f(y0),
                    f(x1),
                    f(y1)
                )
            }
        };
        for row in self.cells.chunks(self.cols) {
            for cell in row {
                out.push(cell.as_deref().map_or('.', symbol));
            }
            out.push('\n');
        }
        out.push_str("key");
        for (name, k) in &counts {
            out.push_str(&format!(" {}={name}:{k}", symbol(name)));
            for (_, word) in self.notes.iter().filter(|(n, _)| n == name) {
                out.push(' ');
                out.push_str(word);
            }
        }
        out.push('\n');
        let unseen: Vec<&str> = expected
            .iter()
            .copied()
            .filter(|e| !counts.iter().any(|(n, _)| n == e))
            .collect();
        if !unseen.is_empty() {
            out.push_str("unseen ");
            out.push_str(&unseen.join(" "));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Fonts, Geometry, Materials, Scene};

    fn list(src: &str) -> DrawList {
        Scene::from_dump(src)
            .unwrap()
            .draw_list_with(&Geometry::new(), &Materials::new(), &mut Fonts::new())
            .unwrap()
    }

    /// A box in front of a wall: the box's cells are its own, the wall
    /// fills the rest, and a ball behind the box is unseen.
    #[test]
    fn the_sketch_shows_what_is_in_front() {
        let l = list(
            "sg1\nwall Quad 8x6 c=777777 t=0,0,-2\nbox Mesh Box c=aa5533 t=0,0,0 s=1.5,1.5,1.5\nball Mesh Sphere t=0,0,-1 s=0.5,0.5,0.5\ncam Camera eye=0,0,6 at=0,0,0\n",
        );
        let s = sketch(&l, 1.5, None, 24, 8);
        let d = s.dump(&["wall", "box", "ball"]);
        let counts = s.counts();
        assert_eq!(counts[0].0, "wall", "{d}");
        assert_eq!(counts[1].0, "box", "{d}");
        assert!(d.contains("unseen ball"), "{d}");
        let mid = &d.lines().nth(1 + 4).unwrap();
        assert!(mid.contains("bbb"), "the box is in the middle row: {d}");
        assert!(mid.starts_with("aa") || mid.starts_with(".."), "{d}");
    }

    /// A label far smaller than a cell still claims the cell at its centre;
    /// one hidden behind a wall does not.
    #[test]
    fn small_text_shows_where_it_is() {
        let mut fonts = crate::Fonts::new();
        fonts
            .insert("ibm", crate::NOTO_SANS_BOLD, Vec::new())
            .unwrap();
        let src = "sg1\nsign Quad 2x1 c=222222\ntiny Label font=ibm size=0.02 \"hi\" p=sign t=0.3,0.2,0.01\nwall Quad 1x1 c=777777 t=-0.5,0,1\nhid Label font=ibm size=0.02 \"no\" t=-0.5,0,0.5\ncam Camera eye=0,0,4 at=0,0,0\n";
        let l = Scene::from_dump(src)
            .unwrap()
            .draw_list_with(&Geometry::new(), &Materials::new(), &mut fonts)
            .unwrap();
        let s = sketch(&l, 1.5, None, 24, 8);
        let d = s.dump(&[]);
        assert!(
            s.counts().iter().any(|(n, k)| *n == "tiny" && *k == 1),
            "{d}"
        );
        assert!(!s.counts().iter().any(|(n, _)| *n == "hid"), "{d}");
    }

    /// A node facing +Z seen from +Z faces toward the eye; turned half round,
    /// away; a quarter, across the screen. No `front=`, no word.
    #[test]
    fn facing_is_read_from_the_view() {
        let word = |r: &str, front: &str| {
            let mut g = Geometry::new();
            g.insert_tet("tet").unwrap();
            let scene = Scene::from_dump(&format!(
                "sg1\nfox Mesh Geom g=tet{front} r=0,{r},0\ncam Camera eye=0,1,6 at=0,0,0\n"
            ))
            .unwrap();
            let l = scene
                .draw_list_with(&g, &Materials::new(), &mut Fonts::new())
                .unwrap();
            facing(&scene, &l, "fox")
        };
        assert_eq!(word("0", " front=+z"), Some("toward"));
        assert_eq!(word("3.1416", " front=+z"), Some("away"));
        assert_eq!(
            word("1.5708", " front=+z"),
            Some("right"),
            "+Z turned to +X"
        );
        assert_eq!(word("-1.5708", " front=+z"), Some("left"));
        assert_eq!(word("0", ""), None);
    }

    /// A box far down a field is a cell or two in the whole view; its
    /// close-up is a window around it where it fills the middle, with its
    /// neighbours at the edges.
    #[test]
    fn a_close_up_frames_one_node() {
        let l = list(
            "sg1\nfield Quad 40x40 c=556655 r=-1.5708,0,0\nbox Mesh Box c=aa5533 t=0,0.25,-12 s=0.5,0.5,0.5\ncam Camera eye=0,1.6,6 at=0,0.5,-12\n",
        );
        let whole = sketch(&l, 1.5, None, 48, 16);
        let far = whole
            .counts()
            .iter()
            .find(|(n, _)| *n == "box")
            .map_or(0, |c| c.1);
        assert!(far <= 4, "{}", whole.dump(&[]));
        let near = close_up(&l, 1.5, None, "box", 48, 16).unwrap();
        let d = near.dump(&[]);
        assert!(d.starts_with("sketch box 48x16 window="), "{d}");
        let boxed = near.counts().iter().find(|(n, _)| *n == "box").unwrap().1;
        assert!(boxed > 100, "the box fills the close-up: {d}");
        assert!(near.counts().iter().any(|(n, _)| *n == "field"), "{d}");
        assert!(close_up(&l, 1.5, None, "nothing", 48, 16).is_none());
        assert!(!d.contains("cut="), "{d}");

        // The same box at the eye's feet runs off the bottom of the view.
        let l = list(
            "sg1\nfield Quad 40x40 c=556655 r=-1.5708,0,0\nbox Mesh Box c=aa5533 t=0,0.25,3.6 s=0.5,0.5,0.5\ncam Camera eye=0,1.6,6 at=0,0.5,-12\n",
        );
        let d = close_up(&l, 1.5, None, "box", 48, 16).unwrap().dump(&[]);
        assert!(d.contains("cut=bottom"), "{d}");
    }

    #[test]
    fn an_empty_view_is_dots() {
        let l = list("sg1\ncam Camera eye=0,0,6 at=0,0,0\n");
        let d = sketch(&l, 1.5, None, 4, 2).dump(&[]);
        assert_eq!(d, "sketch 4x2 aspect=1.5\n....\n....\nkey\n");
    }
}
