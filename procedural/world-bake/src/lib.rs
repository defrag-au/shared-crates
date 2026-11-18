//! Reading a baked world.
//!
//! A world bake is a folder: `world.json`, a typed [`WorldManifest`], and one
//! 16-bit greyscale PNG per layer, each an octahedral map of the whole sphere
//! (see [`octahedral`]). Values are stored linearly between the layer's
//! `min` and `max`, which the manifest records, so every layer keeps its real
//! units: elevation in generator units with the sea floor included,
//! temperature in Kelvin, and so on. The sea level is explicit.
//!
//! What it is not: a stylised map. Terraces, cel colour and equirectangular
//! sheets are derived from a bake on demand and never stored as if they were
//! the terrain.
//!
//! This crate only reads (and writes the manifest for a baker). Generating a
//! world and baking it — tectonics, erosion, climate, the detail stage — is
//! the hodlcroft compositor's `world-baker`, which writes this format; a
//! runtime (a macroquad game, a worker) loads a world with this crate alone.
//!
//! Directions use one convention throughout: the pole is +y, latitude is
//! `asin(y)`, longitude runs from +x towards +z ([`lon_lat_to_dir`]).

pub mod octahedral;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bumped when the files or manifest change shape. A bake from another
/// format version is refused rather than misread.
///
/// - 1: eight base layers, all at the manifest's `size`.
/// - 2: a layer may carry its own `size`; optional `elevation_detail` layer
///   and `detail` record.
pub const FORMAT_VERSION: u32 = 2;

/// Mean Earth radius, km (IUGG mean radius R1 = 6371.0088 km, rounded). The
/// generator's `radius_earth` is in Earth radii; this turns it into a size.
pub const EARTH_RADIUS_KM: f32 = 6371.0;

/// The manifest's file name inside a bake folder.
pub const MANIFEST_FILE: &str = "world.json";

/// The layers a bake carries. The base layers are per-vertex fields of the
/// generator's planet, resampled once onto the octahedral grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerName {
    /// Final (eroded) elevation, sea floor included. Generator units; compare
    /// with [`WorldManifest::sea_level`].
    Elevation,
    /// Accumulated water flow from erosion — where rivers run.
    WaterFlow,
    /// Residual moisture from the erosion water simulation.
    Moisture,
    /// Cloud density from the climate simulation, 0–1.
    CloudDensity,
    /// Surface temperature, Kelvin.
    Temperature,
    /// Annual precipitation, relative 0–1.
    Precipitation,
    /// Smoothed humidity/aridity field.
    Humidity,
    /// Volcanic activity marker from tectonics.
    Volcanic,
    /// Elevation at higher resolution with seeded detail and stream-power
    /// erosion on top of [`LayerName::Elevation`] (see [`DetailParams`]).
    /// The coast is the base layer's. Optional: present once the detail stage
    /// has run.
    ElevationDetail,
}

impl LayerName {
    /// The base layers every bake carries, straight from the generator.
    pub const ALL: [LayerName; 8] = [
        LayerName::Elevation,
        LayerName::WaterFlow,
        LayerName::Moisture,
        LayerName::CloudDensity,
        LayerName::Temperature,
        LayerName::Precipitation,
        LayerName::Humidity,
        LayerName::Volcanic,
    ];

    /// File name of the layer inside a bake folder.
    pub fn file_name(self) -> &'static str {
        match self {
            LayerName::Elevation => "elevation.png",
            LayerName::WaterFlow => "water_flow.png",
            LayerName::Moisture => "moisture.png",
            LayerName::CloudDensity => "cloud_density.png",
            LayerName::Temperature => "temperature.png",
            LayerName::Precipitation => "precipitation.png",
            LayerName::Humidity => "humidity.png",
            LayerName::Volcanic => "volcanic.png",
            LayerName::ElevationDetail => "elevation_detail.png",
        }
    }

    pub fn units(self) -> &'static str {
        match self {
            LayerName::Elevation | LayerName::ElevationDetail => "generator elevation units",
            LayerName::Temperature => "kelvin",
            LayerName::WaterFlow => "accumulated flow (relative)",
            _ => "relative",
        }
    }
}

