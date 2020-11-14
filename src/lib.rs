#![allow(clippy::new_without_default)]

pub mod collectors;
mod config;

pub use config::Config;

use prometheus::core::Collector;
use prometheus::{Encoder, Registry};

use anyhow::{Error, Result};
use futures::future;
use futures::task::{Context, Poll};
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, StatusCode};
use log::{debug, error};

#[derive(Clone, Debug)]
pub struct MetricProvider {
    register: prometheus::Registry,
    config: Config,
}

impl MetricProvider {
    pub fn new(config: Config) -> Self {
        let register = Registry::new_custom(None, None).expect("Unable to build Registry");
        Self { register, config }
    }

    fn register_collector(&self, collector: impl Collector + 'static) -> Result<()> {
        self.register
            .register(Box::new(collector))
            .map_err(Error::from)
    }

    fn gather_with_encoder<BUF>(&self, encoder: impl Encoder, buf: &mut BUF) -> Result<()>
    where
        BUF: std::io::Write,
    {
        encoder
            .encode(&self.register.gather(), buf)
            .map_err(Error::from)
    }

    pub fn into_service(self) -> MetricProviderFactory {
        MetricProviderFactory(self)
    }
}

impl Service<Request<Body>> for MetricProvider {
    type Response = Response<Body>;
    type Error = hyper::Error;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        debug!("New Request to endpoint {}", req.uri().path());

        let output = match (req.method(), req.uri().path()) {
            // Metrics handler
            (&Method::GET, "/metrics") => {
                let encoder = prometheus::TextEncoder::new();
                let mut buffer = Vec::<u8>::new();
                match self.gather_with_encoder(encoder, &mut buffer) {
                    Ok(_) => Response::builder()
                        .status(StatusCode::OK)
                        .body(Body::from(buffer))
                        .unwrap(),
                    Err(e) => {
                        error!("{:?}", e);
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::empty())
                            .unwrap()
                    }
                }
            }
            // All other paths and methods
            _ => Response::builder()
                .status(StatusCode::OK)
                .body(Body::from("Yep, we're running.."))
                .unwrap(),
        };

        future::ok(output)
    }
}

pub struct MetricProviderFactory(pub MetricProvider);

impl<T> Service<T> for MetricProviderFactory {
    type Response = MetricProvider;
    type Error = std::io::Error;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, _: T) -> Self::Future {
        future::ok(self.0.clone())
    }
}
