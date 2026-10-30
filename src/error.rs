use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("invalid URL '{0}': expected an absolute http or https URL")]
    InvalidUrl(String),
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("server returned HTTP {status} {reason}")]
    HttpStatus {
        status: reqwest::StatusCode,
        reason: String,
    },
    #[error("response exceeded the {limit} byte safety limit")]
    ResponseTooLarge { limit: usize },
    #[error("invalid CSS selector '{selector}': {message}")]
    InvalidSelector { selector: String, message: String },
    #[error("no elements matched CSS selector '{0}'")]
    SelectorNotFound(String),
    #[error("readability extraction failed: {0}")]
    Readability(#[from] dom_smoothie::ReadabilityError),
}
