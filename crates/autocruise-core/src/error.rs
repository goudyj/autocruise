//! Error categories shared by all connectors.
//!
//! Connectors keep their own detailed error types and classify them with
//! [`Classify`]. The MCP layer reports the category to the assistant, so it
//! can react (ask the user for a key, fix its input, retry later...).

/// What kind of failure happened, from the assistant's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// A required credential (API key...) is not configured.
    MissingCredentials,
    /// The service rejected the credentials.
    Unauthorized,
    /// Too many requests; retry later.
    RateLimited,
    /// The tool input is invalid (bad date, bad id, impossible request...).
    InvalidInput,
    /// Nothing matches the input (unknown station...).
    NotFound,
    /// The service answered with an error or an unexpected response.
    Upstream,
    /// The service could not be reached.
    Network,
    /// The user must act before retrying: log in, install the app, pass a
    /// verification (CAPTCHA...) in their own browser or app. autocruise never
    /// does these on the user's behalf.
    UserActionRequired,
    /// A bug or misconfiguration in autocruise itself.
    Internal,
}

impl ErrorKind {
    /// Stable `snake_case` identifier, part of the tool error contract.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingCredentials => "missing_credentials",
            Self::Unauthorized => "unauthorized",
            Self::RateLimited => "rate_limited",
            Self::InvalidInput => "invalid_input",
            Self::NotFound => "not_found",
            Self::Upstream => "upstream",
            Self::Network => "network",
            Self::UserActionRequired => "user_action_required",
            Self::Internal => "internal",
        }
    }
}

/// Implemented by every connector error type.
pub trait Classify {
    fn kind(&self) -> ErrorKind;
}
