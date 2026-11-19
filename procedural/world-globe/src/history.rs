//! Playing a planet's crater history back: what each impact looks like at a
//! moment of the playback.
//!
//! A playback is a sequence of epochs, morphed between (see the globe
//! viewer's timeline). Stepping forward from epoch `e − 1` to `e`, epoch
//! `e`'s impacts land during the morph, each at its own moment, as a
//! [`Look::flash`] and then rays. Rays mark a young crater, as they do on
//! the Moon: full on the epoch it lands, faint the epoch after, gone after
//! that (the land's own wear is in the sheets).

/// Where the playback is: settled on `from` (`from == to`), or `t` (0..1,
/// linear) of the way from `from` to `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub from: usize,
    pub to: usize,
    pub t: f32,
}

/// How one impact looks at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// How far through its landing flash (0..1), while it lands.
    pub flash: Option<f32>,
    /// Brightness of its rays, 0..1.
    pub rays: f32,
}

/// Share of a forward step during which impacts begin to land: the rest of
/// the morph lets the last of them play out.
const LANDING_WINDOW: f32 = 0.6;
/// Share of a step one flash lasts.
const FLASH: f32 = 0.3;
/// Ray brightness one epoch after landing.
const RAYS_AFTER_ONE: f32 = 0.35;

/// When, as a share of a forward step, the impact with `seed` (0..1) lands.
pub fn landing(seed: f32) -> f32 {
    seed.clamp(0.0, 1.0) * LANDING_WINDOW
}

/// Ray brightness `age` epochs after landing (negative: not yet landed).
fn rays_at(age: isize) -> f32 {
    match age {
        0 => 1.0,
        1 => RAYS_AFTER_ONE,
        _ => 0.0,
    }
}

/// How the impact of `epoch` with `seed` looks at `clock`.
pub fn look(epoch: u32, seed: f32, clock: Clock) -> Look {
    let epoch = epoch as isize;
    let (from, to) = (clock.from as isize, clock.to as isize);
    let at = |e: isize| rays_at(e - epoch);
    if from == to {
        return Look {
            flash: None,
            rays: at(from),
        };
    }
    let t = clock.t.clamp(0.0, 1.0);
    if to == from + 1 && epoch == to {
        // Landing now: a flash at its moment, then its rays come up.
        let since = (t - landing(seed)) / FLASH;
        return Look {
            flash: (0.0..=1.0).contains(&since).then_some(since),
            rays: (since.clamp(0.0, 1.0)) * rays_at(0),
        };
    }
    // Anything else (older craters, or stepping back): the rays fade between
    // the two epochs' brightness. Stepping back takes impacts away quietly.
    Look {
        flash: None,
        rays: at(from) + (at(to) - at(from)) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(at: usize) -> Clock {
        Clock {
            from: at,
            to: at,
            t: 0.0,
        }
    }

    fn stepping(from: usize, to: usize, t: f32) -> Clock {
        Clock { from, to, t }
    }

    #[test]
    fn rays_mark_young_craters_and_fade() {
        assert_eq!(look(2, 0.5, settled(2)).rays, 1.0, "landed this epoch");
        assert_eq!(look(2, 0.5, settled(3)).rays, RAYS_AFTER_ONE);
        assert_eq!(look(2, 0.5, settled(4)).rays, 0.0, "worn away");
        assert_eq!(look(2, 0.5, settled(1)).rays, 0.0, "not landed yet");
        assert_eq!(look(2, 0.5, settled(2)).flash, None, "no flash at rest");
    }

    #[test]
    fn an_impact_flashes_once_at_its_moment_on_a_forward_step() {
        let seed = 0.5;
        let lands = landing(seed);
        let at = |t: f32| look(3, seed, stepping(2, 3, t));
        assert_eq!(at(lands - 0.01).flash, None, "not yet");
        assert_eq!(at(lands - 0.01).rays, 0.0);
        let mid = at(lands + FLASH * 0.5).flash.expect("flashing");
        assert!((mid - 0.5).abs() < 1e-4);
        assert_eq!(at(lands + FLASH + 0.01).flash, None, "over");
        assert_eq!(at(1.0).rays, 1.0, "rays fully up by the end");
        // Every seed has landed and finished by the end of the step.
        assert!(landing(1.0) + FLASH <= 1.0);
    }

    #[test]
    fn older_rays_fade_through_a_step_and_steps_back_are_quiet() {
        // Epoch 2's rays dim from full to faint as the playback moves to 3.
        let mid = look(2, 0.1, stepping(2, 3, 0.5)).rays;
        assert!((mid - (1.0 + RAYS_AFTER_ONE) / 2.0).abs() < 1e-5);
        // Stepping back from 3 to 2: epoch 3's impact un-lands, no flash.
        let back = look(3, 0.1, stepping(3, 2, 0.5));
        assert_eq!(back.flash, None);
        assert!((back.rays - 0.5).abs() < 1e-5);
    }
}
