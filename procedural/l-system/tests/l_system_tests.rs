use l_system::{LSystem, TurtleConfig, interpret, Symbol};
use rand::rngs::SmallRng;
use rand::SeedableRng;

/// Koch curve: F → F+F-F-F+F
#[test]
fn test_koch_curve() {
    let mut sys = LSystem::new("F");
    sys.add_rule('F', "F+F-F-F+F");

    // Iteration 0: just "F"
    let gen0 = sys.iterate(0);
    assert_eq!(gen0.len(), 1);
    assert_eq!(gen0[0].ch, 'F');

    // Iteration 1: "F+F-F-F+F" (9 symbols)
    let gen1 = sys.iterate(1);
    let s: String = gen1.iter().map(|s| s.ch).collect();
    assert_eq!(s, "F+F-F-F+F");

    // Iteration 2: each F expands again
    let gen2 = sys.iterate(2);
    assert_eq!(gen2.iter().filter(|s| s.ch == 'F').count(), 25); // 5^2
}

/// Sierpinski triangle: A → B-A-B, B → A+B+A
#[test]
fn test_sierpinski() {
    let mut sys = LSystem::new("A");
    sys.add_rule('A', "B-A-B");
    sys.add_rule('B', "A+B+A");

    let gen2 = sys.iterate(2);
    let s: String = gen2.iter().map(|s| s.ch).collect();
    // A → B-A-B → (A+B+A)-(B-A-B)-(A+B+A)
    assert_eq!(s, "A+B+A-B-A-B-A+B+A");
}

/// Binary tree: 0 → 1[0]0, 1 → 11
#[test]
fn test_binary_tree() {
    let mut sys = LSystem::new("0");
    sys.add_rule('0', "1[0]0");
    sys.add_rule('1', "11");

    let gen1 = sys.iterate(1);
    let s: String = gen1.iter().map(|s| s.ch).collect();
    assert_eq!(s, "1[0]0");

    let gen2 = sys.iterate(2);
    let s: String = gen2.iter().map(|s| s.ch).collect();
    assert_eq!(s, "11[1[0]0]1[0]0");
}

/// Stochastic rules: same predecessor with weighted alternatives.
#[test]
fn test_stochastic_rules() {
    let mut sys = LSystem::new("F");
    sys.add_stochastic_rule('F', 1.0, "F+F");
    sys.add_stochastic_rule('F', 1.0, "F-F");

    let mut rng = SmallRng::seed_from_u64(42);
    let gen1 = sys.iterate_stochastic(1, &mut rng);
    let s: String = gen1.iter().map(|s| s.ch).collect();

    // Should be either "F+F" or "F-F"
    assert!(s == "F+F" || s == "F-F", "Got unexpected: {s}");

    // Multiple iterations should produce varied results
    let mut results = std::collections::HashSet::new();
    for seed in 0..20 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let gen = sys.iterate_stochastic(3, &mut rng);
        let s: String = gen.iter().map(|s| s.ch).collect();
        results.insert(s);
    }
    assert!(results.len() > 1, "Stochastic rules should produce variety");
}

/// Turtle interpretation: Koch curve produces segments.
#[test]
fn test_turtle_koch() {
    let mut sys = LSystem::new("F");
    sys.add_rule('F', "F+F-F-F+F");

    let gen2 = sys.iterate(2);

    let config = TurtleConfig {
        step_length: 10.0,
        angle_delta: std::f32::consts::FRAC_PI_2, // 90 degrees
        ..Default::default()
    };

    let segments = interpret(&gen2, &config);

    // Each F produces a segment; gen2 has 25 F's
    assert_eq!(segments.len(), 25);

    // All segments should have depth 0 (no branching in Koch)
    assert!(segments.iter().all(|s| s.depth == 0));

    eprintln!("Koch curve gen2: {} segments", segments.len());
}

