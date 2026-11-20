//! Tile set definition, TOML loading, and rotational expansion.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;

/// Unique tile identifier (index into the expanded tile array).
pub type TileId = u16;

/// Socket identifier — single ASCII char mapped to a u8.
pub type SocketId = u8;

/// The 6 hex directions in clockwise order starting from NE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HexDirection {
    NE = 0,
    E = 1,
    SE = 2,
    SW = 3,
    W = 4,
    NW = 5,
}

impl HexDirection {
    pub const ALL: [HexDirection; 6] = [
        HexDirection::NE,
        HexDirection::E,
        HexDirection::SE,
        HexDirection::SW,
        HexDirection::W,
        HexDirection::NW,
    ];

    /// The opposite direction (the edge the neighbour sees).
    pub fn opposite(self) -> HexDirection {
        match self {
            HexDirection::NE => HexDirection::SW,
            HexDirection::E => HexDirection::W,
            HexDirection::SE => HexDirection::NW,
            HexDirection::SW => HexDirection::NE,
            HexDirection::W => HexDirection::E,
            HexDirection::NW => HexDirection::SE,
        }
    }
}

/// A single expanded tile (after rotation). `T` is the caller's terrain type.
#[derive(Debug, Clone)]
pub struct Tile<T> {
    /// Index in the expanded tile array.
    pub id: TileId,
    /// Source tile id from the TOML definition.
    pub source_id: String,
    /// Terrain for this tile.
    pub terrain: T,
    /// Socket on each of the 6 edges, indexed by [`HexDirection`].
    pub sockets: [SocketId; 6],
    /// Base frequency weight for weighted random selection.
    pub weight: f32,
    /// Which rotation of the source tile (0-5). 0 = original.
    pub rotation: u8,
}

/// The complete tile set: expanded tiles + socket pairing rules.
#[derive(Debug, Clone)]
pub struct TileSet<T> {
    /// All tiles including rotational variants.
    pub tiles: Vec<Tile<T>>,
    /// Valid socket pairs — (a, b) means socket `a` on one hex can face socket `b`
    /// on the adjacent hex. Stored both directions for O(1) lookup.
    pub valid_pairs: HashSet<(SocketId, SocketId)>,
    /// Hex layout string (e.g. "even-r") — threaded through to consumers.
    pub layout: String,
    /// Chunk size from config.
    pub chunk_size: u32,
    /// Socket char → SocketId mapping.
    pub socket_names: HashMap<char, SocketId>,
    /// Reverse mapping for display.
    pub socket_chars: HashMap<SocketId, char>,
    /// Precomputed: for each (socket_id, direction), which tile IDs are compatible?
    /// Key: socket_id that appears on the neighbour's facing edge.
    /// Value: tile IDs that have a socket compatible with that socket in the given direction.
    pub compatible_tiles: HashMap<(SocketId, HexDirection), Vec<TileId>>,
}

#[derive(Debug, thiserror::Error)]
pub enum TileSetError {
    #[error("failed to read tile set file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse tile set TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unknown socket char '{0}' in tile '{1}'")]
    UnknownSocket(char, String),
    #[error("socket string must be exactly 6 chars, got {0} in tile '{1}'")]
    BadSocketLength(usize, String),
    #[error("invalid socket pair '{0}' — each side must be a single char")]
    BadPairFormat(String),
    #[error("unknown socket char '{0}' in pair definition")]
    UnknownPairSocket(char),
    #[error("unknown terrain '{0}' in tile '{1}' — the terrain type does not parse it")]
    UnknownTerrain(String, String),
}

// --- TOML schema ---

#[derive(Deserialize)]
struct TomlTileSet {
    chunk: TomlChunk,
    sockets: TomlSockets,
    tiles: Vec<TomlTile>,
}

#[derive(Deserialize)]
struct TomlChunk {
    layout: String,
    size: u32,
}

#[derive(Deserialize)]
struct TomlSockets {
    pairs: Vec<String>,
}

#[derive(Deserialize)]
struct TomlTile {
    id: String,
    terrain: String,
    sockets: String,
    weight: f32,
    #[serde(default)]
    rotate: bool,
}

