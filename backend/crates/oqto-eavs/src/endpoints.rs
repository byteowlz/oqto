//! Shared validation for configured control, inference and container base URLs.
use crate::{EavsError, EavsResult};
use serde::{Deserialize, Deserializer, de::Error as _};

pub(crate) fn normalize_endpoint(value: &str) -> EavsResult<String> {
    let parsed = reqwest::Url::parse(value).map_err(|_| EavsError::InvalidEndpoint)?;
    if value.trim() != value
        || !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(EavsError::InvalidEndpoint);
    }
    Ok(parsed.as_str().trim_end_matches('/').to_owned())
}

/// Deserialize a validated base URL without echoing invalid credential-bearing input.
pub fn deserialize_endpoint<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    normalize_endpoint(&String::deserialize(deserializer)?).map_err(D::Error::custom)
}

/// Omission retains the existing unified endpoint; explicit invalid URLs fail closed.
pub fn deserialize_optional_endpoint<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|value| normalize_endpoint(&value))
        .transpose()
        .map_err(D::Error::custom)
}
