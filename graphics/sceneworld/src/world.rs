use std::fmt;

use scenegraph::{Kind, Node, Prim, Scene, Shade, Vec3};

use crate::error::Error;
use crate::hilbert::{hilbert_index, hilbert_side, hilbert_xy, Print};

/// Default cell size in world units. Origin cell is centred on (0,0,0).
pub const CELL: f32 = 8.0;
/// Hilbert palace: 2^6 × 2^6 cells (the regl example's 6th-order wing).
pub const PALACE_ORDER: u32 = 6;
/// Cells along the Hilbert curve to emit. The regl palace streams
/// paintings with `renderDist=20`; this is the architecture stretch.
/// Chebyshev `span` on a palace dump *is* this reach, not a square.
pub const PALACE_SPAN: u32 = 24;
/// Wall / room height.
pub const HEIGHT: f32 = 3.0;
/// Rest asset-print size (1:1). Pane may rewrite `w`×`h` from pixels.
pub const PRINT: f32 = 1.5;
/// Opening on a shared face. Span=0 has no shared faces, so no doors.
pub const DOOR: f32 = 2.0;
/// Slab front offset into the room (local +Z on a wall-facing Quad).
pub const HANG_Z: f32 = 0.04;
/// Label em height.
pub const TITLE_SIZE: f32 = 0.12;
/// Title origin on a 1.5-tall print (`-(PRINT/2 − 0.1)`).
pub const TITLE_Y: f32 = -0.65;

const CAM_EYE: Vec3 = Vec3 {
    x: 0.0,
    y: 1.6,
    z: 2.0,
};
const CAM_AT: Vec3 = Vec3 {
    x: 0.0,
    y: 1.5,
    z: -1.0,
};
const FONT: &str = "ibm";
const LAMP_I: f32 = 0.6;

const FLOOR_C: [u8; 4] = [0xc8, 0xc2, 0xb4, 255];
const CEIL_C: [u8; 4] = [0xd8, 0xd4, 0xcc, 255];
const PLINTH_C: [u8; 4] = [0xb0, 0xa0, 0x90, 255];
const LAMP_C: [u8; 4] = [0xff, 0xe8, 0xc8, 255];

/// Cardinal face of a cell. +X is east, +Z is south.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wall {
    N,
    E,
    S,
    W,
}

impl Wall {
    pub fn as_str(self) -> &'static str {
        match self {
            Wall::N => "n",
            Wall::E => "e",
            Wall::S => "s",
            Wall::W => "w",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "n" => Ok(Wall::N),
            "e" => Ok(Wall::E),
            "s" => Ok(Wall::S),
            "w" => Ok(Wall::W),
            other => Err(Error::UnknownWall(other.to_string())),
        }
    }

    pub(crate) fn dump_rank(self) -> u8 {
        match self {
            Wall::N => 0,
            Wall::E => 1,
            Wall::S => 2,
            Wall::W => 3,
        }
    }

    /// Cursor-cell insert order matches `gallery.sg1`: n, e, w, s.
    fn emit_order() -> [Wall; 4] {
        [Wall::N, Wall::E, Wall::W, Wall::S]
    }

    /// Cell on the other side of this face. +X east, +Z south.
    pub fn neighbour(self, c: (i32, i32)) -> (i32, i32) {
        match self {
            Wall::N => (c.0, c.1 - 1),
            Wall::S => (c.0, c.1 + 1),
            Wall::E => (c.0 + 1, c.1),
            Wall::W => (c.0 - 1, c.1),
        }
    }

    /// The west/north cell of a shared face owns the quads so the plane
    /// is drawn once. East and south faces are the owned ones.
    pub fn owns_shared(self) -> bool {
        matches!(self, Wall::E | Wall::S)
    }

    pub fn opposite(self) -> Wall {
        match self {
            Wall::N => Wall::S,
            Wall::S => Wall::N,
            Wall::E => Wall::W,
            Wall::W => Wall::E,
        }
    }

    fn color(self) -> [u8; 4] {
        match self {
            Wall::N => [0x8a, 0x9b, 0xb0, 255],
            Wall::E => [0x9a, 0x8a, 0x7a, 255],
            Wall::W => [0x7a, 0x8a, 0x9a, 255],
            Wall::S => [0x8a, 0x80, 0x78, 255],
        }
    }

    fn xf(self, cell: f32) -> (Vec3, Vec3) {
        let half = cell * 0.5;
        let y = HEIGHT * 0.5;
        let pi = std::f32::consts::PI;
        let hpi = std::f32::consts::FRAC_PI_2;
        match self {
            Wall::N => (Vec3::new(0.0, y, -half), Vec3::ZERO),
            Wall::E => (Vec3::new(half, y, 0.0), Vec3::new(0.0, -hpi, 0.0)),
            Wall::W => (Vec3::new(-half, y, 0.0), Vec3::new(0.0, hpi, 0.0)),
            Wall::S => (Vec3::new(0.0, y, half), Vec3::new(0.0, pi, 0.0)),
        }
    }
}

