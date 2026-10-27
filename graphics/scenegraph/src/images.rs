//! Backend-facing atlas. The graph stores image *keys*; pixels live here.

use std::collections::HashMap;

use crate::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Nearest sample. v=0 is the first row (image top).
    pub fn alpha_at(&self, uv: [f32; 2]) -> u8 {
        if self.width == 0 || self.height == 0 || self.pixels.len() < 4 {
            return 255;
        }
        let x = ((uv[0].clamp(0.0, 1.0) * self.width as f32) as u32).min(self.width - 1);
        let y = ((uv[1].clamp(0.0, 1.0) * self.height as f32) as u32).min(self.height - 1);
        self.pixels[((y * self.width + x) * 4 + 3) as usize]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Images {
    map: HashMap<String, RgbaImage>,
}

impl Images {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<(), Error> {
        let key = key.into();
        let expect = width as usize * height as usize * 4;
        if width == 0 || height == 0 || pixels.len() != expect {
            return Err(Error::BadImage(key));
        }
        self.map.insert(
            key,
            RgbaImage {
                width,
                height,
                pixels,
            },
        );
        Ok(())
    }

    /// 4×4 solid. Tests and placeholders.
    pub fn insert_solid(&mut self, key: impl Into<String>, rgb: [u8; 3]) -> Result<(), Error> {
        let mut pixels = Vec::with_capacity(64);
        for _ in 0..16 {
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        self.insert(key, 4, 4, pixels)
    }

    /// Every image, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &RgbaImage)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Take every image from `other`; its keys win.
    pub fn absorb(&mut self, other: Images) {
        self.map.extend(other.map);
    }

    pub fn get(&self, key: &str) -> Option<&RgbaImage> {
        self.map.get(key)
    }

    /// Alpha at surface UV. v=0 is the top of the image (same as GPU).
    /// Missing keys are opaque (255) so a dump still picks without pixels.
    pub fn alpha_at(&self, key: &str, uv: [f32; 2]) -> u8 {
        let Some(img) = self.map.get(key) else {
            return 255;
        };
        img.alpha_at(uv)
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
