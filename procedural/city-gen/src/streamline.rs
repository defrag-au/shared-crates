use crate::spatial::SpatialGrid;
use crate::tensor::TensorField;
use crate::Vec2;

/// Configuration for streamline tracing.
pub struct StreamlineConfig {
    /// Minimum distance between parallel streamlines.
    pub dsep: f32,
    /// Integration step size.
    pub dstep: f32,
    /// Test distance for collision during tracing (must be <= dsep).
    pub dtest: f32,
    /// Maximum distance to search for dangling ends to connect to.
    pub dlookahead: f32,
    /// Maximum number of integration steps per streamline.
    pub max_steps: usize,
    /// Whether to trace both major and minor eigenvectors.
    pub trace_minor: bool,
}

impl Default for StreamlineConfig {
    fn default() -> Self {
        Self {
            dsep: 30.0,
            dstep: 5.0,
            dtest: 15.0,
            dlookahead: 40.0,
            max_steps: 500,
            trace_minor: true,
        }
    }
}

/// A traced streamline — a polyline following the tensor field.
#[derive(Debug, Clone)]
pub struct Streamline {
    pub points: Vec<Vec2>,
    /// Whether this follows the major (true) or minor (false) eigenvector.
    pub is_major: bool,
}

/// Trace streamlines through a tensor field.
///
/// Uses RK4 integration along eigenvectors. New streamline seeds are placed
/// at endpoints of completed streamlines, perpendicular at `dsep` distance.
/// Spacing is enforced by a spatial index.
pub fn trace_streamlines(
    field: &TensorField,
    config: &StreamlineConfig,
    seed_points: &[Vec2],
) -> Vec<Streamline> {
    let mut streamlines: Vec<Streamline> = Vec::new();

    // Spatial index of all sampled points (for spacing enforcement)
    let mut sample_grid = SpatialGrid::new(
        field.min.x, field.min.y, field.max.x, field.max.y,
        config.dsep,
    );
    let mut all_points: Vec<Vec2> = Vec::new();

    // Seed queue: positions to try starting new streamlines from
    let mut seeds: Vec<(Vec2, bool)> = seed_points.iter()
        .flat_map(|&p| {
            let mut v = vec![(p, true)];
            if config.trace_minor {
                v.push((p, false));
            }
            v
        })
        .collect();

    let mut seed_idx = 0;
    let mut rejected = 0u32;

    while seed_idx < seeds.len() {
        let (seed, is_major) = seeds[seed_idx];
        seed_idx += 1;

        // Must be in bounds
        if !field.in_bounds(seed) {
            continue;
        }

        // Skip if too close to existing streamlines
        if sample_grid.has_nearby(seed, config.dtest * 0.5, &all_points) {
            rejected += 1;
            continue;
        }

        // Trace in both directions from seed
        let forward = trace_one_direction(field, &sample_grid, &all_points, config, seed, is_major, false);
        let backward = trace_one_direction(field, &sample_grid, &all_points, config, seed, is_major, true);

        // Combine into one streamline (backward reversed + forward)
        let mut points = Vec::with_capacity(backward.len() + forward.len());
        for p in backward.into_iter().rev() {
            points.push(p);
        }
        if !points.is_empty() && !forward.is_empty() {
            // Avoid duplicate seed point
            points.pop();
        }
        for p in forward {
            points.push(p);
        }

        // Minimum length check
        if points.len() < 3 {
            continue;
        }

        // Register all points in spatial index
        for &p in &points {
            let pt_idx = all_points.len();
            all_points.push(p);
            sample_grid.insert(p, pt_idx);
        }

        // Generate new seeds perpendicular to this streamline's endpoints
        if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
            // Seeds perpendicular at endpoints
            if points.len() >= 2 {
                let start_dir = (points[1] - points[0]).normalize();
                let end_dir = (points[points.len() - 1] - points[points.len() - 2]).normalize();

                let perp_start = start_dir.perpendicular();
                let perp_end = end_dir.perpendicular();

                seeds.push((first + perp_start * config.dsep, !is_major));
                seeds.push((first - perp_start * config.dsep, !is_major));
                seeds.push((last + perp_end * config.dsep, !is_major));
                seeds.push((last - perp_end * config.dsep, !is_major));
            }

            // Seeds along the streamline at regular intervals
            let total_len: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
            let num_seeds = (total_len / (config.dsep * 2.0)).ceil() as usize;
            if num_seeds > 0 {
                let step = points.len() / (num_seeds + 1);
                for i in 1..=num_seeds {
                    let idx = (i * step).min(points.len() - 1);
                    let dir = if idx + 1 < points.len() {
                        (points[idx + 1] - points[idx]).normalize()
                    } else if idx > 0 {
                        (points[idx] - points[idx - 1]).normalize()
                    } else {
                        Vec2::new(1.0, 0.0)
                    };
                    let perp = dir.perpendicular();
                    seeds.push((points[idx] + perp * config.dsep, !is_major));
                    seeds.push((points[idx] - perp * config.dsep, !is_major));
                }
            }
        }

        streamlines.push(Streamline { points, is_major });
    }

    streamlines
}

/// Trace one direction from a seed point using RK4 integration.
fn trace_one_direction(
    field: &TensorField,
    grid: &SpatialGrid,
    all_points: &[Vec2],
    config: &StreamlineConfig,
    start: Vec2,
    is_major: bool,
    reverse: bool,
) -> Vec<Vec2> {
    let mut points = vec![start];
    let mut pos = start;

    let mut too_close_count = 0u32;

    for _ in 0..config.max_steps {
        // RK4 integration
        let next = rk4_step(field, pos, config.dstep, is_major, reverse);

        // Bounds check
        if !field.in_bounds(next) {
            break;
        }

        // Collision check — too close to existing streamlines?
        // Allow brief crossings (perpendicular streets crossing)
        if grid.has_nearby(next, config.dtest * 0.4, all_points) {
            too_close_count += 1;
            if too_close_count > 3 {
                // Running parallel too close — stop
                break;
            }
            // Allow crossing through (perpendicular intersection)
        } else {
            too_close_count = 0;
        }

        // Segment too short (stalled)?
        if next.distance(pos) < config.dstep * 0.1 {
            break;
        }

        points.push(next);
        pos = next;
    }

    points
}

/// RK4 integration step along the tensor field's eigenvector.
fn rk4_step(
    field: &TensorField,
    pos: Vec2,
    h: f32,
    is_major: bool,
    reverse: bool,
) -> Vec2 {
    let dir = |p: Vec2| -> Vec2 {
        let tensor = field.sample(p);
        let v = if is_major { tensor.major() } else { tensor.minor() };
        if reverse { -v } else { v }
    };

    let k1 = dir(pos);
    let k2 = dir(pos + k1 * (h * 0.5));
    let k3 = dir(pos + k2 * (h * 0.5));
    let k4 = dir(pos + k3 * h);

    // Ensure consistent direction (avoid 180° flips between steps)
    let k2 = if k1.dot(k2) < 0.0 { -k2 } else { k2 };
    let k3 = if k1.dot(k3) < 0.0 { -k3 } else { k3 };
    let k4 = if k1.dot(k4) < 0.0 { -k4 } else { k4 };

    let dx = (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (h / 6.0);
    pos + dx
}
