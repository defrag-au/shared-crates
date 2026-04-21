use crate::spatial::SpatialGrid;
use crate::streamline::Streamline;
use crate::Vec2;

/// A node in the road graph (intersection or endpoint).
#[derive(Debug, Clone)]
pub struct RoadNode {
    pub pos: Vec2,
    /// Indices of connected edges.
    pub edges: Vec<usize>,
}

/// An edge in the road graph (road segment between two nodes).
#[derive(Debug, Clone)]
pub struct RoadEdge {
    pub node_a: usize,
    pub node_b: usize,
    /// Intermediate points between the two nodes (for curved roads).
    pub path: Vec<Vec2>,
}

/// The road network graph.
pub struct RoadGraph {
    pub nodes: Vec<RoadNode>,
    pub edges: Vec<RoadEdge>,
}

impl RoadGraph {
    /// Build a road graph from traced streamlines.
    ///
    /// Strategy:
    /// 1. Register all streamline points in a spatial index, tagged by streamline
    /// 2. For each streamline, walk its points and detect where it passes near
    ///    points from other streamlines — these are intersection candidates
    /// 3. Create nodes at endpoints and intersections
    /// 4. Create edges between consecutive nodes along each streamline
    pub fn from_streamlines(streamlines: &[Streamline], snap_distance: f32) -> Self {
        if streamlines.is_empty() {
            return RoadGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            };
        }

        // Build spatial index of all points tagged with (streamline_idx, point_idx)
        let mut bounds_min = streamlines[0].points[0];
        let mut bounds_max = bounds_min;
        for s in streamlines {
            for &p in &s.points {
                bounds_min.x = bounds_min.x.min(p.x);
                bounds_min.y = bounds_min.y.min(p.y);
                bounds_max.x = bounds_max.x.max(p.x);
                bounds_max.y = bounds_max.y.max(p.y);
            }
        }

        let cell_size = snap_distance * 2.0;
        let mut point_grid = SpatialGrid::new(
            bounds_min.x - 1.0,
            bounds_min.y - 1.0,
            bounds_max.x + 1.0,
            bounds_max.y + 1.0,
            cell_size,
        );

        // Flat list of (position, streamline_idx)
        let mut all_points: Vec<(Vec2, usize)> = Vec::new();
        for (si, streamline) in streamlines.iter().enumerate() {
            for &p in &streamline.points {
                let idx = all_points.len();
                all_points.push((p, si));
                point_grid.insert(p, idx);
            }
        }

        // For each streamline, find intersection points (where it passes near another streamline)
        // intersection_flags[streamline_idx] = set of point indices that are intersections
        let snap_sq = snap_distance * snap_distance;
        let mut intersection_points: Vec<Vec<usize>> = streamlines
            .iter()
            .map(|s| vec![0; s.points.len()]) // 0 = not intersection, 1 = intersection
            .collect();

        for (si, streamline) in streamlines.iter().enumerate() {
            for (pi, &p) in streamline.points.iter().enumerate() {
                let candidates = point_grid.query_radius(p, snap_distance);
                for &ci in &candidates {
                    let (cp, csi) = all_points[ci];
                    if csi == si {
                        continue;
                    } // same streamline

                    if (p - cp).length_sq() < snap_sq {
                        intersection_points[si][pi] = 1;
                        break;
                    }
                }
            }
        }

        // Build graph: nodes at endpoints and intersections, edges between them
        let mut nodes: Vec<RoadNode> = Vec::new();
        let mut edges: Vec<RoadEdge> = Vec::new();

        for (si, streamline) in streamlines.iter().enumerate() {
            if streamline.points.len() < 2 {
                continue;
            }

            let flags = &intersection_points[si];
            let mut segment_start = 0;

            for (i, &flag) in flags.iter().enumerate() {
                let is_endpoint = i == 0 || i == streamline.points.len() - 1;
                let is_intersection = flag == 1;

                if is_endpoint || is_intersection {
                    let p = streamline.points[i];
                    let node_idx = find_or_create_node(&mut nodes, p, snap_sq);

                    if i > segment_start {
                        let start_p = streamline.points[segment_start];
                        let start_node = find_or_create_node(&mut nodes, start_p, snap_sq);

                        if start_node != node_idx {
                            let edge_idx = edges.len();
                            let path: Vec<Vec2> = streamline.points[segment_start..=i].to_vec();

                            edges.push(RoadEdge {
                                node_a: start_node,
                                node_b: node_idx,
                                path,
                            });

                            nodes[start_node].edges.push(edge_idx);
                            nodes[node_idx].edges.push(edge_idx);
                        }
                    }

                    segment_start = i;
                }
            }
        }

        RoadGraph { nodes, edges }
    }

    /// Get all edge segments as line pairs (for rendering).
    pub fn segments(&self) -> Vec<(Vec2, Vec2)> {
        let mut segs = Vec::new();
        for edge in &self.edges {
            for pair in edge.path.windows(2) {
                segs.push((pair[0], pair[1]));
            }
        }
        segs
    }

    /// Get the bounding box of all nodes.
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut min = self.nodes[0].pos;
        let mut max = self.nodes[0].pos;
        for node in &self.nodes {
            min.x = min.x.min(node.pos.x);
            min.y = min.y.min(node.pos.y);
            max.x = max.x.max(node.pos.x);
            max.y = max.y.max(node.pos.y);
        }
        Some((min, max))
    }
}

/// Find or create a node near `pos`.
fn find_or_create_node(nodes: &mut Vec<RoadNode>, pos: Vec2, snap_sq: f32) -> usize {
    if let Some((idx, _)) = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| (n.pos - pos).length_sq() < snap_sq)
        .min_by(|(_, a), (_, b)| {
            let da = (a.pos - pos).length_sq();
            let db = (b.pos - pos).length_sq();
            da.partial_cmp(&db).unwrap()
        })
    {
        idx
    } else {
        let idx = nodes.len();
        nodes.push(RoadNode {
            pos,
            edges: Vec::new(),
        });
        idx
    }
}