/// Generation quality, mirroring `planet_gen::Quality`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Fast,
    Standard,
    High,
    Ultra,
    Extreme,
}

/// Everything the generator was asked for, mirroring
/// `planet_gen::PlanetGenParams`. Two bakes with equal params from the same
/// generator revision are the same world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GenParams {
    pub seed: u64,
    pub quality: Quality,
    /// Equilibrium temperature, Kelvin (not the surface mean).
    pub temperature_k: f32,
    pub radius_earth: f32,
    pub mass_earth: f32,
    pub density: f32,
    pub eccentricity: f32,
    pub insolation: f32,
    pub stellar_age_gyr: f32,
    pub ocean_fraction: f32,
    pub habitability: f32,
}

/// Which code produced the bake.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generator {
    /// Crate name, `planet-gen`.
    pub name: String,
    /// The generator's crate version at bake time.
    pub version: String,
    /// `planet_gen::FIELDS_REVISION`: bumped when the same params start
    /// producing different fields, which the crate version does not track.
    pub revision: u32,
}

/// How big the world is. `provisional` until the world's scale is decided;
/// the radius comes from the generator params, not from canon.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scale {
    pub radius_km: f32,
    pub provisional: bool,
}

/// How one layer's values are stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerInfo {
    pub name: LayerName,
    pub file: String,
    pub units: String,
    /// The stored value 0 decodes to `min`, 65535 to `max`.
    pub min: f32,
    pub max: f32,
    /// Side of this layer's map when it differs from the manifest's `size`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
}

impl LayerInfo {
    pub fn decode(&self, stored: u16) -> f32 {
        self.min + stored as f32 / 65_535.0 * (self.max - self.min)
    }

    pub fn encode(&self, value: f32) -> u16 {
        let span = (self.max - self.min).max(f32::MIN_POSITIVE);
        (((value - self.min) / span).clamp(0.0, 1.0) * 65_535.0).round() as u16
    }
}

/// `world.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldManifest {
    pub format_version: u32,
    /// A name for people; optional.
    pub name: Option<String>,
    pub params: GenParams,
    pub generator: Generator,
    pub scale: Scale,
    /// The sea surface, in elevation units: the level that gives the
    /// requested ocean fraction.
    pub sea_level: f32,
    /// Side of every layer's square octahedral map, in texels.
    pub size: u32,
    /// Vertices in the generator's icosphere — the true sampling density;
    /// the octahedral grid resamples it, it does not add detail.
    pub source_vertices: usize,
    pub layers: Vec<LayerInfo>,
    /// How `elevation_detail` was made, when the bake has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<DetailInfo>,
}

/// Parameters of the detail stage. Together with the base bake they fix the
/// detail layer exactly: the stage is deterministic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetailParams {
    /// Side of the detail layer's octahedral map, texels.
    pub size: u32,
    /// Seeds the detail noise. Independent of the generator seed so the same
    /// world can be re-detailed.
    pub seed: u64,
    /// Radius (radians) the base elevation is averaged over before detail is
    /// added: removes the generator's texel-scale grain.
    pub smoothing: f32,
    /// Detail relief as a share of the world's land relief.
    pub amplitude: f32,
    /// Stream-power iterations.
    pub iterations: u32,
    /// Incision per iteration for a one-cell catchment one texel long.
    pub erodibility: f32,
    /// Exponent on drainage area (`m` in `E = K A^m S`).
    pub area_exponent: f32,
    /// Hillslope diffusion per iteration, 0–0.25.
    pub diffusion: f32,
    /// Share of the gap back to the amplified shape restored each
    /// iteration: the uplift erosion works against. 0 lets erosion only
    /// lower the land. Absent from revision-1 records, which had none.
    #[serde(default)]
    pub uplift: f32,
}

impl DetailParams {
    /// The defaults the PERPS world was tuned with.
    pub fn standard(seed: u64) -> Self {
        Self {
            size: 2048,
            seed,
            smoothing: 0.0092,
            amplitude: 0.35,
            iterations: 120,
            erodibility: 0.05,
            area_exponent: 0.45,
            diffusion: 0.12,
            uplift: 0.08,
        }
    }
}

