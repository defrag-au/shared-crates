use crate::error::Error;

/// Closed easing. Warps the interpolation parameter `u` in `[0, 1]`.
/// Default [`Ease::Linear`] is omitted from dumps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ease {
    #[default]
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InSine,
    OutSine,
    InOutSine,
}

impl Ease {
    pub fn as_str(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::InQuad => "in-quad",
            Ease::OutQuad => "out-quad",
            Ease::InOutQuad => "inout-quad",
            Ease::InCubic => "in-cubic",
            Ease::OutCubic => "out-cubic",
            Ease::InOutCubic => "inout-cubic",
            Ease::InSine => "in-sine",
            Ease::OutSine => "out-sine",
            Ease::InOutSine => "inout-sine",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "linear" => Ok(Ease::Linear),
            "in-quad" => Ok(Ease::InQuad),
            "out-quad" => Ok(Ease::OutQuad),
            "inout-quad" => Ok(Ease::InOutQuad),
            "in-cubic" => Ok(Ease::InCubic),
            "out-cubic" => Ok(Ease::OutCubic),
            "inout-cubic" => Ok(Ease::InOutCubic),
            "in-sine" => Ok(Ease::InSine),
            "out-sine" => Ok(Ease::OutSine),
            "inout-sine" => Ok(Ease::InOutSine),
            other => Err(Error::UnknownEase(other.to_string())),
        }
    }

    pub fn apply(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Ease::Linear => u,
            Ease::InQuad => u * u,
            Ease::OutQuad => {
                let t = 1.0 - u;
                1.0 - t * t
            }
            Ease::InOutQuad => {
                if u < 0.5 {
                    2.0 * u * u
                } else {
                    1.0 - (-2.0 * u + 2.0).powi(2) / 2.0
                }
            }
            Ease::InCubic => u * u * u,
            Ease::OutCubic => {
                let t = 1.0 - u;
                1.0 - t * t * t
            }
            Ease::InOutCubic => {
                if u < 0.5 {
                    4.0 * u * u * u
                } else {
                    1.0 - (-2.0 * u + 2.0).powi(3) / 2.0
                }
            }
            Ease::InSine => 1.0 - (u * std::f32::consts::FRAC_PI_2).cos(),
            Ease::OutSine => (u * std::f32::consts::FRAC_PI_2).sin(),
            Ease::InOutSine => -((std::f32::consts::PI * u).cos() - 1.0) / 2.0,
        }
    }
}
