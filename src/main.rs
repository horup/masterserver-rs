
use matchbox_signaling::SignalingServer;
use std::net::Ipv4Addr;

mod roomy;

#[tokio::main]
async fn main() {
    println!("Starting...");
    let server1 = tokio::spawn(roomy::start());

    let matchbox = tokio::spawn(async {
        // todo needs to implement room support
        println!("Starting 'matchbox'...");
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
        let _ = server.serve().await;
    });

    let res = server1.await;
    println!("{:?}", res);
    let res = matchbox.await;
    println!("{:?}", res);
}
