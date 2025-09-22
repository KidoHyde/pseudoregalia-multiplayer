use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::collections::HashMap;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub struct PlayerState {
    num: u32,
    pub num_bytes: [u8; 4],
    pub id: u32,
    bytes: [u8; 48],
}

impl PlayerState {
    /// Creates a new PlayerState from its byte representation.
    pub fn from_bytes(bytes: &[u8; 52]) -> Self {
        let num_bytes = bytes[0..4].try_into().unwrap();
        Self {
            num: u32::from_be_bytes(num_bytes),
            num_bytes,
            id: u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
            bytes: bytes[4..52].try_into().unwrap(),
        }
    }

    fn new(id: u32) -> Self {
        Self { num: 0, num_bytes: [0u8; 4], id, bytes: [0u8; 48] }
    }

    fn update(&mut self, other: Self) -> bool {
        if other.num > self.num {
            *self = other;
            true
        } else {
            false
        }
    }
}

pub enum ConnectionUpdate {
    Connected(u32),
    Disconnected(u32),
}

struct Player {
    state: PlayerState,
    tx: UnboundedSender<ConnectionUpdate>,
}

impl Player {
    fn new(id: u32, tx: UnboundedSender<ConnectionUpdate>) -> Self {
        Self { state: PlayerState::new(id), tx }
    }
}

/// Shared state between all threads, used to track what has been received from and what should be
/// sent to players.
pub struct State {
    players: HashMap<u32, Player>,
    rng: SmallRng,
}

impl State {
    pub fn new() -> Self {
        Self { players: HashMap::new(), rng: SmallRng::from_rng(&mut rand::rng()) }
    }

    pub fn connect(&mut self) -> Option<(u32, UnboundedReceiver<ConnectionUpdate>, Vec<u32>)> {
        let id: u32 = self.rng.random();
        if self.players.contains_key(&id) {
            // id collision
            return None;
        }

        // create list of other players' ids while informing other players of this new connection
        let mut others = Vec::with_capacity(self.players.len());
        for (player_id, player) in &self.players {
            others.push(*player_id);
            // if the corresponding rx has been dropped, it doesn't matter that this message won't
            // get read, so we can ignore the error
            let _ = player.tx.send(ConnectionUpdate::Connected(id));
        }

        let (tx, rx) = mpsc::unbounded_channel();
        self.players.insert(id, Player::new(id, tx));

        Some((id, rx, others))
    }

    /// Removes the player associated with id from state and informs other players that they
    /// disconnected.
    pub fn disconnect(&mut self, id: u32) {
        if let None = self.players.remove(&id) {
            // TODO this shouldn't happen, right?
            return;
        }

        for player in self.players.values() {
            let _ = player.tx.send(ConnectionUpdate::Disconnected(id));
        }
    }

    /// Updates player state if the id exists and the state number is greater than the current.
    /// Returns true if state was updated.
    pub fn update(&mut self, player_state: PlayerState) -> bool {
        // should this be combined with filtered_state?
        // and return Option<Vec<[u8; 48]>>
        match self.players.get_mut(&player_state.id) {
            Some(player) => player.state.update(player_state),
            None => false,
        }
    }

    /// Returns the current bytes for every player, skipping the player with id matching the `id`
    /// param and other players that haven't been updated yet.
    pub fn filtered_state(&self, id: u32) -> Vec<[u8; 48]> {
        let mut filtered_state = Vec::with_capacity(self.players.len());
        for (inner_id, player) in &self.players {
            if id == *inner_id || player.state.num == 0u32 {
                continue;
            }
            filtered_state.push(player.state.bytes);
        }
        filtered_state
    }
}
