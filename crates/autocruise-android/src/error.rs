use std::time::Duration;

use autocruise_core::error::{Classify, ErrorKind};

/// The adb command itself failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdbError {
    #[error("adb could not be started ({0}); install Android platform tools or set AUTOCRUISE_ADB")]
    NotInstalled(String),
    #[error(
        "the Android device {serial} is not reachable ({detail}); is it running? See docs/ANDROID.md"
    )]
    DeviceUnavailable { serial: String, detail: String },
    #[error("adb command timed out after {0:?}")]
    Timeout(Duration),
    #[error("adb command failed: {0}")]
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum AndroidError {
    #[error(transparent)]
    Adb(#[from] AdbError),
    #[error("the Android device is still booting; try again in a moment")]
    NotBooted,
    #[error(
        "the app {0} is not installed on the Android device; install it and log in first (see docs/ANDROID.md)"
    )]
    AppNotInstalled(String),
    #[error("could not read the app screen: {0}")]
    UiDump(String),
    #[error("timed out after {after:?} waiting for {what}")]
    Timeout { what: String, after: Duration },
    #[error("{0}")]
    InvalidText(String),
}

impl Classify for AdbError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::NotInstalled(_) => ErrorKind::Internal,
            Self::DeviceUnavailable { .. } | Self::Timeout(_) => ErrorKind::Network,
            Self::Failed(_) => ErrorKind::Upstream,
        }
    }
}

impl Classify for AndroidError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Adb(e) => e.kind(),
            Self::NotBooted => ErrorKind::Network,
            Self::AppNotInstalled(_) => ErrorKind::UserActionRequired,
            Self::UiDump(_) | Self::Timeout { .. } => ErrorKind::Upstream,
            Self::InvalidText(_) => ErrorKind::InvalidInput,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        let unavailable = AdbError::DeviceUnavailable {
            serial: "s".into(),
            detail: "offline".into(),
        };
        assert_eq!(AndroidError::from(unavailable).kind(), ErrorKind::Network);
        assert_eq!(
            AndroidError::AppNotInstalled("x".into()).kind(),
            ErrorKind::UserActionRequired
        );
        assert_eq!(
            AdbError::NotInstalled("x".into()).kind(),
            ErrorKind::Internal
        );
    }
}
