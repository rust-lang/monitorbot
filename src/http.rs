use anyhow::{Context, Error, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, RequestBuilder};
use std::collections::HashMap;

pub(crate) const GH_API_RATE_LIMIT_ENDPOINT: &str = "https://api.github.com/rate_limit";

pub(crate) fn get(token: &str, url: &str) -> RequestBuilder {
    Client::new()
        .request(Method::GET, url)
        .header(
            USER_AGENT,
            "https://github.com/rust-lang/monitorbot (infra@rust-lang.org)",
        )
        .header(AUTHORIZATION, format!("{} {}", "token", token))
        .header(ACCEPT, "application/vnd.github.v3+json")
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GithubRateLimit {
    pub rate: HashMap<String, usize>,
}

pub(crate) async fn is_token_flagged(token: &str) -> Result<bool, Error> {
    get_token_rate_limit_stats(token)
        .await
        .map(|mut r| Ok(r.rate.remove("remaining").unwrap_or(0) == 0))?
}

pub(crate) async fn get_token_rate_limit_stats(token: &str) -> Result<GithubRateLimit, Error> {
    get(token, GH_API_RATE_LIMIT_ENDPOINT)
        .send()
        .await?
        .json::<GithubRateLimit>()
        .await
        .context("Unable to get rate limit stats")
}
