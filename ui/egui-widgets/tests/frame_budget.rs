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