/// Turtle interpretation: branching tree.
#[test]
fn test_turtle_branching() {
    // Simple branching: F[+F][-F]
    let mut sys = LSystem::new("F");
    sys.add_rule('F', "F[+F][-F]");

    let gen2 = sys.iterate(2);

    let config = TurtleConfig {
        step_length: 20.0,
        angle_delta: 30.0_f32.to_radians(),
        ..Default::default()
    };

    let segments = interpret(&gen2, &config);

    // Should have segments at different depths
    let max_depth = segments.iter().map(|s| s.depth).max().unwrap();
    assert!(max_depth >= 2, "Branching tree should reach depth 2+, got {max_depth}");

    // Trunk segments (depth 0) should exist
    let trunk_count = segments.iter().filter(|s| s.depth == 0).count();
    assert!(trunk_count > 0, "Should have trunk segments");

    eprintln!(
        "Branching tree gen2: {} segments, max depth {max_depth}, trunk segments: {trunk_count}",
        segments.len()
    );
}

/// Turtle with bounds clipping.
#[test]
fn test_turtle_bounds() {
    let mut sys = LSystem::new("F");
    sys.add_rule('F', "FF"); // doubles length each iteration

    let gen5 = sys.iterate(5); // 32 F's → 320 pixels

    let config = TurtleConfig {
        step_length: 10.0,
        angle_delta: 0.0,
        initial_heading: 0.0, // east
        bounds: Some((0.0, -50.0, 100.0, 50.0)), // only 100px wide
        ..Default::default()
    };

    let segments = interpret(&gen5, &config);

    // Should have clipped — not all 32 segments drawn
    assert!(segments.len() < 32, "Bounds should clip some segments, got {}", segments.len());
    // All segments should be within bounds
    for seg in &segments {
        assert!(seg.x2 <= 100.0 + 0.1, "Segment end {:.1} exceeds x bound", seg.x2);
    }
}

/// Parametric symbols: step length from parameter.
#[test]
fn test_parametric_step() {
    let axiom = vec![
        Symbol::with_params('F', vec![20.0]),
        Symbol::new('+'),
        Symbol::with_params('F', vec![10.0]),
    ];

    let config = TurtleConfig {
        step_length: 5.0, // default, overridden by params
        angle_delta: std::f32::consts::FRAC_PI_2,
        ..Default::default()
    };

    let segments = interpret(&axiom, &config);
    assert_eq!(segments.len(), 2);

    // First segment should be length 20
    let len1 = ((segments[0].x2 - segments[0].x1).powi(2) + (segments[0].y2 - segments[0].y1).powi(2)).sqrt();
    assert!((len1 - 20.0).abs() < 0.01, "First segment should be 20px, got {len1:.1}");

    // Second segment should be length 10
    let len2 = ((segments[1].x2 - segments[1].x1).powi(2) + (segments[1].y2 - segments[1].y1).powi(2)).sqrt();
    assert!((len2 - 10.0).abs() < 0.01, "Second segment should be 10px, got {len2:.1}");
}

/// Road network generation: stochastic L-system for urban roads.
#[test]
fn test_road_l_system() {
    // A simple road grammar:
    // R = main road segment
    // S = side street
    // R → R[+S][-S]R  (road continues, spawns side streets)
    // S → >SF          (side street gets shorter, draws one segment)
    let mut sys = LSystem::new("R");
    sys.add_stochastic_rule('R', 3.0, "RF[+S]R");
    sys.add_stochastic_rule('R', 3.0, "RF[-S]R");
    sys.add_stochastic_rule('R', 2.0, "RF[+S][-S]R");
    sys.add_stochastic_rule('R', 1.0, "RFR");
    sys.add_rule('S', ">SF");

    let mut rng = SmallRng::seed_from_u64(42);
    let gen3 = sys.iterate_stochastic(3, &mut rng);

    let config = TurtleConfig {
        start_x: 512.0,
        start_y: 512.0,
        initial_heading: -std::f32::consts::FRAC_PI_2, // north
        step_length: 40.0,
        angle_delta: std::f32::consts::FRAC_PI_2, // 90 degree turns
        step_scale: 0.6,
        bounds: Some((0.0, 0.0, 1024.0, 1024.0)),
    };

    let segments = interpret(&gen3, &config);
    assert!(!segments.is_empty(), "Road system should produce segments");

    let main_road = segments.iter().filter(|s| s.depth == 0).count();
    let side_streets = segments.iter().filter(|s| s.depth > 0).count();

    eprintln!(
        "Road L-system gen3: {} total segments, {main_road} main road, {side_streets} side streets",
        segments.len()
    );

    assert!(main_road > 0, "Should have main road segments");
    assert!(side_streets > 0, "Should have side streets");
}
