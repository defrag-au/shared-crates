//! The motion axis — that it exists, and that it REACHES something.
//!
//! `MotionTokens` shipped fully formed and fully unread: `duration`,
//! `travel_allowed` and `MotionMode` were all defined and tested, while every
//! animated widget in the crate passed a literal `0.3` to the tween helpers. It
//! was the exact failure this crate keeps hitting — an axis that is wired,
//! tested and entirely invisible — and no test caught it, because a hardcoded
//! duration animates perfectly well.
//!
//! So these tests check two different things. The first half is the token
//! algebra. The second half (`reaches_*`) is the part that would have failed
//! before the wiring: it asserts the axis is observable from the outside.

use egui_widgets::machine::{HState, Machine, StatePath};
use egui_widgets::motion::Easing;
use egui_widgets::theme::{MotionMode, MotionTokens, Speed, Theme, ThemeExt};

fn tokens(mode: MotionMode) -> MotionTokens {
    MotionTokens {
        mode,
        ..MotionTokens::standard()
    }
}

#[test]
fn speed_ordering_holds_in_every_mode() {
    // Fast must stay faster than Normal than Slow whatever the mode does to the
    // magnitudes, or "fast" stops meaning anything.
    for mode in [MotionMode::Full, MotionMode::Reduced] {
        let m = tokens(mode);
        assert!(
            m.duration(Speed::Fast) < m.duration(Speed::Normal),
            "{mode:?}: fast is not faster than normal"
        );
        assert!(
            m.duration(Speed::Normal) < m.duration(Speed::Slow),
            "{mode:?}: normal is not faster than slow"
        );
    }
}

#[test]
fn none_zeroes_every_speed() {
    // The tween helpers treat 0.0 as "snap", which is what lets a call site
    // stay branch-free under screenshot mode.
    let m = tokens(MotionMode::None);
    for speed in Speed::ALL {
        assert_eq!(m.duration(*speed), 0.0, "{speed:?} should snap");
    }
}

#[test]
fn reduced_shortens_but_never_zeroes() {
    // Reduced is "a fade that still happens, quickly" — not None. A zero here
    // would silently collapse the two modes into one.
    let full = tokens(MotionMode::Full);
    let reduced = tokens(MotionMode::Reduced);
    for speed in Speed::ALL {
        let (f, r) = (full.duration(*speed), reduced.duration(*speed));
        assert!(r > 0.0, "{speed:?}: reduced must still animate");
        assert!(r < f, "{speed:?}: reduced must be shorter than full");
    }
}

#[test]
fn travel_is_allowed_only_at_full() {
    assert!(tokens(MotionMode::Full).travel_allowed());
    assert!(!tokens(MotionMode::Reduced).travel_allowed());
    assert!(!tokens(MotionMode::None).travel_allowed());
}

#[test]
fn overshoot_does_not_survive_reduced_motion() {
    // `MotionMode::Reduced` is documented as "cross-fades survive; travel and
    // overshoot do not". Duration and travel were enforceable; overshoot was
    // not, so a reduced-motion reader still got the one curve that moves past
    // its target and comes back.
    for mode in [MotionMode::Reduced, MotionMode::None] {
        let m = tokens(mode);
        assert_ne!(
            m.easing(Easing::OutBack),
            Easing::OutBack,
            "{mode:?} still overshoots"
        );
        // Degrading to a curve that overshoots differently would be no fix.
        let degraded = m.easing(Easing::OutBack);
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            assert!(
                degraded.apply(t) <= 1.0 + f32::EPSILON,
                "{mode:?}: degraded easing exceeds its target at t={t}"
            );
        }
    }
}

#[test]
fn full_motion_leaves_every_easing_alone() {
    let m = tokens(MotionMode::Full);
    for e in [
        Easing::Linear,
        Easing::OutCubic,
        Easing::InOutCubic,
        Easing::OutBack,
    ] {
        assert_eq!(m.easing(e), e, "Full must not degrade {e:?}");
    }
}

#[test]
fn non_overshooting_easings_pass_through_unchanged() {
    // Only overshoot is the problem; a linear or cubic curve is already
    // reduced-motion safe and silently swapping it would be a second bug.
    for mode in MotionMode::ALL {
        let m = tokens(*mode);
        for e in [Easing::Linear, Easing::OutCubic, Easing::InOutCubic] {
            assert_eq!(m.easing(e), e, "{mode:?} altered {e:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// The half that would have failed before the wiring.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum S {
    Root,
    Deep,
}

impl HState for S {
    fn path(&self) -> StatePath {
        match self {
            S::Root => StatePath::root("a"),
            S::Deep => StatePath::root("a").then("b"),
        }
    }
}

/// A context with `theme` installed.
fn ctx_with(theme: Theme) -> egui::Context {
    let ctx = egui::Context::default();
    egui_widgets::theme::install_theme(&ctx, theme);
    ctx
}

#[test]
fn reaches_the_context_through_theme_ext() {
    // `ThemeExt::duration` is the single door every widget now goes through.
    for mode in MotionMode::ALL {
        let mut theme = Theme::tokyo_night();
        theme.motion.mode = *mode;
        let ctx = ctx_with(theme);
        assert_eq!(
            ctx.duration(Speed::Normal),
            tokens(*mode).duration(Speed::Normal),
            "{mode:?} did not reach the context"
        );
        assert_eq!(ctx.travel_allowed(), tokens(*mode).travel_allowed());
        assert_eq!(
            ctx.easing(Easing::OutBack),
            tokens(*mode).easing(Easing::OutBack)
        );
    }
}

#[test]
fn reaches_the_state_machine() {
    // `Machine::progress` used to scale a private constant. If the motion mode
    // does not change what it computes, the machine is not on the axis.
    let mut settings = Theme::tokyo_night();
    settings.motion.mode = MotionMode::None;
    let snap = ctx_with(settings);

    let mut m = Machine::new(S::Root);
    m.transition(S::Deep);

    let full = ctx_with(Theme::tokyo_night());
    assert_ne!(
        m.transition_secs_from(full.duration(Speed::Normal)),
        m.transition_secs_from(snap.duration(Speed::Normal)),
        "the machine's transition length ignores the motion mode"
    );
    assert_eq!(
        m.transition_secs_from(snap.duration(Speed::Normal)),
        0.0,
        "screenshot mode must make a transition instantaneous"
    );
}

#[test]
fn the_depth_falloff_is_a_ratio_the_theme_cannot_invert() {
    // Splitting base-from-theme and falloff-from-machine must not let a theme
    // make a deeper edit take LONGER, which is the property the machine owns.
    let mut m = Machine::new(S::Root);
    m.transition(S::Deep);
    let deep = m.transition_secs_from(1.0);

    let mut m2 = Machine::new(S::Root);
    m2.transition(S::Root);
    let shallow = m2.transition_secs_from(1.0);

    assert!(
        deep <= shallow,
        "a deeper change must not take longer ({deep} vs {shallow})"
    );
}

#[test]
fn a_preset_can_carry_a_motion_mode() {
    // The axis is only a theme axis if a preset can actually set it. If every
    // shipped preset were Full, `MotionMode` would be a runtime setting wearing
    // a theme's clothes — which is fine, but should be a deliberate choice
    // rather than an accident, so this pins the mechanism rather than the data.
    let mut t = Theme::tokyo_night();
    assert_eq!(t.motion.mode, MotionMode::Full);
    t.motion.mode = MotionMode::Reduced;
    let ctx = ctx_with(t);
    assert!(!ctx.travel_allowed());
    assert!(ctx.duration(Speed::Fast) > 0.0);
}
