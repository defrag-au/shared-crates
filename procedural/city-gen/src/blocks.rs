use crate::Vec2;
use crate::graph::RoadGraph;

/// A city block — an enclosed polygon formed by road edges.
#[derive(Debug, Clone)]
pub struct Block {
    /// Vertices of the block polygon (in order).
    pub polygon: Vec<Vec2>,
    /// Area in square units.
    pub area: f32,
}

/// A building lot — a subdivision of a block.
#[derive(Debug, Clone)]
pub struct Lot {
    /// Vertices of the lot polygon.
    pub polygon: Vec<Vec2>,
    /// Area in square units.
    pub area: f32,
}

/// Detect enclosed blocks from a road graph using clockwise traversal.
///
/// For each directed edge, follows the rightmost turn at each intersection
/// until returning to the start, forming a minimal polygon (city block).
pub fn detect_blocks(graph: &RoadGraph, min_area: f32, max_area: f32) -> Vec<Block> {
    if graph.edges.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    let mut visited_directed: Vec<(usize, usize)> = Vec::new();

    for edge_idx in 0..graph.edges.len() {
        let edge = &graph.edges[edge_idx];

        // Try both directions of each edge
        for &(start_node, end_node) in &[(edge.node_a, edge.node_b), (edge.node_b, edge.node_a)] {
            let directed = (start_node, end_node);
            if visited_directed.contains(&directed) {
                continue;
            }

            // Trace the block by always turning right
            if let Some(polygon) = trace_block(graph, start_node, end_node, &mut visited_directed) {
                let area = polygon_area(&polygon);
                if area > min_area && area < max_area && is_good_shape(&polygon, area) {
                    blocks.push(Block { polygon, area });
                }
            }
        }
    }

    blocks
}

/// Trace a block boundary by always taking the rightmost turn.
fn trace_block(
    graph: &RoadGraph,
    first_node: usize,
    second_node: usize,
    visited: &mut Vec<(usize, usize)>,
) -> Option<Vec<Vec2>> {
    let mut polygon = Vec::new();
    let mut prev = first_node;
    let mut current = second_node;
    let max_steps = 20; // Prevent infinite loops

    polygon.push(graph.nodes[first_node].pos);

    for _ in 0..max_steps {
        visited.push((prev, current));
        polygon.push(graph.nodes[current].pos);

        if current == first_node {
            // Closed the loop
            return Some(polygon);
        }

        // Find the rightmost turn from `current`, coming from `prev`
        let incoming_dir = (graph.nodes[current].pos - graph.nodes[prev].pos).normalize();

        let next = find_rightmost_neighbor(graph, current, prev, incoming_dir)?;

        prev = current;
        current = next;
    }

    None // Didn't close within max_steps
}

/// Find the neighbor of `node` that represents the rightmost turn when arriving from `from_node`.
fn find_rightmost_neighbor(
    graph: &RoadGraph,
    node: usize,
    from_node: usize,
    incoming_dir: Vec2,
) -> Option<usize> {
    let node_data = &graph.nodes[node];

    let mut best_neighbor = None;
    let mut best_angle = f32::MAX;

    for &edge_idx in &node_data.edges {
        let edge = &graph.edges[edge_idx];
        let neighbor = if edge.node_a == node { edge.node_b } else { edge.node_a };

        if neighbor == from_node {
            continue;
        }

        let outgoing_dir = (graph.nodes[neighbor].pos - graph.nodes[node].pos).normalize();

        // Signed angle from incoming to outgoing (clockwise = negative)
        let angle = signed_angle(incoming_dir, outgoing_dir);

        // We want the smallest positive angle (rightmost turn)
        // Normalize to [0, 2π)
        let normalized = if angle < 0.0 {
            angle + std::f32::consts::TAU
        } else {
            angle
        };

        if normalized < best_angle {
            best_angle = normalized;
            best_neighbor = Some(neighbor);
        }
    }

    best_neighbor
}

/// Signed angle from vector a to vector b (positive = counterclockwise).
fn signed_angle(a: Vec2, b: Vec2) -> f32 {
    b.y.atan2(b.x) - a.y.atan2(a.x)
}

/// Compute the area of a polygon using the shoelace formula.
fn polygon_area(polygon: &[Vec2]) -> f32 {
    let n = polygon.len();
    if n < 3 { return 0.0; }

    let mut area = 0.0f32;
    for i in 0..n {
        let j = (i + 1) % n;
        area += polygon[i].x * polygon[j].y;
        area -= polygon[j].x * polygon[i].y;
    }

    area.abs() / 2.0
}

/// Subdivide a block into building lots by recursive splitting.
///
/// Splits perpendicular to the longest edge at a randomized position.
/// Continues until lots are smaller than `max_lot_area`.
pub fn subdivide_block(block: &Block, max_lot_area: f32, min_lot_area: f32) -> Vec<Lot> {
    let mut lots = Vec::new();
    recursive_subdivide(&block.polygon, block.area, max_lot_area, min_lot_area, &mut lots, 0);
    lots
}

