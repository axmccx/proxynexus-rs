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

pub type Result<T> = std::result::Result<T, ProxyNexusError>;
