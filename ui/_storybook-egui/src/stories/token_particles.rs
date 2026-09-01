//! Supply as a settling fluid in dynamic vessels — $Aliens, warped time.
//!
//! ## A particle is a QUANTUM OF SUPPLY, not a holder
//!
//! One particle per holder is wrong twice: a whale and a dust holder become the
//! same dot, so mass is misrepresented; and the count swings from 60 ($Aliens)
//! to 173,388 (WRT), so cost is set by the token rather than by us.
//!
//! Each particle here is `supply / N`. The count is a property of the
//! resolution we picked, identical for every token, and **conservation becomes
//! literal** — particles never appear or vanish, they only migrate. The
//! ledger's invariant (a transaction's deltas sum to the net mint) turns into
//! something you watch.
//!
//! ## Vessels are dynamic and individually sized
//!
//! Every pool is its OWN vessel, and **a pool does not exist until it is
//! created**. On $Aliens, Splash materialises 2026-02-22 and CSwap 2026-03-18 —
//! 24 days apart. Drawing both as empty boxes from t=0 would misstate the
//! token's life; the vessel has to appear when the pool does.
//!
//! Vessel width tracks what it holds, eased, so growth and drainage are motion
//! rather than a redraw.
//!
//! ## Fluid, not scatter
//!
//! Particles settle: they pack upward from the floor of their vessel, so the
//! **fill level is the quantity** and you read it the way you read a liquid.
//! A uniform random scatter (the first attempt) encodes amount only as density,
//! which the eye is bad at, and it looked like static rather than substance.

use egui::{pos2, vec2, Color32, Pos2, Rect, Vec2};

use crate::stories::aliens_fixture::{COHORTS, POOL_META, POOL_SERIES, SERIES};
use crate::{ACCENT, TEXT_MUTED};

const SUPPLY: f64 = 1_000_000_000.0;
const N: usize = 1100;
/// Particle spacing when packed. Also the dot pitch, so a full vessel reads as
/// a continuous body rather than a dotted grid.
const PITCH: f32 = 5.0;

/// A vessel is either a cohort or one specific pool.
struct Vessel {
    label: String,
    colour: Color32,
    /// Share of supply, 0..1.
    share: f32,
    /// `false` before the pool that owns it existed.
    exists: bool,
}

fn ramp(name: &str) -> Color32 {
    match name {
        "burn" => Color32::from_rgb(0x3c, 0x5c, 0x94),
        "vesting" => Color32::from_rgb(0x4a, 0x72, 0xb5),
        "wallet" => Color32::from_rgb(0xa3, 0xc6, 0xfb),
        // Unmeasured sits off the ladder.
        "script" => Color32::from_rgb(0xe0, 0xaf, 0x68),
        // Pools get the mid-ramp step, tinted per venue so two pools are
        // distinguishable without leaving the ordinal family.
        "splash" => Color32::from_rgb(0x5b, 0x8a, 0xd6),
        _ => Color32::from_rgb(0x6f, 0xb0, 0xc8),
    }
}

/// The vessels at one sample: cohorts, with `pool` expanded per venue.
fn vessels_at(i: usize) -> Vec<Vessel> {
    let now = SERIES[i].0;
    let mut v = Vec::new();
    for (c, name) in COHORTS.iter().enumerate() {
        if *name == "pool" {
            // Expanded below — the aggregate would hide when each venue began.
            continue;
        }
        v.push(Vessel {
            label: (*name).to_string(),
            colour: ramp(name),
            share: (SERIES[i].1[c] as f64 / SUPPLY) as f32,
            exists: true,
        });
    }
    for (p, (dex, _, first)) in POOL_META.iter().enumerate() {
        v.push(Vessel {
            label: (*dex).to_string(),
            colour: ramp(dex),
            share: (POOL_SERIES[i][p] as f64 / SUPPLY) as f32,
            exists: now >= *first,
        });
    }
    v
}

#[derive(Clone)]
struct P {
    pos: Pos2,
    vessel: usize,
}

#[derive(Clone)]
struct Field {
    ps: Vec<P>,
    t: f32,
    playing: bool,
    last: f64,
    /// Eased vessel widths, so appearing and growing are motion.
    w: Vec<f32>,
}

