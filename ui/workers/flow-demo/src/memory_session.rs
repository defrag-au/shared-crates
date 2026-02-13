//! Durable Object for managing Memory Game sessions.
//!
//! This implements the server-side game logic for the Black Flag memory game,
//! supporting both turn-taking and race modes with 2-8 players.

use crate::assets::{fetch_game_cards, AssetId};
use crate::types::*;

/// Helper trait for looking up cards by CardId
trait CardLookup {
    fn find_card(&self, card_id: &CardId) -> Option<&Card>;
    fn find_card_mut(&mut self, card_id: &CardId) -> Option<&mut Card>;
}

impl CardLookup for Vec<Card> {
    fn find_card(&self, card_id: &CardId) -> Option<&Card> {
        self.iter().find(|c| &c.card_id == card_id)
    }

    fn find_card_mut(&mut self, card_id: &CardId) -> Option<&mut Card> {
        self.iter_mut().find(|c| &c.card_id == card_id)
    }
}
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use ui_flow_protocol::{encode, OpId, PresenceInfo, PresenceStatus, ServerMessage};
use worker::*;

/// Storage keys for persisted state
const STORAGE_KEY_GAME: &str = "game_state";
const STORAGE_KEY_SEQ: &str = "game_seq";

/// Connection information stored as WebSocket attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConnectionInfo {
    user_id: String,
    user_name: String,
    connected_at: u64,
}

/// The Durable Object that manages a single memory game room.
#[durable_object]
pub struct MemoryGameSessionDO {
    state: State,
    #[allow(dead_code)]
    env: Env,
    /// Current game state (cached from storage)
    game_state: RefCell<Option<MemoryGameState>>,
    /// Current sequence number (cached from storage)
    seq: RefCell<Option<u64>>,
}

impl DurableObject for MemoryGameSessionDO {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            game_state: RefCell::new(None),
            seq: RefCell::new(None),
        }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        if req.headers().get("Upgrade")?.as_deref() == Some("websocket") {
            return self.handle_websocket_upgrade(req).await;
        }
        Response::error("Expected WebSocket upgrade", 400)
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let bytes = match message {
            WebSocketIncomingMessage::Binary(bytes) => bytes,
            WebSocketIncomingMessage::String(text) => text.into_bytes(),
        };

        let client_msg: MemoryClientMsg = match ui_flow_protocol::decode(&bytes) {
            Ok(msg) => msg,
            Err(e) => {
                let error_msg: MemoryServerMsg =
                    ServerMessage::error(format!("Failed to decode message: {e}"), false);
                if let Ok(bytes) = encode(&error_msg) {
                    let _ = ws.send_with_bytes(&bytes);
                }
                return Ok(());
            }
        };

        let conn_info: Option<ConnectionInfo> = ws.deserialize_attachment().ok().flatten();
        let conn = conn_info.unwrap_or_else(|| ConnectionInfo {
            user_id: "anonymous".to_string(),
            user_name: "Anonymous".to_string(),
            connected_at: 0,
        });

        self.handle_client_message(&ws, &conn, client_msg).await?;
        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        if let Ok(Some(conn)) = ws.deserialize_attachment::<ConnectionInfo>() {
            self.handle_player_disconnect(&conn.user_id).await?;
        }
        Ok(())
    }

    async fn alarm(&self) -> Result<Response> {
        self.handle_flip_timer_expired().await?;
        Response::ok("OK")
    }
}

impl MemoryGameSessionDO {
    // =========================================================================
    // Storage helpers
    // =========================================================================

    async fn get_game_state(&self) -> MemoryGameState {
        if let Some(state) = self.game_state.borrow().clone() {
            return state;
        }

        let state: MemoryGameState = self
            .state
            .storage()
            .get(STORAGE_KEY_GAME)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        *self.game_state.borrow_mut() = Some(state.clone());
        state
    }

    async fn save_game_state(&self, state: &MemoryGameState) {
        *self.game_state.borrow_mut() = Some(state.clone());
        let _ = self.state.storage().put(STORAGE_KEY_GAME, state).await;
    }

    async fn get_seq(&self) -> u64 {
        if let Some(seq) = *self.seq.borrow() {
            return seq;
        }

        let seq: u64 = self
            .state
            .storage()
            .get(STORAGE_KEY_SEQ)
            .await
            .ok()
            .flatten()
            .unwrap_or(0);

        *self.seq.borrow_mut() = Some(seq);
        seq
    }

