use crate::Config;
use anyhow::Error;
use log::{debug, error};
use prometheus::core::Desc;
use prometheus::proto::MetricFamily;
use prometheus::{core::Collector, IntGauge, Opts};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::Method;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::time::Duration;

const GH_RUNNERS_ENDPOINT: &str = "https://api.github.com/repos/{owner_repo}/actions/runners";

#[derive(Debug, serde::Deserialize)]
struct ApiResponse {
    total_count: usize,
    runners: Vec<Runner>,
}

#[derive(Debug, serde::Deserialize)]
struct Runner {
    id: usize,
    name: String,
    os: String,
    status: String,
    busy: bool,
    labels: Vec<Label>,
}

#[derive(Debug, serde::Deserialize)]
struct Label {
    id: usize,
    name: String,
    #[serde(rename = "type")]
    the_type: String,
}

#[derive(Clone)]
pub struct GithubRunners {
    //api token to use
    token: String,
    // repos to track gha runners
    repos: Vec<String>,
    // metric namespace
    ns: String,
    // actual metrics
    metrics: Arc<RwLock<Vec<IntGauge>>>,
    // default metric description
    desc: Desc,
}

impl GithubRunners {
    pub async fn new(config: &Config) -> Result<Self, Error> {
        let token = config.rust_runners_token.to_string();
        let repos: Vec<String> = config
            .gha_runners_repos
            .split(',')
            .map(|v| v.trim().to_string())
            .collect();

        let ns = String::from("gha_runner");
        let rv = Self {
            token,
            repos,
            ns: ns.clone(),
            metrics: Arc::new(RwLock::new(Vec::new())),
            desc: Desc::new(
                ns,
                "GHA runner's status".to_string(),
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

    async fn update_stats(&mut self) -> Result<(), Error> {
        let mut gauges = Vec::new();
        let client = reqwest::Client::new();

        for repo in self.repos.iter() {
            let url = String::from(GH_RUNNERS_ENDPOINT).replace("{owner_repo}", repo);

            debug!("Querying gha runner's status at: {}", url);
            let req = client
                .request(Method::GET, &url)
                .header(
                    USER_AGENT,
                    "https://github.com/rust-lang/monitorbot (infra@rust-lang.org)",
                )
                .header(AUTHORIZATION, format!("{} {}", "token", self.token))
                .header(ACCEPT, "application/vnd.github.v3+json")
                .build()?;

            let resp = client.execute(req).await?.json::<ApiResponse>().await?;

            //debug!("ApiResponse: {:#?}", resp);

            // convert to metrics
            for runner in resp.runners.iter() {
                let status = &runner.status.clone();
                let value_busy = if runner.busy { 1 } else { 0 };
                let label_repo = repo.clone();
                let label_runner = runner.name.clone();

                // online
                let online = IntGauge::with_opts(
                    Opts::new("online", "runner is online.")
                        .namespace(self.ns.clone())
                        .const_label("repo", label_repo.clone())
                        .const_label("runner", label_runner.clone()),
                )
                .unwrap();

                online.set(if status == "online" { 1 } else { 0 });
                gauges.push(online);

                // busy
                let busy = IntGauge::with_opts(
                    Opts::new("busy", "runner is busy.")
                        .namespace(self.ns.clone())
                        .const_label("repo", label_repo)
                        .const_label("runner", label_runner),
                )
                .unwrap();

                busy.set(value_busy);
                gauges.push(busy);
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
                Vec::new()
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
