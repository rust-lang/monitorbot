use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, RequestBuilder};

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
