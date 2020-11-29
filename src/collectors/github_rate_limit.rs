use prometheus::{core::Collector, IntGauge, Opts};

use crate::{http, Config};
use anyhow::{bail, Context, Error, Result};
use log::{debug, error, warn};
use prometheus::core::Desc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
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

#[derive(Clone)]
pub struct GitHubRateLimit {
    // uninitialized tokens
    uninit_tokens: Vec<String>,
    // metric namespace
    ns: String,
    // metrics collection
    metrics: Arc<RwLock<Vec<User>>>,
    // default metric description
    desc: Desc,
}

impl GitHubRateLimit {
    pub async fn new(config: &Config) -> Result<Self, Error> {
        let tokens: Vec<String> = config
            .gh_rate_limit_tokens
            .split(',')
            .map(|v| v.trim().to_string())
            .collect();

        let ns = String::from("github_rate_limit");
        let users = Self::get_users_for_tokens(ns.clone(), tokens)
            .await
            .context("Unable to get usernames for rate limit stats")?;

        let rv = Self {
            uninit_tokens: users.1,
            ns: ns.clone(),
            metrics: Arc::new(RwLock::new(users.0)),
            desc: Desc::new(
                ns,
                "GH rate limit stats".to_string(),
                Vec::new(),
                HashMap::new(),
            )
            .unwrap(),
        };

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

        // start task to catch up with uninit tokens
        if !rv.uninit_tokens.is_empty() {
            let refresh_rate = config.gh_rate_limit_stats_cache_refresh + 10;
            let mut rv2 = rv.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::delay_for(Duration::from_secs(refresh_rate)).await;
                    match Self::get_users_for_tokens(rv2.ns.clone(), rv2.uninit_tokens.clone())
                        .await
                    {
                        Err(e) => {
                            error!("{:#?}", e);
                        }
                        Ok(data) => {
                            if data.0.is_empty() {
                                continue;
                            }

                            if let Ok(mut guard) = rv2.metrics.try_write() {
                                for u in data.0 {
                                    guard.push(u)
                                }
                            }

                            // drop the task, all tokens are under monitoring
                            if data.1.is_empty() {
                                rv2.uninit_tokens.clear();
                                return;
                            }

                            // update data for next iteration
                            rv2.uninit_tokens = data.1;
                        }
                    };
                }
            });
        }

        Ok(rv)
    }

    async fn get_users_for_tokens(
        ns: String,
        tokens: Vec<String>,
    ) -> Result<(Vec<User>, Vec<String>), Error> {
        let mut uninit_tokens: Vec<String> = Vec::with_capacity(tokens.len());
        let mut metrics: Vec<User> = Vec::new();
        for token in tokens.into_iter() {
            let username = match GitHubRateLimit::get_github_api_username(&token).await {
                Ok(login) => login,
                Err(e) => {
                    warn!(
                        "unable to get token's '{}' username at this time\n{:?}",
                        token, e
                    );
                    uninit_tokens.push(token);
                    continue;
                }
            };

            let ns2 = ns.clone();
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
                        .namespace(ns2)
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

            metrics.push(user);
        }

        Ok((metrics, uninit_tokens))
    }

    async fn get_github_api_username(token: &str) -> Result<String, Error> {
        #[derive(serde::Deserialize)]
        struct GithubUser {
            pub login: String,
        }

        match http::is_token_flagged(token).await {
            Err(e) => bail!("checking if token is flagged: {:?}", e),
            Ok(v) => {
                if v {
                    bail!("Token is flagged. unable to get username")
                } else {
                    Ok(http::get(&token, GH_API_USER_ENDPOINT)
                        .send()
                        .await?
                        .json::<GithubUser>()
                        .await?
                        .login)
                }
            }
        }
    }

    async fn update_stats(&mut self) -> Result<(), Error> {
        debug!("Updating rate limit stats");

        // lock and read what to query for
        let tokens = {
            self.metrics.read().map_or_else(
                |_| Vec::new(),
                |lock| {
                    lock.iter().fold(Vec::new(), |mut acc, item| {
                        acc.push(item.token.clone());
                        acc
                    })
                },
            )
        };

        // querying new updated data
        let mut new_stats = Vec::<(String, i64, i64, i64)>::with_capacity(tokens.len());
        for token in tokens.into_iter() {
            let mut data = http::get_token_rate_limit_stats(&token).await?;

            let remaining = data.rate.remove("remaining").unwrap_or(0);
            let limit = data.rate.remove("limit").unwrap_or(0);
            let reset = data.rate.remove("reset").unwrap_or(0);

            new_stats.push((
                token.to_owned(),
                remaining as i64,
                limit as i64,
                reset as i64,
            ));
        }

        // lock and write
        if let Ok(guard) = self.metrics.try_write() {
            for new_data in new_stats.into_iter() {
                guard
                    .iter()
                    .find(|item| item.token == new_data.0)
                    .map(|item| {
                        item.remaining.set(new_data.1);
                        item.limit.set(new_data.2);
                        item.reset.set(new_data.3);
                    })
                    .unwrap();
            }
        }

        Ok(())
    }
}

impl Collector for GitHubRateLimit {
    fn desc(&self) -> Vec<&Desc> {
        vec![&self.desc]
    }

    fn collect(&self) -> std::vec::Vec<prometheus::proto::MetricFamily> {
        self.metrics.read().map_or_else(
            |e| {
                error!("Unable to collect: {:#?}", e);
                Vec::new()
            },
            |guard| {
                guard.iter().fold(Vec::new(), |mut acc, item| {
                    acc.extend(item.limit.collect());
                    acc.extend(item.remaining.collect());
                    acc.extend(item.reset.collect());

                    acc
                })
            },
        )
    }
}
