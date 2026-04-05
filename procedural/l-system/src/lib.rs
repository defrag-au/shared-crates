//! L-system string rewriting and turtle interpretation for procedural generation.
//!
//! An L-system defines a grammar (axiom + production rules) that is iteratively
//! rewritten to produce complex patterns from simple rules. A turtle interpreter
//! then walks the resulting string to produce geometry.
//!
//! Supports:
//! - **Deterministic** rules — one successor per symbol
//! - **Stochastic** rules — weighted random choice between multiple successors
//! - **Parametric** symbols — symbols carrying f32 parameters
//! - **Turtle interpretation** — configurable heading, step length, branch stack
//!
//! # Example
//!
//! ```
//! use l_system::{LSystem, TurtleConfig, interpret};
//!
//! let mut sys = LSystem::new("F");
//! sys.add_rule('F', "F+F-F-F+F"); // Koch curve
//!
//! let result = sys.iterate(3);
//!
//! let config = TurtleConfig {
//!     step_length: 5.0,
//!     angle_delta: 90.0_f32.to_radians(),
//!     ..Default::default()
//! };
//! let segments = interpret(&result, &config);
//! assert!(!segments.is_empty());
//! ```

mod grammar;
mod turtle;

pub use grammar::{LSystem, Symbol};
pub use turtle::{interpret, Segment, TurtleConfig};
