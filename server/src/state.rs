use crate::message::{PlayerInfo, ServerMessage};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::collections::{BTreeMap, HashMap, HashSet};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

// semi-arbitrary limit on number of connected players, but this guarantees that server updates fit
// into a single packet
const MAX_PLAYERS: usize = 22;

// how many updates to keep for each player
const MAX_UPDATES: usize = 20;

pub const STATE_LEN: usize = 24;

pub struct PlayerState {
    bytes: [u8; STATE_LEN],
    sent_to: HashSet<u8>,
}

impl PlayerState {
    /// Creates a new PlayerState from its byte representation. Also returns the parsed id and
    /// millis.
    pub fn from_bytes(bytes: [u8; STATE_LEN]) -> (u8, u32, Self) {
        let id = bytes[0];
        let millis = u32::from_be_bytes(bytes[1..5].try_into().unwrap());
        (id, millis, Self { bytes, sent_to: HashSet::new() })
    }
}

struct Player {
    color: [u8; 3],
    states: BTreeMap<u32, PlayerState>,
    tx: UnboundedSender<ServerMessage>,
}

impl Player {
    fn new(color: [u8; 3], tx: UnboundedSender<ServerMessage>) -> Self {
        Self { color, states: BTreeMap::new(), tx }
    }

    fn update(&mut self, millis: u32, player_state: PlayerState) {
        // ignore duplicates
        if self.states.contains_key(&millis) {
            return;
        }

        // if states is not full, just put it in
        if self.states.len() < MAX_UPDATES {
            self.states.insert(millis, player_state);
            return;
        }

        // if states is full, only put it in if it wouldn't be first; then pop first
        if self.states.first_key_value().unwrap().0 < &millis {
            self.states.insert(millis, player_state);
            self.states.pop_first();
        }
    }
}

/// Shared state between all threads, used to track what has been received from and what should be
/// sent to players.
pub struct State {
    players: HashMap<u8, Player>,
    rng: SmallRng,
}

impl State {
    pub fn new() -> Self {
        Self { players: HashMap::new(), rng: SmallRng::from_rng(&mut rand::rng()) }
    }

    pub fn connect(
        &mut self,
        color: [u8; 3],
    ) -> Option<(u8, UnboundedReceiver<ServerMessage>, Vec<PlayerInfo>)> {
        if self.players.len() == MAX_PLAYERS {
            return None;
        }

        // player limit means this should be fine, right?
        let id = loop {
            let id = self.rng.random::<u8>();
            if !self.players.contains_key(&id) {
                break id;
            }
        };

        // create list of other players' ids while informing other players of this new connection
        let mut players = Vec::with_capacity(self.players.len());
        for (player_id, player) in &self.players {
            players.push(PlayerInfo { id: *player_id, color: player.color });
            // if the corresponding rx has been dropped, it doesn't matter that this message won't
            // get read, so we can ignore the error
            let _ = player.tx.send(ServerMessage::PlayerJoined { id, color });
        }

        let (tx, rx) = mpsc::unbounded_channel();
        self.players.insert(id, Player::new(color, tx));

        Some((id, rx, players))
    }

    /// Removes the player associated with id from state and informs other players that they
    /// disconnected.
    pub fn disconnect(&mut self, id: u8) {
        if self.players.remove(&id).is_none() {
            // TODO this shouldn't happen, right?
            return;
        }

        for player in self.players.values() {
            let _ = player.tx.send(ServerMessage::PlayerLeft { id });
        }
    }

    /// Updates player state and returns up to one update for each other connected player. Returns
    /// None if `id` isn't a connected player.
    pub fn update(
        &mut self,
        id: u8,
        millis: u32,
        player_state: PlayerState,
    ) -> Option<Vec<[u8; STATE_LEN]>> {
        let player = self.players.get_mut(&id)?;
        player.update(millis, player_state);

        Some(self.filtered_state(id))
    }

    fn filtered_state(&mut self, id: u8) -> Vec<[u8; STATE_LEN]> {
        let mut filtered_state = Vec::with_capacity(self.players.len());
        for (player_id, player) in &mut self.players {
            if id == *player_id {
                continue;
            }

            // get the most recent update that hasn't been sent to the player
            for state in player.states.values_mut().rev() {
                if !state.sent_to.contains(&id) {
                    state.sent_to.insert(id);
                    filtered_state.push(state.bytes);
                    break;
                }
            }
        }
        filtered_state
    }
}
