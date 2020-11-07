mod github_rate_limit;
pub use crate::collectors::github_rate_limit::GitHubRateLimit;

use crate::MetricProvider;
use futures::FutureExt;
use std::borrow::Borrow;

// register collectors for metrics gathering
pub async fn register_collectors(p: &MetricProvider) -> Result<(), prometheus::Error> {
    GitHubRateLimit::new(p.config.borrow())
        .map(|rl| p.register_collector(rl))
        .await
}
