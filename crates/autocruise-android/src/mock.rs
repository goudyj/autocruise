//! In-memory adb for tests: serves canned outputs and records commands.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use autocruise_core::http::BoxFuture;

use crate::adb::AdbTransport;
use crate::error::AdbError;

#[derive(Debug)]
struct Route {
    prefix: String,
    replies: VecDeque<Result<Vec<u8>, AdbError>>,
}

/// Routes match when the space-joined command starts with their prefix; the
/// first matching route wins. A route registered several times replies in
/// order and then keeps repeating its last reply (handy for screens that
/// change while polling). Unmatched commands fail with [`AdbError::Failed`].
#[derive(Debug, Default)]
pub struct MockAdb {
    routes: Mutex<Vec<Route>>,
    calls: Mutex<Vec<String>>,
}

impl MockAdb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reply(self, prefix: &str, stdout: impl Into<Vec<u8>>) -> Self {
        self.push(prefix, Ok(stdout.into()))
    }

    pub fn fail(self, prefix: &str, error: AdbError) -> Self {
        self.push(prefix, Err(error))
    }

    fn push(self, prefix: &str, reply: Result<Vec<u8>, AdbError>) -> Self {
        {
            let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
            match routes.iter_mut().find(|r| r.prefix == prefix) {
                Some(route) => route.replies.push_back(reply),
                None => routes.push(Route {
                    prefix: prefix.to_owned(),
                    replies: VecDeque::from([reply]),
                }),
            }
        }
        self
    }

    /// Every command received so far (arguments joined by spaces), in order.
    pub fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl AdbTransport for MockAdb {
    fn run(&self, args: Vec<String>) -> BoxFuture<'_, Result<Vec<u8>, AdbError>> {
        let command = args.join(" ");
        let reply = {
            let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
            match routes.iter_mut().find(|r| command.starts_with(&r.prefix)) {
                Some(route) if route.replies.len() > 1 => route.replies.pop_front().unwrap(),
                Some(route) => route.replies[0].clone(),
                None => Err(AdbError::Failed(format!("no mock reply for {command:?}"))),
            }
        };
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(command);
        Box::pin(std::future::ready(reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Vec<String> {
        s.split(' ').map(String::from).collect()
    }

    #[tokio::test]
    async fn replies_in_order_then_repeats() {
        let mock = MockAdb::new().reply("shell a", "1").reply("shell a", "2");
        assert_eq!(mock.run(cmd("shell a x")).await.unwrap(), b"1");
        assert_eq!(mock.run(cmd("shell a")).await.unwrap(), b"2");
        assert_eq!(mock.run(cmd("shell a")).await.unwrap(), b"2");
        assert!(mock.run(cmd("shell b")).await.is_err());
        assert_eq!(mock.calls(), ["shell a x", "shell a", "shell a", "shell b"]);
    }
}
