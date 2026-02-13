//! UI Flow stories - overview, FlowState trait, operations

use super::helpers::{ConfigOptionCard, FlowConceptCard, LoaderStepCard};
use leptos::prelude::*;

// ============================================================================
// Flow Overview Story
// ============================================================================

#[component]
pub fn FlowOverviewStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"UI Flow Overview"</h2>
                <p>"Real-time state synchronization using the snapshot + delta pattern over WebSockets."</p>
            </div>

            <div class="story-section">
                <h3>"Core Concepts"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <FlowConceptCard
                            title="Snapshot"
                            description="Full state sent on initial connection. Client replaces local state entirely."
                        />
                        <FlowConceptCard
                            title="Delta"
                            description="Incremental updates sent after snapshot. Client applies changes to existing state."
                        />
                        <FlowConceptCard
                            title="FlowState"
                            description="Trait for state types that can receive deltas via apply_delta() method."
                        />
                        <FlowConceptCard
                            title="OpId"
                            description="Operation ID for tracking client actions through their lifecycle (optimistic UI)."
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Protocol Flow"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <LoaderStepCard
                            step="1"
                            title="Connect"
                            description="Client opens WebSocket connection to server"
                        />
                        <LoaderStepCard
                            step="2"
                            title="Snapshot"
                            description="Server sends full state snapshot with sequence number"
                        />
                        <LoaderStepCard
                            step="3"
                            title="Deltas"
                            description="Server sends incremental updates as state changes"
                        />
                        <LoaderStepCard
                            step="4"
                            title="Actions"
                            description="Client sends actions with OpId for optimistic UI tracking"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Quick Start"</h3>
                <pre class="code-block">{r#"use ui_flow::{FlowConnection, FlowState, OpId};

// Connect with callbacks
let conn = FlowConnection::<MyState, MyAction>::builder()
    .url("wss://example.com/realtime")
    .on_snapshot(|state, seq| {
        // Replace local state
    })
    .on_delta(|delta, seq| {
        // Apply incremental update
    })
    .on_status(|status| {
        // Update connection indicator
    })
    .connect()?;

// Send action with tracking
let op_id = OpId::new();
conn.send_action(op_id, MyAction::DoSomething)?;"#}</pre>
            </div>
        </div>
    }
}

// ============================================================================
// Flow State Story
// ============================================================================

#[component]
pub fn FlowStateStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"FlowState Trait"</h2>
                <p>"Define how your state type receives incremental updates from the server."</p>
            </div>

            <div class="story-section">
                <h3>"Trait Definition"</h3>
                <div class="story-canvas">
                    <pre class="code-block">{r#"pub trait FlowState: Clone + Default {
    /// The delta type for incremental updates
    type Delta: DeserializeOwned;

    /// Apply a delta to update the state
    fn apply_delta(&mut self, delta: Self::Delta);
}"#}</pre>
                </div>
            </div>

            <div class="story-section">
                <h3>"Implementation Example"</h3>
                <div class="story-canvas">
                    <pre class="code-block">{r#"use ui_flow::FlowState;
use std::collections::HashMap;

#[derive(Clone, Default, Deserialize)]
struct GameState {
    entities: HashMap<String, Entity>,
    tick: u64,
}

#[derive(Deserialize)]
enum GameDelta {
    EntityUpdated { id: String, entity: Entity },
    EntityRemoved { id: String },
    TickAdvanced { tick: u64 },
}

impl FlowState for GameState {
    type Delta = GameDelta;

    fn apply_delta(&mut self, delta: Self::Delta) {
        match delta {
            GameDelta::EntityUpdated { id, entity } => {
                self.entities.insert(id, entity);
            }
            GameDelta::EntityRemoved { id } => {
                self.entities.remove(&id);
            }
            GameDelta::TickAdvanced { tick } => {
                self.tick = tick;
            }
        }
    }
}"#}</pre>
                </div>
            </div>

            <div class="story-section">
                <h3>"Benefits"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <FlowConceptCard
                            title="Bandwidth Efficient"
                            description="Only changed data is sent after initial snapshot, reducing network traffic."
                        />
                        <FlowConceptCard
                            title="Type Safe"
                            description="Compile-time guarantees that deltas match state structure."
                        />
                        <FlowConceptCard
                            title="Framework Agnostic"
                            description="Works with any UI framework - Seed, Leptos, Yew, or vanilla."
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}

// ============================================================================
// Flow Operations Story
// ============================================================================

#[component]
pub fn FlowOperationsStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Operation Tracking"</h2>
                <p>"Track client actions through their lifecycle for optimistic UI updates."</p>
            </div>

            <div class="story-section">
                <h3>"OpId"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <ConfigOptionCard
                            name="OpId::new()"
                            type_name="OpId"
                            default="-"
                            description="Generate a unique operation ID for tracking an action"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Operation Lifecycle"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <LoaderStepCard step="1" title="Start" description="Client sends action with OpId" />
                        <LoaderStepCard step="2" title="Progress" description="Server sends ActionProgress updates" />
                        <LoaderStepCard step="3" title="Complete" description="Server sends ActionComplete or ActionError" />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"OperationTracker"</h3>
                <div class="story-canvas">
                    <pre class="code-block">{r#"use ui_flow::{OperationTracker, OpId};

let mut tracker = OperationTracker::new();

// Start tracking when sending an action
let op_id = tracker.start(MyActionData { item_id: 42 });
connection.send_action(op_id, Action::BuyItem { id: 42 })?;

// Show optimistic UI immediately
show_pending_purchase(42);

// On progress update
connection.on_progress(|progress| {
    tracker.update_progress(progress);
    update_progress_bar(progress.percent);
});

// On completion
connection.on_action_complete(|op_id| {
    if let Some(data) = tracker.complete(op_id) {
        confirm_purchase(data.item_id);
    }
});

// On error - rollback optimistic update
connection.on_action_error(|error| {
    if let Some(data) = tracker.fail(&error) {
        rollback_purchase(data.item_id);
        show_error(error.message);
    }
});"#}</pre>
                </div>
            </div>

            <div class="story-section">
                <h3>"Related Types"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <ConfigOptionCard
                            name="ActionProgress"
                            type_name="struct"
                            default="-"
                            description="Contains op_id, optional percent (0-100), and optional message"
                        />
                        <ConfigOptionCard
                            name="ActionError"
                            type_name="struct"
                            default="-"
                            description="Contains op_id, optional error code, and error message"
                        />
                        <ConfigOptionCard
                            name="PendingOperation<T>"
                            type_name="struct"
                            default="-"
                            description="Wraps your action data with op_id, progress, and timestamp"
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}
