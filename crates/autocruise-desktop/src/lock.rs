//! Exclusive use of a desktop application across processes.
//!
//! One browser can only run one flow at a time. A connector serializes its
//! own calls with a mutex, but two autocruise processes (an MCP server and a
//! CLI call, two MCP clients) would still fight over the same window. An
//! [`AppLock`] is an advisory lock on a file shared by every process that
//! drives the same display.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::time::Instant;

use crate::DesktopError;

/// Held while a flow drives the application; released on drop (or when the
/// process exits, even on a crash).
#[derive(Debug)]
pub struct AppLock {
    _file: File,
}

impl AppLock {
    /// Default lock file for a display: `$XDG_RUNTIME_DIR` (else the temp
    /// directory) / `autocruise-desktop-<display>.lock`.
    pub fn default_path(display: Option<&str>) -> PathBuf {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let name: String = display
            .unwrap_or("default")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        dir.join(format!("autocruise-desktop-{name}.lock"))
    }

    /// Takes the lock, waiting up to `timeout` for another process to
    /// finish. Fails with [`DesktopError::Busy`] after that.
    pub async fn acquire(
        path: &Path,
        timeout: Duration,
        poll: Duration,
    ) -> Result<Self, DesktopError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(|e| DesktopError::Unavailable(format!("lock file {}: {e}", path.display())))?;
        let deadline = Instant::now() + timeout;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    tokio::time::sleep(poll).await;
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(DesktopError::Busy(
                        "another autocruise process is using the browser".into(),
                    ));
                }
                Err(TryLockError::Error(e)) => {
                    return Err(DesktopError::Protocol(format!("lock: {e}")));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn second_holder_waits_then_gets_busy() {
        let path =
            std::env::temp_dir().join(format!("autocruise-lock-test-{}", std::process::id()));
        let short = Duration::from_millis(50);
        let first = AppLock::acquire(&path, short, Duration::from_millis(10))
            .await
            .unwrap();
        let err = AppLock::acquire(&path, short, Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(matches!(err, DesktopError::Busy(_)));
        drop(first);
        AppLock::acquire(&path, short, Duration::from_millis(10))
            .await
            .unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn path_is_per_display() {
        let a = AppLock::default_path(Some(":50"));
        let b = AppLock::default_path(Some(":99"));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().ends_with("autocruise-desktop-_50.lock"));
    }
}