    async fn next_seq(&self) -> u64 {
        let seq = self.get_seq().await + 1;
        *self.seq.borrow_mut() = Some(seq);
        let _ = self.state.storage().put(STORAGE_KEY_SEQ, seq).await;
        seq
    }

    // =========================================================================
    // WebSocket handling
    // =========================================================================

    async fn handle_websocket_upgrade(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let user_id = url
            .query_pairs()
            .find(|(k, _)| k == "user_id")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| format!("user_{}", js_random_u32()));
        let user_name = url
            .query_pairs()
            .find(|(k, _)| k == "user_name")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| format!("User {}", &user_id[..8.min(user_id.len())]));

        tracing::info!(
            "WebSocket upgrade for user_id={}, user_name={}",
            user_id,
            user_name
        );

        let WebSocketPair { client, server } = WebSocketPair::new()?;

        let conn_info = ConnectionInfo {
            user_id: user_id.clone(),
            user_name: user_name.clone(),
            connected_at: now(),
        };
        server.serialize_attachment(&conn_info)?;

        self.state.accept_web_socket(&server);

        // Send Connected message
        let connected_msg: MemoryServerMsg =
            ServerMessage::connected(1, self.state.id().to_string());
        if let Ok(bytes) = encode(&connected_msg) {
            tracing::debug!("Sending Connected message ({} bytes)", bytes.len());
            let _ = server.send_with_bytes(&bytes);
        }

        // Send current state snapshot
        let game_state = self.get_game_state().await;
        let seq = self.get_seq().await;
        tracing::info!(
            "Sending initial snapshot: phase={:?}, cards={}, seq={}",
            match &game_state.phase {
                GamePhase::Lobby { .. } => "Lobby",
                GamePhase::Starting { .. } => "Starting",
                GamePhase::Loading { .. } => "Loading",
                GamePhase::Playing => "Playing",
                GamePhase::Finished { .. } => "Finished",
            },
            game_state.cards.len(),
            seq
        );
        let snapshot_msg: MemoryServerMsg = ServerMessage::snapshot(game_state, seq, now());
        if let Ok(bytes) = encode(&snapshot_msg) {
            tracing::debug!("Sending Snapshot message ({} bytes)", bytes.len());
            let _ = server.send_with_bytes(&bytes);
        }

        // Broadcast updated presence
        self.broadcast_presence().await;

        Response::from_websocket(client)
    }

    async fn handle_client_message(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        msg: MemoryClientMsg,
    ) -> Result<()> {
        use ui_flow_protocol::ClientMessage;

        match msg {
            ClientMessage::Ping { ts } => {
                let pong: MemoryServerMsg = ServerMessage::pong(ts, now());
                if let Ok(bytes) = encode(&pong) {
                    let _ = ws.send_with_bytes(&bytes);
                }
            }

            ClientMessage::Resync { last_seq: _ } => {
                let game_state = self.get_game_state().await;
                let seq = self.get_seq().await;
                let snapshot_msg: MemoryServerMsg = ServerMessage::snapshot(game_state, seq, now());
                if let Ok(bytes) = encode(&snapshot_msg) {
                    let _ = ws.send_with_bytes(&bytes);
                }
            }

            ClientMessage::Action { op_id, action } => {
                self.handle_action(ws, conn, op_id, action).await?;
            }

            _ => {}
        }

        Ok(())
    }

    // =========================================================================
    // Game Actions
    // =========================================================================

    async fn handle_action(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
        action: MemoryAction,
    ) -> Result<()> {
        match action {
            MemoryAction::JoinGame { user_name } => {
                self.handle_join_game(ws, conn, op_id, user_name).await?;
            }
            MemoryAction::LeaveGame => {
                self.handle_leave_game(ws, conn, op_id).await?;
            }
            MemoryAction::SetConfig { mode, grid_size } => {
                self.handle_set_config(ws, conn, op_id, mode, grid_size)
                    .await?;
            }
            MemoryAction::StartGame => {
                self.handle_start_game(ws, conn, op_id).await?;
            }
            MemoryAction::FlipCard { card_id } => {
                self.handle_flip_card(ws, conn, op_id, card_id).await?;
            }
            MemoryAction::AckCardLoaded { card_id } => {
                self.handle_ack_card_loaded(ws, conn, op_id, card_id)
                    .await?;
            }
            MemoryAction::Ready => {
                self.handle_ready(ws, conn, op_id).await?;
            }
            MemoryAction::RequestRematch => {
                self.handle_request_rematch(ws, conn, op_id).await?;
            }
            MemoryAction::ResetGame => {
                self.handle_reset_game(ws, op_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_join_game(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
        user_name: String,
    ) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Determine if joining as spectator
        let spectating = !matches!(state.phase, GamePhase::Lobby { .. });

        // Check if player already exists
        if state.players.contains_key(&conn.user_id) {
            // Update name if changed
            if let Some(player) = state.players.get_mut(&conn.user_id) {
                player.user_name = user_name.clone();
            }
        } else {
            // Add new player
            let player = PlayerState {
                user_id: conn.user_id.clone(),
                user_name: user_name.clone(),
                score: 0,
                flipped: vec![],
                spectating,
                joined_at: now(),
            };
            state.players.insert(conn.user_id.clone(), player);

            // First player becomes host
            if state.host.is_none() {
                state.host = Some(conn.user_id.clone());
            }
        }

        self.save_game_state(&state).await;

        // Broadcast join
        let delta = MemoryDelta::PlayerJoined {
            user_id: conn.user_id.clone(),
            user_name,
            spectating,
        };
        self.broadcast_delta(delta).await;

        // Send success
        self.send_action_ok(ws, op_id).await;
        self.broadcast_presence().await;

        Ok(())
    }

    async fn handle_leave_game(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
    ) -> Result<()> {
        self.handle_player_disconnect(&conn.user_id).await?;
        self.send_action_ok(ws, op_id).await;
        Ok(())
    }

    async fn handle_player_disconnect(&self, user_id: &str) -> Result<()> {
        let mut state = self.get_game_state().await;

        if state.players.remove(user_id).is_some() {
            // If host left, assign new host
            if state.host.as_deref() == Some(user_id) {
                state.host = state.players.keys().next().cloned();
                if let Some(new_host) = &state.host {
                    let delta = MemoryDelta::HostChanged {
                        user_id: new_host.clone(),
                    };
                    self.broadcast_delta(delta).await;
                }
            }

            // Remove from turn order
            state.turn_order.retain(|id| id != user_id);

            // Adjust current turn if needed
            if !state.turn_order.is_empty() {
                state.current_turn %= state.turn_order.len();
            }

            self.save_game_state(&state).await;

            let delta = MemoryDelta::PlayerLeft {
                user_id: user_id.to_string(),
            };
            self.broadcast_delta(delta).await;
            self.broadcast_presence().await;
        }

        Ok(())
    }

    async fn handle_set_config(
        &self,
        ws: &WebSocket,
        _conn: &ConnectionInfo,
        op_id: OpId,
        mode: Option<GameMode>,
        grid_size: Option<(u8, u8)>,
    ) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Only in lobby
        if !matches!(state.phase, GamePhase::Lobby { .. }) {
            self.send_action_error(ws, op_id, "Can only change settings in lobby")
                .await;
            return Ok(());
        }

        if let Some(m) = mode {
            state.config.mode = m;
        }
        if let Some(gs) = grid_size {
            state.config.grid_size = gs;
        }

        self.save_game_state(&state).await;

        let delta = MemoryDelta::ConfigChanged {
            config: state.config.clone(),
        };
        self.broadcast_delta(delta).await;
        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    async fn handle_start_game(
        &self,
        ws: &WebSocket,
        _conn: &ConnectionInfo,
        op_id: OpId,
    ) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Must be in lobby
        if !matches!(state.phase, GamePhase::Lobby { .. }) {
            self.send_action_error(ws, op_id, "Game already started")
                .await;
            return Ok(());
        }

        // Need at least 1 non-spectating player
        let active_players: Vec<_> = state.players.values().filter(|p| !p.spectating).collect();
        let total_players = active_players.len();

        if total_players == 0 {
            self.send_action_error(ws, op_id, "Need at least one player")
                .await;
            return Ok(());
        }

        // Generate shuffle seed
        let seed = now() ^ (js_random_u32() as u64);
        state.config.shuffle_seed = seed;

        // Calculate pair count from grid size
        let (cols, rows) = state.config.grid_size;
        let total_cards = (cols as usize) * (rows as usize);
        let pair_count = (total_cards / 2) as u8;

        // Fetch cards from PFP City
        let cards = match fetch_game_cards(&state.config.policy_id, pair_count, seed).await {
            Ok(cards) => cards,
            Err(e) => {
                self.send_action_error(ws, op_id, &format!("Failed to fetch assets: {e}"))
                    .await;
                return Ok(());
            }
        };

        state.cards = cards;

        // Set up turn order (non-spectating players)
        state.turn_order = state
            .players
            .values()
            .filter(|p| !p.spectating)
            .map(|p| p.user_id.clone())
            .collect();

        // Shuffle turn order
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
        state.turn_order.shuffle(&mut rng);

        state.current_turn = 0;
        state.turn_state = TurnState::AwaitingFirst;

        // Reset player scores
        for player in state.players.values_mut() {
            player.score = 0;
            player.flipped.clear();
        }

        // Enter Loading phase - wait for players to signal ready
        state.phase = GamePhase::Loading {
            ready_players: Vec::new(),
            total_players,
        };

        self.save_game_state(&state).await;

        // Broadcast cards dealt (hidden card structure - no face data revealed)
        let hidden_cards: Vec<HiddenCard> = state.cards.iter().map(HiddenCard::from).collect();

        // Extract unique asset IDs for preloading (each pair shares an asset_id)
        let mut seen_pairs = std::collections::HashSet::new();
        let asset_ids: Vec<AssetId> = state
            .cards
            .iter()
            .filter(|card| seen_pairs.insert(card.pair_id))
            .filter_map(|card| AssetId::parse_smart(&card.asset_id).ok())
            .collect();

        tracing::info!(
            "Dealing cards - broadcasting {} hidden cards ({} unique assets for preload), waiting for {} players to be ready",
            hidden_cards.len(),
            asset_ids.len(),
            total_players
        );
        let cards_delta = MemoryDelta::CardsDealt {
            cards: hidden_cards,
            asset_ids,
            total_players,
        };
        self.broadcast_delta(cards_delta).await;

        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    async fn handle_ready(&self, ws: &WebSocket, conn: &ConnectionInfo, op_id: OpId) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Must be in Loading phase
        let (ready_players, total_players) = match &mut state.phase {
            GamePhase::Loading {
                ready_players,
                total_players,
            } => (ready_players, *total_players),
            _ => {
                self.send_action_error(ws, op_id, "Not in loading phase")
                    .await;
                return Ok(());
            }
        };

        // Check if already ready
        if ready_players.contains(&conn.user_id) {
            self.send_action_ok(ws, op_id).await;
            return Ok(());
        }

        // Mark player as ready
        ready_players.push(conn.user_id.clone());
        let ready_count = ready_players.len();

        tracing::info!(
            "Player {} is ready ({}/{})",
            conn.user_id,
            ready_count,
            total_players
        );

        // Broadcast player ready
        let ready_delta = MemoryDelta::PlayerReady {
            user_id: conn.user_id.clone(),
            ready_count,
            total_players,
        };
        self.broadcast_delta(ready_delta).await;

        // Check if all players are ready
        if ready_count >= total_players {
            tracing::info!("All players ready - starting game!");

            // Transition to Playing phase
            state.phase = GamePhase::Playing;
            self.save_game_state(&state).await;

            // Broadcast game started
            let started_delta = MemoryDelta::GameStarted {
                turn_order: state.turn_order.clone(),
            };
            self.broadcast_delta(started_delta).await;
        } else {
            self.save_game_state(&state).await;
        }

        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    async fn handle_flip_card(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
        card_id: CardId,
    ) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Must be playing
        if !matches!(state.phase, GamePhase::Playing) {
            self.send_action_error(ws, op_id, "Game is not in progress")
                .await;
            return Ok(());
        }

        // Validate card exists
        let card = match state.cards.find_card(&card_id) {
            Some(c) => c,
            None => {
                self.send_action_error(ws, op_id, "Invalid card ID").await;
                return Ok(());
            }
        };

        // Card must not be already matched
        if card.matched {
            self.send_action_error(ws, op_id, "Card already matched")
                .await;
            return Ok(());
        }

        match state.config.mode {
            GameMode::TurnTaking => {
                self.handle_flip_turn_taking(ws, conn, op_id, card_id, &mut state)
                    .await?;
            }
            GameMode::Race => {
                self.handle_flip_race(ws, conn, op_id, card_id, &mut state)
                    .await?;
            }
        }

        Ok(())
    }

    async fn handle_flip_turn_taking(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
        card_id: CardId,
        state: &mut MemoryGameState,
    ) -> Result<()> {
        // Must be player's turn
        let current_player = state.turn_order.get(state.current_turn);
        if current_player != Some(&conn.user_id) {
            self.send_action_error(ws, op_id, "Not your turn").await;
            return Ok(());
        }

        // Use FSM to validate and transition
        let new_turn_state = match state.turn_state.on_flip(card_id.clone()) {
            Some(s) => s,
            None => {
                self.send_action_error(ws, op_id, "Invalid flip action")
                    .await;
                return Ok(());
            }
        };

        // Card must not already be matched
        let card = match state.cards.find_card(&card_id) {
            Some(c) => c,
            None => {
                self.send_action_error(ws, op_id, "Card not found").await;
                return Ok(());
            }
        };

        if card.matched {
            self.send_action_error(ws, op_id, "Card already matched")
                .await;
            return Ok(());
        }

        let face = CardFace::from(card);

        state.turn_state = new_turn_state;

        // Broadcast the flip
        let delta = MemoryDelta::CardFlipped {
            card_id,
            by: conn.user_id.clone(),
            face,
        };
        self.broadcast_delta(delta).await;

        self.save_game_state(state).await;
        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    /// Handle ACK that a card's image has loaded on the client.
    /// This advances the turn state FSM and may trigger the flip-back timer.
    async fn handle_ack_card_loaded(
        &self,
        ws: &WebSocket,
        _conn: &ConnectionInfo,
        op_id: OpId,
        card_id: CardId,
    ) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Must be playing
        if !matches!(state.phase, GamePhase::Playing) {
            self.send_action_error(ws, op_id, "Game is not in progress")
                .await;
            return Ok(());
        }

        // Only turn-taking mode uses this ACK flow for now
        if state.config.mode != GameMode::TurnTaking {
            self.send_action_ok(ws, op_id).await;
            return Ok(());
        }

        // Compute if it's a match (needed for FSM transition)
        let is_match = match &state.turn_state {
            TurnState::SecondFlipped { first, second, .. } => {
                let first_card = state.cards.find_card(first);
                let second_card = state.cards.find_card(second);
                match (first_card, second_card) {
                    (Some(fc), Some(sc)) => fc.pair_id == sc.pair_id,
                    _ => false,
                }
            }
            _ => false,
        };

        tracing::info!(
            %card_id,
            is_match,
            turn_state = ?state.turn_state,
            "AckCardLoaded received"
        );

        // Advance FSM with ACK
        let new_turn_state = state.turn_state.on_ack(&card_id, is_match, now());
        let was_both_ready = matches!(new_turn_state, TurnState::BothReady { .. });

        tracing::info!(?new_turn_state, was_both_ready, "After FSM transition");

        state.turn_state = new_turn_state;

        // If we just transitioned to BothReady, handle match/no-match resolution
        if was_both_ready {
            if let TurnState::BothReady {
                first,
                second,
                is_match,
                ..
            } = &state.turn_state
            {
                let first = first.clone();
                let second = second.clone();
                let is_match = *is_match;

                if is_match {
                    tracing::info!(%first, %second, "Match found - resolving immediately");
                    // Use the current turn player, not the ACK sender
                    let current_player_id = state.turn_order.get(state.current_turn).cloned();
                    self.resolve_match(&mut state, &first, &second, current_player_id.as_deref())
                        .await;
                } else {
                    // Schedule the flip-back after delay using Duration
                    let delay_ms = state.config.flip_delay_ms;
                    let duration = std::time::Duration::from_millis(delay_ms);
                    tracing::info!(?duration, "No match - scheduling alarm");
                    match self.state.storage().set_alarm(duration).await {
                        Ok(_) => tracing::info!("Alarm set successfully"),
                        Err(e) => tracing::error!("Failed to set alarm: {:?}", e),
                    }
                }
            }
        }

        self.save_game_state(&state).await;
        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    /// Handle the flip timer expiring - flip cards back and advance turn
    async fn handle_flip_timer_expired(&self) -> Result<()> {
        tracing::info!("Alarm fired - handle_flip_timer_expired called");
        let mut state = self.get_game_state().await;
        tracing::info!(turn_state = ?state.turn_state, "Current turn state in alarm handler");

        // Only process if we're in BothReady state with a non-match
        if let TurnState::BothReady {
            first,
            second,
            is_match,
            ..
        } = &state.turn_state
        {
            if !is_match {
                // Flip cards back
                let delta = MemoryDelta::CardsReset {
                    card_ids: [first.clone(), second.clone()],
                    for_player: None,
                };
                self.broadcast_delta(delta).await;

                // Advance turn
                state.current_turn = (state.current_turn + 1) % state.turn_order.len();
                let next_player = state.turn_order[state.current_turn].clone();

                let delta = MemoryDelta::TurnChanged {
                    user_id: next_player,
                };
                self.broadcast_delta(delta).await;

                // Reset turn state for next player
                state.turn_state = TurnState::AwaitingFirst;

                self.save_game_state(&state).await;
            }
        }

        Ok(())
    }

    /// Resolve a successful match - mark cards, update score, check game end
    async fn resolve_match(
        &self,
        state: &mut MemoryGameState,
        first: &CardId,
        second: &CardId,
        player_id: Option<&str>,
    ) {
        let player_id = match player_id {
            Some(id) => id.to_string(),
            None => return, // No player to credit
        };

        // Mark cards as matched
        if let Some(card) = state.cards.find_card_mut(first) {
            card.matched = true;
            card.matched_by = Some(player_id.clone());
        }
        if let Some(card) = state.cards.find_card_mut(second) {
            card.matched = true;
            card.matched_by = Some(player_id.clone());
        }

        // Update score
        if let Some(player) = state.players.get_mut(&player_id) {
            player.score += 1;
        }

        let new_score = state.players.get(&player_id).map(|p| p.score).unwrap_or(0);
        let user_name = state
            .players
            .get(&player_id)
            .map(|p| p.user_name.clone())
            .unwrap_or_default();

        let first_card = state.cards.find_card(first);
        let face = first_card.map(CardFace::from).unwrap_or_else(|| CardFace {
            asset_id: String::new(),
            name: String::new(),
        });
        let pair_name = first_card.map(|c| c.name.clone()).unwrap_or_default();

        let delta = MemoryDelta::PairMatched {
            card_ids: [first.clone(), second.clone()],
            by: player_id.clone(),
            by_name: user_name.clone(),
            new_score,
            face,
        };
        self.broadcast_delta(delta).await;

        // Broadcast match event
        let event = MemoryEvent::MatchFound {
            player_name: user_name,
            pair_name,
        };
        self.broadcast_event("game", event).await;

        // Reset turn state (player gets another turn)
        state.turn_state = TurnState::AwaitingFirst;

        // Check for game end
        if state.cards.iter().all(|c| c.matched) {
            self.end_game(state).await;
        }
    }

    async fn handle_flip_race(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
        card_id: CardId,
        state: &mut MemoryGameState,
    ) -> Result<()> {
        // Get player's flipped cards
        let player = match state.players.get_mut(&conn.user_id) {
            Some(p) if !p.spectating => p,
            _ => {
                self.send_action_error(ws, op_id, "You are not a player")
                    .await;
                return Ok(());
            }
        };

        // Can't flip same card twice
        if player.flipped.contains(&card_id) {
            self.send_action_error(ws, op_id, "Card already flipped")
                .await;
            return Ok(());
        }

        // Can only have 2 cards flipped
        if player.flipped.len() >= 2 {
            self.send_action_error(ws, op_id, "Two cards already flipped")
                .await;
            return Ok(());
        }

        // Get card data before mutating player state
        let card = match state.cards.find_card(&card_id) {
            Some(c) => c,
            None => {
                self.send_action_error(ws, op_id, "Card not found").await;
                return Ok(());
            }
        };
        let face = CardFace::from(card);

        // Flip the card (only this player sees it initially)
        let player = state.players.get_mut(&conn.user_id).unwrap();
        player.flipped.push(card_id.clone());

        // Send only to this player
        let delta = MemoryDelta::OwnCardFlipped {
            card_id: card_id.clone(),
            face: face.clone(),
        };
        self.send_delta_to(ws, delta).await;

        // Check if player has two cards flipped
        let player = state.players.get(&conn.user_id).unwrap();
        if player.flipped.len() == 2 {
            let id1 = player.flipped[0].clone();
            let id2 = player.flipped[1].clone();

            // Look up cards to check for pair match
            let card1 = state.cards.find_card(&id1);
            let card2 = state.cards.find_card(&id2);

            let (pair_match, card1_matched, card2_matched, pair_name) = match (card1, card2) {
                (Some(c1), Some(c2)) => (
                    c1.pair_id == c2.pair_id,
                    c1.matched,
                    c2.matched,
                    c1.name.clone(),
                ),
                _ => (false, false, false, String::new()),
            };

            if pair_match {
                // Check if cards are still available (race condition)
                if card1_matched || card2_matched {
                    // Someone else got it first
                    let player = state.players.get_mut(&conn.user_id).unwrap();
                    player.flipped.clear();

                    let delta = MemoryDelta::CardsReset {
                        card_ids: [id1, id2],
                        for_player: Some(conn.user_id.clone()),
                    };
                    self.send_delta_to(ws, delta).await;
                } else {
                    // Claim the match!
                    if let Some(card) = state.cards.find_card_mut(&id1) {
                        card.matched = true;
                        card.matched_by = Some(conn.user_id.clone());
                    }
                    if let Some(card) = state.cards.find_card_mut(&id2) {
                        card.matched = true;
                        card.matched_by = Some(conn.user_id.clone());
                    }

                    let player = state.players.get_mut(&conn.user_id).unwrap();
                    player.score += 1;
                    let new_score = player.score;
                    let user_name = player.user_name.clone();
                    player.flipped.clear();

                    // Broadcast match to all players
                    let delta = MemoryDelta::PairMatched {
                        card_ids: [id1, id2],
                        by: conn.user_id.clone(),
                        by_name: user_name.clone(),
                        new_score,
                        face,
                    };
                    self.broadcast_delta(delta).await;

                    let event = MemoryEvent::MatchFound {
                        player_name: user_name,
                        pair_name,
                    };
                    self.broadcast_event("game", event).await;

                    // Check for game end
                    if state.cards.iter().all(|c| c.matched) {
                        self.end_game(state).await;
                    }
                }
            } else {
                // No match - reset for this player only
                let player = state.players.get_mut(&conn.user_id).unwrap();
                player.flipped.clear();

                let delta = MemoryDelta::CardsReset {
                    card_ids: [id1, id2],
                    for_player: Some(conn.user_id.clone()),
                };
                self.send_delta_to(ws, delta).await;
            }
        }

        self.save_game_state(state).await;
        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    async fn end_game(&self, state: &mut MemoryGameState) {
        // Calculate rankings
        let mut rankings: Vec<_> = state
            .players
            .values()
            .filter(|p| !p.spectating)
            .map(|p| (p.user_id.clone(), p.user_name.clone(), p.score))
            .collect();

        rankings.sort_by(|a, b| b.2.cmp(&a.2));

        let winner = rankings.first().map(|(id, _, _)| id.clone());

        state.phase = GamePhase::Finished {
            winner: winner.clone(),
            rankings: rankings.clone(),
        };

        let delta = MemoryDelta::GameEnded { winner, rankings };
        self.broadcast_delta(delta).await;
    }

    async fn handle_reset_game(&self, ws: &WebSocket, op_id: OpId) -> Result<()> {
        // Delete all storage and reset to fresh default state
        self.state.storage().delete_all().await?;

        let state = MemoryGameState::default();
        self.save_game_state(&state).await;

        // Send full snapshot to resync everyone
        let seq = 0u64;
        self.state
            .storage()
            .put("seq", seq)
            .await
            .map_err(|e| Error::from(format!("Failed to save seq: {e}")))?;

        let snapshot_msg: MemoryServerMsg = ServerMessage::snapshot(state, seq, now());
        if let Ok(bytes) = encode(&snapshot_msg) {
            for ws in self.state.get_websockets() {
                let _ = ws.send_with_bytes(&bytes);
            }
        }

        self.send_action_ok(ws, op_id).await;
        tracing::info!("Game state reset by admin");

        Ok(())
    }

    async fn handle_request_rematch(
        &self,
        ws: &WebSocket,
        conn: &ConnectionInfo,
        op_id: OpId,
    ) -> Result<()> {
        let mut state = self.get_game_state().await;

        // Only after game finished
        if !matches!(state.phase, GamePhase::Finished { .. }) {
            self.send_action_error(ws, op_id, "Game is not finished")
                .await;
            return Ok(());
        }

        // Reset to lobby
        state.phase = GamePhase::Lobby {
            min_players: 1,
            max_players: 8,
        };
        state.cards.clear();
        state.turn_order.clear();
        state.current_turn = 0;
        state.turn_state = TurnState::AwaitingFirst;

        // Reset player states but keep players
        for player in state.players.values_mut() {
            player.score = 0;
            player.flipped.clear();
            player.spectating = false;
        }

        // Requester becomes host
        state.host = Some(conn.user_id.clone());

        self.save_game_state(&state).await;

        // Send full snapshot to resync everyone
        let seq = self.get_seq().await;
        let snapshot_msg: MemoryServerMsg = ServerMessage::snapshot(state, seq, now());
        if let Ok(bytes) = encode(&snapshot_msg) {
            for ws in self.state.get_websockets() {
                let _ = ws.send_with_bytes(&bytes);
            }
        }

        self.send_action_ok(ws, op_id).await;

        Ok(())
    }

    // =========================================================================
    // Broadcast helpers
    // =========================================================================

    async fn broadcast_delta(&self, delta: MemoryDelta) {
        let seq = self.next_seq().await;
        let msg: MemoryServerMsg = ServerMessage::delta(delta, seq, now());

        if let Ok(bytes) = encode(&msg) {
            for ws in self.state.get_websockets() {
                let _ = ws.send_with_bytes(&bytes);
            }
        }
    }

    async fn send_delta_to(&self, ws: &WebSocket, delta: MemoryDelta) {
        let seq = self.next_seq().await;
        let msg: MemoryServerMsg = ServerMessage::delta(delta, seq, now());

        if let Ok(bytes) = encode(&msg) {
            let _ = ws.send_with_bytes(&bytes);
        }
    }

    async fn broadcast_event(&self, domain: &str, event: MemoryEvent) {
        let msg: MemoryServerMsg = ServerMessage::notify(domain, event, None);

        if let Ok(bytes) = encode(&msg) {
            for ws in self.state.get_websockets() {
                let _ = ws.send_with_bytes(&bytes);
            }
        }
    }

    async fn broadcast_presence(&self) {
        let websockets = self.state.get_websockets();
        let mut users = Vec::new();

        for ws in &websockets {
            if let Ok(Some(conn)) = ws.deserialize_attachment::<ConnectionInfo>() {
                users.push(PresenceInfo {
                    user_id: conn.user_id,
                    name: Some(conn.user_name),
                    status: PresenceStatus::Active,
                    connected_at: conn.connected_at,
                });
            }
        }

        let msg: MemoryServerMsg = ServerMessage::presence(users);

        if let Ok(bytes) = encode(&msg) {
            for ws in websockets {
                let _ = ws.send_with_bytes(&bytes);
            }
        }
    }

    async fn send_action_ok(&self, ws: &WebSocket, op_id: OpId) {
        let msg: MemoryServerMsg = ServerMessage::action_ok(op_id, None);
        if let Ok(bytes) = encode(&msg) {
            let _ = ws.send_with_bytes(&bytes);
        }
    }

    async fn send_action_error(&self, ws: &WebSocket, op_id: OpId, reason: &str) {
        let msg: MemoryServerMsg = ServerMessage::action_err(op_id, reason.to_string());
        if let Ok(bytes) = encode(&msg) {
            let _ = ws.send_with_bytes(&bytes);
        }
    }
}

fn now() -> u64 {
    js_sys::Date::now() as u64
}

fn js_random_u32() -> u32 {
    (js_sys::Math::random() * (u32::MAX as f64)) as u32
}
