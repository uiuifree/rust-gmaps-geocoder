//! Forward-compatible enums for the API's string fields, plus the `components` filter.
//!
//! `status` and `location_type` arrive as strings and are parsed into enums here. Unknown
//! values land in `Other(String)` instead of failing, so a value Google adds tomorrow
//! cannot break deserialization of a response you need today.
use serde::{Deserialize, Serialize};

/// The `status` field of a Geocoding API response.
///
/// See [Google's status codes][status-codes]. Parsed from and serialized back to the exact
/// string Google uses, so a round trip through JSON is lossless.
///
/// Deliberately **not** `#[non_exhaustive]`: [`Other`](GeocodeStatus::Other) already keeps
/// matches future-proof, and marking the enum non-exhaustive on top of it would force a
/// second, unreachable `_` arm on every caller.
///
/// [status-codes]: https://developers.google.com/maps/documentation/geocoding/requests-geocoding#StatusCodes
///
/// ```
/// use uiuifree_gmaps_geocoder::GeocodeStatus;
///
/// assert_eq!(GeocodeStatus::from("OK".to_string()), GeocodeStatus::Ok);
/// assert_eq!(GeocodeStatus::Ok.as_str(), "OK");
/// // A status this crate has never heard of still parses.
/// assert_eq!(
///     GeocodeStatus::from("BRAND_NEW".to_string()),
///     GeocodeStatus::Other("BRAND_NEW".to_string()),
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum GeocodeStatus {
    /// At least one result was found.
    Ok,
    /// The request was valid but nothing matched. This crate turns it into an empty `Vec`.
    ZeroResults,
    /// Daily quota exhausted. Retrying will not help until the quota resets.
    OverDailyLimit,
    /// Rate limit exceeded. Worth retrying with exponential backoff.
    OverQueryLimit,
    /// Request rejected: invalid key, API not enabled, billing disabled, key restrictions.
    RequestDenied,
    /// Malformed request, e.g. a missing or empty required parameter.
    InvalidRequest,
    /// A transient server-side error. Worth retrying with exponential backoff.
    UnknownError,
    /// A status this crate does not know yet (forward-compatibility fallback).
    Other(String),
}

impl GeocodeStatus {
    /// The exact string Google uses for this status (`"OK"`, `"ZERO_RESULTS"`, …).
    pub fn as_str(&self) -> &str {
        match self {
            GeocodeStatus::Ok => "OK",
            GeocodeStatus::ZeroResults => "ZERO_RESULTS",
            GeocodeStatus::OverDailyLimit => "OVER_DAILY_LIMIT",
            GeocodeStatus::OverQueryLimit => "OVER_QUERY_LIMIT",
            GeocodeStatus::RequestDenied => "REQUEST_DENIED",
            GeocodeStatus::InvalidRequest => "INVALID_REQUEST",
            GeocodeStatus::UnknownError => "UNKNOWN_ERROR",
            GeocodeStatus::Other(s) => s,
        }
    }

    /// Whether backing off and retrying could recover: `OVER_QUERY_LIMIT` and
    /// `UNKNOWN_ERROR`, the two statuses Google documents as retryable.
    ///
    /// `OVER_DAILY_LIMIT` is *not* retryable — the quota only comes back the next day.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            GeocodeStatus::OverQueryLimit | GeocodeStatus::UnknownError
        )
    }
}

impl From<String> for GeocodeStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "OK" => Self::Ok,
            "ZERO_RESULTS" => Self::ZeroResults,
            "OVER_DAILY_LIMIT" => Self::OverDailyLimit,
            "OVER_QUERY_LIMIT" => Self::OverQueryLimit,
            "REQUEST_DENIED" => Self::RequestDenied,
            "INVALID_REQUEST" => Self::InvalidRequest,
            "UNKNOWN_ERROR" => Self::UnknownError,
            _ => Self::Other(s),
        }
    }
}

impl From<GeocodeStatus> for String {
    fn from(s: GeocodeStatus) -> String {
        s.as_str().to_string()
    }
}

impl std::fmt::Display for GeocodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How precise a result's coordinates are (`geometry.location_type`).
///
/// This is the field to check before trusting a coordinate: `ROOFTOP` is a specific
/// building, while `APPROXIMATE` may be the centre of a whole city or postal code.
/// See [Google's location types][location-types].
///
/// Like [`GeocodeStatus`], it is not `#[non_exhaustive]` because
/// [`Other`](LocationType::Other) already absorbs unknown values.
///
/// [location-types]: https://developers.google.com/maps/documentation/geocoding/requests-geocoding#results
///
/// ```
/// use uiuifree_gmaps_geocoder::LocationType;
///
/// assert!(LocationType::Rooftop.is_street_level());
/// assert!(!LocationType::Approximate.is_street_level());
/// assert!(LocationType::Rooftop.precision_rank() > LocationType::GeometricCenter.precision_rank());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum LocationType {
    /// A precise, building-level position.
    Rooftop,
    /// Interpolated between two known points along a road.
    RangeInterpolated,
    /// The geometric centre of a polyline (a street) or an area (a lot).
    GeometricCenter,
    /// Approximate — typically the centre of a locality or postal code.
    Approximate,
    /// A location type this crate does not know yet (forward-compatibility fallback).
    Other(String),
}

