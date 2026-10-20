//! Records what a desktop showed and received, to write test fixtures.
//!
//! Development tool only: recorded trees contain whatever was on screen.
//! Sanitize them (names, e-mail addresses, card numbers) before committing.

use std::sync::{Mutex, PoisonError};

use crate::{BoxFuture, Desktop, DesktopError, Input, Node};

/// One step of a recorded session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Tree(Node),
    Input(Input),
}

/// Wraps a desktop and logs trees (consecutive duplicates dropped) and inputs.
#[derive(Debug)]
pub struct RecordingDesktop<D> {
    inner: D,
    log: Mutex<Vec<Event>>,
}

impl<D: Desktop> RecordingDesktop<D> {
    pub fn new(inner: D) -> Self {
        Self {
            inner,
            log: Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<Event> {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn push(&self, event: Event) {
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        if let Event::Tree(t) = &event
            && let Some(Event::Tree(last)) = log.iter().rev().find(|e| matches!(e, Event::Tree(_)))
            && last == t
            && matches!(log.last(), Some(Event::Tree(_)))
        {
            return;
        }
        log.push(event);
    }
}

impl<D: Desktop> Desktop for RecordingDesktop<D> {
    fn applications(&self) -> BoxFuture<'_, Result<Vec<String>, DesktopError>> {
        self.inner.applications()
    }

    fn tree<'a>(&'a self, app: &'a str) -> BoxFuture<'a, Result<Node, DesktopError>> {
        Box::pin(async move {
            let tree = self.inner.tree(app).await?;
            self.push(Event::Tree(tree.clone()));
            Ok(tree)
        })
    }

    fn input(&self, input: Input) -> BoxFuture<'_, Result<(), DesktopError>> {
        Box::pin(async move {
            self.push(Event::Input(input.clone()));
            self.inner.input(input).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockDesktop;

    #[tokio::test]
    async fn logs_without_consecutive_duplicates() {
        let rec = RecordingDesktop::new(MockDesktop::new(vec![Node::new("application", "Chrome")]));
        rec.tree("chrome").await.unwrap();
        rec.tree("chrome").await.unwrap();
        rec.input(Input::Key("Return".into())).await.unwrap();
        rec.tree("chrome").await.unwrap();
        let events = rec.events();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[1], Event::Input(_)));
    }
}
