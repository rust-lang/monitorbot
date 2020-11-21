mod github_rate_limit;
mod github_runners;

pub use crate::collectors::github_rate_limit::GitHubRateLimit;
pub use crate::collectors::github_runners::GithubRunners;

use crate::MetricProvider;
use anyhow::{Error, Result};
use futures::TryFutureExt;
use log::info;

// register collectors for metrics gathering
pub async fn register_collectors(p: &MetricProvider) -> Result<(), Error> {
    GitHubRateLimit::new(&p.config)
        .and_then(|rl| async {
            info!("Registering GitHubRateLimit collector");
            p.register_collector(rl)
        })
        .await?;

    GithubRunners::new(&p.config)
        .and_then(|gr| async {
            info!("Registering GitHubActionsRunners collector");
            p.register_collector(gr)
        })
        .await
}
