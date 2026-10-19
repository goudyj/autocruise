//! [`Device`]: high-level actions on the user's Android device.

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::throttle::Throttle;
use tokio::time::Instant;

use crate::adb::{AdbTransport, CommandAdb, sh_quote};
use crate::error::AndroidError;
use crate::ui::{By, Point, UiNode, UiTree};

/// Where uiautomator writes its dump on the device.
const DUMP_PATH: &str = "/data/local/tmp/autocruise_ui.xml";

/// Hardware/system keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Home,
    Back,
    Enter,
    Delete,
    Tab,
}

impl Key {
    fn code(self) -> u8 {
        match self {
            Self::Home => 3,
            Self::Back => 4,
            Self::Tab => 61,
            Self::Enter => 66,
            Self::Delete => 67,
        }
    }
}

/// One Android device. Input actions go through a throttle so the app is
/// driven at a human pace; reading the screen is not throttled.
#[derive(Debug)]
pub struct Device {
    adb: Arc<dyn AdbTransport>,
    throttle: Throttle,
    poll_interval: Duration,
}

impl Device {
    pub fn new(adb: Arc<dyn AdbTransport>) -> Self {
        Self {
            adb,
            throttle: Throttle::new(Duration::from_millis(600)),
            poll_interval: Duration::from_millis(500),
        }
    }

    /// Device configured from the environment (see [`CommandAdb::from_env`]).
    pub fn from_env() -> Self {
        Self::new(Arc::new(CommandAdb::from_env()))
    }

