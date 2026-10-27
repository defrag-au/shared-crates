//! Named surfaces. The graph stores a bind (`mat=` or inline `c`/`m`/`sh`);
//! pixels still live in [`crate::Images`]. Shader *programs* live in backends.

use std::collections::HashMap;
use std::fmt;

use crate::error::Error;

pub const VERSION: &str = "mt1";

/// Closed shading model. The backend picks a program; this is not source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Shade {
    #[default]
    Lambert,
    Unlit,
}

impl Shade {
    pub fn as_str(self) -> &'static str {
        match self {
            Shade::Lambert => "Lambert",
            Shade::Unlit => "Unlit",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "Lambert" => Ok(Shade::Lambert),
            "Unlit" => Ok(Shade::Unlit),
            other => Err(Error::UnknownShade(other.to_string())),
        }
    }
}

impl fmt::Display for Shade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolved surface. Default is white Lambert, no image.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Surface {
    pub shade: Shade,
    pub color: [u8; 4],
    pub image: Option<String>,
    pub image_back: Option<String>,
}

impl Surface {
    pub const DEFAULT: Self = Self {
        shade: Shade::Lambert,
        color: [255, 255, 255, 255],
        image: None,
        image_back: None,
    };

    pub fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }
}

impl Default for Surface {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Inline params *or* a key into [`Materials`]. Mix is a load error.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceBind {
    Inline(Surface),
    Name(String),
}

impl SurfaceBind {
    pub fn resolve<'a>(&'a self, mats: &'a Materials) -> Result<&'a Surface, Error> {
        match self {
            SurfaceBind::Inline(s) => Ok(s),
            SurfaceBind::Name(k) => mats.get(k).ok_or_else(|| Error::UnknownMaterial(k.clone())),
        }
    }
}

impl Default for SurfaceBind {
    fn default() -> Self {
        SurfaceBind::Inline(Surface::DEFAULT)
    }
}

/// Interned named surfaces. Dump grammar is `mt1` (round-trips).
#[derive(Clone, Debug, Default)]
pub struct Materials {
    map: HashMap<String, Surface>,
}

impl Materials {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, surface: Surface) -> Result<(), Error> {
        let key = key.into();
        if key.is_empty() {
            return Err(Error::BadMaterial(key));
        }
        if self.map.contains_key(&key) {
            return Err(Error::DuplicateMaterial(key));
        }
        self.map.insert(key, surface);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&Surface> {
        self.map.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub(crate) fn names_sorted(&self) -> Vec<&String> {
        let mut names: Vec<_> = self.map.keys().collect();
        names.sort();
        names
    }

    pub(crate) fn map(&self) -> &HashMap<String, Surface> {
        &self.map
    }
}