/// `world.json`'s record of the detail stage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetailInfo {
    pub params: DetailParams,
    /// The baker's detail revision at bake time.
    pub revision: u32,
}

impl WorldManifest {
    pub fn layer(&self, name: LayerName) -> Option<&LayerInfo> {
        self.layers.iter().find(|layer| layer.name == name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BakeError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Image {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("{path}: {source}")]
    Manifest {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{path} is bake format {found}; this reader understands {expected}")]
    FormatVersion {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    #[error("{path} is {found}x{found_h}, the manifest says {expected}x{expected}")]
    LayerSize {
        path: PathBuf,
        found: u32,
        found_h: u32,
        expected: u32,
    },
    #[error("the manifest lists no {0:?} layer")]
    MissingLayer(LayerName),
    #[error("bake does not match what was asked for: {0}")]
    Mismatch(String),
}

/// One loaded layer: decoded values on the octahedral grid.
pub struct Layer {
    pub info: LayerInfo,
    size: u32,
    values: Vec<f32>,
}

impl Layer {
    /// A layer from decoded values, row-major, `size × size`.
    pub fn from_values(info: LayerInfo, size: u32, values: Vec<f32>) -> Self {
        assert_eq!(values.len(), (size * size) as usize, "size × size values");
        Self { info, size, values }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    /// Decoded values, row-major, `size × size`, row 0 at `v = 0`.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// The value averaged over a small disc on the sphere, `radius` in
    /// radians: two rings of eight samples around the centre. Use it to
    /// remove the generator's texel-scale grain at a fixed size on the ground,
    /// whatever projection or zoom the consumer draws at.
    pub fn sample_disc(&self, dir: [f32; 3], radius: f32) -> f32 {
        let d = normalize(dir);
        let up = if d[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let u = normalize(cross(d, up));
        let v = cross(d, u);
        let mut total = self.sample(d) * 2.0;
        let mut weight = 2.0;
        for (ring, w) in [(0.5_f32, 1.0_f32), (1.0, 0.6)] {
            for k in 0..8 {
                let theta = k as f32 * std::f32::consts::FRAC_PI_4 + ring * 0.4;
                let (s, c) = theta.sin_cos();
                let off = radius * ring;
                let p = [
                    d[0] + (u[0] * c + v[0] * s) * off,
                    d[1] + (u[1] * c + v[1] * s) * off,
                    d[2] + (u[2] * c + v[2] * s) * off,
                ];
                total += self.sample(p) * w;
                weight += w;
            }
        }
        total / weight
    }

    /// The value in a direction, bilinear between texel centres. Edges clamp:
    /// across the octahedral fold lines that is a small error, at most one
    /// texel's worth.
    pub fn sample(&self, dir: [f32; 3]) -> f32 {
        let (u, v) = octahedral::dir_to_uv(dir);
        let s = self.size as f32;
        let (fx, fy) = (u * s - 0.5, v * s - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let at = |x: f32, y: f32| {
            let max = self.size as i64 - 1;
            let xi = (x as i64).clamp(0, max) as usize;
            let yi = (y as i64).clamp(0, max) as usize;
            self.values[yi * self.size as usize + xi]
        };
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1.0, y0) * tx;
        let bottom = at(x0, y0 + 1.0) * (1.0 - tx) + at(x0 + 1.0, y0 + 1.0) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// A loaded bake.
pub struct World {
    pub manifest: WorldManifest,
    layers: Vec<Layer>,
}

impl World {
    /// Load a bake folder, checking the format version and every layer's size.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, BakeError> {
        let dir = dir.as_ref();
        let manifest = read_manifest(dir)?;
        let mut layers = Vec::with_capacity(manifest.layers.len());
        for info in &manifest.layers {
            let path = dir.join(&info.file);
            let image = image::open(&path)
                .map_err(|source| BakeError::Image {
                    path: path.clone(),
                    source,
                })?
                .into_luma16();
            let size = info.size.unwrap_or(manifest.size);
            if image.width() != size || image.height() != size {
                return Err(BakeError::LayerSize {
                    path,
                    found: image.width(),
                    found_h: image.height(),
                    expected: size,
                });
            }
            let values = image.pixels().map(|p| info.decode(p.0[0])).collect();
            layers.push(Layer {
                info: info.clone(),
                size,
                values,
            });
        }
        Ok(Self { manifest, layers })
    }

    /// Load a bake and refuse it unless it is the world `params` describe, at
    /// the expected generator revision. This is the check that stops a stale
    /// bake being used silently after the generator changes.
    pub fn open_matching(
        dir: impl AsRef<Path>,
        params: &GenParams,
        revision: u32,
    ) -> Result<Self, BakeError> {
        let world = Self::open(dir)?;
        if &world.manifest.params != params {
            return Err(BakeError::Mismatch(format!(
                "baked for {:?}, asked for {params:?}",
                world.manifest.params
            )));
        }
        if world.manifest.generator.revision != revision {
            return Err(BakeError::Mismatch(format!(
                "baked at generator revision {}, current is {revision}",
                world.manifest.generator.revision
            )));
        }
        Ok(world)
    }

    pub fn layer(&self, name: LayerName) -> Result<&Layer, BakeError> {
        self.layers
            .iter()
            .find(|layer| layer.info.name == name)
            .ok_or(BakeError::MissingLayer(name))
    }

    /// Elevation above (positive) or below (negative) the sea, in elevation
    /// units, in a direction.
    pub fn height_above_sea(&self, dir: [f32; 3]) -> Result<f32, BakeError> {
        Ok(self.layer(LayerName::Elevation)?.sample(dir) - self.manifest.sea_level)
    }
}

/// One traced river: from a source down to the sea or to the river it joins
/// (the junction is its last point, so the network connects). Plain data, so
/// a reader can draw rivers without the generator; tracing them is the
/// baker's drainage pass.
pub struct River {
    /// Unit directions, upstream first.
    pub points: Vec<[f32; 3]>,
    /// Rain-weighted catchment at each point.
    pub flow: Vec<f32>,
}

/// The unit direction for a longitude and latitude, radians — the convention
/// every flat export of a bake uses. The pole is +y (latitude is `asin(y)`);
/// longitude runs from +x towards +z.
pub fn lon_lat_to_dir(lon: f32, lat: f32) -> [f32; 3] {
    let (sl, cl) = lat.sin_cos();
    let (so, co) = lon.sin_cos();
    [cl * co, sl, cl * so]
}

/// Longitude and latitude, radians, of a direction (need not be unit).
pub fn dir_to_lon_lat(dir: [f32; 3]) -> (f32, f32) {
    let d = normalize(dir);
    (d[2].atan2(d[0]), d[1].clamp(-1.0, 1.0).asin())
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2])
        .sqrt()
        .max(f32::MIN_POSITIVE);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Write `world.json` into a bake folder — for a baker, after its layers.
pub fn write_manifest(dir: &Path, manifest: &WorldManifest) -> Result<(), BakeError> {
    let path = dir.join(MANIFEST_FILE);
    let text = serde_json::to_string_pretty(manifest).map_err(|source| BakeError::Manifest {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, text).map_err(|source| BakeError::Io { path, source })
}

fn read_manifest(dir: &Path) -> Result<WorldManifest, BakeError> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|source| BakeError::Io {
        path: path.clone(),
        source,
    })?;
    let manifest: WorldManifest =
        serde_json::from_str(&text).map_err(|source| BakeError::Manifest {
            path: path.clone(),
            source,
        })?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(BakeError::FormatVersion {
            path,
            found: manifest.format_version,
            expected: FORMAT_VERSION,
        });
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    fn params(seed: u64) -> GenParams {
        GenParams {
            seed,
            quality: Quality::Fast,
            temperature_k: 255.0,
            radius_earth: 1.0,
            mass_earth: 1.0,
            density: 5.5,
            eccentricity: 0.017,
            insolation: 1.0,
            stellar_age_gyr: 4.6,
            ocean_fraction: 0.7,
            habitability: 0.85,
        }
    }

    /// A tiny bake written by hand: elevation equal to the direction's y, so
    /// sampling can be checked against the geometry.
    fn write_bake(dir: &Path, size: u32) -> WorldManifest {
        let name = LayerName::Elevation;
        let info = LayerInfo {
            name,
            file: name.file_name().to_string(),
            units: name.units().to_string(),
            min: -1.0,
            max: 1.0,
            size: None,
        };
        let raw: Vec<u16> = (0..size * size)
            .map(|i| {
                let (x, y) = (i % size, i / size);
                let d = octahedral::uv_to_dir(
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                );
                info.encode(d[1])
            })
            .collect();
        let image: ImageBuffer<Luma<u16>, Vec<u16>> =
            ImageBuffer::from_raw(size, size, raw).unwrap();
        image.save(dir.join(&info.file)).unwrap();
        let manifest = WorldManifest {
            format_version: FORMAT_VERSION,
            name: Some("test".to_string()),
            params: params(1),
            generator: Generator {
                name: "planet-gen".to_string(),
                version: "0.1.0".to_string(),
                revision: 2,
            },
            scale: Scale {
                radius_km: EARTH_RADIUS_KM,
                provisional: true,
            },
            sea_level: 0.0,
            size,
            source_vertices: 0,
            layers: vec![info],
            detail: None,
        };
        write_manifest(dir, &manifest).unwrap();
        manifest
    }

    #[test]
    fn a_bake_reads_back_what_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_bake(dir.path(), 64);
        let world = World::open(dir.path()).unwrap();
        assert_eq!(world.manifest, written);
        let elevation = world.layer(LayerName::Elevation).unwrap();
        assert_eq!(elevation.values().len(), 64 * 64);
        // Elevation is y: high at the north pole, low at the south, zero on
        // the equator — so height above the (zero) sea follows latitude.
        for (lat, expect) in [(80.0f32, 0.985f32), (0.0, 0.0), (-45.0, -0.707)] {
            let d = lon_lat_to_dir(0.3, lat.to_radians());
            let h = world.height_above_sea(d).unwrap();
            assert!((h - expect).abs() < 0.03, "lat {lat}: {h} vs {expect}");
        }
        assert!(matches!(
            world.layer(LayerName::Temperature),
            Err(BakeError::MissingLayer(LayerName::Temperature))
        ));
    }

    #[test]
    fn another_format_or_world_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = write_bake(dir.path(), 16);
        assert!(World::open_matching(dir.path(), &params(1), 2).is_ok());
        assert!(matches!(
            World::open_matching(dir.path(), &params(2), 2),
            Err(BakeError::Mismatch(_))
        ));
        assert!(matches!(
            World::open_matching(dir.path(), &params(1), 3),
            Err(BakeError::Mismatch(_))
        ));
        manifest.format_version = FORMAT_VERSION + 1;
        write_manifest(dir.path(), &manifest).unwrap();
        assert!(matches!(
            World::open(dir.path()),
            Err(BakeError::FormatVersion { .. })
        ));
    }

    #[test]
    fn values_survive_encoding_to_a_step() {
        let info = LayerInfo {
            name: LayerName::Temperature,
            file: String::new(),
            units: String::new(),
            min: 200.0,
            max: 320.0,
            size: None,
        };
        let step = (info.max - info.min) / 65_535.0;
        for v in [200.0, 255.5, 273.15, 319.99] {
            assert!((info.decode(info.encode(v)) - v).abs() <= step);
        }
    }

    #[test]
    fn longitude_and_latitude_round_trip() {
        for (lon, lat) in [(0.0f32, 0.0f32), (1.2, 0.4), (-2.9, -1.3)] {
            let (lon2, lat2) = dir_to_lon_lat(lon_lat_to_dir(lon, lat));
            assert!((lon - lon2).abs() < 1e-5 && (lat - lat2).abs() < 1e-5);
        }
    }
}
