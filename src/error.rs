//! The error type returned by every fallible call in this crate.
use crate::types::GeocodeStatus;
use thiserror::Error;

/// Anything that can go wrong while geocoding.
///
/// Note what is *not* an error: `ZERO_RESULTS` (Google found no match) comes back as an
/// empty `Vec`, not an `Err`.
///
/// `#[non_exhaustive]`, so matching on it needs a `_` arm; new variants can be added
/// without a breaking change.
///
/// ```no_run
/// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient, address: &str) {
/// use uiuifree_gmaps_geocoder::{Error, GeocodeStatus};
///
/// match client.geocode(address).await {
///     Ok(results) if results.is_empty() => { /* no match — not an error */ }
///     Ok(results) => { /* use results */ }
///     // Bad key, API not enabled, billing off: retrying cannot fix it.
///     Err(Error::Api { status: GeocodeStatus::RequestDenied, message }) => {
///         eprintln!("denied: {message:?}");
///     }
///     // The built-in retry already gave up; back off further or give up.
///     Err(e) if e.is_retryable() => eprintln!("temporary failure: {e}"),
///     Err(e) => eprintln!("{e}"),
/// }
/// # }
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Transport-level failure from `reqwest`: connection refused, TLS problem, timeout,
    /// or a non-success HTTP status (429, 5xx, …).
    ///
    /// The URL is removed from the wrapped `reqwest::Error` on conversion
    /// (`without_url()`) because it carries the `key=` query parameter. Without that,
    /// `Display`/`Debug` on this error would print your API key straight into the logs.
    /// Because of that stripping, this error is safe to log as-is.
    #[error("HTTP request failed: {0}")]
    Http(#[source] reqwest::Error),

    /// The API answered with a `status` other than `OK` or `ZERO_RESULTS` — for example
    /// `OVER_QUERY_LIMIT`, `REQUEST_DENIED`, `INVALID_REQUEST` or `UNKNOWN_ERROR`.
    ///
    /// See [`GeocodeStatus`] for what each one means and
    /// [`is_retryable`](Error::is_retryable) for which ones are worth another attempt.
    #[error("geocoding API error: status={status}{}", .message.as_deref().map(|m| format!(" ({m})")).unwrap_or_default())]
    Api {
        /// The `status` string Google returned, as a typed value.
        status: GeocodeStatus,
        /// Google's `error_message`, when the response carried one.
        message: Option<String>,
    },
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // API キー入りの URL をエラーから除去する（doc コメント参照）。
        Error::Http(e.without_url())
    }
}

impl Error {
    /// Whether backing off and trying again could plausibly succeed.
    ///
    /// - [`Error::Http`] — timeouts, connection failures, HTTP 429 and HTTP 5xx.
    /// - [`Error::Api`] — `OVER_QUERY_LIMIT` and `UNKNOWN_ERROR`, the two statuses Google
    ///   itself tells clients to retry with exponential backoff.
    ///
    /// Everything else (`REQUEST_DENIED`, `INVALID_REQUEST`, `OVER_DAILY_LIMIT`, a 4xx
    /// other than 429, a JSON decoding failure) is a configuration or request problem
    /// that another attempt will not fix.
    ///
    /// [`GeocodingClient`](crate::GeocodingClient) already retries these on its own; this
    /// method is for callers who want to make that decision themselves — for instance to
    /// requeue a job after the built-in retries are exhausted.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Http(e) => {
                e.is_timeout()
                    || e.is_connect()
                    || e.status().is_some_and(|s| {
                        s == reqwest::StatusCode::TOO_MANY_REQUESTS || s.is_server_error()
                    })
            }
            Error::Api { status, .. } => status.is_retryable(),
        }
    }
}
