//! Procedural city layout generation using tensor fields and streamline tracing.
//!
//! The pipeline:
//! 1. Define a tensor field by placing basis fields (grid, radial)
//! 2. Trace streamlines along the field's eigenvectors to form roads
//! 3. Build a road graph with intersections as nodes
//! 4. Detect enclosed blocks from the graph
//! 5. Subdivide blocks into building lots
//!
//! The tensor field approach naturally supports mixed city styles — place a grid
//! field in one district and a radial field in another, and they blend smoothly.

pub mod tensor;
pub mod streamline;
pub mod graph;
pub mod blocks;
mod spatial;
mod vec2;

pub use vec2::Vec2;
pub use tensor::{TensorField, BasisField, FieldType};
pub use streamline::{StreamlineConfig, trace_streamlines, Streamline};
pub use graph::{RoadGraph, RoadNode, RoadEdge};
pub use blocks::{detect_blocks, subdivide_block, Block, Lot};