impl LocationType {
    /// Accuracy as a number, higher being more precise: `Rooftop` 3,
    /// `RangeInterpolated` 2, `GeometricCenter` 1, `Approximate` and unknown values 0.
    ///
    /// Use it to compare results; [`most_precise`](crate::most_precise) is this ranking
    /// applied to a whole result set.
    pub fn precision_rank(&self) -> u8 {
        match self {
            LocationType::Rooftop => 3,
            LocationType::RangeInterpolated => 2,
            LocationType::GeometricCenter => 1,
            LocationType::Approximate | LocationType::Other(_) => 0,
        }
    }

    /// Whether the coordinates identify a specific address (`ROOFTOP` or
    /// `RANGE_INTERPOLATED`) rather than the centre of an area (`GEOMETRIC_CENTER`,
    /// `APPROXIMATE`, unknown).
    ///
    /// The usual guard before storing a coordinate as "this building".
    pub fn is_street_level(&self) -> bool {
        matches!(
            self,
            LocationType::Rooftop | LocationType::RangeInterpolated
        )
    }

    /// The exact string Google uses for this location type (`"ROOFTOP"`, …). This is also
    /// what [`GeocodeRequest::location_type`](crate::GeocodeRequest::location_type) sends.
    pub fn as_str(&self) -> &str {
        match self {
            LocationType::Rooftop => "ROOFTOP",
            LocationType::RangeInterpolated => "RANGE_INTERPOLATED",
            LocationType::GeometricCenter => "GEOMETRIC_CENTER",
            LocationType::Approximate => "APPROXIMATE",
            LocationType::Other(s) => s,
        }
    }
}

impl From<String> for LocationType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "ROOFTOP" => Self::Rooftop,
            "RANGE_INTERPOLATED" => Self::RangeInterpolated,
            "GEOMETRIC_CENTER" => Self::GeometricCenter,
            "APPROXIMATE" => Self::Approximate,
            _ => Self::Other(s),
        }
    }
}

impl From<LocationType> for String {
    fn from(t: LocationType) -> String {
        t.as_str().to_string()
    }
}

impl std::fmt::Display for LocationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One entry of the `components` filter, which restricts a forward geocode to a specific
/// country, postal code, locality, administrative area or street.
///
/// Each variant becomes `key:value` in the query, and repeated
/// [`component`](crate::GeocodeRequest::component) calls are joined with `|`, which the
/// API treats as AND. See [component filtering][components].
///
/// [components]: https://developers.google.com/maps/documentation/geocoding/requests-geocoding#component-filtering
///
/// ```no_run
/// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient) -> Result<(), uiuifree_gmaps_geocoder::Error> {
/// use uiuifree_gmaps_geocoder::{Component, GeocodeRequest};
///
/// // components=country:JP|locality:渋谷区
/// let req = GeocodeRequest::address("道玄坂")
///     .component(Component::Country("JP"))
///     .component(Component::Locality("渋谷区"));
/// let results = client.send(req).await?;
/// # let _ = results;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component<'a> {
    /// Country as an ISO 3166-1 alpha-2 code, e.g. `"JP"`.
    Country(&'a str),
    /// Postal code.
    PostalCode(&'a str),
    /// City or town.
    Locality(&'a str),
    /// Administrative area — prefecture, state, province.
    AdministrativeArea(&'a str),
    /// Street name.
    Route(&'a str),
}

impl Component<'_> {
    fn key(&self) -> &'static str {
        match self {
            Component::Country(_) => "country",
            Component::PostalCode(_) => "postal_code",
            Component::Locality(_) => "locality",
            Component::AdministrativeArea(_) => "administrative_area",
            Component::Route(_) => "route",
        }
    }

    fn value(&self) -> &str {
        match self {
            Component::Country(v)
            | Component::PostalCode(v)
            | Component::Locality(v)
            | Component::AdministrativeArea(v)
            | Component::Route(v) => v,
        }
    }

    /// `key:value` 形式のクエリ片。
    pub(crate) fn to_param(self) -> String {
        format!("{}:{}", self.key(), self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_rank_and_street_level() {
        assert_eq!(LocationType::Rooftop.precision_rank(), 3);
        assert_eq!(LocationType::RangeInterpolated.precision_rank(), 2);
        assert_eq!(LocationType::GeometricCenter.precision_rank(), 1);
        assert_eq!(LocationType::Approximate.precision_rank(), 0);
        assert_eq!(LocationType::Other("NEW".into()).precision_rank(), 0);
        assert!(LocationType::Rooftop.is_street_level());
        assert!(LocationType::RangeInterpolated.is_street_level());
        assert!(!LocationType::GeometricCenter.is_street_level());
        assert!(!LocationType::Approximate.is_street_level());
    }
}
