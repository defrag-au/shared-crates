//! Frame-budget check at real-collection scale (ignored by default).
//!
//! The widgets were designed against fixtures (~160 assets). A real collection
//! is 10,001 assets / 25,728 acquisition events / ~7,900 holders, and the
//! per-frame work is what decides whether the window is usable. 60fps = 16.6ms
//! for EVERYTHING, so a single widget's data pass needs to be well under that.
use egui_widgets::{
    Acquisition, Arrival, distribution_at, distribution_series, peak_pile, piles_at,
};
use std::time::Instant;

#[test]
#[ignore = "timing, not correctness — run with --ignored --nocapture"]
fn frame_budget_at_real_scale() {
    let holders: Vec<String> = (0..7_900).map(|i| format!("stake1holder{i:05}")).collect();
    let arrivals: Vec<Arrival<'_>> = (0..10_001)
        .map(|i| Arrival::new(i as i64, holders[i % holders.len()].as_str(), 1))
        .collect();
    let events: Vec<Acquisition<'_>> = (0..25_728)
        .map(|i| Acquisition::new(i as i64, holders[i % holders.len()].as_str(), 1))
        .collect();

    let t = Instant::now();
    let _ = piles_at(&arrivals, 5_000);
    println!("piles_at            {:>8.2?}", t.elapsed());

    let t = Instant::now();
    let _ = peak_pile(&arrivals);
    println!("peak_pile           {:>8.2?}", t.elapsed());

    let t = Instant::now();
    let _ = distribution_at(&events, 12_000);
    let one = t.elapsed();
    println!("distribution_at ×1  {:>8.2?}", one);

    // The chart samples once per horizontal pixel; ~900px is a wide window.
    let steps: Vec<i64> = (0..900).map(|i| i * 28).collect();
    let steps300: Vec<i64> = (0..300).map(|i| i * 84).collect();
    let t = Instant::now();
    for i in 0..900 {
        let _ = distribution_at(&events, i * 28);
    }
    println!(
        "OLD per-step ×900   {:>8.2?}   [16.6ms = 60fps]",
        t.elapsed()
    );

    let t = Instant::now();
    let series = distribution_series(&events, &steps);
    println!(
        "NEW one-pass ×900   {:>8.2?}   [16.6ms = 60fps]",
        t.elapsed()
    );

    // Same answers, or the speed is worthless.
    for (i, s) in series.iter().enumerate() {
        assert_eq!(*s, distribution_at(&events, steps[i]), "step {i}");
    }
    println!("series matches distribution_at at all 900 steps");

    // What the chart actually asks for now: one sample per ~3px.
    let t = Instant::now();
    let _ = distribution_series(&events, &steps300);
    println!(
        "NEW one-pass ×300   {:>8.2?}   [16.6ms = 60fps]",
        t.elapsed()
    );
}

/// `HolderField` renders one dot per asset and walks the whole move timeline
/// every frame. At real scale that is 10,001 dots and ~50,000 moves — if that
/// pass costs more than a few ms the window stops being scrubbable, which is
/// the entire point of the widget.
fn pass_sized(
    ctx: &egui::Context,
    spine: &egui_widgets::SpineState,
    sel: &mut egui_widgets::Selection,
    moves: &[egui_widgets::AssetMove<'_>],
    h: f32,
) {
    use egui::{Id, Pos2, Rect, vec2};
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 400.0))),
        ..Default::default()
    };
    ctx.begin_pass(raw);
    egui::Area::new(Id::new("hf")).show(ctx, |ui| {
        ui.set_min_size(vec2(1400.0, 400.0));
        egui_widgets::HolderField::new(moves, spine, sel)
            .height(h)
            .show(ui);
    });
    let _ = ctx.end_pass();
}

#[test]
#[ignore = "timing, not correctness — run with --ignored --nocapture"]
fn holder_field_frame_budget() {
    use egui::{Id, Pos2, Rect, vec2};
    use egui_widgets::{AssetMove, HolderField, Selection, SpineState};

    let holders: Vec<String> = (0..7_900).map(|i| format!("stake1holder{i:05}")).collect();
    let assets: Vec<String> = (0..10_001).map(|i| format!("asset{i:05}")).collect();
    let mut moves: Vec<AssetMove<'_>> = Vec::new();
    for (i, a) in assets.iter().enumerate() {
        moves.push(AssetMove::mint(i as i64, a, &holders[i % holders.len()]));
    }
    // 40,000 trades on top of the mint — a heavily traded collection.
    for k in 0..40_000usize {
        let a = &assets[(k * 7919) % assets.len()];
        let from = &holders[(k * 104_729) % holders.len()];
        let to = &holders[(k * 15_485_863) % holders.len()];
        if from == to {
            continue;
        }
        moves.push(AssetMove::transfer(
            10_001 + k as i64,
            a,
            from.as_str(),
            to.as_str(),
        ));
    }
    moves.sort_by_key(|m| m.timestamp);
    let span = (0, moves.last().unwrap().timestamp);

    let ctx = egui::Context::default();
    let mut spine = SpineState::new(span);
    let mut sel = Selection::default();
    let pass = |spine: &SpineState, sel: &mut Selection| {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 400.0))),
            ..Default::default()
        };
        ctx.begin_pass(raw);
        egui::Area::new(Id::new("hf")).show(&ctx, |ui| {
            ui.set_min_size(vec2(1400.0, 400.0));
            HolderField::new(&moves, spine, sel).height(340.0).show(ui);
        });
        let _ = ctx.end_pass();
    };

    // egui's own first frame (font atlas, tessellator warm-up) is not ours —
    // measure it on a trivial data set so the number below is the widget's.
    {
        let tiny = vec![AssetMove::mint(0, "a", "h")];
        let t = Instant::now();
        pass_sized(
            &egui::Context::default(),
            &SpineState::new((0, 1)),
            &mut Selection::default(),
            &tiny,
            340.0,
        );
        println!(
            "egui baseline first frame  {:>8.2?}   (not the widget)",
            t.elapsed()
        );
    }

    // First pass builds the model + packing; every later frame reuses them.
    let t = Instant::now();
    pass(&spine, &mut sel);
    println!(
        "first frame (model + pack) {:>8.2?}   {} moves / {} assets",
        t.elapsed(),
        moves.len(),
        assets.len()
    );

    // A different field HEIGHT changes the aspect, so this is the packing cost
    // alone — model cached, placement recomputed. (Changing the WIDTH would
    // not: the widget takes the full available width either way.)
    let t = Instant::now();
    pass_sized(&ctx, &spine, &mut sel, &moves, 300.0);
    println!("re-pack, new aspect        {:>8.2?}", t.elapsed());

    // A nudge that lands in the same aspect bucket reuses the placement and
    // only refits it. This is what a window drag costs.
    let t = Instant::now();
    pass_sized(&ctx, &spine, &mut sel, &moves, 302.0);
    println!("refit, same aspect         {:>8.2?}", t.elapsed());

    let t = Instant::now();
    for i in 0..30 {
        spine.set_playhead(span.0 + (span.1 - span.0) * i / 30);
        pass(&spine, &mut sel);
    }
    println!(
        "steady frame ×30           {:>8.2?} total, {:>8.2?}/frame   [16.6ms = 60fps]",
        t.elapsed(),
        t.elapsed() / 30
    );
}
