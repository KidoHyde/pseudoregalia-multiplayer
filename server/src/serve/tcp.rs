use crate::state::{ConnectionUpdate, State};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::{net::TcpStream, sync::mpsc::UnboundedReceiver};
use tokio_tungstenite::WebSocketStream;

#[derive(Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Connected { id: u8, players: Vec<u8> },
    PlayerJoined { id: u8 },
    PlayerLeft { id: u8 },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    Connect,
}

struct Connection {
    ws_stream: WebSocketStream<TcpStream>,
    id: u8,
    rx: UnboundedReceiver<ConnectionUpdate>,
    state: Arc<Mutex<State>>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.state.lock().unwrap().disconnect(self.id);
    }
}

pub async fn handle_connection(state: Arc<Mutex<State>>, raw_stream: TcpStream) {
    let mut connection = match receive_connection(state, raw_stream).await {
        Ok(connection) => connection,
        Err(err) => {
            println!("connection refused: {}", err);
            return;
        }
    };
    println!("{:02x}: connection established", connection.id);

    // arbitrary, but probably more than enough to handle all updates available at a time
    let limit = 16;
    let mut buf = Vec::with_capacity(limit);
    let reason = loop {
        tokio::select! {
            // ignore the number returned because buf is guaranteed to be empty as
            // send_connection_updates drains all of buf
            _ = connection.rx.recv_many(&mut buf, limit) => {
                if let Err(err) = send_updates(&mut connection.ws_stream, &mut buf).await {
                    break format!("failed to send connection updates: {}", err);
                }
            }
            msg = connection.ws_stream.next() => {
                let Some(msg) = msg else {
                    break "stream reader closed".to_owned();
                };
                let msg = match msg {
                    Ok(msg) => msg,
                    Err(err) => {
                        break format!("failed to read next from stream: {}", err);
                    },
                };
                if msg.is_close() {
                    break "received close message".to_owned();
                }
            }
        }
    };
    println!("{:02x}: disconnected: {}", connection.id, reason);
}

async fn receive_connection(
    state: Arc<Mutex<State>>,
    raw_stream: TcpStream,
) -> Result<Connection, String> {
    let mut ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .map_err(|e| format!("error during WebSocket handshake occured: {}", e))?;
    receive_connect_message(&mut ws_stream)
        .await
        .map_err(|e| format!("failed to receive connect message: {}", e))?;
    let (id, rx, players) = state.lock().unwrap().connect().ok_or("server full".to_owned())?;
    let mut connection = Connection { ws_stream, id, rx, state };

    let msg = ServerMessage::Connected { id, players };
    let msg = serde_json::to_string(&msg).unwrap();
    connection
        .ws_stream
        .send(msg.into())
        .await
        .map_err(|e| format!("{:02x}: error sending connected message: {}", id, e))?;
    Ok(connection)
}

async fn receive_connect_message(ws_stream: &mut WebSocketStream<TcpStream>) -> Result<(), String> {
    loop {
        // Validate message
        let Some(msg) = ws_stream.next().await else {
            return Err("stream reader closed".to_owned());
        };
        let msg = msg.map_err(|e| format!("failed to get next: {}", e))?;
        if msg.is_close() {
            return Err("received close message".to_owned());
        }
        if !msg.is_text() && !msg.is_binary() {
            continue;
        }

        // Parse message
        let msg = msg.to_text().map_err(|e| format!("failed to cast message to text: {}", e))?;
        // Currently, the only valid client message is Connect, which has no fields, so we only care
        // if deserialization is successful.
        // TODO if more client message types get added or the Connect message gets fields added to
        // it, msg will have to actually be used.
        let _msg = serde_json::from_str::<ClientMessage>(msg)
            .map_err(|e| format!("failed to deserialize message: {}", e))?;

        return Ok(());
    }
}

async fn send_updates(
    ws_stream: &mut WebSocketStream<TcpStream>,
    buf: &mut Vec<ConnectionUpdate>,
) -> Result<(), String> {
    if buf.len() == 0 {
        // I don't think this should ever happen because rx is only dropped when we disconnect
        return Err("rx is closed??".to_owned());
    }
    for connection_update in buf.drain(..) {
        feed_connection_update(ws_stream, connection_update).await?;
    }
    ws_stream.flush().await.map_err(|e| format!("failed to send connection updates: {}", e))
}

async fn feed_connection_update(
    ws_stream: &mut WebSocketStream<TcpStream>,
    connection_update: ConnectionUpdate,
) -> Result<(), String> {
    let msg = match connection_update {
        ConnectionUpdate::Connected(id) => ServerMessage::PlayerJoined { id },
        ConnectionUpdate::Disconnected(id) => ServerMessage::PlayerLeft { id },
    };
    let msg = serde_json::to_string(&msg).unwrap();
    ws_stream.feed(msg.into()).await.map_err(|e| format!("failed to feed connection update: {}", e))
}