    /// Minimum delay between two input actions (default 600 ms).
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.throttle = Throttle::new(interval);
        self
    }

    /// Delay between two screen reads while waiting (default 500 ms).
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Runs a shell command line on the device and returns its output.
    async fn shell(&self, command: String) -> Result<String, AndroidError> {
        let out = self.adb.run(vec!["shell".into(), command]).await?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// Fails unless the device is reachable and fully booted.
    pub async fn ensure_ready(&self) -> Result<(), AndroidError> {
        let booted = self.shell("getprop sys.boot_completed".into()).await?;
        if booted.trim() == "1" {
            Ok(())
        } else {
            Err(AndroidError::NotBooted)
        }
    }

    pub async fn is_installed(&self, package: &str) -> Result<bool, AndroidError> {
        // `pm path` exits non-zero for unknown packages: use the output only.
        let out = self
            .shell(format!("pm path {} 2>/dev/null; true", sh_quote(package)))
            .await?;
        Ok(out.lines().any(|l| l.starts_with("package:")))
    }

    /// Brings the app to the foreground (starting it if needed).
    pub async fn launch(&self, package: &str) -> Result<(), AndroidError> {
        if !self.is_installed(package).await? {
            return Err(AndroidError::AppNotInstalled(package.to_owned()));
        }
        let activity = self
            .shell(format!(
                "cmd package resolve-activity --brief -c android.intent.category.LAUNCHER {} | tail -n 1",
                sh_quote(package)
            ))
            .await?;
        let activity = activity.trim();
        if !activity.contains('/') {
            return Err(AndroidError::AppNotInstalled(package.to_owned()));
        }
        self.throttle.wait().await;
        self.shell(format!("am start -n {}", sh_quote(activity)))
            .await?;
        Ok(())
    }

    /// The current screen.
    pub async fn ui(&self) -> Result<UiTree, AndroidError> {
        let status = self.shell(format!("uiautomator dump {DUMP_PATH}")).await?;
        if !status.contains("dumped to") {
            return Err(AndroidError::UiDump(status.trim().to_owned()));
        }
        let xml = self
            .adb
            .run(vec!["exec-out".into(), "cat".into(), DUMP_PATH.into()])
            .await?;
        UiTree::parse(&String::from_utf8_lossy(&xml)).map_err(AndroidError::UiDump)
    }

    /// Waits until a node matching `by` is on screen; returns that screen.
    pub async fn wait_for(&self, by: &By, timeout: Duration) -> Result<UiTree, AndroidError> {
        self.wait_until(&by.to_string(), timeout, |tree| tree.contains(by))
            .await
    }

    /// Waits until `done` accepts the screen; returns that screen.
    pub async fn wait_until(
        &self,
        what: &str,
        timeout: Duration,
        mut done: impl FnMut(&UiTree) -> bool,
    ) -> Result<UiTree, AndroidError> {
        let deadline = Instant::now() + timeout;
        loop {
            let tree = self.ui().await?;
            if done(&tree) {
                return Ok(tree);
            }
            if Instant::now() >= deadline {
                return Err(AndroidError::Timeout {
                    what: what.to_owned(),
                    after: timeout,
                });
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    pub async fn tap(&self, node: &UiNode) -> Result<(), AndroidError> {
        self.tap_at(node.bounds.center()).await
    }

    pub async fn tap_at(&self, p: Point) -> Result<(), AndroidError> {
        self.throttle.wait().await;
        self.shell(format!("input tap {} {}", p.x, p.y)).await?;
        Ok(())
    }

    /// Types ASCII text into the focused field.
    pub async fn type_text(&self, text: &str) -> Result<(), AndroidError> {
        if text.is_empty() {
            return Ok(());
        }
        if !text.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(AndroidError::InvalidText(
                "only printable ASCII text can be typed on the device".into(),
            ));
        }
        // `input text` turns "%s" into a space and has no other escape.
        let encoded = text.replace(' ', "%s");
        self.throttle.wait().await;
        self.shell(format!("input text {}", sh_quote(&encoded)))
            .await?;
        Ok(())
    }

    pub async fn press(&self, key: Key) -> Result<(), AndroidError> {
        self.throttle.wait().await;
        self.shell(format!("input keyevent {}", key.code())).await?;
        Ok(())
    }

    pub async fn swipe(
        &self,
        from: Point,
        to: Point,
        duration: Duration,
    ) -> Result<(), AndroidError> {
        self.throttle.wait().await;
        self.shell(format!(
            "input swipe {} {} {} {} {}",
            from.x,
            from.y,
            to.x,
            to.y,
            duration.as_millis()
        ))
        .await?;
        Ok(())
    }

    /// Scrolls a scrollable node's content by about half its height.
    pub async fn scroll_down(&self, node: &UiNode) -> Result<(), AndroidError> {
        let b = node.bounds;
        let x = b.center().x;
        let from = Point {
            x,
            y: b.bottom - b.height() / 4,
        };
        let to = Point {
            x,
            y: b.top + b.height() / 4,
        };
        self.swipe(from, to, Duration::from_millis(400)).await
    }

    /// PNG screenshot of the screen.
    pub async fn screenshot(&self) -> Result<Vec<u8>, AndroidError> {
        Ok(self
            .adb
            .run(vec!["exec-out".into(), "screencap".into(), "-p".into()])
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AdbError;
    use crate::mock::MockAdb;

    const LAUNCHER: &str = include_str!("../tests/fixtures/launcher.xml");
    const DUMPED: &str = "UI hierchary dumped to: /data/local/tmp/autocruise_ui.xml\n";

    fn device(mock: MockAdb) -> (Device, Arc<MockAdb>) {
        let mock = Arc::new(mock);
        let device = Device::new(mock.clone())
            .with_min_interval(Duration::ZERO)
            .with_poll_interval(Duration::from_millis(100));
        (device, mock)
    }

    #[tokio::test]
    async fn readiness() {
        let (d, _) = device(MockAdb::new().reply("shell getprop sys.boot_completed", "1\n"));
        d.ensure_ready().await.unwrap();
        let (d, _) = device(MockAdb::new().reply("shell getprop sys.boot_completed", "\n"));
        assert!(matches!(
            d.ensure_ready().await,
            Err(AndroidError::NotBooted)
        ));
        let (d, _) = device(MockAdb::new().fail(
            "shell",
            AdbError::DeviceUnavailable {
                serial: "s".into(),
                detail: "offline".into(),
            },
        ));
        assert!(matches!(d.ensure_ready().await, Err(AndroidError::Adb(_))));
    }

    #[tokio::test]
    async fn launch_resolves_the_launcher_activity() {
        let (d, mock) = device(
            MockAdb::new()
                .reply("shell pm path com.app.tgtg", "package:/data/app/base.apk\n")
                .reply(
                    "shell cmd package resolve-activity",
                    "com.app.tgtg/com.app.tgtg.Main\n",
                )
                .reply("shell am start", "Starting: Intent\n"),
        );
        d.launch("com.app.tgtg").await.unwrap();
        assert_eq!(
            mock.calls().last().unwrap(),
            "shell am start -n com.app.tgtg/com.app.tgtg.Main"
        );
    }

    #[tokio::test]
    async fn launch_requires_the_app() {
        let (d, _) = device(MockAdb::new().reply("shell pm path", ""));
        assert!(matches!(
            d.launch("com.app.tgtg").await,
            Err(AndroidError::AppNotInstalled(_))
        ));
    }

    #[tokio::test]
    async fn reads_the_screen_and_taps_a_node() {
        let (d, mock) = device(
            MockAdb::new()
                .reply("shell uiautomator dump", DUMPED)
                .reply("exec-out cat", LAUNCHER)
                .reply("shell input", ""),
        );
        let tree = d.ui().await.unwrap();
        d.tap(tree.find(&By::desc("Gallery")).unwrap())
            .await
            .unwrap();
        assert_eq!(mock.calls().last().unwrap(), "shell input tap 274 1032");
    }

    #[tokio::test]
    async fn dump_failures_are_reported() {
        let (d, _) = device(MockAdb::new().reply(
            "shell uiautomator dump",
            "ERROR: could not get idle state.\n",
        ));
        assert!(matches!(d.ui().await, Err(AndroidError::UiDump(_))));
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_a_node() {
        let (d, _) = device(
            MockAdb::new()
                .reply("shell uiautomator dump", DUMPED)
                .reply("exec-out cat", "<hierarchy rotation=\"0\"></hierarchy>")
                .reply("exec-out cat", LAUNCHER),
        );
        let tree = d
            .wait_for(&By::desc("Gallery"), Duration::from_secs(5))
            .await
            .unwrap();
        assert!(tree.contains(&By::desc("Gallery")));
        let err = d
            .wait_for(&By::text("Nope"), Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(matches!(err, AndroidError::Timeout { .. }));
    }

    #[tokio::test]
    async fn types_escaped_text() {
        let (d, mock) = device(MockAdb::new().reply("shell input", ""));
        d.type_text("it's 100% me").await.unwrap();
        assert_eq!(
            mock.calls().last().unwrap(),
            "shell input text 'it'\\''s%s100%%sme'"
        );
        assert!(matches!(
            d.type_text("café").await,
            Err(AndroidError::InvalidText(_))
        ));
    }
}
