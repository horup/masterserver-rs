use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod matchbox;
mod roomy;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
        )
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_file(false)
                .with_target(false),
        )
        .init();

    info!("Starting...");
    let roomy_server = tokio::spawn(roomy::start());
    let matcbox_server = tokio::spawn(matchbox::start());

    let res = roomy_server.await;
    println!("{:?}", res);
    let res = matcbox_server.await;
    println!("{:?}", res);
}
