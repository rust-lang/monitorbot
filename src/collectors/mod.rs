mod github_rate_limit;

pub use crate::collectors::github_rate_limit::GitHubRateLimit;

use crate::MetricProvider;
use anyhow::Result;
use futures::FutureExt;
use log::info;

// register collectors for metrics gathering
pub async fn register_collectors(p: &MetricProvider) -> Result<()> {
    GitHubRateLimit::new(&p.config)
        .map(|rl| {
            info!("Registering GitHubRateLimit collector");
            p.register_collector(rl)
        })
        .await
}
