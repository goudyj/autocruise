//! HTTP transport abstraction.
//!
//! Connectors build [`HttpRequest`]s and send them through an
//! [`HttpTransport`]: [`ReqwestTransport`] in production,
//! [`mock::MockTransport`] in tests (records requests, serves fixtures).

pub mod mock;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub use reqwest::Url;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: Url,
    pub headers: Vec<(String, String)>,
}

impl HttpRequest {
    pub fn get(url: Url) -> Self {
        Self {
            url,
            headers: Vec::new(),
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Header value, case-insensitive name lookup.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// First value of a query parameter.
    pub fn query(&self, key: &str) -> Option<String> {
        self.query_all(key).into_iter().next()
    }

    /// All values of a (possibly repeated) query parameter.
    pub fn query_all(&self, key: &str) -> Vec<String> {
        self.url
            .query_pairs()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The request could not be completed (DNS, TLS, timeout...).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

pub trait HttpTransport: Send + Sync + fmt::Debug {
    fn get(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>>;
}

/// Production transport.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new(user_agent: &str, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(timeout)
            .build()
            .expect("HTTP client configuration is valid");
        Self { client }
    }
}

impl HttpTransport for ReqwestTransport {
    fn get(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let mut builder = self.client.get(request.url);
            for (name, value) in &request.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            let response = builder
                .send()
                .await
                .map_err(|e| TransportError(e.without_url().to_string()))?;
            let status = response.status().as_u16();
            let body = response
                .bytes()
                .await
                .map_err(|e| TransportError(e.without_url().to_string()))?
                .to_vec();
            Ok(HttpResponse { status, body })
        })
    }
}
