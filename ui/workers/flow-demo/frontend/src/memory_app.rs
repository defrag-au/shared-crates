//! Memory Game Application
//!
//! The main entry point for the Black Flag memory game frontend.

use crate::components::{
    AdminPanel, CardView, GameBoard, GameMode, GameResults, Lobby, PlayerInfo, PlayerList,
};
use crate::get_or_create_user_id;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use ui_components::{AssetCache, ConnectionState, ConnectionStatus, PreloadAsset};
use ui_flow_protocol::{ClientMessage, OpId, PresenceInfo, ServerMessage};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

// =============================================================================
// Types mirroring the server types
// =============================================================================

/// Unique stable identifier for a card (mirrors server's CardId)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardId(pub String);

impl std::fmt::Display for CardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerGameMode {
    #[default]
    TurnTaking,
    Race,
}

impl From<ServerGameMode> for GameMode {
    fn from(m: ServerGameMode) -> Self {
        match m {
            ServerGameMode::TurnTaking => GameMode::TurnTaking,
            ServerGameMode::Race => GameMode::Race,
        }
    }
}

impl From<GameMode> for ServerGameMode {
    fn from(m: GameMode) -> Self {
        match m {
            GameMode::TurnTaking => ServerGameMode::TurnTaking,
            GameMode::Race => ServerGameMode::Race,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameConfig {
    pub grid_size: (u8, u8),
    pub mode: ServerGameMode,
    pub policy_id: String,
    pub flip_delay_ms: u64,
    pub shuffle_seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum GamePhase {
    Lobby {
        min_players: u8,
        max_players: u8,
    },
    Starting {
        countdown: u8,
    },
    Loading {
        ready_players: Vec<String>,
        total_players: usize,
    },
    Playing,
    Finished {
        winner: Option<String>,
        rankings: Vec<(String, String, u32)>,
    },
}

impl Default for GamePhase {
    fn default() -> Self {
        Self::Lobby {
            min_players: 1,
            max_players: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub card_id: CardId,
    pub asset_id: String,
    pub name: String,
    pub image_url: String,
    pub matched: bool,
    pub matched_by: Option<String>,
    pub pair_id: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub user_id: String,
    pub user_name: String,
    pub score: u32,
    pub flipped: Vec<CardId>,
    pub spectating: bool,
    pub joined_at: u64,
}

/// Turn state machine - mirrors server's TurnState
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TurnState {
    #[default]
    AwaitingFirst,
    FirstFlipped {
        card_id: CardId,
        acked: bool,
    },
    SecondFlipped {
        first: CardId,
        second: CardId,
        first_acked: bool,
        second_acked: bool,
    },
    BothReady {
        first: CardId,
        second: CardId,
        is_match: bool,
        ready_at: u64,
    },
}

impl TurnState {
    /// Get the flipped card IDs for display
    pub fn flipped_card_ids(&self) -> Vec<CardId> {
        match self {
            TurnState::AwaitingFirst => vec![],
            TurnState::FirstFlipped { card_id, .. } => vec![card_id.clone()],
            TurnState::SecondFlipped { first, second, .. } => vec![first.clone(), second.clone()],
            TurnState::BothReady { first, second, .. } => vec![first.clone(), second.clone()],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryGameState {
    pub config: GameConfig,
    pub cards: Vec<Card>,
    pub phase: GamePhase,
    pub players: HashMap<String, PlayerState>,
    pub turn_order: Vec<String>,
    pub current_turn: usize,
    pub turn_state: TurnState,
    pub host: Option<String>,
}

impl MemoryGameState {
    /// Get currently flipped card IDs (convenience method)
    pub fn flipped_card_ids(&self) -> Vec<CardId> {
        self.turn_state.flipped_card_ids()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardFace {
    pub asset_id: String,
    pub name: String,
}

/// Hidden card data - just the structure, no revealed info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiddenCard {
    pub card_id: CardId,
    pub matched: bool,
    pub matched_by: Option<String>,
}

use cardano_assets::AssetId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryDelta {
    PlayerJoined {
        user_id: String,
        user_name: String,
        spectating: bool,
    },
    PlayerLeft {
        user_id: String,
    },
    ConfigChanged {
        config: GameConfig,
    },
    HostChanged {
        user_id: String,
    },
    CountdownTick {
        seconds: u8,
    },
    CardsDealt {
        cards: Vec<HiddenCard>,
        #[serde(default)]
        asset_ids: Vec<AssetId>,
        #[serde(default)]
        total_players: usize,
    },
    PlayerReady {
        user_id: String,
        ready_count: usize,
        total_players: usize,
    },
    GameStarted {
        turn_order: Vec<String>,
    },
    CardFlipped {
        card_id: CardId,
        by: String,
        face: CardFace,
    },
    TurnChanged {
        user_id: String,
    },
    OwnCardFlipped {
        card_id: CardId,
        face: CardFace,
    },
    PairMatched {
        card_ids: [CardId; 2],
        by: String,
        by_name: String,
        new_score: u32,
        face: CardFace,
    },
    CardsReset {
        card_ids: [CardId; 2],
        for_player: Option<String>,
    },
    ScoreChanged {
        user_id: String,
        score: u32,
    },
    GameEnded {
        winner: Option<String>,
        rankings: Vec<(String, String, u32)>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryEvent {
    StartingIn {
        seconds: u8,
    },
    MatchFound {
        player_name: String,
        pair_name: String,
    },
    Streak {
        player_name: String,
        count: u8,
    },
    NearVictory {
        player_name: String,
        pairs_remaining: u8,
    },
    InvalidAction {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryAction {
    JoinGame {
        user_name: String,
    },
    LeaveGame,
    SetConfig {
        mode: Option<ServerGameMode>,
        grid_size: Option<(u8, u8)>,
    },
    StartGame,
    FlipCard {
        card_id: CardId,
    },
    AckCardLoaded {
        card_id: CardId,
    },
    Ready,
    RequestRematch,
    ResetGame,
}

type ServerMsg = ServerMessage<MemoryGameState, MemoryDelta, MemoryEvent>;
type ClientMsg = ClientMessage<MemoryAction>;

// =============================================================================
// Memory Game App Component
// =============================================================================

/// Memory Game Application component
#[component]
pub fn MemoryApp() -> impl IntoView {
    let user_id = get_or_create_user_id();
    let user_id_for_ws = user_id.clone();

    let (room_id, _set_room_id) = signal("default".to_string());
    let (status, set_status) = signal(ConnectionState::Disconnected);
    let (game_state, set_game_state) = signal(MemoryGameState::default());
    let (_presence, set_presence) = signal(Vec::<PresenceInfo>::new());
    let (current_user_id, _) = signal(user_id);

    // Local UI state for flipped cards (before server confirms)
    let (local_flipped, set_local_flipped) = signal(Vec::<CardId>::new());
    // Revealed card faces (from server) - keyed by CardId
    let (revealed_faces, set_revealed_faces) = signal(HashMap::<CardId, CardFace>::new());
    // Asset IDs for preloading images
    let (preload_assets, set_preload_assets) = signal(Vec::<AssetId>::new());

    // Use SendWrapper to make Rc<RefCell<...>> work with Leptos 0.8's Send+Sync requirements
    let ws: SendWrapper<Rc<RefCell<Option<WebSocket>>>> =
        SendWrapper::new(Rc::new(RefCell::new(None)));

    // Disconnect helper (currently unused but kept for future use)
    let ws_disconnect = ws.clone();
    let _disconnect = SendWrapper::new(Rc::new(move || {
        if let Some(socket) = ws_disconnect.borrow_mut().take() {
            let _ = socket.close();
        }
        set_status.set(ConnectionState::Disconnected);
    }));

    // Connect to WebSocket
    let ws_connect = ws.clone();
    let user_id_ws = user_id_for_ws.clone();
    let connect = SendWrapper::new(Rc::new(move || {
        let room = room_id.get();
        let ws_url = get_memory_ws_url(&room, &user_id_ws);

        set_status.set(ConnectionState::Connecting);

        match WebSocket::new(&ws_url) {
            Ok(socket) => {
                socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

                let set_status_clone = set_status;
                let on_open = Closure::wrap(Box::new(move |_: JsValue| {
                    set_status_clone.set(ConnectionState::Connected);
                    tracing::info!("Memory game WebSocket connected");
                }) as Box<dyn FnMut(JsValue)>);
                socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
                on_open.forget();

                let set_status_clone = set_status;
                let on_close = Closure::wrap(Box::new(move |_: JsValue| {
                    set_status_clone.set(ConnectionState::Disconnected);
                    tracing::info!("Memory game WebSocket disconnected");
                }) as Box<dyn FnMut(JsValue)>);
                socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
                on_close.forget();

                let on_error = Closure::wrap(Box::new(move |e: JsValue| {
                    tracing::error!("Memory game WebSocket error: {:?}", e);
                }) as Box<dyn FnMut(JsValue)>);
                socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
                on_error.forget();

                let set_game_state_clone = set_game_state;
                let set_presence_clone = set_presence;
                let set_revealed_clone = set_revealed_faces;
                let set_local_flipped_clone = set_local_flipped;
                let set_preload_assets_clone = set_preload_assets;
                let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                    if let Ok(buffer) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                        let array = js_sys::Uint8Array::new(&buffer);
                        let bytes = array.to_vec();

                        match ui_flow_protocol::decode::<ServerMsg>(&bytes) {
                            Ok(msg) => {
                                handle_server_message(
                                    msg,
                                    set_game_state_clone,
                                    set_presence_clone,
                                    set_revealed_clone,
                                    set_local_flipped_clone,
                                    set_preload_assets_clone,
                                );
                            }
                            Err(e) => {
                                tracing::error!("Failed to decode message: {}", e);
                            }
                        }
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                on_message.forget();

                *ws_connect.borrow_mut() = Some(socket);
            }
            Err(e) => {
                tracing::error!("Failed to create WebSocket: {:?}", e);
                set_status.set(ConnectionState::Disconnected);
            }
        }
    }));

    // Send action helper
    let ws_send = ws.clone();
    let send_action = SendWrapper::new(Rc::new(move |action: MemoryAction| {
        if let Some(socket) = ws_send.borrow().as_ref() {
            if socket.ready_state() == WebSocket::OPEN {
                let msg: ClientMsg = ClientMessage::action(OpId::new(), action);
                if let Ok(bytes) = ui_flow_protocol::encode(&msg) {
                    let _ = socket.send_with_u8_array(&bytes);
                }
            }
        }
    }));
    let send_action_for_admin = send_action.clone();

    // Auto-connect on mount
    let connect_effect = connect.clone();
    Effect::new(move |_| {
        connect_effect();
    });

    // Auto-join game when connected
    let send_join = send_action.clone();
    let user_id_join = current_user_id.get();
    Effect::new(move |prev_status: Option<ConnectionState>| {
        let current = status.get();
        if prev_status != Some(ConnectionState::Connected) && current == ConnectionState::Connected
        {
            // Send join action
            let display_name = format!("User {}", &user_id_join[..8.min(user_id_join.len())]);
            send_join(MemoryAction::JoinGame {
                user_name: display_name,
            });
        }
        current
    });

    // Derived signals
    let game_mode = Signal::derive(move || game_state.get().config.mode.into());
    let grid_size = Signal::derive(move || game_state.get().config.grid_size);

    let players_for_lobby = Signal::derive(move || {
        let state = game_state.get();
        state
            .players
            .values()
            .map(|p| (p.user_id.clone(), p.user_name.clone()))
            .collect::<Vec<_>>()
    });

    let players_for_list = Signal::derive(move || {
        let state = game_state.get();
        state
            .players
            .values()
            .map(|p| PlayerInfo {
                user_id: p.user_id.clone(),
                user_name: p.user_name.clone(),
                score: p.score,
                spectating: p.spectating,
            })
            .collect::<Vec<_>>()
    });

    let current_turn_user = Signal::derive(move || {
        let state = game_state.get();
        state.turn_order.get(state.current_turn).cloned()
    });

    let is_my_turn = Signal::derive(move || {
        let state = game_state.get();
        let my_id = current_user_id.get();

        // In race mode, it's always "your turn" if you're playing
        if state.config.mode == ServerGameMode::Race {
            return state
                .players
                .get(&my_id)
                .map(|p| !p.spectating)
                .unwrap_or(false);
        }

        // In turn-taking mode, check if it's actually your turn
        state.turn_order.get(state.current_turn) == Some(&my_id)
    });

    let cards_view = {
        let _current_user_id = current_user_id;
        Signal::derive(move || {
            let state = game_state.get();
            let revealed = revealed_faces.get();
            let _my_id = _current_user_id.get();
            let local = local_flipped.get();

            state
                .cards
                .iter()
                .map(|card| {
                    let card_id = &card.card_id;
                    // Card is visible if:
                    // 1. It's matched
                    // 2. It's in revealed_faces (server confirmed flip)
                    // 3. In turn-taking mode and it's in turn_state flipped
                    let is_matched = card.matched;
                    let is_revealed = revealed.contains_key(card_id);
                    let is_shared_flipped = state.config.mode == ServerGameMode::TurnTaking
                        && state.flipped_card_ids().contains(card_id);
                    let is_local_flipped = local.contains(card_id);

                    let visible =
                        is_matched || is_revealed || is_shared_flipped || is_local_flipped;

                    // Get face data if visible - use asset_id for IIIF URL generation
                    let (asset_id, name) = if visible {
                        if let Some(face) = revealed.get(card_id) {
                            (Some(face.asset_id.clone()), Some(face.name.clone()))
                        } else if is_matched {
                            (Some(card.asset_id.clone()), Some(card.name.clone()))
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    };

                    CardView {
                        card_id: card_id.clone(),
                        visible,
                        asset_id,
                        name,
                        matched: is_matched,
                        matched_by: card.matched_by.clone(),
                    }
                })
                .collect::<Vec<_>>()
        })
    };

    let flipped_card_ids = Signal::derive(move || {
        let state = game_state.get();
        let local = local_flipped.get();
        let mut ids = state.flipped_card_ids();
        for id in local {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    });

    // Clone for view handlers
    let connect_reconnect = connect.clone();

    // Convert preload_assets to PreloadAsset structs for the Leptos component
    let preload_assets_signal = Signal::derive(move || {
        preload_assets
            .get()
            .into_iter()
            .map(|asset| PreloadAsset {
                policy_id: asset.policy_id,
                asset_name_hex: asset.asset_name_hex,
            })
            .collect::<Vec<_>>()
    });

    // Clone send_action for the cache-ready handler
    let send_ready = send_action.clone();

    view! {
        <div class="memory-app">
            <div class="header">
                <h1>"Black Flag Memory"</h1>
                <ConnectionStatus
                    status=status
                    on_reconnect=move || {
                        connect_reconnect();
                    }
                />
            </div>

            // Asset cache for preloading card images - sends Ready when loaded
            <AssetCache
                assets=preload_assets_signal
                on_progress=move |_loaded, _total| {
                    // Optional progress tracking
                }
                on_ready=move |loaded, failed| {
                    tracing::info!("Asset cache ready: {} loaded, {} failed - signaling ready", loaded, failed);
                    // Signal to server that we're ready to play
                    send_ready(MemoryAction::Ready);
                }
            />

            // Use Show components to keep GameBoard mounted during Playing phase
            // This preserves DOM elements so CSS transitions can animate

            // Derive phase signals for Show conditions
            {
                let is_lobby = Signal::derive(move || matches!(game_state.get().phase, GamePhase::Lobby { .. }));
                let is_starting = Signal::derive(move || matches!(game_state.get().phase, GamePhase::Starting { .. }));
                let is_loading = Signal::derive(move || matches!(game_state.get().phase, GamePhase::Loading { .. }));
                let is_playing = Signal::derive(move || matches!(game_state.get().phase, GamePhase::Playing));
                let is_finished = Signal::derive(move || matches!(game_state.get().phase, GamePhase::Finished { .. }));

                let send_mode = send_action.clone();
                let send_grid = send_action.clone();
                let send_start = send_action.clone();
                let send_flip = send_action.clone();
                let send_ack = send_action.clone();
                let send_rematch = send_action.clone();

                // Derived signals for phase-specific data
                let countdown = Signal::derive(move || {
                    match game_state.get().phase {
                        GamePhase::Starting { countdown } => countdown,
                        _ => 0,
                    }
                });

                let loading_info = Signal::derive(move || {
                    let state = game_state.get();
                    match &state.phase {
                        GamePhase::Loading { ready_players, total_players } => {
                            let ready_with_names: Vec<String> = ready_players.iter()
                                .map(|id| state.players.get(id)
                                    .map(|p| p.user_name.clone())
                                    .unwrap_or_else(|| id.clone()))
                                .collect();
                            (ready_with_names, *total_players)
                        }
                        _ => (vec![], 0),
                    }
                });

                let finished_info = Signal::derive(move || {
                    match game_state.get().phase {
                        GamePhase::Finished { winner, rankings } => (winner, rankings),
                        _ => (None, vec![]),
                    }
                });

                view! {
                    // Lobby phase
                    <Show when=move || is_lobby.get() fallback=|| ()>
                        <Lobby
                            players=players_for_lobby
                            current_user_id=current_user_id
                            game_mode=game_mode
                            grid_size=grid_size
                            on_mode_change={
                                let send = send_mode.clone();
                                move |mode: GameMode| {
                                    send(MemoryAction::SetConfig {
                                        mode: Some(mode.into()),
                                        grid_size: None,
                                    });
                                }
                            }
                            on_grid_change={
                                let send = send_grid.clone();
                                move |size: (u8, u8)| {
                                    send(MemoryAction::SetConfig {
                                        mode: None,
                                        grid_size: Some(size),
                                    });
                                }
                            }
                            on_start={
                                let send = send_start.clone();
                                move || {
                                    send(MemoryAction::StartGame);
                                }
                            }
                        />
                    </Show>

                    // Starting countdown phase
                    <Show when=move || is_starting.get() fallback=|| ()>
                        <div class="countdown">
                            <h2>"Game starting in..."</h2>
                            <div class="countdown-number">{move || countdown.get()}</div>
                        </div>
                    </Show>

                    // Loading phase
                    <Show when=move || is_loading.get() fallback=|| ()>
                        {move || {
                            let (ready_names, total) = loading_info.get();
                            view! {
                                <div class="loading-phase">
                                    <h2>"Loading assets..."</h2>
                                    <div class="loading-progress">
                                        <div class="loading-spinner"></div>
                                        <p>{format!("Waiting for players: {}/{}", ready_names.len(), total)}</p>
                                    </div>
                                    <div class="ready-players">
                                        {ready_names.into_iter().map(|name| {
                                            view! {
                                                <span class="ready-player">{name}" ✓"</span>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                </div>
                            }
                        }}
                    </Show>

                    // Playing phase - GameBoard stays mounted!
                    <Show when=move || is_playing.get() fallback=|| ()>
                        <div class="game-playing">
                            <div class="game-main">
                                <GameBoard
                                    grid_size=grid_size
                                    cards=cards_view
                                    flipped_card_ids=flipped_card_ids
                                    is_my_turn=is_my_turn
                                    on_flip={
                                        let send = send_flip.clone();
                                        move |card_id: CardId| {
                                            // Optimistically add to local flipped
                                            set_local_flipped.update(|v| {
                                                if !v.contains(&card_id) && v.len() < 2 {
                                                    v.push(card_id.clone());
                                                }
                                            });
                                            send(MemoryAction::FlipCard { card_id: card_id.clone() });
                                        }
                                    }
                                    on_card_loaded={
                                        let send = send_ack.clone();
                                        move |card_id: CardId| {
                                            // ACK that card image has loaded - starts flip-back timer
                                            send(MemoryAction::AckCardLoaded { card_id });
                                        }
                                    }
                                    disabled=Signal::derive(move || status.get() != ConnectionState::Connected)
                                />
                            </div>
                            <div class="game-sidebar">
                                <PlayerList
                                    players=players_for_list
                                    current_turn=current_turn_user
                                    current_user_id=current_user_id
                                />
                            </div>
                        </div>
                    </Show>

                    // Finished phase
                    <Show when=move || is_finished.get() fallback=|| ()>
                        {
                            let send = send_rematch.clone();
                            move || {
                                let (winner, rankings) = finished_info.get();
                                let (winner_sig, _) = signal(winner);
                                let (rankings_sig, _) = signal(rankings);
                                let send = send.clone();
                                view! {
                                    <GameResults
                                        winner=winner_sig
                                        rankings=rankings_sig
                                        current_user_id=current_user_id
                                        on_rematch=move || {
                                            send(MemoryAction::RequestRematch);
                                        }
                                    />
                                }
                            }
                        }
                    </Show>
                }
            }

            // Admin panel
            {
                let send_reset = send_action_for_admin.clone();
                view! {
                    <AdminPanel on_reset=move || {
                        send_reset(MemoryAction::ResetGame);
                    } />
                }
            }
        </div>
    }
}

fn handle_server_message(
    msg: ServerMsg,
    set_game_state: WriteSignal<MemoryGameState>,
    set_presence: WriteSignal<Vec<PresenceInfo>>,
    set_revealed_faces: WriteSignal<HashMap<CardId, CardFace>>,
    set_local_flipped: WriteSignal<Vec<CardId>>,
    set_preload_assets: WriteSignal<Vec<AssetId>>,
) {
    match msg {
        ServerMessage::Connected { .. } => {
            tracing::info!("Received Connected message");
        }

        ServerMessage::Snapshot { state, seq, .. } => {
            tracing::info!("Received Snapshot at seq {}", seq);
            set_game_state.set(state);
            // Clear local state on snapshot
            set_revealed_faces.set(HashMap::new());
            set_local_flipped.set(Vec::new());
        }

        ServerMessage::Delta { delta, seq, .. } => {
            tracing::debug!("Received Delta at seq {}", seq);
            apply_delta(
                delta,
                set_game_state,
                set_revealed_faces,
                set_local_flipped,
                set_preload_assets,
            );
        }

        ServerMessage::Deltas { deltas, seq, .. } => {
            tracing::debug!("Received {} Deltas, final seq {}", deltas.len(), seq);
            for delta in deltas {
                apply_delta(
                    delta,
                    set_game_state,
                    set_revealed_faces,
                    set_local_flipped,
                    set_preload_assets,
                );
            }
        }

        ServerMessage::Presence { users } => {
            tracing::debug!("Received Presence: {} users", users.len());
            set_presence.set(users);
        }

        ServerMessage::Notify { domain, event, .. } => {
            tracing::debug!("Received Notify on domain: {}", domain);
            match event {
                MemoryEvent::MatchFound {
                    player_name,
                    pair_name,
                } => {
                    tracing::info!("{} found a match: {}", player_name, pair_name);
                }
                MemoryEvent::InvalidAction { reason } => {
                    tracing::warn!("Invalid action: {}", reason);
                }
                _ => {}
            }
        }

        ServerMessage::ActionOk { op_id, .. } => {
            tracing::debug!("Action {} completed successfully", op_id);
        }

        ServerMessage::ActionErr { op_id, message, .. } => {
            tracing::error!("Action {} failed: {}", op_id, message);
            // Clear local flipped on error
            set_local_flipped.set(Vec::new());
        }

        _ => {}
    }
}

fn apply_delta(
    delta: MemoryDelta,
    set_game_state: WriteSignal<MemoryGameState>,
    set_revealed_faces: WriteSignal<HashMap<CardId, CardFace>>,
    set_local_flipped: WriteSignal<Vec<CardId>>,
    set_preload_assets: WriteSignal<Vec<AssetId>>,
) {
    match delta {
        MemoryDelta::PlayerJoined {
            user_id,
            user_name,
            spectating,
        } => {
            set_game_state.update(|s| {
                s.players.insert(
                    user_id.clone(),
                    PlayerState {
                        user_id,
                        user_name,
                        score: 0,
                        flipped: vec![],
                        spectating,
                        joined_at: 0,
                    },
                );
            });
        }

        MemoryDelta::PlayerLeft { user_id } => {
            set_game_state.update(|s| {
                s.players.remove(&user_id);
            });
        }

        MemoryDelta::ConfigChanged { config } => {
            set_game_state.update(|s| {
                s.config = config;
            });
        }

        MemoryDelta::HostChanged { user_id } => {
            set_game_state.update(|s| {
                s.host = Some(user_id);
            });
        }

        MemoryDelta::CountdownTick { seconds } => {
            set_game_state.update(|s| {
                s.phase = GamePhase::Starting { countdown: seconds };
            });
        }

        MemoryDelta::GameStarted { turn_order } => {
            set_game_state.update(|s| {
                s.turn_order = turn_order;
                s.current_turn = 0;
                s.phase = GamePhase::Playing;
            });
            set_revealed_faces.set(HashMap::new());
            set_local_flipped.set(Vec::new());
        }

        MemoryDelta::CardsDealt {
            cards,
            asset_ids,
            total_players,
        } => {
            // Convert hidden cards to full Card structs (face data hidden)
            // and enter Loading phase
            set_game_state.update(|s| {
                s.cards = cards
                    .into_iter()
                    .map(|hidden| Card {
                        card_id: hidden.card_id,
                        asset_id: String::new(), // Hidden until flipped
                        name: String::new(),     // Hidden until flipped
                        image_url: String::new(),
                        matched: hidden.matched,
                        matched_by: hidden.matched_by,
                        pair_id: 0, // Not needed on client
                    })
                    .collect();
                s.phase = GamePhase::Loading {
                    ready_players: Vec::new(),
                    total_players,
                };
            });
            // Trigger preloading of asset images
            if !asset_ids.is_empty() {
                tracing::info!("Preloading {} assets", asset_ids.len());
                set_preload_assets.set(asset_ids);
            }
        }

        MemoryDelta::PlayerReady {
            user_id,
            ready_count,
            total_players,
        } => {
            set_game_state.update(|s| {
                if let GamePhase::Loading {
                    ready_players,
                    total_players: tp,
                } = &mut s.phase
                {
                    if !ready_players.contains(&user_id) {
                        ready_players.push(user_id);
                    }
                    *tp = total_players;
                }
            });
            tracing::info!("Player ready: {}/{}", ready_count, total_players);
        }

        MemoryDelta::CardFlipped {
            card_id,
            by: _,
            face,
        } => {
            // Add to revealed faces
            set_revealed_faces.update(|m| {
                m.insert(card_id.clone(), face);
            });
            // Update turn_state to reflect the flip
            set_game_state.update(|s| match &s.turn_state {
                TurnState::AwaitingFirst => {
                    s.turn_state = TurnState::FirstFlipped {
                        card_id: card_id.clone(),
                        acked: false,
                    };
                }
                TurnState::FirstFlipped {
                    card_id: first,
                    acked,
                } => {
                    s.turn_state = TurnState::SecondFlipped {
                        first: first.clone(),
                        second: card_id.clone(),
                        first_acked: *acked,
                        second_acked: false,
                    };
                }
                _ => {}
            });
            // Clear from local flipped (server confirmed)
            set_local_flipped.update(|v| {
                v.retain(|i| *i != card_id);
            });
        }

        MemoryDelta::TurnChanged { user_id } => {
            set_game_state.update(|s| {
                if let Some(idx) = s.turn_order.iter().position(|id| id == &user_id) {
                    s.current_turn = idx;
                }
            });
        }

        MemoryDelta::OwnCardFlipped { card_id, face } => {
            // Race mode: only we see this
            set_revealed_faces.update(|m| {
                m.insert(card_id.clone(), face);
            });
            set_local_flipped.update(|v| {
                v.retain(|i| *i != card_id);
            });
        }

        MemoryDelta::PairMatched {
            card_ids,
            by,
            by_name: _,
            new_score,
            face: _,
        } => {
            set_game_state.update(|s| {
                // Mark cards as matched
                for card_id in &card_ids {
                    if let Some(card) = s.cards.iter_mut().find(|c| &c.card_id == card_id) {
                        card.matched = true;
                        card.matched_by = Some(by.clone());
                    }
                }
                // Update score
                if let Some(player) = s.players.get_mut(&by) {
                    player.score = new_score;
                }
                // Reset turn state (player gets another turn)
                s.turn_state = TurnState::AwaitingFirst;
            });
            // Clear revealed faces for these cards (they stay visible as matched)
            set_revealed_faces.update(|m| {
                for card_id in &card_ids {
                    m.remove(card_id);
                }
            });
            set_local_flipped.set(Vec::new());
        }

        MemoryDelta::CardsReset {
            card_ids,
            for_player: _,
        } => {
            // Clear revealed faces
            set_revealed_faces.update(|m| {
                for card_id in &card_ids {
                    m.remove(card_id);
                }
            });
            // Reset turn state (next player's turn)
            set_game_state.update(|s| {
                s.turn_state = TurnState::AwaitingFirst;
            });
            set_local_flipped.set(Vec::new());
        }

        MemoryDelta::ScoreChanged { user_id, score } => {
            set_game_state.update(|s| {
                if let Some(player) = s.players.get_mut(&user_id) {
                    player.score = score;
                }
            });
        }

        MemoryDelta::GameEnded { winner, rankings } => {
            set_game_state.update(|s| {
                s.phase = GamePhase::Finished { winner, rankings };
            });
        }
    }
}

fn get_memory_ws_url(room_id: &str, user_id: &str) -> String {
    let window = web_sys::window().expect("no window");
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
    let host = location
        .host()
        .unwrap_or_else(|_| "localhost:8787".to_string());

    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    let display_name = &user_id[..8.min(user_id.len())];

    format!(
        "{}//{}/memory/{}?user_id={}&user_name={}",
        ws_protocol, host, room_id, user_id, display_name
    )
}
