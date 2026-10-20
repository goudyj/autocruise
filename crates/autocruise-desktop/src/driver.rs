//! Human-paced driving on top of a [`Desktop`].

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::throttle::Throttle;
use tokio::time::Instant;

use crate::{Desktop, DesktopError, Input, Node};

/// Outcome of one check while waiting for a page.
#[derive(Debug)]
pub enum Check<T, E> {
    Ready(T),
    Fail(E),
    Pending,
}

/// Drives one application: every input goes through a [`Throttle`], so the
/// pace stays human whatever the caller does.
#[derive(Debug)]
pub struct Driver {
    desktop: Arc<dyn Desktop>,
    app: String,
    throttle: Throttle,
    poll: Duration,
}

impl Driver {
    /// `app`: substring of the application name (e.g. `"chrome"`).
    /// `min_interval`: minimum delay between two inputs.
    pub fn new(desktop: Arc<dyn Desktop>, app: &str, min_interval: Duration) -> Self {
        Self {
            desktop,
            app: app.to_owned(),
            throttle: Throttle::new(min_interval),
            poll: Duration::from_millis(700),
        }
    }

    /// Delay between two snapshots while waiting.
    pub fn with_poll_interval(mut self, poll: Duration) -> Self {
        self.poll = poll;
        self
    }

    pub fn app(&self) -> &str {
        &self.app
    }

    /// Current accessibility tree of the application.
    pub async fn snapshot(&self) -> Result<Node, DesktopError> {
        self.desktop.tree(&self.app).await
    }

    /// Polls the tree until `check` is ready or fails, or `timeout` elapses
    /// (then fails with [`DesktopError::Timeout`] about `what`).
    pub async fn wait_until<T, E>(
        &self,
        what: &str,
        timeout: Duration,
        mut check: impl FnMut(&Node) -> Check<T, E>,
    ) -> Result<T, E>
    where
        E: From<DesktopError>,
    {
        let start = Instant::now();
        loop {
            let tree = self.snapshot().await?;
            match check(&tree) {
                Check::Ready(value) => return Ok(value),
                Check::Fail(error) => return Err(error),
                Check::Pending => {}
            }
            if start.elapsed() >= timeout {
                return Err(DesktopError::Timeout(what.to_owned()).into());
            }
            tokio::time::sleep(self.poll).await;
        }
    }

    /// Like [`Driver::wait_until`], for a node about to be clicked: returns it
    /// only once its bounds are the same on two consecutive reads, so menus
    /// and lists that are still animating or refreshing are not clicked.
    pub async fn wait_until_stable<E>(
        &self,
        what: &str,
        timeout: Duration,
        mut find: impl FnMut(&Node) -> Check<Node, E>,
    ) -> Result<Node, E>
    where
        E: From<DesktopError>,
    {
        let mut previous: Option<Node> = None;
        let start = Instant::now();
        loop {
            let tree = self.snapshot().await?;
            match find(&tree) {
                Check::Ready(node) => {
                    if previous
                        .as_ref()
                        .is_some_and(|p| p.bounds == node.bounds && p.name == node.name)
                    {
                        return Ok(node);
                    }
                    previous = Some(node);
                }
                Check::Fail(error) => return Err(error),
                Check::Pending => previous = None,
            }
            if start.elapsed() >= timeout {
                return Err(DesktopError::Timeout(what.to_owned()).into());
            }
            tokio::time::sleep(self.poll / 2).await;
        }
    }

    /// Clicks the centre of a node.
    pub async fn click(&self, node: &Node) -> Result<(), DesktopError> {
        let (x, y) = node
            .center()
            .ok_or_else(|| DesktopError::NotClickable(format!("{} {:?}", node.role, node.name)))?;
        self.click_at(x, y).await
    }

    pub async fn click_at(&self, x: i32, y: i32) -> Result<(), DesktopError> {
        self.send(Input::Click { x, y }).await
    }

    pub async fn type_text(&self, text: &str) -> Result<(), DesktopError> {
        self.send(Input::Type(text.to_owned())).await
    }

    pub async fn key(&self, combo: &str) -> Result<(), DesktopError> {
        self.send(Input::Key(combo.to_owned())).await
    }

    /// Wheel at a point: positive `notches` scroll down.
    pub async fn scroll(&self, x: i32, y: i32, notches: i32) -> Result<(), DesktopError> {
        self.send(Input::Scroll { x, y, notches }).await
    }

    /// Waits without acting (e.g. to let a page settle).
    pub async fn pause(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    async fn send(&self, input: Input) -> Result<(), DesktopError> {
        self.throttle.wait().await;
        self.desktop.input(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rect;
    use crate::mock::MockDesktop;

    fn screen(title: &str) -> Node {
        Node::new("application", "Chrome").with_children(vec![
            Node::new("document web", title).with_children(vec![
                Node::new("button", "Go").with_bounds(Rect::new(0, 0, 10, 10)),
            ]),
        ])
    }

    #[tokio::test(start_paused = true)]
    async fn inputs_are_paced_and_recorded() {
        let mock = Arc::new(MockDesktop::new(vec![screen("A")]));
        let driver = Driver::new(mock.clone(), "chrome", Duration::from_millis(500));
        let start = Instant::now();
        driver.key("ctrl+l").await.unwrap();
        driver.type_text("hello").await.unwrap();
        driver.click_at(5, 5).await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(1000));
        assert_eq!(
            mock.inputs(),
            [
                Input::Key("ctrl+l".into()),
                Input::Type("hello".into()),
                Input::Click { x: 5, y: 5 }
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_a_transition() {
        let mock = Arc::new(MockDesktop::new(vec![screen("A"), screen("B")]).on_click("Go", 1));
        let driver = Driver::new(mock.clone(), "chrome", Duration::ZERO);
        let tree = driver.snapshot().await.unwrap();
        driver
            .click(tree.find_role("button", |n| n == "Go").unwrap())
            .await
            .unwrap();
        let title = driver
            .wait_until("page B", Duration::from_secs(5), |t| {
                match t.find_role("document web", |n| n == "B") {
                    Some(n) => Check::<_, DesktopError>::Ready(n.name.clone()),
                    None => Check::Pending,
                }
            })
            .await
            .unwrap();
        assert_eq!(title, "B");
    }

    #[tokio::test(start_paused = true)]
    async fn stable_needs_two_identical_reads() {
        let mock = Arc::new(MockDesktop::new(vec![screen("A")]));
        let driver = Driver::new(mock.clone(), "chrome", Duration::ZERO);
        let node = driver
            .wait_until_stable("Go", Duration::from_secs(5), |t| {
                t.find_role("button", |n| n == "Go")
                    .map_or(Check::<_, DesktopError>::Pending, |n| {
                        Check::Ready(n.clone())
                    })
            })
            .await
            .unwrap();
        assert_eq!(node.name, "Go");
        assert_eq!(mock.snapshots(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn times_out() {
        let mock = Arc::new(MockDesktop::new(vec![screen("A")]));
        let driver = Driver::new(mock, "chrome", Duration::ZERO);
        let err = driver
            .wait_until("never", Duration::from_secs(3), |_| {
                Check::<(), DesktopError>::Pending
            })
            .await
            .unwrap_err();
        assert_eq!(err, DesktopError::Timeout("never".into()));
    }
}
