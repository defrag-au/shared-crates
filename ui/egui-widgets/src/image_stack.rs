//! `ImageStack` — several images as a fanned pile of mounted prints, so a lot
//! of many reads as a lot of many at a glance.
//!
//! ## Why a pile and not a row
//!
//! [`crate::asset_strip`] already lays thumbnails out side by side, overlapping
//! as the count grows, and that is the right shape for a list you scan. This is
//! for the other job: making ONE unit — a sale, a mint, a bundle — look like
//! the several things it actually was. A two-item sale drawn as a single
//! thumbnail reads as a one-item sale; the count line underneath says otherwise
//! and loses, because the picture is what a reader takes in first.
//!
//! ## Tunable on purpose
//!
//! Every proportion here is in [`ImageStackStyle`] rather than baked in as a
//! constant, because the treatment is the difference between "a pile of prints"
//! and "some overlapping squares" and that difference is a matter of a few
//! percent in the mount width and the shadow spread. The storybook drives all
//! of them from sliders. [`ImageStackStyle::default()`] is where a tuned answer
//! gets written down.
//!
//! ## The art is drawn as a mesh, not through `egui::Image`
//!
//! `Image::rotate` and `Image::corner_radius` are mutually exclusive and each
//! silently cancels the other — set rounding after rotation and the rotation is
//! gone, with no warning. The first version of this widget did exactly that,
//! and every print rendered as an upright picture inside a tilted white mount,
//! which is the one arrangement that is unmistakably wrong. The texture is now
//! resolved through the image loader and painted as a rotated quad directly,
//! which is what `Image` does internally minus the footgun. There is no
//! rounding on the art: it sits inside a square mount, and a rounded picture in
//! a square frame is not a thing a print is.
//!
//! ## The shadow is faked, and has to be
//!
//! `epaint` can blur a *rectangle* ([`egui::Shadow`]) but not an arbitrary
//! polygon, and every print here is rotated — so the shadow is approximated by
//! stacking concentric polygons at low alpha, from a spread outer edge inward.
//! Alpha accumulates where they overlap, which produces a falloff that reads as
//! a soft shadow at these sizes. The ramp is eased so the outer layers are
//! fainter than a linear stack would make them: with a linear ramp the steps
//! were visible as stripes along the bottom edge of the pile.
//!
//! ## Small sizes: check, don't assume
//!
//! The first-guess style (a wide sideways step per print) fell apart below
//! about 60px — the peek was a few pixels of white and read as an artefact. The
//! tuned style does not: with almost no step the buried prints are corners,
//! and corners still read at 30px. The bench has a fanned-vs-single row at 30px
//! for exactly this question, and it is what decided that the transaction
//! card fans at every density. [`ImageStack::fan`] stays for a caller who has
//! a reason to show the front image alone; it is no longer the default answer
//! for small.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{ImageStack, StackImage};
//!
//! let images = [
//!     StackImage::new("MachineHeadz527").image(&url_a),
//!     StackImage::new("MachineHeadz357").image(&url_b),
//! ];
//! ImageStack::new(&images).size(96.0).show(ui);
//! ```

use egui::{
    Color32, FontId, Mesh, Pos2, Rect, Response, Sense, Shape, SizeHint, Stroke, TextureOptions,
    Ui, Vec2, emath::Rot2, load::TexturePoll,
};

use crate::theme;

/// The most prints a pile will ever draw.
///
/// Five. A caller can ask for fewer with [`ImageStack::max_shown`]; beyond this
/// the pile is a smear and a count carries the meaning anyway — nobody tells
/// nine prints from twelve by looking, but they do read "12 items".
pub const STACK_MAX: usize = 5;

/// One image in the pile.
#[derive(Clone, Debug)]
pub struct StackImage<'a> {
    pub image_url: Option<&'a str>,
    /// Resolved display name. Drives the fallback initial and the hover, so it
    /// should be the name a reader would recognise rather than an identifier.
    pub label: &'a str,
}

impl<'a> StackImage<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            image_url: None,
            label,
        }
    }

    pub fn image(mut self, url: &'a str) -> Self {
        self.image_url = Some(url);
        self
    }
}

