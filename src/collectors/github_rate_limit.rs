use prometheus::{core::Collector, IntGauge, Opts};

use crate::collectors::{default_headers, guard_rate_limited};
use crate::Config;
use anyhow::{Context, Error, Result};
use log::{debug, error};
use prometheus::core::Desc;
use prometheus::proto::MetricFamily;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

const GH_API_USER_ENDPOINT: &str = "https://api.github.com/user";
const GH_API_RATE_LIMIT_ENDPOINT: &str = "https://api.github.com/rate_limit";

#[derive(Clone)]
pub struct GitHubRateLimit {
    users: Vec<User>,
    desc: Desc,
    http: Client,
}

impl GitHubRateLimit {
    pub async fn new(config: &Config, http: Client) -> Result<Self, Error> {
        let tokens: Vec<String> = config
            .gh_rate_limit_tokens
            .split(',')
            .map(|v| v.trim().to_string())
            .collect();

        let users = get_users_for_tokens(&http, tokens)
            .await
            .context("Unable to get usernames for rate limit stats")?;

        let rv = Self {
            users,
            http,
            desc: Desc::new(
                String::from("gh_rate_limit"),
                String::from("GH rate limit"),
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

        Ok(rv)
    }

    async fn update_stats(&mut self) -> Result<(), Error> {
        #[derive(Debug, serde::Deserialize)]
        struct ResponseBody {
            resources: HashMap<String, ResponseResource>,
        }

        #[derive(Debug, serde::Deserialize)]
        struct ResponseResource {
            limit: i64,
            remaining: i64,
            reset: i64,
        }

        debug!("Updating rate limit stats");

        for user in self.users.iter_mut() {
            let data: ResponseBody = self
                .http
                .get(GH_API_RATE_LIMIT_ENDPOINT)
                .headers(default_headers(&user.token))
                .send()
                .await
                .context("Unable to execute request to update stats")?
                .json()
                .await
                .context("Unable to deserialize rate limit stats")?;

            let mut user_products = user.products.lock().unwrap();
            for (product_name, resource) in data.resources.iter() {
                let product = user_products
                    .entry(product_name.to_string())
                    .or_insert_with(|| ProductMetrics::new(&user.name, &product_name));

                product.limit.set(resource.limit);
                product.remaining.set(resource.remaining);
                product.reset.set(resource.reset);
            }
        }

        Ok(())
    }
}

impl Collector for GitHubRateLimit {
    fn desc(&self) -> Vec<&Desc> {
        vec![&self.desc]
    }

    fn collect(&self) -> Vec<MetricFamily> {
        let mut metrics = Vec::new();
        for user in self.users.iter() {
            for product in user.products.lock().unwrap().values() {
                metrics.extend(product.limit.collect());
                metrics.extend(product.remaining.collect());
                metrics.extend(product.reset.collect());
            }
        }
        metrics
    }
}

async fn get_users_for_tokens(client: &Client, tokens: Vec<String>) -> Result<Vec<User>, Error> {
    #[derive(serde::Deserialize)]
    struct GithubUser {
        login: String,
    }

    let mut result = Vec::with_capacity(tokens.len());
    for token in &tokens {
        let response = client
            .get(GH_API_USER_ENDPOINT)
            .headers(default_headers(token))
            .send()
            .await?;

        guard_rate_limited(&response)?;

        let name = response
            .error_for_status()?
            .json::<GithubUser>()
            .await
            .map(|u| u.login)?;

        result.push(User {
            token: token.to_owned(),
            name,
            products: Arc::new(Mutex::new(HashMap::new())),
        });
    }

    Ok(result)
}

#[derive(Clone)]
struct User {
    token: String,
    name: String,
    products: Arc<Mutex<HashMap<String, ProductMetrics>>>,
}

struct ProductMetrics {
    limit: IntGauge,
    remaining: IntGauge,
    reset: IntGauge,
}

impl ProductMetrics {
    fn new(user: &str, product: &str) -> Self {
        let gauge = |name, help| -> IntGauge {
            IntGauge::with_opts(
                Opts::new(name, help)
                    .namespace("github_rate_limit")
                    .const_label("username", user)
                    .const_label("product", product),
            )
            .unwrap()
        };
        Self {
            limit: gauge("limit", "GitHub API total rate limit"),
            remaining: gauge("remaining", "GitHub API remaining rate limit"),
            reset: gauge("reset", "GitHub API rate limit reset time"),
        }
    }
}