impl<T: FromStr + Clone> TileSet<T> {
    /// Load a tile set from a TOML file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, TileSetError> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_toml(&contents)
    }

    /// Parse a tile set from a TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, TileSetError> {
        let raw: TomlTileSet = toml::from_str(toml_str)?;

        // Build socket char → id mapping from all chars seen in pairs + tiles
        let mut socket_names: HashMap<char, SocketId> = HashMap::new();
        let mut next_id: SocketId = 0;

        let mut register_char = |c: char| -> SocketId {
            *socket_names.entry(c).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            })
        };

        // Register sockets from pair definitions
        for pair_str in &raw.sockets.pairs {
            let parts: Vec<&str> = pair_str.split('-').collect();
            if parts.len() != 2 || parts[0].len() != 1 || parts[1].len() != 1 {
                return Err(TileSetError::BadPairFormat(pair_str.clone()));
            }
            register_char(parts[0].chars().next().unwrap());
            register_char(parts[1].chars().next().unwrap());
        }

        // Register sockets from tile definitions
        for tile in &raw.tiles {
            for c in tile.sockets.chars() {
                register_char(c);
            }
        }

        // Build valid pairs set (both directions)
        let mut valid_pairs: HashSet<(SocketId, SocketId)> = HashSet::new();
        for pair_str in &raw.sockets.pairs {
            let parts: Vec<&str> = pair_str.split('-').collect();
            let a = *socket_names
                .get(&parts[0].chars().next().unwrap())
                .ok_or_else(|| TileSetError::UnknownPairSocket(parts[0].chars().next().unwrap()))?;
            let b = *socket_names
                .get(&parts[1].chars().next().unwrap())
                .ok_or_else(|| TileSetError::UnknownPairSocket(parts[1].chars().next().unwrap()))?;
            valid_pairs.insert((a, b));
            valid_pairs.insert((b, a));
        }

        // Expand tiles (with rotational variants)
        let mut tiles: Vec<Tile<T>> = Vec::new();
        for raw_tile in &raw.tiles {
            if raw_tile.sockets.len() != 6 {
                return Err(TileSetError::BadSocketLength(
                    raw_tile.sockets.len(),
                    raw_tile.id.clone(),
                ));
            }

            // The caller's terrain type parses the tile's terrain string
            let terrain: T = raw_tile.terrain.parse().map_err(|_| {
                TileSetError::UnknownTerrain(raw_tile.terrain.clone(), raw_tile.id.clone())
            })?;

            let base_sockets: [SocketId; 6] = {
                let mut arr = [0u8; 6];
                for (i, c) in raw_tile.sockets.chars().enumerate() {
                    arr[i] = *socket_names
                        .get(&c)
                        .ok_or_else(|| TileSetError::UnknownSocket(c, raw_tile.id.clone()))?;
                }
                arr
            };

            // Collect unique rotational variants first, then distribute weight evenly.
            // A tile with weight 10 and 6 rotations gets 10/6 per variant,
            // so the total "budget" for that source tile stays at 10.
            let rotations = if raw_tile.rotate { 6 } else { 1 };
            let mut variants: Vec<([SocketId; 6], u8)> = Vec::new();
            for rot in 0..rotations {
                let sockets = rotate_sockets(base_sockets, rot);
                let already_exists = variants.iter().any(|(s, _)| *s == sockets);
                if !already_exists {
                    variants.push((sockets, rot as u8));
                }
            }

            let per_variant_weight = raw_tile.weight / variants.len() as f32;

            for (sockets, rot) in variants {
                let id = tiles.len() as TileId;
                tiles.push(Tile {
                    id,
                    source_id: raw_tile.id.clone(),
                    terrain: terrain.clone(),
                    sockets,
                    weight: per_variant_weight,
                    rotation: rot,
                });
            }
        }

        // Reverse socket mapping
        let socket_chars: HashMap<SocketId, char> =
            socket_names.iter().map(|(&c, &id)| (id, c)).collect();

        // Precompute compatibility index
        let compatible_tiles = build_compatibility_index(&tiles, &valid_pairs);

        Ok(TileSet {
            tiles,
            valid_pairs,
            layout: raw.chunk.layout,
            chunk_size: raw.chunk.size,
            socket_names,
            socket_chars,
            compatible_tiles,
        })
    }
}

impl<T> TileSet<T> {
    /// Get a human-readable socket string for a tile.
    pub fn socket_string(&self, tile: &Tile<T>) -> String {
        tile.sockets
            .iter()
            .map(|s| self.socket_chars.get(s).copied().unwrap_or('?'))
            .collect()
    }
}

impl<T: Eq + Hash> TileSet<T> {
    /// Summary statistics for the tile set.
    pub fn summary(&self) -> TileSetSummary {
        let unique_terrains: HashSet<&T> = self.tiles.iter().map(|t| &t.terrain).collect();
        TileSetSummary {
            total_tiles: self.tiles.len(),
            unique_terrains: unique_terrains.len(),
            socket_count: self.socket_names.len(),
            pair_count: self.valid_pairs.len() / 2, // stored both directions
        }
    }
}

#[derive(Debug)]
pub struct TileSetSummary {
    pub total_tiles: usize,
    pub unique_terrains: usize,
    pub socket_count: usize,
    pub pair_count: usize,
}

