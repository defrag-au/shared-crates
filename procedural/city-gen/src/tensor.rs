use crate::Vec2;

/// A 2D tensor encoded as (cos(2θ), sin(2θ)).
///
/// The factor-of-2 encoding means 0° and 180° map to the same tensor,
/// giving us undirected line fields. The major eigenvector points in
/// direction θ, the minor eigenvector is perpendicular.
#[derive(Debug, Clone, Copy)]
pub struct Tensor {
    /// cos(2θ) component
    pub r: f32,
    /// sin(2θ) component
    pub s: f32,
}

impl Tensor {
    pub const ZERO: Tensor = Tensor { r: 0.0, s: 0.0 };

    /// Create a tensor from an angle (direction of major eigenvector).
    pub fn from_angle(theta: f32) -> Self {
        Self {
            r: (2.0 * theta).cos(),
            s: (2.0 * theta).sin(),
        }
    }

    /// Get the major eigenvector direction (angle θ).
    pub fn major_angle(self) -> f32 {
        self.s.atan2(self.r) / 2.0
    }

    /// Major eigenvector as a unit vector.
    pub fn major(self) -> Vec2 {
        let a = self.major_angle();
        Vec2::new(a.cos(), a.sin())
    }

    /// Minor eigenvector (perpendicular to major).
    pub fn minor(self) -> Vec2 {
        let a = self.major_angle() + std::f32::consts::FRAC_PI_2;
        Vec2::new(a.cos(), a.sin())
    }

    /// Add two tensors.
    pub fn add(self, other: Tensor) -> Tensor {
        Tensor {
            r: self.r + other.r,
            s: self.s + other.s,
        }
    }

    /// Scale a tensor.
    pub fn scale(self, factor: f32) -> Tensor {
        Tensor {
            r: self.r * factor,
            s: self.s * factor,
        }
    }
}

/// Type of basis field.
#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    /// Uniform grid field — produces parallel streets at angle θ.
    Grid,
    /// Radial field — streets radiate from center, creating rings + spokes.
    Radial,
}

/// A positioned basis field that contributes to the tensor field.
#[derive(Debug, Clone)]
pub struct BasisField {
    pub field_type: FieldType,
    pub center: Vec2,
    pub angle: f32,
    /// Influence radius — field strength decays with distance.
    pub size: f32,
    /// Decay rate — how quickly influence falls off. Higher = sharper falloff.
    pub decay: f32,
}

impl BasisField {
    /// Sample this basis field at a point. Returns the tensor and its weight.
    pub fn sample(&self, point: Vec2) -> (Tensor, f32) {
        let delta = point - self.center;
        let dist = delta.length();

        // Distance-based weight decay (Gaussian-like)
        let weight = (-self.decay * (dist / self.size).powi(2)).exp();

        if weight < 0.001 {
            return (Tensor::ZERO, 0.0);
        }

        let tensor = match self.field_type {
            FieldType::Grid => {
                // Uniform direction everywhere
                Tensor::from_angle(self.angle)
            }
            FieldType::Radial => {
                // Direction points away from center (radial spokes)
                // Perpendicular gives concentric rings
                let angle = delta.y.atan2(delta.x);
                Tensor::from_angle(angle + self.angle)
            }
        };

        (tensor, weight)
    }
}

/// A tensor field composed of multiple basis fields.
///
/// At any point, the tensor is the weighted sum of all basis fields.
/// This naturally blends different city styles at district boundaries.
pub struct TensorField {
    fields: Vec<BasisField>,
    /// World bounds.
    pub min: Vec2,
    pub max: Vec2,
}

impl TensorField {
    /// Create a new tensor field with the given world bounds.
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            fields: Vec::new(),
            min: Vec2::new(min_x, min_y),
            max: Vec2::new(max_x, max_y),
        }
    }

    /// Add a grid basis field.
    pub fn add_grid(&mut self, center: Vec2, angle: f32, size: f32) {
        self.fields.push(BasisField {
            field_type: FieldType::Grid,
            center,
            angle,
            size,
            decay: 1.0,
        });
    }

    /// Add a radial basis field.
    pub fn add_radial(&mut self, center: Vec2, angle_offset: f32, size: f32) {
        self.fields.push(BasisField {
            field_type: FieldType::Radial,
            center,
            angle: angle_offset,
            size,
            decay: 1.0,
        });
    }

    /// Add a basis field with full control.
    pub fn add_field(&mut self, field: BasisField) {
        self.fields.push(field);
    }

    /// Sample the tensor field at a point.
    pub fn sample(&self, point: Vec2) -> Tensor {
        let mut result = Tensor::ZERO;
        let mut total_weight = 0.0f32;

        for field in &self.fields {
            let (tensor, weight) = field.sample(point);
            if weight > 0.001 {
                result = result.add(tensor.scale(weight));
                total_weight += weight;
            }
        }

        if total_weight > 0.001 {
            result.scale(1.0 / total_weight)
        } else {
            // Fallback: default grid direction
            Tensor::from_angle(0.0)
        }
    }

    /// Check if a point is within bounds.
    pub fn in_bounds(&self, point: Vec2) -> bool {
        point.x >= self.min.x && point.x <= self.max.x
            && point.y >= self.min.y && point.y <= self.max.y
    }
}
