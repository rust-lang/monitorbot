mod github_rate_limit;
mod github_runners;

pub use crate::collectors::github_rate_limit::GitHubRateLimit;
pub use crate::collectors::github_runners::GithubRunners;

use crate::MetricProvider;
use anyhow::{Error, Result};
use futures::TryFutureExt;
use log::info;
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::Client;

// register collectors for metrics gathering
pub async fn register_collectors(p: &MetricProvider) -> Result<(), Error> {
    let http = Client::new();
    GitHubRateLimit::new(&p.config)
        .and_then(|rl| async {
            info!("Registering GitHubRateLimit collector");
            p.register_collector(rl)
        })
        .await?;

    GithubRunners::new(&p.config, http)
        .and_then(|gr| async {
            info!("Registering GitHubActionsRunners collector");
            p.register_collector(gr)
        })
        .await
}

fn default_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        "https://github.com/rust-lang/monitorbot (infra@rust-lang.org)"
            .parse()
            .unwrap(),
    );
    headers.insert(
        AUTHORIZATION,
        format!("{} {}", "token", token).parse().unwrap(),
    );
    headers.insert(ACCEPT, "application/vnd.github.v3+json".parse().unwrap());
    headers
}
