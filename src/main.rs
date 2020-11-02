use crate::collectors::register_collectors;
use hyper::Server;
use monitorbot::{MetricProvider, collectors};
use std::net::SocketAddr;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let port = std::env::var("MONITORBOT_PORT").unwrap_or("80".to_string());
    let addr = match u16::from_str(port.as_ref()) {
        Ok(port) => SocketAddr::from(([0, 0, 0, 0], port)),
        Err(e) => {
            eprintln!("Unable to parse MONITOR PORT: {:?}", e);
            return Ok(());
        }
    };

    let provider = MetricProvider::new();
    if let Err(e) = register_collectors(&provider).await {
        eprintln!("Unable to register collectors: {:#?}", e);
        return Ok(());
    }

    let server = Server::bind(&addr).serve(provider.into_service());
    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
