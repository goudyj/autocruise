use autocruise_core::error::{Classify, ErrorKind};
use autocruise_core::http::TransportError;
use autocruise_core::time::TimeError;

#[derive(Debug, thiserror::Error)]
pub enum SncfError {
    #[error(
        "SNCF_API_TOKEN is not set. Get a free key at https://numerique.sncf.com/startup/api/ and add it to the autocruise server environment."
    )]
    MissingToken,
    #[error("the SNCF API rejected the token (HTTP {0}). Check SNCF_API_TOKEN.")]
    Unauthorized(u16),
    #[error("the SNCF API rate limit was reached; try again later")]
    RateLimited,
    #[error("SNCF API error ({id}): {message}")]
    Api { id: String, message: String },
    #[error("network error while calling the SNCF API: {0}")]
    Network(#[from] TransportError),
    #[error("unexpected SNCF API response: {0}")]
    Decode(#[from] serde_json::Error),
    #[error(transparent)]
    Time(#[from] TimeError),
    #[error("{0}")]
    Invalid(String),
    #[error("no station or place matches {0:?}; try sncf_search_places")]
    NotFound(String),
}

impl SncfError {
    /// Navitia answers "nothing found" with an error object; callers treat it as empty.
    pub(crate) fn is_no_result(&self) -> bool {
        matches!(self, Self::Api { id, .. } if id == "no_solution" || id == "unknown_object")
    }
}

impl Classify for SncfError {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::MissingToken => ErrorKind::MissingCredentials,
            Self::Unauthorized(_) => ErrorKind::Unauthorized,
            Self::RateLimited => ErrorKind::RateLimited,
            Self::Api { id, .. } if is_input_error(id) => ErrorKind::InvalidInput,
            Self::Api { .. } | Self::Decode(_) => ErrorKind::Upstream,
            Self::Network(_) => ErrorKind::Network,
            Self::Time(_) | Self::Invalid(_) => ErrorKind::InvalidInput,
            Self::NotFound(_) => ErrorKind::NotFound,
        }
    }
}

/// Navitia error ids (or bare HTTP 400) caused by the request itself.
fn is_input_error(id: &str) -> bool {
    matches!(
        id,
        "400"
            | "bad_request"
            | "unable_to_parse"
            | "bad_filter"
            | "date_out_of_bounds"
            | "no_origin"
            | "no_destination"
            | "no_origin_nor_destination"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(id: &str) -> SncfError {
        SncfError::Api {
            id: id.into(),
            message: String::new(),
        }
    }

    #[test]
    fn classification() {
        assert_eq!(
            SncfError::MissingToken.kind(),
            ErrorKind::MissingCredentials
        );
        assert_eq!(api("400").kind(), ErrorKind::InvalidInput);
        assert_eq!(api("date_out_of_bounds").kind(), ErrorKind::InvalidInput);
        assert_eq!(api("500").kind(), ErrorKind::Upstream);
        assert!(api("no_solution").is_no_result());
        assert!(!api("date_out_of_bounds").is_no_result());
    }
}
