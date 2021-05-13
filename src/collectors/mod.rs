mod github_rate_limit;
mod github_runners;

pub use crate::collectors::github_rate_limit::GitHubRateLimit;
pub use crate::collectors::github_runners::GithubRunners;

use crate::MetricProvider;
use anyhow::{Context, Error, Result};
use futures::TryFutureExt;
use log::info;
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION};
use reqwest::{ClientBuilder, Response};

// register collectors for metrics gathering
pub async fn register_collectors(p: &MetricProvider) -> Result<(), Error> {
    let http = ClientBuilder::new()
        .user_agent("https://github.com/rust-lang/monitorbot (infra@rust-lang.org)")
        .build()?;

    GitHubRateLimit::new(&p.config, http.clone())
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
        AUTHORIZATION,
        format!("{} {}", "token", token).parse().unwrap(),
    );
    headers.insert(ACCEPT, "application/vnd.github.v3+json".parse().unwrap());
    headers
}

fn guard_rate_limited(response: &Response) -> Result<&Response> {
    let rate_limited = match response.headers().get("x-ratelimit-remaining") {
        Some(rl) => rl.to_str()?.parse::<usize>()? == 0,
        None => unreachable!(),
    };

    if rate_limited {
        return response
            .error_for_status_ref()
            .context("We've hit the rate limit");
    }

    Ok(response)
}
