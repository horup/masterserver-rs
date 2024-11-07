use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct SharedState {
    pub sinks: Arc<Mutex<HashMap<Uuid, SplitSink<WebSocket, Message>>>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    /// received by the client when joining the master server.
    ///
    /// id is the unique id assigned to the client.
    /// other clients will only know about this client if the client broadcasts this information
    Welcome { id: Uuid },
    /// received by both clients and the server
    /// sent by a client and distributed to other clients to let them know about 'self'.
    /// info contains client provided information, e.g. name of a multiplayer room name, number of players, etc, which is application dependend.  
    Broadcast { id: Uuid, info: String },
    /// sent by clients to server
    /// keeps the connection alive
    Keepalive,
}

impl Protocol {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        match serde_json::from_str(json) {
            Ok(msg) => Ok(msg),
            Err(err) => Err(err.to_string()),
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<SharedState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        // new client connected.
        // send the client id to this new client.
        // the client can use this id to send broadcasts to other connected clients
        let client_id = Uuid::new_v4();
        info!("Client {} connected", client_id);
        if sink
            .send(Message::Text(Protocol::Welcome { id: client_id }.to_json()))
            .await
            .is_err()
        {
            return;
        }

        // move sink to list of sinks such that broadcasts can be sent to all connected clients
        {
            let mut sinks = state.sinks.lock().await;
            sinks.insert(client_id, sink);
        }
        // wait for messages
        while let Some(msg) = stream.next().await {
            let Ok(msg) = msg else { break };
            let Ok(json) = msg.to_text() else { break };
            let Ok(msg) = Protocol::from_json(json) else {
                break;
            };
            match msg {
                Protocol::Broadcast { info, .. } => {
                    // forward message to other clients (including self)
                    let msg = Protocol::Broadcast {
                        id: client_id,
                        info,
                    };
                    let mut sinks = state.sinks.lock().await;
                    let json = msg.to_json();
                    for sink in sinks.values_mut() {
                        let _ = sink.send(Message::Text(json.clone())).await;
                    }
                }
                _ => {}
            }
        }

        // connection ended, remove sink from sinks
        let mut sinks = state.sinks.lock().await;
        sinks.remove(&client_id);
        info!("Client {} disconnected", client_id);
    })
}

pub async fn start() {
    info!("Starting 'roomy'...");
    let app = Router::new()
        .route("/", get(ws_handler))
        .with_state(SharedState::default());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
