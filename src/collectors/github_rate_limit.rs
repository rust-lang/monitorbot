use prometheus::{core::Collector, IntGauge, Opts};

use crate::Config;
use anyhow::{Context, Error, Result};
use log::{debug, error};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, Request};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

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
        let mut result = Vec::new();
        for token in &tokens {
            result.push(User {
                token: token.to_owned(),
                name: GitHubRateLimit::get_github_api_username(&token).await?,
                products: Arc::new(Mutex::new(HashMap::new())),
            });
        }
        Ok(result)
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

        let client = reqwest::Client::new();
        for user in self.users.iter_mut() {
            let req = GithubReqBuilder::RateLimit
                .build_request(&client, &user.token)
                .context("Unable to build request to update stats")?;

            let response = client
                .execute(req)
                .await
                .context("Unable to execute request to update stats")?;

            let data: ResponseBody = response
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
    fn desc(&self) -> std::vec::Vec<&prometheus::core::Desc> {
        // descriptions are being defined in the initialization of the metrics options
        Vec::default()
    }

    fn collect(&self) -> std::vec::Vec<prometheus::proto::MetricFamily> {
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
                    .namespace("monitorbot_github_rate_limit")
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
