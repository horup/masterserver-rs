mod roomy;
mod matchbox;

#[tokio::main]
async fn main() {
    println!("Starting...");
    let roomy_server = tokio::spawn(roomy::start());

    let matcbox_server = tokio::spawn(matchbox::start());

    let res = roomy_server.await;
    println!("{:?}", res);
    let res = matcbox_server.await;
    println!("{:?}", res);
}
