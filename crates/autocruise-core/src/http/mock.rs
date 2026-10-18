//! In-memory transport for tests: serves canned responses and records requests.

use std::sync::{Mutex, PoisonError};

use super::{BoxFuture, HttpRequest, HttpResponse, HttpTransport, TransportError};

#[derive(Debug)]
struct Route {
    path_suffix: String,
    query: Vec<(String, String)>,
    status: u16,
    body: String,
}

/// Routes match on the URL path suffix and required query pairs; the first
/// matching route wins. Unmatched requests fail with a [`TransportError`].
#[derive(Debug, Default)]
pub struct MockTransport {
    routes: Vec<Route>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route(
        mut self,
        path_suffix: &str,
        query: &[(&str, &str)],
        status: u16,
        body: impl Into<String>,
    ) -> Self {
        self.routes.push(Route {
            path_suffix: path_suffix.to_owned(),
            query: query
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            status,
            body: body.into(),
        });
        self
    }

    /// Every request received so far, in order.
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl HttpTransport for MockTransport {
    fn get(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        let route = self.routes.iter().find(|r| {
            request.url.path().ends_with(&r.path_suffix)
                && r.query
                    .iter()
                    .all(|(k, v)| request.query_all(k).iter().any(|x| x == v))
        });
        let result = match route {
            Some(r) => Ok(HttpResponse {
                status: r.status,
                body: r.body.clone().into_bytes(),
            }),
            None => Err(TransportError(format!("no mock route for {}", request.url))),
        };
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        Box::pin(std::future::ready(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Url;

    #[tokio::test]
    async fn matches_path_and_query_and_records() {
        let mock = MockTransport::new()
            .route("/a", &[("x", "1")], 200, "one")
            .route("/a", &[], 404, "other");
        let get = |u: &str| HttpRequest::get(Url::parse(u).unwrap()).header("Authorization", "k");

        let r = mock.get(get("http://m/v1/a?x=1")).await.unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, b"one".as_slice()));
        let r = mock.get(get("http://m/v1/a?x=2")).await.unwrap();
        assert_eq!(r.status, 404);
        assert!(mock.get(get("http://m/v1/b")).await.is_err());

        let reqs = mock.requests();
        assert_eq!(reqs.len(), 3);
        assert_eq!(reqs[0].header_value("authorization"), Some("k"));
        assert_eq!(reqs[0].query("x").as_deref(), Some("1"));
    }
}
