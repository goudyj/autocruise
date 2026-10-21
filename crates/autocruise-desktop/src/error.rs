use autocruise_core::error::{Classify, ErrorKind};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesktopError {
    /// The display or the accessibility bus cannot be reached.
    #[error(
        "cannot reach the desktop ({0}). Start the browser on a running display as described in the connector documentation, and check AUTOCRUISE_DESKTOP_DISPLAY / AUTOCRUISE_DESKTOP_DBUS."
    )]
    Unavailable(String),
    /// The application is not running (or does not expose accessibility).
    #[error(
        "no application matching {0:?} is visible to accessibility. Start it on the desktop (Chrome needs --force-renderer-accessibility) and try again."
    )]
    AppNotFound(String),
    /// Something did not appear in time.
    #[error("timed out waiting for {0}")]
    Timeout(String),
    /// The node cannot be clicked (no on-screen bounds).
    #[error("{0} has no clickable area on screen")]
    NotClickable(String),
    /// The key or character cannot be produced.
    #[error("cannot type {0:?}")]
    UnsupportedKey(String),
    /// Another process is driving the same application.
    #[error("{0}; try again in a minute")]
    Busy(String),
    /// An unexpected protocol failure (D-Bus, X11).
    #[error("desktop protocol error: {0}")]
    Protocol(String),
}

impl Classify for DesktopError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Unavailable(_) | Self::AppNotFound(_) => ErrorKind::UserActionRequired,
            Self::Timeout(_) | Self::NotClickable(_) => ErrorKind::Upstream,
            Self::UnsupportedKey(_) => ErrorKind::InvalidInput,
            Self::Busy(_) => ErrorKind::RateLimited,
            Self::Protocol(_) => ErrorKind::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(
            DesktopError::AppNotFound("chrome".into()).kind(),
            ErrorKind::UserActionRequired
        );
        assert_eq!(
            DesktopError::Timeout("results".into()).kind(),
            ErrorKind::Upstream
        );
    }
}