impl Default for Field {
    fn default() -> Self {
        Self {
            ps: (0..N)
                .map(|_| P {
                    pos: pos2(-50.0, -50.0),
                    vessel: 0,
                })
                .collect(),
            t: 0.0,
            playing: true,
            last: 0.0,
            w: Vec::new(),
        }
    }
}

/// Cumulative warped positions — width proportional to how much moved, so the
/// launch breathes and dormancy compresses.
fn warped_axis() -> Vec<f32> {
    let n = SERIES.len();
    let mut w: Vec<f32> = (1..n)
        .map(|i| {
            let moved: f64 = (0..COHORTS.len())
                .map(|c| ((SERIES[i].1[c] - SERIES[i - 1].1[c]) as f64 / SUPPLY).abs())
                .sum();
            0.25 + moved as f32 * 40.0
        })
        .collect();
    let total: f32 = w.iter().sum();
    for x in &mut w {
        *x /= total;
    }
    let mut acc = vec![0.0];
    for x in &w {
        acc.push(acc.last().unwrap() + x);
    }
    acc
}

fn date(ts: i64) -> String {
    let (mut y, mut d) = (1970, ts / 86_400);
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if d < len {
            break;
        }
        d -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let ml = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0;
    while d >= ml[m] {
        d -= ml[m];
        m += 1;
    }
    format!("{y}-{:02}-{:02}", m + 1, d + 1)
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Supply as a settling fluid — $Aliens")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "1,100 particles, each 1/1100th of supply — conserved. Each pool is its own vessel and \
             appears when it is CREATED: Splash 2026-02-22, CSwap 2026-03-18.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);

    let id = ui.id().with("token-fluid");
    let mut f: Field = ui.data_mut(|d| d.get_temp::<Field>(id)).unwrap_or_default();

    ui.horizontal(|ui| {
        if ui
            .button(if f.playing { "⏸ pause" } else { "▶ play" })
            .clicked()
        {
            f.playing = !f.playing;
        }
        ui.add(egui::Slider::new(&mut f.t, 0.0..=1.0).text("warped time"));
    });

    let axis = warped_axis();
    let i = axis.partition_point(|a| *a < f.t).min(SERIES.len() - 1);
    let vs = vessels_at(i);

    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 330.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 3.0, Color32::from_rgb(0x0b, 0x0c, 0x12));

    // dt from ABSOLUTE time — `stable_dt` is ~0 headless, which previously left
    // the particles three frames behind their own labels.
    let now_t = ui.input(|inp| inp.time);
    let dt = ((now_t - f.last) as f32).clamp(0.0, 0.25);
    f.last = now_t;

    // ---- vessel widths, eased -----------------------------------------
    // Proportional to holding, with a floor so a near-empty vessel is still a
    // place a particle can be seen to leave.
    if f.w.len() != vs.len() {
        f.w = vs.iter().map(|_| 0.0).collect();
    }
    // ⚠️ Width must NOT track share. The first cut sized the vessel by holding
    // AND filled it by holding — double-encoding, so the level barely moved and
    // a wide vessel turned 1,100 particles into a thin puddle. Worse, it read
    // backwards: `script 0.1%` drew the largest box on screen.
    //
    // A tank is a container of fixed bore; the LEVEL is the quantity. So every
    // live vessel gets the same width and the fill height carries the share —
    // which is how a liquid is read, and needs no legend.
    let gap = 8.0;
    let live_n = vs.iter().filter(|v| v.exists).count().max(1) as f32;
    let each = (rect.width() - gap * (live_n + 1.0)) / live_n;
    for (k, v) in vs.iter().enumerate() {
        let target = if v.exists { each } else { 0.0 };
        f.w[k] += (target - f.w[k]) * (1.0 - (-dt * 5.0).exp());
    }

    let mut x = rect.left() + gap;
    let mut bounds = Vec::with_capacity(vs.len());
    for k in 0..vs.len() {
        let w = f.w[k];
        bounds.push(Rect::from_min_size(
            pos2(x, rect.top() + 30.0),
            vec2(w, rect.height() - 62.0),
        ));
        x += w + gap;
    }

    // ---- assign particles to vessels ----------------------------------
    let mut want: Vec<usize> = vs
        .iter()
        .map(|v| {
            if v.exists {
                (v.share * N as f32).round() as usize
            } else {
                0
            }
        })
        .collect();
    // Conservation applies to the RENDER too: rounding must not mint or burn a
    // particle, or the metaphor is lying while it claims to be honest.
    let tot: usize = want.iter().sum();
    if tot != N {
        let big = want
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| **v)
            .map(|(i, _)| i)
            .unwrap_or(0);
        want[big] = want[big] + N - tot;
    }
    let mut have = vec![0usize; vs.len()];
    for p in &f.ps {
        if p.vessel < vs.len() {
            have[p.vessel] += 1;
        } else {
            p_reset(&mut have);
        }
    }
    for c in 0..vs.len() {
        while have[c] > want[c] {
            let Some(k) = (0..vs.len()).find(|k| have[*k] < want[*k]) else {
                break;
            };
            let Some(p) = f.ps.iter_mut().find(|p| p.vessel == c) else {
                break;
            };
            p.vessel = k;
            have[c] -= 1;
            have[k] += 1;
        }
    }

    // ---- settle: pack upward from each vessel's floor ------------------
    // Rank within the vessel decides the packed slot, so the FILL LEVEL is the
    // quantity — read like a liquid, not inferred from dot density.
    let mut rank = vec![0usize; vs.len()];
    for p in f.ps.iter_mut() {
        let b = bounds[p.vessel];
        let r = rank[p.vessel];
        rank[p.vessel] += 1;
        let cols = ((b.width() / PITCH).floor() as usize).max(1);
        let (row, col) = (r / cols, r % cols);
        // Half-offset alternate rows: hexagonal packing reads as substance,
        // a square lattice reads as a grid.
        let stagger = if row % 2 == 0 { 0.0 } else { PITCH * 0.5 };
        let target = pos2(
            b.left() + PITCH * 0.5 + col as f32 * PITCH + stagger,
            b.bottom() - PITCH * 0.5 - row as f32 * PITCH * 0.87,
        );
        if p.pos.x < 0.0 {
            p.pos = pos2(target.x, rect.top());
        }
        // Falling is faster than drifting sideways, which is what makes it read
        // as settling under gravity rather than as points lerping to a layout.
        let k = 1.0 - (-dt * 7.0).exp();
        p.pos.x += (target.x - p.pos.x) * k * 0.7;
        p.pos.y += (target.y - p.pos.y) * k;
    }

    // ---- paint ---------------------------------------------------------
    for (k, v) in vs.iter().enumerate() {
        if f.w[k] < 1.0 {
            continue;
        }
        let b = bounds[k];
        ui.painter().rect_stroke(
            b,
            2.0,
            egui_widgets::theme::hairline(Color32::from_rgb(0x1e, 0x22, 0x33)),
            egui::StrokeKind::Inside,
        );
        if f.w[k] > 34.0 {
            ui.painter().text(
                pos2(b.left(), b.top() - 16.0),
                egui::Align2::LEFT_TOP,
                format!("{}  {:.1}%", v.label, 100.0 * v.share),
                egui::FontId::proportional(10.5),
                v.colour,
            );
        }
    }
    for p in &f.ps {
        ui.painter()
            .circle_filled(p.pos, PITCH * 0.42, vs[p.vessel].colour);
    }

    ui.painter().text(
        pos2(rect.left() + gap, rect.bottom() - 22.0),
        egui::Align2::LEFT_TOP,
        format!("{}   ·   {N} particles, always", date(SERIES[i].0)),
        egui::FontId::proportional(11.0),
        Color32::from_rgb(0x6b, 0x73, 0x94),
    );

    if f.playing {
        f.t = (now_t as f32 * 0.045).rem_euclid(1.0);
        ui.ctx().request_repaint();
    }
    ui.data_mut(|d| d.insert_temp(id, f));

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Watch: the launchpad (script) drains; Splash appears, then CSwap 24 days later; the \
             vesting vessel charges then partly discharges — and what is left stays, past maturity.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
}

/// A particle pointing at a vessel that no longer exists is a bug, not a state
/// to tolerate — vessels only ever appear here, so this should be unreachable.
fn p_reset(have: &mut [usize]) {
    if let Some(h) = have.first_mut() {
        *h += 1;
    }
}