/// Every proportion of the treatment, as fractions of the image edge.
///
/// Fractions rather than pixels so one style holds at every size — a 30px pile
/// and a 200px pile should be the same object photographed from further away,
/// not two different designs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageStackStyle {
    /// The white border, as a fraction of the image edge. This is the single
    /// most load-bearing number here: it is what makes a thumbnail read as a
    /// PRINT rather than as a picture with a line around it.
    pub mount: f32,
    /// Horizontal offset between successive mounts, as a fraction of the image
    /// edge. Below 1.0 the prints overlap and this is how much of each buried
    /// one peeks out; above it they separate into a spread with a gap.
    pub spacing: f32,
    /// Vertical scatter per print. Without it a rotated pile still reads as a
    /// fan around one centre line, which looks filed rather than dropped.
    pub lift: f32,
    /// How far the fan spreads, in degrees. Applied as an alternating sequence
    /// so the pile leans both ways rather than shearing off in one direction.
    pub tilt_deg: f32,
    /// How far the shadow falls below its print.
    pub shadow_offset: f32,
    /// The blur radius — how far the shadow spreads past the print's edge.
    pub shadow_spread: f32,
    /// Peak shadow opacity, directly under the print.
    pub shadow_alpha: u8,
    /// Paper white. Not pure `#ffffff` — against a dark surface that glares and
    /// pulls focus off the artwork it is framing.
    pub paper: Color32,
}

impl Default for ImageStackStyle {
    /// Tuned on the storybook bench, 2026-09-03, against real artwork at 120px.
    ///
    /// The first guess had `spacing: 0.26, lift: 0.05, tilt_deg: 6.0` — the
    /// prints stepped sideways like a hand of cards. What reads as a PILE is
    /// almost no horizontal step, no lift at all, and a wider fan: the buried
    /// prints show as corners poking out from behind the front one, which is
    /// how prints actually sit when dropped.
    fn default() -> Self {
        Self {
            mount: 0.07,
            spacing: 0.055,
            lift: 0.0,
            tilt_deg: 9.3,
            shadow_offset: 0.04,
            shadow_spread: 0.09,
            shadow_alpha: 150,
            paper: Color32::from_rgb(244, 244, 239),
        }
    }
}

/// How many polygons approximate one blurred shadow.
///
/// Fourteen, with an eased ramp. Eight on a linear ramp banded visibly along
/// the bottom edge of a 120px pile. More than this costs draw calls in a list
/// of hundreds of rows for no visible gain.
const SHADOW_LAYERS: usize = 14;

pub struct ImageStack<'a> {
    images: &'a [StackImage<'a>],
    size: f32,
    style: ImageStackStyle,
    fan: bool,
    max_shown: usize,
}

