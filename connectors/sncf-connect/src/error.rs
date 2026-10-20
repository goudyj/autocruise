use autocruise_core::error::{Classify, ErrorKind};
use autocruise_core::time::TimeError;
use autocruise_desktop::DesktopError;

#[derive(Debug, thiserror::Error)]
pub enum SncfConnectError {
    #[error(transparent)]
    Desktop(#[from] DesktopError),
    #[error(transparent)]
    Time(#[from] TimeError),
    #[error("{0}")]
    Invalid(String),
    /// A bot check or block page is shown. Never interacted with.
    #[error(
        "SNCF Connect is showing a security check or block page ({0}). Please open the browser on the desktop and complete or wait it out yourself, then ask again. autocruise never interacts with these checks."
    )]
    Challenge(String),
    #[error(
        "you are not logged in to SNCF Connect in the browser. Please log in yourself on sncf-connect.com in the desktop browser, then ask again."
    )]
    LoggedOut,
    #[error("SNCF Connect does not know {0:?}; try the full station or city name")]
    UnknownPlace(String),
    #[error("SNCF Connect showed an unexpected page: {0}")]
    UnexpectedPage(String),
    #[error("{0}")]
    JourneyNotFound(String),
    /// SNCF Connect refuses a free option for this order.
    #[error(
        "SNCF Connect does not offer a free option for this order ({0}). The trip was removed from the basket again; nothing was booked or paid."
    )]
    HoldNotOffered(String),
    /// Safety net: autocruise never presses anything that pays.
    #[error("refusing to press {0:?}: autocruise never pays")]
    PaymentRefused(String),
}

impl Classify for SncfConnectError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Desktop(e) => e.kind(),
            Self::Time(_) | Self::Invalid(_) => ErrorKind::InvalidInput,
            Self::Challenge(_) | Self::LoggedOut => ErrorKind::UserActionRequired,
            Self::UnknownPlace(_) => ErrorKind::NotFound,
            Self::UnexpectedPage(_) => ErrorKind::Upstream,
            Self::JourneyNotFound(_) => ErrorKind::NotFound,
            Self::HoldNotOffered(_) => ErrorKind::InvalidInput,
            Self::PaymentRefused(_) => ErrorKind::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(
            SncfConnectError::Challenge("x".into()).kind(),
            ErrorKind::UserActionRequired
        );
        assert_eq!(
            SncfConnectError::LoggedOut.kind(),
            ErrorKind::UserActionRequired
        );
        assert_eq!(
            SncfConnectError::Desktop(DesktopError::Timeout("x".into())).kind(),
            ErrorKind::Upstream
        );
        assert_eq!(
            SncfConnectError::UnknownPlace("x".into()).kind(),
            ErrorKind::NotFound
        );
    }
}
