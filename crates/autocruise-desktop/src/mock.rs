//! In-memory desktop for tests: serves recorded accessibility trees and
//! records inputs.
//!
//! A mock holds a list of *screens* (application trees, typically recorded
//! fixtures) and *transitions*: when an input matches a trigger, the current
//! screen changes. Unmatched inputs are recorded and change nothing.

use std::sync::{Mutex, PoisonError};

use crate::{BoxFuture, Desktop, DesktopError, Input, Node};

/// What makes a transition fire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// A click inside a node whose name contains this text.
    Click(String),
    /// A key combination, exactly.
    Key(String),
    /// Typed text containing this text.
    Type(String),
}

#[derive(Debug, Clone)]
struct Transition {
    from: Option<usize>,
    trigger: Trigger,
    to: usize,
}

#[derive(Debug, Default)]
struct State {
    current: usize,
    /// Inputs with the screen shown when each arrived.
    inputs: Vec<(usize, Input)>,
    snapshots: usize,
}

#[derive(Debug, Default)]
pub struct MockDesktop {
    screens: Vec<Node>,
    transitions: Vec<Transition>,
    state: Mutex<State>,
}

impl MockDesktop {
    /// Starts on the first screen.
    pub fn new(screens: Vec<Node>) -> Self {
        Self {
            screens,
            ..Self::default()
        }
    }

    /// Parses screens from JSON fixtures (as written by the `dump` example).
    pub fn from_json(screens: &[&str]) -> Self {
        Self::new(
            screens
                .iter()
                .map(|s| serde_json::from_str(s).expect("valid accessibility fixture"))
                .collect(),
        )
    }

    /// From any screen, go to `to` when `trigger` happens.
    pub fn transition(mut self, from: Option<usize>, trigger: Trigger, to: usize) -> Self {
        self.transitions.push(Transition { from, trigger, to });
        self
    }

    pub fn on_click(self, name_contains: &str, to: usize) -> Self {
        self.transition(None, Trigger::Click(name_contains.to_owned()), to)
    }

    pub fn on_key(self, combo: &str, to: usize) -> Self {
        self.transition(None, Trigger::Key(combo.to_owned()), to)
    }

    pub fn on_type(self, text_contains: &str, to: usize) -> Self {
        self.transition(None, Trigger::Type(text_contains.to_owned()), to)
    }

    /// Every input received so far, in order.
    pub fn inputs(&self) -> Vec<Input> {
        self.lock().inputs.iter().map(|(_, i)| i.clone()).collect()
    }

    /// Every input with the index of the screen it was sent to.
    pub fn history(&self) -> Vec<(usize, Input)> {
        self.lock().inputs.clone()
    }

    /// Name of the smallest named node under each click, on the screen that
    /// received it: what was actually pressed.
    pub fn clicked(&self) -> Vec<String> {
        self.history()
            .into_iter()
            .filter_map(|(screen, input)| match input {
                Input::Click { x, y } => Some((screen, x, y)),
                _ => None,
            })
            .map(|(screen, x, y)| {
                self.screens
                    .get(screen)
                    .and_then(|tree| {
                        tree.iter()
                            .filter(|n| {
                                !n.name.is_empty() && n.bounds.is_some_and(|b| b.contains(x, y))
                            })
                            .min_by_key(|n| {
                                n.bounds.map(|b| i64::from(b.width) * i64::from(b.height))
                            })
                    })
                    .map(|n| n.name.clone())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Index of the current screen.
    pub fn current(&self) -> usize {
        self.lock().current
    }

    /// Number of trees served.
    pub fn snapshots(&self) -> usize {
        self.lock().snapshots
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn matches(&self, screen: usize, trigger: &Trigger, input: &Input) -> bool {
        match (trigger, input) {
            (Trigger::Key(k), Input::Key(i)) => k == i,
            (Trigger::Type(t), Input::Type(i)) => i.contains(t.as_str()),
            (Trigger::Click(name), Input::Click { x, y }) => {
                self.screens.get(screen).is_some_and(|tree| {
                    tree.iter().any(|n| {
                        n.name.contains(name.as_str())
                            && n.bounds.is_some_and(|b| b.contains(*x, *y))
                    })
                })
            }
            _ => false,
        }
    }
}

impl Desktop for MockDesktop {
    fn applications(&self) -> BoxFuture<'_, Result<Vec<String>, DesktopError>> {
        let names = self
            .screens
            .first()
            .map(|s| s.name.clone())
            .into_iter()
            .collect();
        Box::pin(std::future::ready(Ok(names)))
    }

    fn tree<'a>(&'a self, app: &'a str) -> BoxFuture<'a, Result<Node, DesktopError>> {
        let mut state = self.lock();
        state.snapshots += 1;
        let result = match self.screens.get(state.current) {
            Some(tree) if tree.name.to_lowercase().contains(&app.to_lowercase()) => {
                Ok(tree.clone())
            }
            _ => Err(DesktopError::AppNotFound(app.to_owned())),
        };
        Box::pin(std::future::ready(result))
    }

    fn input(&self, input: Input) -> BoxFuture<'_, Result<(), DesktopError>> {
        let mut state = self.lock();
        let current = state.current;
        if let Some(t) = self.transitions.iter().find(|t| {
            t.from.is_none_or(|f| f == current) && self.matches(current, &t.trigger, &input)
        }) {
            state.current = t.to;
        }
        state.inputs.push((current, input));
        Box::pin(std::future::ready(Ok(())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rect;

    fn app(title: &str) -> Node {
        Node::new("application", "Google Chrome").with_children(vec![
            Node::new("button", "Next").with_bounds(Rect::new(0, 0, 10, 10)),
            Node::new("document web", title),
        ])
    }

    #[tokio::test]
    async fn transitions_follow_inputs() {
        let mock = MockDesktop::new(vec![app("one"), app("two"), app("three")])
            .on_click("Next", 1)
            .transition(Some(1), Trigger::Key("Return".into()), 2);
        assert!(mock.tree("chrome").await.unwrap().contains_text("one"));
        mock.input(Input::Key("Return".into())).await.unwrap();
        assert_eq!(mock.current(), 0, "Return only fires from screen 1");
        mock.input(Input::Click { x: 50, y: 50 }).await.unwrap();
        assert_eq!(mock.current(), 0, "click outside the button");
        mock.input(Input::Click { x: 5, y: 5 }).await.unwrap();
        assert_eq!(mock.current(), 1);
        mock.input(Input::Key("Return".into())).await.unwrap();
        assert_eq!(mock.current(), 2);
        assert_eq!(mock.inputs().len(), 4);
    }

    #[tokio::test]
    async fn unknown_app() {
        let mock = MockDesktop::new(vec![app("one")]);
        assert_eq!(
            mock.tree("firefox").await.unwrap_err(),
            DesktopError::AppNotFound("firefox".into())
        );
    }
}