impl<'a> ImageStack<'a> {
    pub fn new(images: &'a [StackImage<'a>]) -> Self {
        Self {
            images,
            size: 96.0,
            style: ImageStackStyle::default(),
            fan: true,
            max_shown: STACK_MAX,
        }
    }

    /// The front image's edge length, in points.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn style(mut self, style: ImageStackStyle) -> Self {
        self.style = style;
        self
    }

    /// Fan the pile, or show the front image alone.
    ///
    /// On by default at every size — see the module docs for why small is not
    /// a reason to turn it off. A caller turning this off should be putting
    /// the count somewhere a reader can still see it.
    pub fn fan(mut self, fan: bool) -> Self {
        self.fan = fan;
        self
    }

    /// Draw at most this many prints. Clamped to `1..=`[`STACK_MAX`].
    pub fn max_shown(mut self, n: usize) -> Self {
        self.max_shown = n.clamp(1, STACK_MAX);
        self
    }

    /// The space this pile will take, without drawing it — for a caller
    /// laying out a fixed-height row.
    pub fn desired_size(&self) -> Vec2 {
        let shown = self.shown_count();
        let s = &self.style;
        let mount = (self.size * s.mount).max(1.0);
        let step = self.size * s.spacing;
        let width = self.size + 2.0 * mount + shown.saturating_sub(1) as f32 * step;
        // Slack for the tilt, the lift and the shadow spread: a rotated quad's
        // corners reach past the box it would otherwise occupy, and clipping
        // the pile is the one thing that makes the whole treatment look broken.
        let slack = match shown > 1 {
            true => self.size * (s.lift * (shown - 1) as f32 + s.shadow_spread + 0.10),
            false => self.size * s.shadow_spread,
        };
        Vec2::new(width + slack * 0.5, self.size + 2.0 * mount + slack)
    }

    fn shown_count(&self) -> usize {
        match self.fan {
            true => self.images.len().min(self.max_shown),
            false => self.images.len().min(1),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let shown = self.shown_count();
        let desired = self.desired_size();
        let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
        if shown == 0 {
            return response;
        }

        let s = &self.style;
        let size = self.size;
        let mount = (size * s.mount).max(1.0);
        let step = size * s.spacing;
        let half = size / 2.0 + mount;
        let single = shown == 1;

        // BACK TO FRONT. Later shapes paint over earlier ones, so the list is
        // walked in reverse: images[0] is the one a reader looks at, and it
        // must not be the one buried.
        for i in (0..shown).rev() {
            let image = &self.images[i];
            let from_back = shown - 1 - i;

            // A LONE IMAGE IS NEVER TILTED. The tilt says "there are more of
            // these behind"; with nothing behind it, it is a crooked picture,
            // and the artwork is what the reader is trying to look at.
            let angle = match single {
                true => 0.0,
                false => tilt_of(i, s.tilt_deg).to_radians(),
            };
            let rot = Rot2::from_angle(angle);

            let cx = rect.left() + mount + size / 2.0 + from_back as f32 * step;
            let cy = rect.top() + mount + size / 2.0 + i as f32 * size * s.lift;
            let center = Pos2::new(cx, cy);

            let quad = |c: Pos2, h: f32| -> Vec<Pos2> {
                [(-h, -h), (h, -h), (h, h), (-h, h)]
                    .iter()
                    .map(|(x, y)| c + rot * Vec2::new(*x, *y))
                    .collect()
            };

            soft_shadow(ui, &quad, center, half, size, s);

            // The mount. Also the separating edge between overlapping prints:
            // it reads as a border without depending on a stroke being drawn
            // between two shapes that are rotated differently.
            ui.painter().add(Shape::convex_polygon(
                quad(center, half),
                s.paper,
                Stroke::NONE,
            ));

            let texture = image
                .image_url
                .and_then(|url| loaded_texture(ui, url, size));
            match texture {
                Some(id) => {
                    // A textured quad rotated about the SAME centre as the
                    // mount — see the module docs for why this is not
                    // `egui::Image`.
                    let art = Rect::from_center_size(center, Vec2::splat(size));
                    let mut mesh = Mesh::with_texture(id);
                    mesh.add_rect_with_uv(
                        art,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    mesh.rotate(rot, center);
                    ui.painter().add(Shape::mesh(mesh));
                }
                // No loader installed, no artwork, or not arrived yet: a tinted
                // block with the initial, so a pile still reads as a pile of
                // somethings before any image does.
                None => {
                    ui.painter().add(Shape::convex_polygon(
                        quad(center, size / 2.0),
                        theme::BG_HIGHLIGHT,
                        Stroke::NONE,
                    ));
                    if let Some(ch) = image.label.chars().next() {
                        ui.painter().text(
                            center,
                            egui::Align2::CENTER_CENTER,
                            ch.to_uppercase().to_string(),
                            FontId::proportional(size * 0.4),
                            theme::TEXT_MUTED,
                        );
                    }
                }
            }
        }

        let names: Vec<&str> = self.images.iter().map(|i| i.label).collect();
        response.on_hover_text(names.join("\n"))
    }
}

/// The texture for `url`, if the loader has it.
///
/// Goes through the same loader `egui::Image` uses, so a URL that renders in an
/// `Image` elsewhere in the app renders here, and the fetch is started on the
/// first frame it is asked for. `None` while it is loading or if it failed —
/// both draw the placeholder, because a print with a broken picture in it is
/// worse than a print still developing.
fn loaded_texture(ui: &Ui, url: &str, size: f32) -> Option<egui::TextureId> {
    let hint = SizeHint::Size {
        width: size.ceil() as u32,
        height: size.ceil() as u32,
        maintain_aspect_ratio: true,
    };
    match ui.ctx().try_load_texture(url, TextureOptions::LINEAR, hint) {
        Ok(TexturePoll::Ready { texture }) => Some(texture.id),
        Ok(TexturePoll::Pending { .. }) | Err(_) => None,
    }
}

/// The house tilt for print `i`, counting from the front.
///
/// ALTERNATING, and scaled off the front rather than distributed evenly: the
/// front sits nearly straight so its subject reads cleanly, and the ones behind
/// lean further and in opposite directions so the pile looks dropped rather
/// than shuffled into a single lean.
///
/// Fixed rather than random. A row is re-laid every frame, so "random" would
/// mean "jitters while you look at it" — and a card cached as an image would
/// differ between two renders of the same transaction.
fn tilt_of(i: usize, spread: f32) -> f32 {
    match i {
        0 => -spread * 0.25,
        n => {
            let magnitude = spread * (0.55 + 0.45 * (n as f32 - 1.0));
            match n % 2 {
                1 => magnitude,
                _ => -magnitude,
            }
        }
    }
}

/// A blurred drop shadow under a rotated quad, faked by accumulation.
///
/// `epaint` blurs rectangles only, and these are rotated, so concentric
/// polygons at low alpha stand in. The alpha per layer is EASED — outer layers
/// carry less than inner ones — because equal alpha per layer gives a linear
/// falloff whose steps were visible as stripes. See the module docs.
fn soft_shadow(
    ui: &Ui,
    quad: &dyn Fn(Pos2, f32) -> Vec<Pos2>,
    center: Pos2,
    half: f32,
    size: f32,
    s: &ImageStackStyle,
) {
    if s.shadow_alpha == 0 {
        return;
    }
    let drop = Vec2::new(0.0, size * s.shadow_offset);
    let spread = size * s.shadow_spread;
    let painter = ui.painter();

    // Weights sum to 1 so the peak, where every layer overlaps, lands on
    // `shadow_alpha`. A quadratic ease front-loads the alpha onto the inner
    // layers; the outer ones contribute a whisper each.
    let weights: Vec<f32> = (0..SHADOW_LAYERS)
        .map(|l| {
            let t = (l + 1) as f32 / SHADOW_LAYERS as f32;
            t * t
        })
        .collect();
    let total: f32 = weights.iter().sum();

    for (layer, w) in weights.iter().enumerate() {
        // Outermost first: each successive layer is smaller, so alpha piles up
        // toward the print's own edge.
        let t = layer as f32 / (SHADOW_LAYERS - 1) as f32;
        let expand = spread * (1.0 - t);
        let alpha = (s.shadow_alpha as f32 * w / total).round().max(1.0) as u8;
        painter.add(Shape::convex_polygon(
            quad(center + drop, half + expand),
            Color32::from_black_alpha(alpha),
            Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_print_is_nearly_straight() {
        // The front carries the subject, so it leans least — otherwise the
        // artwork the reader is trying to look at is the crooked one.
        let front = tilt_of(0, 6.0).abs();
        let second = tilt_of(1, 6.0).abs();
        let third = tilt_of(2, 6.0).abs();
        assert!(front < second, "{front} !< {second}");
        assert!(second < third, "{second} !< {third}");
    }

    #[test]
    fn the_pile_leans_both_ways() {
        // A fan that shears one direction reads as a stack that is falling
        // over. Signs must alternate behind the front.
        assert!(tilt_of(1, 6.0) > 0.0);
        assert!(tilt_of(2, 6.0) < 0.0);
        assert!(tilt_of(3, 6.0) > 0.0);
        assert!(tilt_of(4, 6.0) < 0.0);
    }

    #[test]
    fn tilt_scales_with_spread() {
        assert_eq!(tilt_of(1, 0.0), 0.0);
        assert!(tilt_of(1, 12.0) > tilt_of(1, 6.0));
    }

    /// A pile has to claim room for its own tilt and shadow. Allocating the
    /// bare image size clips the corners of the rotated quads, which is the
    /// failure that makes the whole treatment look like a bug.
    #[test]
    fn desired_size_leaves_room_for_tilt_and_shadow() {
        let images = [
            StackImage::new("a"),
            StackImage::new("b"),
            StackImage::new("c"),
        ];
        let stack = ImageStack::new(&images).size(100.0);
        let d = stack.desired_size();
        assert!(d.y > 100.0, "height {} must exceed the image edge", d.y);
        assert!(d.x > 100.0, "width {} must exceed the image edge", d.x);
    }

    /// Spacing past 1.0 separates the prints; the allocation has to grow with
    /// it or the spread ones are drawn outside the widget's own rect.
    #[test]
    fn desired_width_grows_with_spacing() {
        let images = [StackImage::new("a"), StackImage::new("b")];
        let tight = ImageStack::new(&images)
            .size(100.0)
            .style(ImageStackStyle {
                spacing: 0.2,
                ..Default::default()
            })
            .desired_size()
            .x;
        let spread = ImageStack::new(&images)
            .size(100.0)
            .style(ImageStackStyle {
                spacing: 1.2,
                ..Default::default()
            })
            .desired_size()
            .x;
        assert!(spread > tight + 90.0, "{spread} vs {tight}");
    }

    #[test]
    fn unfanned_shows_one_image_only() {
        let images = [
            StackImage::new("a"),
            StackImage::new("b"),
            StackImage::new("c"),
        ];
        assert_eq!(ImageStack::new(&images).fan(false).shown_count(), 1);
        assert_eq!(ImageStack::new(&images).fan(true).shown_count(), 3);
    }

    /// More images than the pile draws is the ordinary case for a sweep, and it
    /// must not widen the allocation — the count line carries the remainder.
    #[test]
    fn stack_is_capped() {
        let many: Vec<StackImage<'_>> = (0..12).map(|_| StackImage::new("x")).collect();
        assert_eq!(ImageStack::new(&many).shown_count(), STACK_MAX);
        assert_eq!(ImageStack::new(&many).max_shown(3).shown_count(), 3);
        // Clamped both ways: never zero, never past the hard cap.
        assert_eq!(ImageStack::new(&many).max_shown(0).shown_count(), 1);
        assert_eq!(
            ImageStack::new(&many).max_shown(99).shown_count(),
            STACK_MAX
        );
    }
}
