pub mod collectors;

use prometheus::core::Collector;
use prometheus::{Encoder, Registry};

use futures::future;
use futures::task::{Context, Poll};
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, StatusCode};

#[derive(Clone, Debug)]
pub struct MetricProvider {
    register: prometheus::Registry,
}

impl MetricProvider {
    pub fn new() -> Self {
        let register = Registry::new_custom(None, None).expect("Unable to build Registry");
        Self { register }
    }

    fn register_collector(
        &self,
        collector: impl Collector + 'static,
    ) -> Result<(), prometheus::Error> {
        self.register.register(Box::new(collector))
    }

    fn gather_with_encoder<BUF: std::io::Write>(&self, encoder: impl Encoder, buf: &mut BUF) {
        encoder.encode(&self.register.gather(), buf).unwrap();
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
        let output = match (req.method(), req.uri().path()) {
            // Metrics handler
            (&Method::GET, "/metrics") => {
                let encoder = prometheus::TextEncoder::new();
                let mut buffer = Vec::<u8>::new();
                self.gather_with_encoder(encoder, &mut buffer);

                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(buffer))
                    .unwrap()
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