fn recursive_subdivide(
    polygon: &[Vec2],
    area: f32,
    max_area: f32,
    min_area: f32,
    lots: &mut Vec<Lot>,
    depth: usize,
) {
    // Base case: small enough to be a lot
    if area < max_area || depth > 6 {
        if area >= min_area && is_good_shape(polygon, area) {
            lots.push(Lot {
                polygon: polygon.to_vec(),
                area,
            });
        }
        return;
    }

    // Find the longest edge
    let n = polygon.len();
    if n < 3 { return; }

    let mut longest_idx = 0;
    let mut longest_len = 0.0f32;
    for i in 0..n {
        let j = (i + 1) % n;
        let len = polygon[i].distance(polygon[j]);
        if len > longest_len {
            longest_len = len;
            longest_idx = i;
        }
    }

    // Split perpendicular to longest edge at ~40-60% along it
    let i = longest_idx;
    let j = (i + 1) % n;
    let split_t = 0.4 + (depth as f32 * 0.037) % 0.2; // varies with depth
    let split_point = polygon[i].lerp(polygon[j], split_t);
    let edge_dir = (polygon[j] - polygon[i]).normalize();
    let split_dir = edge_dir.perpendicular();

    // Find where the split line intersects the opposite edges
    if let Some((poly_a, poly_b)) = split_polygon(polygon, split_point, split_dir) {
        let area_a = polygon_area(&poly_a);
        let area_b = polygon_area(&poly_b);

        recursive_subdivide(&poly_a, area_a, max_area, min_area, lots, depth + 1);
        recursive_subdivide(&poly_b, area_b, max_area, min_area, lots, depth + 1);
    } else {
        // Couldn't split — accept as-is if shape is good
        if area >= min_area && is_good_shape(polygon, area) {
            lots.push(Lot {
                polygon: polygon.to_vec(),
                area,
            });
        }
    }
}

/// Check if a polygon has a reasonable shape (not too elongated or spiky).
///
/// Rejects:
/// - Very elongated shapes (area / perimeter² < 0.04)
/// - Polygons with very acute interior angles (< 15°)
fn is_good_shape(polygon: &[Vec2], area: f32) -> bool {
    let n = polygon.len();
    if n < 3 { return false; }

    // Elongation check: compactness = area / perimeter²
    // A square has compactness ~0.0625, a 10:1 rectangle ~0.023
    let perimeter: f32 = (0..n)
        .map(|i| polygon[i].distance(polygon[(i + 1) % n]))
        .sum();
    if perimeter < 1.0 { return false; }

    let compactness = area / (perimeter * perimeter);
    if compactness < 0.035 {
        return false; // too elongated
    }

    // Reject triangles — they almost always look like artifacts
    if n == 3 {
        return false;
    }

    // Acute angle check: reject if any interior angle < 30°
    let min_cos = (30.0f32).to_radians().cos(); // cos(30°) ≈ 0.866
    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let curr = polygon[i];
        let next = polygon[(i + 1) % n];

        let a = (prev - curr).normalize();
        let b = (next - curr).normalize();

        // Skip degenerate edges
        if a.length_sq() < 0.001 || b.length_sq() < 0.001 {
            return false;
        }

        let dot = a.dot(b);

        // dot close to 1.0 means very acute angle
        if dot > min_cos {
            return false;
        }
    }

    true
}

/// Split a polygon along a line defined by a point and direction.
/// Returns two sub-polygons, or None if the line doesn't properly bisect.
fn split_polygon(polygon: &[Vec2], point: Vec2, dir: Vec2) -> Option<(Vec<Vec2>, Vec<Vec2>)> {
    let n = polygon.len();
    let normal = dir.perpendicular();

    // Classify each vertex as on the positive or negative side of the line
    let signs: Vec<f32> = polygon.iter()
        .map(|v| (*v - point).dot(normal))
        .collect();

    let mut poly_a = Vec::new();
    let mut poly_b = Vec::new();

    for i in 0..n {
        let j = (i + 1) % n;
        let si = signs[i];
        let sj = signs[j];

        if si >= 0.0 {
            poly_a.push(polygon[i]);
        } else {
            poly_b.push(polygon[i]);
        }

        // If edge crosses the line, add intersection point to both
        if (si > 0.0 && sj < 0.0) || (si < 0.0 && sj > 0.0) {
            let t = si / (si - sj);
            let intersection = polygon[i].lerp(polygon[j], t);
            poly_a.push(intersection);
            poly_b.push(intersection);
        }
    }

    if poly_a.len() >= 3 && poly_b.len() >= 3 {
        Some((poly_a, poly_b))
    } else {
        None
    }
}
