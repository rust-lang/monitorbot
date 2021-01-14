use prometheus::{core::Collector, IntGauge, Opts};

use crate::Config;
use anyhow::{Context, Error, Result};
use log::{debug, error};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, Request};
use std::collections::HashMap;
use tokio::time::Duration;

#[derive(Clone)]
struct User {
    token: String,
    name: String,
    limit: IntGauge,
    remaining: IntGauge,
    reset: IntGauge,
}

const GH_API_USER_ENDPOINT: &str = "https://api.github.com/user";
const GH_API_RATE_LIMIT_ENDPOINT: &str = "https://api.github.com/rate_limit";

enum GithubReqBuilder {
    User,
    RateLimit,
}

impl GithubReqBuilder {
    fn build_request(&self, client: &Client, token: &str) -> Result<Request, Error> {
        let rb = match self {
            Self::User => client.request(Method::GET, GH_API_USER_ENDPOINT),
            Self::RateLimit => client.request(Method::GET, GH_API_RATE_LIMIT_ENDPOINT),
        };

        rb.header(
            USER_AGENT,
            "https://github.com/rust-lang/monitorbot (infra@rust-lang.org)",
        )
        .header(AUTHORIZATION, format!("{} {}", "token", token))
        .header(ACCEPT, "application/vnd.github.v3+json")
        .build()
        .map_err(Error::from)
    }
}

#[derive(Clone)]
pub struct GitHubRateLimit {
    users: Vec<User>,
}

impl GitHubRateLimit {
    pub async fn new(config: &Config) -> Result<Self, Error> {
        let tokens: Vec<String> = config
            .gh_rate_limit_tokens
            .split(',')
            .map(|v| v.trim().to_string())
            .collect();

        let users = Self::get_users_for_tokens(tokens)
            .await
            .context("Unable to get usernames for rate limit stats")?;

        let rv = Self { users };

        let refresh_rate = config.gh_rate_limit_stats_cache_refresh;
        let mut rv2 = rv.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = rv2.update_stats().await {
                    error!("{:#?}", e);
                }

                tokio::time::delay_for(Duration::from_secs(refresh_rate)).await;
            }
        });

        Ok(rv)
    }

    async fn get_users_for_tokens(tokens: Vec<String>) -> Result<Vec<User>, Error> {
        let ns = String::from("monitorbot_github_rate_limit");
        let mut rv: Vec<User> = Vec::new();
        for token in tokens.into_iter() {
            let ns2 = ns.clone();
            let username = GitHubRateLimit::get_github_api_username(&token).await?;
            let user_future = tokio::task::spawn_blocking(move || {
                let rate_limit = IntGauge::with_opts(
                    Opts::new("limit", "Rate limit.")
                        .namespace(ns2.clone())
                        .const_label("username", username.clone()),
                )
                .unwrap();

                let rate_remaining = IntGauge::with_opts(
                    Opts::new("remaining", "Rate remaining.")
                        .namespace(ns2.clone())
                        .const_label("username", username.clone()),
                )
                .unwrap();

                let rate_reset = IntGauge::with_opts(
                    Opts::new("reset", "Rate reset.")
                        .namespace(ns2.clone())
                        .const_label("username", username.clone()),
                )
                .unwrap();

                User {
                    token: token.to_owned(),
                    name: username,
                    limit: rate_limit,
                    remaining: rate_remaining,
                    reset: rate_reset,
                }
            });

            let user = match user_future.await {
                Ok(u) => u,
                _ => panic!("We need to decide if we wanna panic or keep going"),
            };

            rv.push(user);
        }

        Ok(rv)
    }

    async fn get_github_api_username(token: &str) -> Result<String, Error> {
        #[derive(serde::Deserialize)]
        struct GithubUser {
            pub login: String,
        }

        let client = reqwest::Client::new();
        let req = GithubReqBuilder::User.build_request(&client, &token)?;

        let u = client
            .execute(req)
            .await?
            .error_for_status()?
            .json::<GithubUser>()
            .await?;

        Ok(u.login)
    }

    async fn update_stats(&mut self) -> Result<(), Error> {
        debug!("Updating rate limit stats");

        #[derive(Debug, serde::Deserialize)]
        struct GithubRateLimit {
            pub rate: HashMap<String, usize>,
        }

        let client = reqwest::Client::new();
        for u in self.users.iter_mut() {
            let req = GithubReqBuilder::RateLimit
                .build_request(&client, &u.token)
                .context("Unable to build request to update stats")?;

            let response = client
                .execute(req)
                .await
                .context("Unable to execute request to update stats")?;

            let mut data = response
                .json::<GithubRateLimit>()
                .await
                .context("Unable to deserialize rate limit stats")?;

            let remaining = data.rate.remove("remaining").unwrap_or(0);
            let limit = data.rate.remove("limit").unwrap_or(0);
            let reset = data.rate.remove("reset").unwrap_or(0);

            u.remaining.set(remaining as i64);
            u.reset.set(reset as i64);
            u.limit.set(limit as i64);
        }

        Ok(())
    }
}

impl Collector for GitHubRateLimit {
    fn desc(&self) -> std::vec::Vec<&prometheus::core::Desc> {
        // descriptions are being defined in the initialization of the metrics options
        Vec::default()
    }

    fn collect(&self) -> std::vec::Vec<prometheus::proto::MetricFamily> {
        // collect MetricFamilys.
        let mut mfs = Vec::new();
        for user in self.users.iter() {
            mfs.extend(user.limit.collect());
            mfs.extend(user.remaining.collect());
            mfs.extend(user.reset.collect());
        }

        mfs
    }
}
