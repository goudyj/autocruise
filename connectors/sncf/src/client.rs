//! Thin HTTP client for the SNCF API (Navitia).
//!
//! Authentication: the key is sent as the raw `Authorization` header value
//! (equivalent to HTTP basic auth with the key as user name and no password).

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::cache::TtlCache;
use autocruise_core::http::{HttpRequest, HttpTransport, ReqwestTransport, Url};
use autocruise_core::throttle::Throttle;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::SncfError;

pub const DEFAULT_BASE_URL: &str = "https://api.sncf.com/v1/coverage/sncf";
pub const TOKEN_ENV: &str = "SNCF_API_TOKEN";
pub const BASE_URL_ENV: &str = "AUTOCRUISE_SNCF_BASE_URL";

const USER_AGENT: &str = concat!(
    "autocruise/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/goudyj/autocruise)"
);
const TIMEOUT: Duration = Duration::from_secs(20);
const MIN_INTERVAL: Duration = Duration::from_millis(300);
const CACHE_TTL: Duration = Duration::from_secs(3600);
const CACHE_CAPACITY: usize = 256;

#[derive(Debug)]
pub struct SncfClient {
    transport: Arc<dyn HttpTransport>,
    base_url: String,
    token: Option<String>,
    throttle: Throttle,
    cache: TtlCache<Arc<Value>>,
}

impl SncfClient {
    /// Reads `SNCF_API_TOKEN` (and optionally `AUTOCRUISE_SNCF_BASE_URL`).
    /// A missing token is reported when a tool is called, not at startup.
    pub fn from_env() -> Self {
        let token = std::env::var(TOKEN_ENV)
            .ok()
            .filter(|t| !t.trim().is_empty());
        let base_url = std::env::var(BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self::new(
            Arc::new(ReqwestTransport::new(USER_AGENT, TIMEOUT)),
            token,
            base_url,
        )
    }

    pub fn new(
        transport: Arc<dyn HttpTransport>,
        token: Option<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            token,
            throttle: Throttle::new(MIN_INTERVAL),
            cache: TtlCache::new(CACHE_TTL, CACHE_CAPACITY),
        }
    }

    /// Overrides the minimum delay between requests (tests use zero).
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.throttle = Throttle::new(interval);
        self
    }

    /// GET `{base_url}/{path}?{query}`. `cacheable` responses are kept in memory for an hour.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        cacheable: bool,
    ) -> Result<T, SncfError> {
        let token = self.token.as_deref().ok_or(SncfError::MissingToken)?;

        let mut url = Url::parse(&format!("{}/{}", self.base_url, path))
            .map_err(|e| SncfError::Invalid(format!("invalid request URL: {e}")))?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }

        let key = url.to_string();
        if cacheable && let Some(value) = self.cache.get(&key) {
            return Ok(T::deserialize(value.as_ref())?);
        }

        self.throttle.wait().await;
        let response = self
            .transport
            .get(HttpRequest::get(url).header("Authorization", token))
            .await?;

        match response.status {
            401 | 403 => return Err(SncfError::Unauthorized(response.status)),
            429 => return Err(SncfError::RateLimited),
            _ => {}
        }

        let value: Value = serde_json::from_slice(&response.body).map_err(|e| {
            if response.is_success() {
                SncfError::Decode(e)
            } else {
                SncfError::Api {
                    id: response.status.to_string(),
                    message: String::from_utf8_lossy(&response.body)
                        .chars()
                        .take(200)
                        .collect(),
                }
            }
        })?;

        if let Some(error) = value.get("error") {
            return Err(SncfError::Api {
                id: error["id"].as_str().unwrap_or("unknown").to_owned(),
                message: error["message"].as_str().unwrap_or_default().to_owned(),
            });
        }
        if !response.is_success() {
            return Err(SncfError::Api {
                id: response.status.to_string(),
                message: value["message"].as_str().unwrap_or_default().to_owned(),
            });
        }

        let value = Arc::new(value);
        if cacheable {
            self.cache.insert(key, Arc::clone(&value));
        }
        Ok(T::deserialize(value.as_ref())?)
    }
}

/// Rejects identifiers that could alter the request path.
pub(crate) fn check_id(id: &str) -> Result<&str, SncfError> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.' | ';'));
    if ok {
        Ok(id)
    } else {
        Err(SncfError::Invalid(format!("invalid identifier {id:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(check_id("stop_area:SNCF:87391003").is_ok());
        assert!(check_id("2.3212;48.8412").is_ok());
        assert!(check_id("../journeys").is_err());
        assert!(check_id("a?b=c").is_err());
        assert!(check_id("").is_err());
    }
}
