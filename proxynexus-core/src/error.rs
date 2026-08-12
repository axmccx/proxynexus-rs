use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyNexusError {
    #[error("Database error: {0}")]
    Database(#[from] gluesql::core::error::Error),

    #[error("Row conversion error: {0}")]
    RowConversion(#[from] gluesql::core::error::RowConversionError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(not(target_arch = "wasm32"))]
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[cfg(target_arch = "wasm32")]
    #[error("Network error: {0}")]
    Network(#[from] gloo_net::Error),

    #[error("{domain} returned error: {status}")]
    HttpStatus { domain: String, status: u16 },

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Toml Serialization error: {0}")]
    Toml(#[from] toml::ser::Error),

    #[error("Toml Deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
}

impl ProxyNexusError {
    /// Whether a failed catalog fetch is worth another try: the upstream is
    /// overloaded, briefly unavailable, or the connection dropped, rather than
    /// the request itself being wrong. 4xx other than 408/429 would just fail
    /// the same way again, and a decode failure means we got a real response.
    pub fn is_retryable_fetch(&self) -> bool {
        match self {
            Self::HttpStatus { status, .. } => {
                *status == 408 || *status == 429 || (500..600).contains(status)
            }

            #[cfg(not(target_arch = "wasm32"))]
            Self::Network(e) => e.is_timeout() || e.is_connect(),

            // gloo_net does not classify its failures, and in the browser this
            // path is a network error rather than a rejected request.
            #[cfg(target_arch = "wasm32")]
            Self::Network(_) => true,

            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, ProxyNexusError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16) -> ProxyNexusError {
        ProxyNexusError::HttpStatus {
            domain: "ringsdb.com".to_string(),
            status,
        }
    }

    #[test]
    fn server_errors_are_retried() {
        // The 504 that aborted a catalog update while RingsDB was overloaded.
        for status in [500, 502, 503, 504] {
            assert!(http(status).is_retryable_fetch(), "{status} should retry");
        }
    }

    #[test]
    fn throttling_and_request_timeout_are_retried() {
        assert!(http(408).is_retryable_fetch());
        assert!(http(429).is_retryable_fetch());
    }

    #[test]
    fn client_errors_are_not_retried() {
        // Retrying these repeats the same bad request against a healthy server.
        for status in [400, 401, 403, 404, 410] {
            assert!(
                !http(status).is_retryable_fetch(),
                "{status} should not retry"
            );
        }
    }

    #[test]
    fn non_fetch_errors_are_not_retried() {
        assert!(!ProxyNexusError::Internal("bad catalog".into()).is_retryable_fetch());
    }

    #[test]
    fn http_status_error_names_the_domain() {
        assert_eq!(http(504).to_string(), "ringsdb.com returned error: 504");
    }
}
