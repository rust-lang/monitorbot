use super::default_headers;
use crate::Config;
use anyhow::{Context, Result};
use log::{debug, error};
use prometheus::core::AtomicI64;
use prometheus::core::{Desc, GenericGauge};
use prometheus::proto::MetricFamily;
use prometheus::{core::Collector, IntGauge, Opts};
use reqwest::header::{HeaderValue, LINK};
use reqwest::{Client, Response};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::time::Duration;

const GH_RUNNERS_ENDPOINT: &str =
    "https://api.github.com/repos/{owner_repo}/actions/runners?per_page=100";

#[derive(Debug, serde::Deserialize)]
struct ApiResponse {
    #[expect(dead_code)]
    total_count: usize,
    runners: Vec<Runner>,
}

#[derive(Debug, serde::Deserialize)]
struct Runner {
    #[expect(dead_code)]
    id: usize,
    name: String,
    #[expect(dead_code)]
    os: String,
    status: String,
    busy: bool,
}

#[derive(Clone)]
pub struct GithubRunners {
    //api token to use
    token: String,
    // repos to track gha runners
    repos: Vec<String>,
    // actual metrics
    metrics: Arc<RwLock<Vec<IntGauge>>>,
    // default metric description
    desc: Desc,
    http: Client,
}

impl GithubRunners {
    pub async fn new(config: &Config, http: Client) -> Result<Self> {
        let token = config.github_token.to_string();
        let repos: Vec<String> = config
            .gha_runners_repos
            .split(',')
            .map(|v| v.trim().to_string())
            .collect();

        let rv = Self {
            token,
            repos,
            http,
            metrics: Arc::new(RwLock::new(Vec::new())),
            desc: Desc::new(
                String::from("gha_runner"),
                String::from("GHA runner's status"),
                Vec::new(),
                HashMap::new(),
            )
            .unwrap(),
        };

        let refresh_rate = config.gha_runners_cache_refresh;
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

    async fn update_stats(&mut self) -> Result<()> {
        let mut gauges = Vec::with_capacity(self.repos.len() * 2);
        for repo in self.repos.iter() {
            let mut url: Option<String> = String::from(GH_RUNNERS_ENDPOINT)
                .replace("{owner_repo}", repo)
                .into();

            debug!("Updating runner's stats");

            while let Some(endpoint) = url.take() {
                let response = self
                    .http
                    .get(&endpoint)
                    .headers(default_headers(&self.token))
                    .send()
                    .await?;

                url = guard_rate_limited(&response)?
                    .error_for_status_ref()
                    .map(|res| next_uri(res.headers().get(LINK)))?;

                let resp = response.json::<ApiResponse>().await?;

                for runner in resp.runners.iter() {
                    let online = metric_factory(
                        "online",
                        "runner is online",
                        &self.desc.fq_name,
                        repo,
                        &runner.name,
                    );
                    online.set(if runner.status == "online" { 1 } else { 0 });
                    gauges.push(online);

                    let busy = metric_factory(
                        "busy",
                        "runner is busy",
                        &self.desc.fq_name,
                        repo,
                        &runner.name,
                    );
                    busy.set(if runner.busy { 1 } else { 0 });
                    gauges.push(busy);
                }
            }
        }

        // lock and replace old data
        let mut guard = self.metrics.write().unwrap();
        *guard = gauges;

        Ok(())
    }
}

impl Collector for GithubRunners {
    fn desc(&self) -> Vec<&Desc> {
        vec![&self.desc]
    }

    fn collect(&self) -> Vec<MetricFamily> {
        self.metrics.read().map_or_else(
            |e| {
                error!("Unable to collect: {:#?}", e);
                Vec::with_capacity(0)
            },
            |guard| {
                guard.iter().fold(Vec::new(), |mut acc, item| {
                    acc.extend(item.collect());
                    acc
                })
            },
        )
    }
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

fn next_uri(header: Option<&HeaderValue>) -> Option<String> {
    if let Some(header) = header {
        return match header.to_str() {
            Ok(header_str) => match parse_link_header::parse(header_str) {
                Ok(links) => links
                    .get(&Some("next".to_string()))
                    .map(|next| next.uri.to_string()),
                _ => None,
            },
            _ => None,
        };
    }

    None
}

fn metric_factory<S: Into<String>>(
    name: S,
    help: S,
    ns: S,
    repo: S,
    runner: S,
) -> GenericGauge<AtomicI64> {
    IntGauge::with_opts(
        Opts::new(name, help)
            .namespace(ns)
            .const_label("repo", repo)
            .const_label("runner", runner),
    )
    .unwrap()
}
