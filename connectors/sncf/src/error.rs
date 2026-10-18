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
    Http(#[from] reqwest::Error),
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
        matches!(self, Self::Api { id, .. } if id == "no_solution" || id == "unknown_object" || id == "date_out_of_bounds")
    }
}
