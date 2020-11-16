use anyhow::{bail, Error, Result};
use hyper::Server;
use log::info;
use monitorbot::Config;
use monitorbot::{collectors::register_collectors, MetricProvider};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv().ok();
    env_logger::init();

    let config = Config::from_env()?;
    let port = config.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let provider = MetricProvider::new(config);
    if let Err(e) = register_collectors(&provider).await {
        bail!("(Registering collectors) {}", e)
    }

    let server = Server::bind(&addr).serve(provider.into_service());
    info!("Server listening on port: {}", port);

    if let Err(e) = server.await {
        bail!("(Hyper server error) {}", e);
    }

    Ok(())
}
