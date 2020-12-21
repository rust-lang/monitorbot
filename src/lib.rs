#![allow(clippy::new_without_default)]

pub mod collectors;
pub(crate) mod http;
mod config;

pub use config::Config;

use prometheus::core::Collector;
use prometheus::{Encoder, Registry};

use anyhow::{Error, Result};
use futures::future;
use futures::task::{Context, Poll};
use hyper::header::AUTHORIZATION;
use hyper::http::HeaderValue;
use hyper::service::Service;
use hyper::{Body, HeaderMap, Method, Request, Response, StatusCode};
use log::{debug, error};

#[derive(Clone, Debug)]
pub struct MetricProvider {
    register: prometheus::Registry,
    config: Config,
}

impl MetricProvider {
    pub fn new(config: Config) -> Self {
        let register = Registry::new_custom(Some("monitorbot".to_string()), None)
            .expect("Unable to build Registry");
        Self { register, config }
    }

    fn register_collector(&self, collector: impl Collector + 'static) -> Result<(), Error> {
        self.register
            .register(Box::new(collector))
            .map_err(Error::from)
    }

    fn gather_with_encoder<BUF>(&self, encoder: impl Encoder, buf: &mut BUF) -> Result<(), Error>
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

        let authorized = is_auth_token_valid(&self.config.secret, req.headers());
        let output = match (req.method(), req.uri().path(), authorized) {
            // Metrics handler
            (&Method::GET, "/metrics", true) => {
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
            // Unauthorized request
            (&Method::GET, "/metrics", false) => Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap(),
            // All other paths and methods
            _ => Response::builder()
                .status(StatusCode::OK)
                .body(Body::from("Yep, we're running.."))
                .unwrap(),
        };

        future::ok(output)
    }
}

fn is_auth_token_valid(secret: &str, headers: &HeaderMap<HeaderValue>) -> bool {
    match headers.get(AUTHORIZATION) {
        Some(value) => value.to_str().map_or_else(
            |_| false,
            |t| {
                if let Some(t) = t.strip_prefix("Bearer ") {
                    t == secret
                } else {
                    false
                }
            },
        ),
        None => false,
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

#[cfg(test)]
mod tests {
    use crate::is_auth_token_valid;
    use hyper::http::HeaderValue;
    use hyper::HeaderMap;

    #[test]
    fn auth_token_strip_bearer() {
        use hyper::header::AUTHORIZATION;

        let secret = "aASgwyfbFAKETOKEN44562uj36";
        let token = "Bearer aASgwyfbFAKETOKEN44562uj36";
        let v = HeaderValue::from_static(token);
        let mut hv = HeaderMap::new();
        hv.insert(AUTHORIZATION, v);

        // should be true
        let result = is_auth_token_valid(secret, &hv);
        assert!(result);
    }

    #[test]
    fn auth_token_strip_bearer_fail() {
        use hyper::header::AUTHORIZATION;

        let secret = "aASgwyfbFAKETOKEN44562uj36";
        let token = "Bearer aASgwyfbFAKETOKEN44562uj36 "; // notice the whitespace in the end
        let v = HeaderValue::from_static(token);
        let mut hv = HeaderMap::new();
        hv.insert(AUTHORIZATION, v);

        // should be true
        let result = is_auth_token_valid(secret, &hv);
        assert_eq!(false, result);
    }
}
