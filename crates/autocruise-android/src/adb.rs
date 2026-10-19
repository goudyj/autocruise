//! adb transport: [`CommandAdb`] runs the `adb` binary, [`crate::mock::MockAdb`]
//! serves canned outputs in tests.

use std::fmt;
use std::process::Stdio;
use std::time::Duration;

use autocruise_core::http::BoxFuture;
use tokio::process::Command;

use crate::error::AdbError;

/// Default device: the redroid container from `deploy/android/compose.yaml`.
pub const DEFAULT_SERIAL: &str = "127.0.0.1:5555";

/// Runs adb commands against one device.
pub trait AdbTransport: Send + Sync + fmt::Debug {
    /// Runs `adb -s <serial> <args...>` and returns its standard output.
    fn run(&self, args: Vec<String>) -> BoxFuture<'_, Result<Vec<u8>, AdbError>>;
}

/// Production transport: spawns the `adb` binary.
#[derive(Debug, Clone)]
pub struct CommandAdb {
    binary: String,
    serial: String,
    timeout: Duration,
}

impl CommandAdb {
    pub fn new(binary: impl Into<String>, serial: impl Into<String>, timeout: Duration) -> Self {
        Self {
            binary: binary.into(),
            serial: serial.into(),
            timeout,
        }
    }

    /// `AUTOCRUISE_ADB` (default `adb`) and `AUTOCRUISE_ANDROID_SERIAL`
    /// (default [`DEFAULT_SERIAL`]).
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self::new(
            var("AUTOCRUISE_ADB").unwrap_or_else(|| "adb".into()),
            var("AUTOCRUISE_ANDROID_SERIAL").unwrap_or_else(|| DEFAULT_SERIAL.into()),
            Duration::from_secs(30),
        )
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    async fn exec(&self, args: &[String]) -> Result<Vec<u8>, AdbError> {
        let child = Command::new(&self.binary)
            .arg("-s")
            .arg(&self.serial)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AdbError::NotInstalled(e.to_string()))?;
        let output = tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| AdbError::Timeout(self.timeout))?
            .map_err(|e| AdbError::Failed(e.to_string()))?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(classify_failure(&self.serial, stderr))
    }

    /// Network devices (redroid, wireless debugging) need `adb connect` once
    /// per adb server.
    async fn connect(&self) -> Result<(), AdbError> {
        let output = tokio::time::timeout(
            self.timeout,
            Command::new(&self.binary)
                .args(["connect", &self.serial])
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| AdbError::Timeout(self.timeout))?
        .map_err(|e| AdbError::NotInstalled(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("connected to") {
            Ok(())
        } else {
            Err(AdbError::DeviceUnavailable {
                serial: self.serial.clone(),
                detail: stdout.trim().to_owned(),
            })
        }
    }

    fn is_network_serial(&self) -> bool {
        self.serial.contains(':')
    }
}

impl AdbTransport for CommandAdb {
    fn run(&self, args: Vec<String>) -> BoxFuture<'_, Result<Vec<u8>, AdbError>> {
        Box::pin(async move {
            match self.exec(&args).await {
                Err(AdbError::DeviceUnavailable { .. }) if self.is_network_serial() => {
                    self.connect().await?;
                    self.exec(&args).await
                }
                other => other,
            }
        })
    }
}

fn classify_failure(serial: &str, stderr: String) -> AdbError {
    const UNAVAILABLE: [&str; 5] = [
        "not found",
        "offline",
        "no devices",
        "unauthorized",
        "failed to connect",
    ];
    if UNAVAILABLE.iter().any(|m| stderr.contains(m)) {
        AdbError::DeviceUnavailable {
            serial: serial.to_owned(),
            detail: stderr,
        }
    } else {
        AdbError::Failed(stderr)
    }
}

/// Quotes one argument for the device shell (`adb shell` joins arguments
/// into a single `sh` command line).
pub fn sh_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@,+%".contains(c))
    {
        return arg.to_owned();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("com.app.x/.Main"), "com.app.x/.Main");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        assert_eq!(sh_quote(""), "''");
    }

    #[test]
    fn failure_classification() {
        let e = classify_failure("s", "error: device 's' not found".into());
        assert!(matches!(e, AdbError::DeviceUnavailable { .. }));
        let e = classify_failure("s", "Error: unknown command".into());
        assert_eq!(e, AdbError::Failed("Error: unknown command".into()));
    }

    #[tokio::test]
    async fn missing_binary() {
        let adb = CommandAdb::new("/nonexistent/adb", "s", Duration::from_secs(1));
        let err = adb
            .run(vec!["shell".into(), "true".into()])
            .await
            .unwrap_err();
        assert!(matches!(err, AdbError::NotInstalled(_)));
    }
}
