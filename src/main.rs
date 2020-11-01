pub mod collectors;

use crate::collectors::register_collectors;
use hyper::Server;
use monitorbot::MetricProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let addr = "0.0.0.0:3001".parse().unwrap();
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