impl fmt::Display for Wall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One print on a cell wall. `image` is an opaque `m=` key (AssetId or not).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hang {
    pub c: (i32, i32),
    pub wall: Wall,
    pub name: String,
    pub image: String,
    pub title: Option<String>,
}

impl Hang {
    pub fn new(
        c: (i32, i32),
        wall: Wall,
        name: impl Into<String>,
        image: impl Into<String>,
    ) -> Self {
        Self {
            c,
            wall,
            name: name.into(),
            image: image.into(),
            title: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Permille of non-corridor faces that still open (seeded shortcuts).
const SHORTCUT_PERMILLE: u32 = 350;

/// Sparse hang table plus a Chebyshev window onto [`Scene`].
///
/// `order` is a Hilbert palace: the map and the prints are generated,
/// not listed. Hand dumps leave it `None` and keep an open grid in the
/// window (the `gallery.sg1` golden).
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    pub name: String,
    pub cell: f32,
    pub span: u32,
    pub at: (i32, i32),
    pub order: Option<u32>,
    pub seed: u32,
    hangs: Vec<Hang>,
    prints: Vec<Print>,
}

impl World {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cell: CELL,
            span: 0,
            at: (0, 0),
            order: None,
            seed: 0,
            hangs: Vec::new(),
            prints: Vec::new(),
        }
    }

    pub fn hangs(&self) -> &[Hang] {
        &self.hangs
    }

    pub fn prints(&self) -> &[Print] {
        &self.prints
    }

    pub fn push_print(&mut self, print: Print) -> Result<(), Error> {
        check_token(&print.image, "m")?;
        if !print.title.is_empty() {
            check_token(&print.title, "title")?;
        }
        self.prints.push(print);
        Ok(())
    }

    pub fn push(&mut self, hang: Hang) -> Result<(), Error> {
        check_token(&hang.name, "name")?;
        check_token(&hang.image, "m")?;
        if let Some(t) = &hang.title {
            check_token(t, "title")?;
        }
        if self.hangs.iter().any(|h| h.name == hang.name) {
            return Err(Error::DuplicateHang(hang.name));
        }
        if let Some(h) = self
            .hangs
            .iter()
            .find(|h| h.c == hang.c && h.wall == hang.wall)
        {
            return Err(Error::DuplicateSlot {
                wall: hang.wall,
                x: h.c.0,
                z: h.c.1,
            });
        }
        self.hangs.push(hang);
        Ok(())
    }

    /// Cell containing world XZ. Origin cell is centred on the origin;
    /// +X is east, +Z is south. The +X/+Z faces belong to the neighbour.
    pub fn cell_of(&self, x: f32, z: f32) -> (i32, i32) {
        let cell = self.cell.max(1e-4);
        let half = cell * 0.5;
        (
            ((x + half) / cell).floor() as i32,
            ((z + half) / cell).floor() as i32,
        )
    }

    pub fn in_grid(&self, c: (i32, i32)) -> bool {
        let Some(order) = self.order else {
            return true;
        };
        let Ok(n) = hilbert_side(order) else {
            return false;
        };
        let n = n as i32;
        c.0 >= 0 && c.1 >= 0 && c.0 < n && c.1 < n
    }

    pub fn in_window(&self, c: (i32, i32)) -> bool {
        if !self.in_grid(c) {
            return false;
        }
        if let Some(order) = self.order {
            return self.on_stretch(order, c);
        }
        chebyshev(c, self.at) <= self.span
    }

    fn on_stretch(&self, order: u32, c: (i32, i32)) -> bool {
        let Ok(ia) = hilbert_index(order, self.at.0 as u32, self.at.1 as u32) else {
            return false;
        };
        let Ok(ic) = hilbert_index(order, c.0 as u32, c.1 as u32) else {
            return false;
        };
        ia.abs_diff(ic) <= self.span
    }

