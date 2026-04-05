use crate::Direction;

/// Unique identifier for a tile in a tileset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId(pub u16);

/// Edge label — tiles are compatible when their touching edges share a label.
type EdgeLabel = u16;

/// A single tile definition.
#[derive(Debug, Clone)]
pub struct TileDef {
    pub id: TileId,
    pub name: String,
    /// Edge labels: [North, East, South, West].
    pub edges: [EdgeLabel; 4],
    /// Relative weight for random selection (higher = more common).
    pub weight: f32,
}

/// A collection of tiles with adjacency rules derived from edge labels.
///
/// Two tiles can be placed adjacent when the touching edges have the same label.
/// For example, tile A's East edge must match tile B's West edge for A to be
/// placed to the left of B.
#[derive(Debug, Clone)]
pub struct Tileset {
    tiles: Vec<TileDef>,
    /// Interned edge label strings → EdgeLabel ids.
    edge_labels: Vec<String>,
    /// Precomputed: for each (tile, direction), which tile IDs are compatible neighbors.
    compatibility: Vec<Vec<Vec<TileId>>>, // [tile_idx][dir] → Vec<TileId>
    dirty: bool,
}

impl Tileset {
    pub fn new() -> Self {
        Self {
            tiles: Vec::new(),
            edge_labels: Vec::new(),
            compatibility: Vec::new(),
            dirty: true,
        }
    }

    /// Add a tile by name. Returns its TileId.
    pub fn add_tile(&mut self, name: &str) -> TileId {
        let id = TileId(self.tiles.len() as u16);
        self.tiles.push(TileDef {
            id,
            name: name.to_string(),
            edges: [0; 4],
            weight: 1.0,
        });
        self.dirty = true;
        id
    }

    /// Add a tile with a specific weight (for biasing frequency).
    pub fn add_tile_weighted(&mut self, name: &str, weight: f32) -> TileId {
        let id = self.add_tile(name);
        self.tiles[id.0 as usize].weight = weight;
        id
    }

    /// Set edge labels for a tile: [North, East, South, West].
    pub fn set_edges(&mut self, tile: TileId, edges: [&str; 4]) {
        let labels: [EdgeLabel; 4] = [
            self.intern_edge(edges[0]),
            self.intern_edge(edges[1]),
            self.intern_edge(edges[2]),
            self.intern_edge(edges[3]),
        ];
        self.tiles[tile.0 as usize].edges = labels;
        self.dirty = true;
    }

    /// Number of tiles in the set.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Get a tile definition.
    pub fn tile(&self, id: TileId) -> &TileDef {
        &self.tiles[id.0 as usize]
    }

    /// Get all tile IDs.
    pub fn tile_ids(&self) -> impl Iterator<Item = TileId> {
        (0..self.tiles.len() as u16).map(TileId)
    }

    /// Get the weight of a tile.
    pub fn weight(&self, id: TileId) -> f32 {
        self.tiles[id.0 as usize].weight
    }

    /// Get compatible neighbors for a tile in a given direction.
    ///
    /// Returns the set of tile IDs that can be placed adjacent to `tile`
    /// in the given `direction`.
    pub fn compatible(&mut self, tile: TileId, direction: Direction) -> &[TileId] {
        if self.dirty {
            self.rebuild_compatibility();
        }
        &self.compatibility[tile.0 as usize][direction.index()]
    }

    /// Rebuild the compatibility lookup from edge labels.
    fn rebuild_compatibility(&mut self) {
        let n = self.tiles.len();
        self.compatibility = vec![vec![Vec::new(); 4]; n];

        for i in 0..n {
            for dir in Direction::ALL {
                let my_edge = self.tiles[i].edges[dir.index()];
                let opp = dir.opposite().index();

                for j in 0..n {
                    if self.tiles[j].edges[opp] == my_edge {
                        self.compatibility[i][dir.index()].push(TileId(j as u16));
                    }
                }
            }
        }

        self.dirty = false;
    }

    /// Intern an edge label string to a numeric ID.
    fn intern_edge(&mut self, label: &str) -> EdgeLabel {
        if let Some(pos) = self.edge_labels.iter().position(|l| l == label) {
            pos as EdgeLabel
        } else {
            let id = self.edge_labels.len() as EdgeLabel;
            self.edge_labels.push(label.to_string());
            id
        }
    }
}

impl Default for Tileset {
    fn default() -> Self {
        Self::new()
    }
}
