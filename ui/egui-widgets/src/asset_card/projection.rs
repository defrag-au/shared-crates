use egui::Pos2;

#[derive(Clone, Copy, Default)]
pub struct TiltState {
    pub current_x: f32,
    pub current_y: f32,
}

/// Project a 2D point through 3D rotation + perspective division.
///
/// 1. Translate so `center` is the origin
/// 2. Rotate around X axis by `angle_x` (tilt forward/back)
/// 3. Rotate around Y axis by `angle_y` (tilt left/right)
/// 4. Perspective divide: x' = x * d/(d+z), y' = y * d/(d+z)
/// 5. Translate back to screen space
pub fn project_3d(point: Pos2, center: Pos2, angle_x: f32, angle_y: f32, perspective: f32) -> Pos2 {
    let x = point.x - center.x;
    let y = point.y - center.y;
    let z: f32 = 0.0;

    // Rotate around X axis (pitch): y' = y*cos - z*sin, z' = y*sin + z*cos
    let (sx, cx) = angle_x.sin_cos();
    let y1 = y * cx - z * sx;
    let z1 = y * sx + z * cx;

    // Rotate around Y axis (yaw): x' = x*cos + z*sin, z' = -x*sin + z*cos
    let (sy, cy) = angle_y.sin_cos();
    let x2 = x * cy + z1 * sy;
    let z2 = -x * sy + z1 * cy;

    // Perspective divide
    let scale = perspective / (perspective + z2);
    Pos2::new(center.x + x2 * scale, center.y + y1 * scale)
}

/// Project a slice of points through 3D perspective.
pub fn project_points(
    points: &[Pos2],
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
) -> Vec<Pos2> {
    points
        .iter()
        .map(|&p| project_3d(p, center, angle_x, angle_y, perspective))
        .collect()
}

/// Update tilt state from hover, return angles in radians for 3D projection.
pub fn update_tilt(
    response: &egui::Response,
    center: Pos2,
    half: f32,
    tilt: &mut TiltState,
    ease: f32,
    max_angle_deg: f32,
) -> (f32, f32) {
    let (target_x, target_y) = if let Some(hover_pos) = response.hover_pos() {
        let rel_x = (hover_pos.x - center.x) / half;
        let rel_y = (hover_pos.y - center.y) / half;
        (rel_x.clamp(-1.0, 1.0), rel_y.clamp(-1.0, 1.0))
    } else {
        (0.0, 0.0)
    };
    tilt.current_x += (target_x - tilt.current_x) * ease;
    tilt.current_y += (target_y - tilt.current_y) * ease;

    let max_rad = max_angle_deg.to_radians();
    let angle_x = tilt.current_y * max_rad;
    let angle_y = -tilt.current_x * max_rad;
    (angle_x, angle_y)
}