    /// Re-emit when the walk has used half the loaded stretch (palace:
    /// Hilbert index; hand dump: Chebyshev).
    pub fn should_page(&self, cell: (i32, i32)) -> bool {
        if cell == self.at || !self.in_grid(cell) {
            return false;
        }
        let keep = (self.span / 2).max(1);
        if let Some(order) = self.order {
            let Ok(ia) = hilbert_index(order, self.at.0 as u32, self.at.1 as u32) else {
                return false;
            };
            let Ok(ic) = hilbert_index(order, cell.0 as u32, cell.1 as u32) else {
                return false;
            };
            return ia.abs_diff(ic) >= keep;
        }
        chebyshev(cell, self.at) >= keep
    }

    /// Hilbert corridor: consecutive curve cells share a door.
    pub fn corridor(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        let Some(order) = self.order else {
            return false;
        };
        if !self.in_grid(a) || !self.in_grid(b) {
            return false;
        }
        let Ok(ia) = hilbert_index(order, a.0 as u32, a.1 as u32) else {
            return false;
        };
        let Ok(ib) = hilbert_index(order, b.0 as u32, b.1 as u32) else {
            return false;
        };
        ia.abs_diff(ib) == 1
    }

    fn shortcut(&self, c: (i32, i32), wall: Wall) -> bool {
        let nb = wall.neighbour(c);
        if !self.in_grid(nb) || self.corridor(c, nb) {
            return false;
        }
        let (oc, ow) = if wall.owns_shared() {
            (c, wall)
        } else {
            (nb, wall.opposite())
        };
        mix(oc.0, oc.1, ow.dump_rank(), self.seed) % 1000 < SHORTCUT_PERMILLE
    }

    fn face_open(&self, c: (i32, i32), wall: Wall) -> bool {
        let nb = wall.neighbour(c);
        if !self.in_window(nb) {
            return false;
        }
        if self.order.is_none() {
            return true;
        }
        self.corridor(c, nb) || self.shortcut(c, wall)
    }

    fn hang_at(&self, c: (i32, i32), wall: Wall) -> Option<Hang> {
        if let Some(h) = self.hangs.iter().find(|h| h.c == c && h.wall == wall) {
            return Some(h.clone());
        }
        self.generated_hang(c, wall)
    }

    fn generated_hang(&self, c: (i32, i32), wall: Wall) -> Option<Hang> {
        let order = self.order?;
        if self.prints.is_empty() || !self.in_grid(c) {
            return None;
        }
        let d = hilbert_index(order, c.0 as u32, c.1 as u32).ok()?;
        let i = d * 4 + u32::from(wall.dump_rank());
        let print = &self.prints[i as usize % self.prints.len()];
        let mut hang = Hang::new(c, wall, format!("p-{i}"), print.image.clone());
        if !print.title.is_empty() {
            hang = hang.title(print.title.clone());
        }
        Some(hang)
    }

    fn short_names(&self) -> bool {
        self.order.is_none()
    }

    /// Rooms in the Chebyshev window (empty cells still rooms), plus
    /// cursor furniture, a camera in the cursor cell, and a sun.
    pub fn emit(&self) -> Result<Scene, Error> {
        if self.cell <= 0.0 {
            return Err(Error::Parse("cell must be > 0".into()));
        }
        if self.name.is_empty() {
            return Err(Error::Parse("missing world name".into()));
        }
        let mut scene = Scene::new();
        let short = self.short_names();
        if self.order.is_some() {
            emit_palace_shell(&mut scene, self)?;
            emit_curve(&mut scene, self)?;
        } else {
            if self.in_window(self.at) {
                emit_cell(&mut scene, self, self.at, short)?;
            }
            let s = self.span as i32;
            for dz in -s..=s {
                for dx in -s..=s {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let c = (self.at.0 + dx, self.at.1 + dz);
                    if self.in_window(c) {
                        emit_cell(&mut scene, self, c, false)?;
                    }
                }
            }
        }
        let origin = cell_origin(self.at, self.cell);
        scene.insert(Node::camera(
            "cam",
            Vec3::new(
                origin.x + CAM_EYE.x,
                origin.y + CAM_EYE.y,
                origin.z + CAM_EYE.z,
            ),
            Vec3::new(
                origin.x + CAM_AT.x,
                origin.y + CAM_AT.y,
                origin.z + CAM_AT.z,
            ),
        ))?;
        scene.insert(Node::light("sun", Vec3::new(-0.4, -1.0, -0.5)))?;
        Ok(scene)
    }
}

/// Copy the live camera onto a freshly emitted window so a walk that
/// crosses a cell keeps its eye/at.
pub fn transplant_cam(from: &Scene, to: &mut Scene) {
    let Some(src) = from.get("cam") else {
        return;
    };
    let kind = src.kind.clone();
    if let Some(dst) = to.get_mut("cam") {
        dst.kind = kind;
        dst.xf = src.xf;
    }
}

