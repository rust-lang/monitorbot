use hyper::Server;
use monitorbot::Config;
use monitorbot::{collectors::register_collectors, MetricProvider};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let config = Config::from_env()?;
    let port = config.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let provider = MetricProvider::new(config);
    if let Err(e) = register_collectors(&provider).await {
        eprintln!("Unable to register collectors: {:#?}", e);
        return Ok(());
    }

    let server = Server::bind(&addr).serve(provider.into_service());
    println!("Server listening on port {}", port);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
