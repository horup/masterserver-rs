use std::net::{IpAddr, Ipv4Addr};
use matchbox_signaling::{SignalingServer, SignalingServerBuilder};
use axum::{async_trait, extract::ws::Message, Error};
use matchbox_protocol::PeerId;
use matchbox_signaling::{
    common_logic::{self, StateObj},
    SignalingError, SignalingState,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Deserialize, Default, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RoomId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RequestedRoom {
    pub id: RoomId,
    pub next: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct Peer {
    pub uuid: PeerId,
    pub room: RequestedRoom,
    pub sender: UnboundedSender<Result<Message, Error>>,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct ServerState {
    clients_waiting: StateObj<HashMap<SocketAddr, RequestedRoom>>,
    clients_in_queue: StateObj<HashMap<PeerId, RequestedRoom>>,
    clients: StateObj<HashMap<PeerId, Peer>>,
    rooms: StateObj<HashMap<RequestedRoom, HashSet<PeerId>>>,
}
impl SignalingState for ServerState {}

impl ServerState {
    /// Add a waiting client to matchmaking
    pub fn add_waiting_client(&mut self, origin: SocketAddr, room: RequestedRoom) {
        self.clients_waiting.lock().unwrap().insert(origin, room);
    }

    /// Assign a peer id to a waiting client
    pub fn assign_id_to_waiting_client(&mut self, origin: SocketAddr, peer_id: PeerId) {
        let room = {
            let mut lock = self.clients_waiting.lock().unwrap();
            lock.remove(&origin).expect("waiting client")
        };
        {
            let mut lock = self.clients_in_queue.lock().unwrap();
            lock.insert(peer_id, room);
        }
    }

    /// Remove the waiting peer, returning the peer's requested room
    pub fn remove_waiting_peer(&mut self, peer_id: PeerId) -> RequestedRoom {
        let room = {
            let mut lock = self.clients_in_queue.lock().unwrap();
            lock.remove(&peer_id).expect("waiting peer")
        };
        room
    }

    /// Add a peer, returning the peers already in room
    pub fn add_peer(&mut self, peer: Peer) -> Vec<PeerId> {
        let peer_id = peer.uuid;
        let room = peer.room.clone();
        {
            let mut clients = self.clients.lock().unwrap();
            clients.insert(peer.uuid, peer);
        };
        let mut rooms = self.rooms.lock().unwrap();
        let peers = rooms.entry(room.clone()).or_default();
        let prev_peers = peers.iter().cloned().collect();

        match room.next {
            None => {
                peers.insert(peer_id);
            }
            Some(num_players) => {
                if peers.len() == num_players - 1 {
                    peers.clear(); // room is complete
                } else {
                    peers.insert(peer_id);
                }
            }
        };

        prev_peers
    }

    /// Get a peer
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<Peer> {
        let clients = self.clients.lock().unwrap();
        clients.get(peer_id).cloned()
    }

    /// Get the peers in a room currently
    pub fn get_room_peers(&self, room: &RequestedRoom) -> Vec<PeerId> {
        self.rooms
            .lock()
            .unwrap()
            .get(room)
            .map(|room_peers| room_peers.iter().copied().collect::<Vec<PeerId>>())
            .unwrap_or_default()
    }

    /// Remove a peer from the state if it existed, returning the peer removed.
    #[must_use]
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Option<Peer> {
        let peer = { self.clients.lock().unwrap().remove(peer_id) };

        if let Some(ref peer) = peer {
            // Best effort to remove peer from their room
            _ = self
                .rooms
                .lock()
                .unwrap()
                .get_mut(&peer.room)
                .map(|room| room.remove(peer_id));
        }
        peer
    }

    /// Send a message to a peer without blocking.
    pub fn try_send(&self, id: PeerId, message: Message) -> Result<(), SignalingError> {
        let clients = self.clients.lock().unwrap();
        match clients.get(&id) {
            Some(peer) => Ok(common_logic::try_send(&peer.sender, message)?),
            None => Err(SignalingError::UnknownPeer),
        }
    }
}


use futures::StreamExt;
use matchbox_protocol::{JsonPeerEvent, PeerRequest};
use matchbox_signaling::{
    common_logic::parse_request, ClientRequestError, NoCallbacks, SignalingTopology, WsStateMeta,
};
use tracing::{error, info, warn};

#[derive(Debug, Default)]
pub struct MatchmakingDemoTopology;

#[async_trait]
impl SignalingTopology<NoCallbacks, ServerState> for MatchmakingDemoTopology {
    async fn state_machine(upgrade: WsStateMeta<NoCallbacks, ServerState>) {
        let WsStateMeta {
            peer_id,
            sender,
            mut receiver,
            mut state,
            ..
        } = upgrade;

        let room = state.remove_waiting_peer(peer_id);
        let peer = Peer {
            uuid: peer_id,
            sender: sender.clone(),
            room,
        };

        // Tell other waiting peers about me!
        let peers = state.add_peer(peer);
        let event_text = JsonPeerEvent::NewPeer(peer_id).to_string();
        let event = Message::Text(event_text.clone());
        for peer_id in peers {
            if let Err(e) = state.try_send(peer_id, event.clone()) {
                error!("error sending to {peer_id:?}: {e:?}");
            } else {
                info!("{peer_id} -> {event_text:?}");
            }
        }

        // The state machine for the data channel established for this websocket.
        while let Some(request) = receiver.next().await {
            let request = match parse_request(request) {
                Ok(request) => request,
                Err(e) => {
                    match e {
                        ClientRequestError::Axum(_) => {
                            // Most likely a ConnectionReset or similar.
                            warn!("Unrecoverable error with {peer_id:?}: {e:?}");
                            break;
                        }
                        ClientRequestError::Close => {
                            info!("Connection closed by {peer_id:?}");
                            break;
                        }
                        ClientRequestError::Json(_) | ClientRequestError::UnsupportedType(_) => {
                            error!("Error with request: {:?}", e);
                            continue; // Recoverable error
                        }
                    };
                }
            };

            match request {
                PeerRequest::Signal { receiver, data } => {
                    let event = Message::Text(
                        JsonPeerEvent::Signal {
                            sender: peer_id,
                            data,
                        }
                        .to_string(),
                    );
                    if let Some(peer) = state.get_peer(&receiver) {
                        if let Err(e) = peer.sender.send(Ok(event)) {
                            error!("error sending signal event: {e:?}");
                        }
                    } else {
                        warn!("peer not found ({receiver:?}), ignoring signal");
                    }
                }
                PeerRequest::KeepAlive => {
                    // Do nothing. KeepAlive packets are used to protect against idle websocket
                    // connections getting automatically disconnected, common for reverse proxies.
                }
            }
        }

        // Peer disconnected or otherwise ended communication.
        info!("Removing peer: {:?}", peer_id);
        if let Some(removed_peer) = state.remove_peer(&peer_id) {
            let room = removed_peer.room;
            let other_peers = state
                .get_room_peers(&room)
                .into_iter()
                .filter(|other_id| *other_id != peer_id);
            // Tell each connected peer about the disconnected peer.
            let event = Message::Text(JsonPeerEvent::PeerLeft(removed_peer.uuid).to_string());
            for peer_id in other_peers {
                match state.try_send(peer_id, event.clone()) {
                    Ok(()) => info!("Sent peer remove to: {:?}", peer_id),
                    Err(e) => error!("Failure sending peer remove: {e:?}"),
                }
            }
        }
    }
}




pub async fn start() {
    // todo needs to implement room support
    /*println!("Starting 'matchbox'...");
    let server = SignalingServer::client_server_builder((Ipv4Addr::UNSPECIFIED, 8081))
    .on_connection_request(|c| {
        Ok(true) // Allow all connections
    })
    
    .on_id_assignment(|(socket, id)| println!("{socket} received {id}"))
    .on_host_connected(|id| println!("Host joined: {id}"))
    .on_host_disconnected(|id| println!("Host left: {id}"))
    .on_client_connected(|id| println!("Client joined: {id}"))
    .on_client_disconnected(|id| println!("Client left: {id}"))
    .cors()
    .build();
    let _ = server.serve().await;*/

    info!("hell world");
    let mut state = ServerState::default();
    let server = SignalingServerBuilder::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8081), MatchmakingDemoTopology::default(), state.clone())
        .on_connection_request({
            let mut state = state.clone();
            move |connection| {
                let room_id = RoomId(connection.path.clone().unwrap_or_default());
                let next = connection
                    .query_params
                    .get("next")
                    .and_then(|next| next.parse::<usize>().ok());
                let room = RequestedRoom { id: room_id, next };
                state.add_waiting_client(connection.origin, room);
                Ok(true) // allow all clients
            }
        })
        .on_id_assignment({
            move |(origin, peer_id)| {
                info!("Client connected {origin:?}: {peer_id:?}");
                state.assign_id_to_waiting_client(origin, peer_id);
            }
        })
        .cors()
        .trace()
       // .mutate_router(|router| router.route("/health", get(health_handler)))
        .build();
    server
        .serve()
        .await
        .expect("Unable to run signaling server, is it already running?")
}