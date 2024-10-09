use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};

use futures::{
    sink::SinkExt,
    stream::{SplitSink, StreamExt},
};
use matchbox_signaling::SignalingServer;
use std::net::{Ipv4Addr, SocketAddr};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

mod protocol;
use protocol::*;

#[derive(Clone, Default)]
pub struct SharedState {
    pub sinks: Arc<Mutex<HashMap<Uuid, SplitSink<WebSocket, Message>>>>,
}

#[tokio::main]
async fn main() {
    let server1 = tokio::spawn(async {
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
    });

    let matchbox = tokio::spawn(async {
        let server = SignalingServer::client_server_builder((Ipv4Addr::UNSPECIFIED, 8081))
        .on_connection_request(|_| {
            Ok(true) // Allow all connections
        })
        .on_id_assignment(|(socket, id)| println!("{socket} received {id}"))
        .on_host_connected(|id| println!("Host joined: {id}"))
        .on_host_disconnected(|id| println!("Host left: {id}"))
        .on_client_connected(|id| println!("Client joined: {id}"))
        .on_client_disconnected(|id| println!("Client left: {id}"))
        .cors()
        .build();
        let _ = server.serve().await;
    });

    let _ = server1.await;
    let _ = matchbox.await;
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<SharedState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        // new client connected.
        // send the client id to this new client.
        // the client can use this id to send broadcasts to other connected clients
        let client_id = Uuid::new_v4();
        println!("Client {} connected", client_id);
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
        println!("Client {} disconnected", client_id);
    })
}
