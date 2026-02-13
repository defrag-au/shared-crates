//! State trait for snapshot + delta synchronization
//!
//! The `FlowState` trait defines how state types receive incremental updates.
//! This enables efficient real-time synchronization where the server sends
//! a full snapshot on connect, then only deltas for subsequent changes.

use serde::de::DeserializeOwned;

/// A state type that can receive incremental updates
///
/// Implement this trait for your application state to enable snapshot + delta
/// synchronization. The server sends a full `Self` on connect (snapshot),
/// then `Self::Delta` messages for changes.
///
/// # Example
///
/// ```ignore
/// use ui_flow::FlowState;
///
/// #[derive(Clone, Default)]
/// struct GameState {
///     entities: HashMap<EntityId, Entity>,
///     tick: u64,
/// }
///
/// #[derive(Deserialize)]
/// enum GameDelta {
///     EntityUpdated { id: EntityId, entity: Entity },
///     EntityRemoved { id: EntityId },
///     TickAdvanced { tick: u64 },
/// }
///
/// impl FlowState for GameState {
///     type Delta = GameDelta;
///
///     fn apply_delta(&mut self, delta: Self::Delta) {
///         match delta {
///             GameDelta::EntityUpdated { id, entity } => {
///                 self.entities.insert(id, entity);
///             }
///             GameDelta::EntityRemoved { id } => {
///                 self.entities.remove(&id);
///             }
///             GameDelta::TickAdvanced { tick } => {
///                 self.tick = tick;
///             }
///         }
///     }
/// }
/// ```
pub trait FlowState: Clone + Default {
    /// The delta type for incremental updates
    type Delta: DeserializeOwned;

    /// Apply a delta to update the state
    fn apply_delta(&mut self, delta: Self::Delta);
}
