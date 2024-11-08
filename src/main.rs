use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod matchbox;
mod roomy;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_target(true),
        )
        .init();

    let roomy_port = 8080;
    let matchbox_port = 8081;
    info!("Starting Roomy on port {roomy_port} and Matchbox on port {matchbox_port}");
    let roomy_server = tokio::spawn(roomy::start(roomy_port));
    let matcbox_server = tokio::spawn(matchbox::start(matchbox_port));
    let _ = roomy_server.await;
    let _ = matcbox_server.await;
}