/// Rotate sockets clockwise by `steps` positions.
/// sockets[0]=NE, [1]=E, [2]=SE, [3]=SW, [4]=W, [5]=NW
/// Rotation by 1: last element moves to front.
fn rotate_sockets(sockets: [SocketId; 6], steps: usize) -> [SocketId; 6] {
    let steps = steps % 6;
    let mut result = [0u8; 6];
    for i in 0..6 {
        result[i] = sockets[(i + 6 - steps) % 6];
    }
    result
}

/// Build a lookup: given a socket that a neighbour presents on a shared edge,
/// which of our tiles can sit in that direction?
fn build_compatibility_index<T>(
    tiles: &[Tile<T>],
    valid_pairs: &HashSet<(SocketId, SocketId)>,
) -> HashMap<(SocketId, HexDirection), Vec<TileId>> {
    let mut index: HashMap<(SocketId, HexDirection), Vec<TileId>> = HashMap::new();

    // Collect all socket IDs that appear anywhere
    let all_sockets: HashSet<SocketId> = tiles.iter().flat_map(|t| t.sockets).collect();

    for &neighbour_socket in &all_sockets {
        for dir in HexDirection::ALL {
            // We need tiles whose socket in `dir` is compatible with `neighbour_socket`
            let our_edge = dir as usize;
            let compatible: Vec<TileId> = tiles
                .iter()
                .filter(|t| valid_pairs.contains(&(t.sockets[our_edge], neighbour_socket)))
                .map(|t| t.id)
                .collect();
            index.insert((neighbour_socket, dir), compatible);
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Terrain as the plain string, so the tests need no game's enum.
    fn load_ocean_islands() -> TileSet<String> {
        let toml_str =
            std::fs::read_to_string("tests/fixtures/ocean_islands.toml").expect("tile set file");
        TileSet::from_toml(&toml_str).expect("valid tile set")
    }

    #[test]
    fn load_ocean_islands_tileset() {
        let ts = load_ocean_islands();

        let summary = ts.summary();
        assert!(summary.total_tiles > 20, "expected rotational expansion");
        assert_eq!(ts.layout, "even-r");
        assert_eq!(ts.chunk_size, 96);

        // Verify coast_bay has 6 rotations
        let bay_count = ts
            .tiles
            .iter()
            .filter(|t| t.source_id == "coast_bay")
            .count();
        assert_eq!(bay_count, 6, "coast_bay should have 6 rotational variants");

        // Verify deep_ocean has only 1 (symmetric)
        let deep_count = ts
            .tiles
            .iter()
            .filter(|t| t.source_id == "deep_ocean")
            .count();
        assert_eq!(deep_count, 1, "deep_ocean is symmetric, no rotation needed");

        // Verify lowland has only 1 (symmetric)
        let land_count = ts.tiles.iter().filter(|t| t.source_id == "lowland").count();
        assert_eq!(land_count, 1);
    }

    #[test]
    fn socket_pairs_are_bidirectional() {
        let ts = load_ocean_islands();

        // c-C should work both ways
        let c = ts.socket_names[&'c'];
        let big_c = ts.socket_names[&'C'];
        assert!(ts.valid_pairs.contains(&(c, big_c)));
        assert!(ts.valid_pairs.contains(&(big_c, c)));

        // d-d should be valid
        let d = ts.socket_names[&'d'];
        assert!(ts.valid_pairs.contains(&(d, d)));
    }

    #[test]
    fn rotate_sockets_works() {
        let base = [1, 2, 3, 4, 5, 6];
        assert_eq!(rotate_sockets(base, 0), [1, 2, 3, 4, 5, 6]);
        assert_eq!(rotate_sockets(base, 1), [6, 1, 2, 3, 4, 5]);
        assert_eq!(rotate_sockets(base, 2), [5, 6, 1, 2, 3, 4]);
        assert_eq!(rotate_sockets(base, 6), [1, 2, 3, 4, 5, 6]); // full rotation = identity
    }

    /// A terrain type that only knows two names: anything else is refused.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Narrow {
        DeepOcean,
        Sea,
    }

    impl FromStr for Narrow {
        type Err = ();
        fn from_str(s: &str) -> Result<Self, ()> {
            match s {
                "deep_ocean" => Ok(Narrow::DeepOcean),
                "sea" => Ok(Narrow::Sea),
                _ => Err(()),
            }
        }
    }

    #[test]
    fn a_terrain_the_type_cannot_parse_is_refused() {
        let toml_str =
            std::fs::read_to_string("tests/fixtures/ocean_islands.toml").expect("tile set file");
        let err = TileSet::<Narrow>::from_toml(&toml_str).expect_err("shallows is not Narrow");
        assert!(matches!(err, TileSetError::UnknownTerrain(..)), "{err}");
    }
}