fn chebyshev(a: (i32, i32), b: (i32, i32)) -> u32 {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

fn cell_origin(c: (i32, i32), cell: f32) -> Vec3 {
    Vec3::new(c.0 as f32 * cell, 0.0, c.1 as f32 * cell)
}

/// One floor and one ceiling for the whole Hilbert wing — never resized
/// on page, so the ground does not jump.
fn emit_palace_shell(scene: &mut Scene, world: &World) -> Result<(), Error> {
    let Some(order) = world.order else {
        return Ok(());
    };
    let n = hilbert_side(order)? as f32;
    let size = n * world.cell;
    let centre = (n - 1.0) * world.cell * 0.5;
    let hpi = std::f32::consts::FRAC_PI_2;
    scene.insert(
        Node::quad("floor", size, size)
            .color(FLOOR_C)
            .at(Vec3::new(centre, 0.0, centre))
            .rot(Vec3::new(-hpi, 0.0, 0.0)),
    )?;
    scene.insert(
        Node::quad("ceil", size, size)
            .color(CEIL_C)
            .at(Vec3::new(centre, HEIGHT, centre))
            .rot(Vec3::new(hpi, 0.0, 0.0)),
    )?;
    Ok(())
}

/// Cells whose Hilbert index is within `span` of the cursor — a winding
/// stretch, not a Chebyshev square of rooms.
fn emit_curve(scene: &mut Scene, world: &World) -> Result<(), Error> {
    let Some(order) = world.order else {
        return Ok(());
    };
    let i0 = hilbert_index(order, world.at.0 as u32, world.at.1 as u32)?;
    let n = hilbert_side(order)?;
    let max = n.saturating_mul(n).saturating_sub(1);
    let start = i0.saturating_sub(world.span);
    let end = i0.saturating_add(world.span).min(max);
    for d in start..=end {
        let c = hilbert_xy(order, d)?;
        emit_cell(scene, world, c, false)?;
    }
    Ok(())
}

fn cell_group(world: &str, c: (i32, i32), cursor: bool) -> String {
    if cursor {
        world.to_string()
    } else {
        format!("c{}_{}", c.0, c.1)
    }
}

fn wall_name(group: &str, wall: Wall, cursor: bool) -> String {
    if cursor {
        format!("wall-{}", wall.as_str())
    } else {
        format!("wall-{}-{group}", wall.as_str())
    }
}

fn title_name(hang: &str) -> String {
    match hang.strip_prefix("p-") {
        Some(rest) if !rest.is_empty() => format!("title-{rest}"),
        _ => format!("title-{hang}"),
    }
}

fn check_token(s: &str, what: &str) -> Result<(), Error> {
    if s.is_empty() || s.chars().any(|ch| ch.is_whitespace() || ch == '=') {
        return Err(Error::Parse(format!("bad {what}")));
    }
    Ok(())
}

fn with_intensity(mut n: Node, i: f32) -> Node {
    if let Kind::Light { intensity, .. } = &mut n.kind {
        *intensity = i;
    }
    n
}

fn mix(x: i32, z: i32, w: u8, seed: u32) -> u32 {
    let mut h = seed;
    h ^= x as u32;
    h = h.wrapping_mul(0x9e3779b1);
    h ^= z as u32;
    h = h.wrapping_mul(0x9e3779b1);
    h ^= u32::from(w);
    h.wrapping_mul(0x9e3779b1)
}

fn emit_cell(scene: &mut Scene, world: &World, c: (i32, i32), cursor: bool) -> Result<(), Error> {
    let group = cell_group(&world.name, c, cursor);
    let origin = cell_origin(c, world.cell);
    let mut g = Node::group(&group);
    if origin != Vec3::ZERO {
        g = g.at(origin);
    }
    scene.insert(g)?;

    if world.order.is_none() {
        let floor = if cursor {
            "floor".to_string()
        } else {
            format!("{group}-floor")
        };
        let ceil = if cursor {
            "ceil".to_string()
        } else {
            format!("{group}-ceil")
        };
        let hpi = std::f32::consts::FRAC_PI_2;
        scene.insert(
            Node::quad(&floor, world.cell, world.cell)
                .parent(&group)
                .color(FLOOR_C)
                .rot(Vec3::new(-hpi, 0.0, 0.0)),
        )?;
        scene.insert(
            Node::quad(&ceil, world.cell, world.cell)
                .parent(&group)
                .color(CEIL_C)
                .at(Vec3::new(0.0, HEIGHT, 0.0))
                .rot(Vec3::new(hpi, 0.0, 0.0)),
        )?;
    }

    for wall in Wall::emit_order() {
        emit_wall(scene, world, c, &group, cursor, wall)?;
    }

    if cursor {
        let plinth = if cursor {
            "plinth".to_string()
        } else {
            format!("{group}-plinth")
        };
        let lamp = if cursor {
            "lamp".to_string()
        } else {
            format!("{group}-lamp")
        };
        scene.insert(
            Node::mesh(&plinth, Prim::Box)
                .color(PLINTH_C)
                .parent(&group)
                .at(Vec3::new(0.0, 0.2, 0.0))
                .scale(Vec3::new(0.8, 0.4, 0.8)),
        )?;
        scene.insert(with_intensity(
            Node::point(&lamp)
                .color(LAMP_C)
                .parent(&group)
                .at(Vec3::new(0.0, 2.6, 0.0)),
            LAMP_I,
        ))?;
    }
    Ok(())
}

fn door_width(cell: f32) -> f32 {
    DOOR.min(cell * 0.4).max(0.0)
}

fn jamb_w(cell: f32, door: f32) -> f32 {
    ((cell - door) * 0.5).max(0.0)
}

/// Local X of the left jamb centre — hangs sit here when the face is open.
fn hang_door_x(cell: f32, door: f32) -> f32 {
    -(door + jamb_w(cell, door)) * 0.5
}

fn posed(mut n: Node, t: Vec3, r: Vec3) -> Node {
    n = n.at(t);
    if r != Vec3::ZERO {
        n = n.rot(r);
    }
    n
}

fn emit_hang(scene: &mut Scene, hang: &Hang, parent: &str, hx: f32) -> Result<(), Error> {
    scene.insert(
        Node::slab(&hang.name, PRINT, PRINT)
            .radius(0.0)
            .shade(Shade::Unlit)
            .image(&hang.image)
            .parent(parent)
            .at(Vec3::new(hx, 0.0, HANG_Z))
            .tag("hang"),
    )?;
    if let Some(text) = &hang.title {
        scene.insert(
            Node::label(title_name(&hang.name), FONT, TITLE_SIZE, text)
                .parent(&hang.name)
                .at(Vec3::new(0.0, TITLE_Y, HANG_Z)),
        )?;
    }
    Ok(())
}

fn emit_wall(
    scene: &mut Scene,
    world: &World,
    c: (i32, i32),
    group: &str,
    cursor: bool,
    wall: Wall,
) -> Result<(), Error> {
    let wname = wall_name(group, wall, cursor);
    let (t, r) = wall.xf(world.cell);
    let open = world.face_open(c, wall);
    // Palace: an opening is a missing wall, not a cut-out. Solid shared
    // faces are drawn once (the owner) so the GPU does not z-fight.
    if world.order.is_some() {
        if open {
            return Ok(());
        }
        let nb = wall.neighbour(c);
        if world.in_window(nb) && !wall.owns_shared() {
            scene.insert(posed(Node::group(&wname).parent(group).tag("wall"), t, r))?;
        } else {
            scene.insert(posed(
                Node::quad(&wname, world.cell, HEIGHT)
                    .parent(group)
                    .color(wall.color())
                    .tag("wall"),
                t,
                r,
            ))?;
        }
        if let Some(hang) = world.hang_at(c, wall) {
            emit_hang(scene, &hang, &wname, 0.0)?;
        }
        return Ok(());
    }
    if open {
        scene.insert(posed(Node::group(&wname).parent(group).tag("wall"), t, r))?;
        if wall.owns_shared() {
            let door = door_width(world.cell);
            let jw = jamb_w(world.cell, door);
            if jw > 1e-4 {
                let x = hang_door_x(world.cell, door).abs();
                scene.insert(
                    Node::quad(format!("{wname}-l"), jw, HEIGHT)
                        .parent(&wname)
                        .color(wall.color())
                        .at(Vec3::new(-x, 0.0, 0.0)),
                )?;
                scene.insert(
                    Node::quad(format!("{wname}-r"), jw, HEIGHT)
                        .parent(&wname)
                        .color(wall.color())
                        .at(Vec3::new(x, 0.0, 0.0)),
                )?;
            }
        }
    } else {
        scene.insert(posed(
            Node::quad(&wname, world.cell, HEIGHT)
                .parent(group)
                .color(wall.color())
                .tag("wall"),
            t,
            r,
        ))?;
    }
    if let Some(hang) = world.hang_at(c, wall) {
        let hx = if open {
            hang_door_x(world.cell, door_width(world.cell))
        } else {
            0.0
        };
        emit_hang(scene, &hang, &wname, hx)?;
    }
    Ok(())
}
