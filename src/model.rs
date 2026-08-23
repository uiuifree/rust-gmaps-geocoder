//! The response types: what Google sends back, one Rust type per JSON object.
//!
//! Every JSON key is snake_case and maps to a field of the same name, so the structs read
//! like [Google's response documentation][response]. All of them implement `Serialize` as
//! well as `Deserialize`, so a response can be stored, queued or replayed unchanged.
//!
//! [response]: https://developers.google.com/maps/documentation/geocoding/requests-geocoding
use crate::types::{GeocodeStatus, LocationType};
use serde::{Deserialize, Serialize};

/// A whole response from `/maps/api/geocode/json`.
///
/// [`GeocodingClient::send`](crate::GeocodingClient::send) hands you just the
/// [`results`](GeocodeResponse::results); reach for
/// [`send_response`](crate::GeocodingClient::send_response) when you also need the
/// top-level [`plus_code`](GeocodeResponse::plus_code) or
/// [`address_descriptor`](GeocodeResponse::address_descriptor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeocodeResponse {
    /// Outcome of the request. Anything but `OK` or `ZERO_RESULTS` is turned into
    /// [`Error::Api`](crate::Error::Api) before you see the response.
    pub status: GeocodeStatus,
    /// Matching places, ordered by Google's own relevance ranking. Empty on `ZERO_RESULTS`.
    #[serde(default)]
    pub results: Vec<GeocodeResult>,
    /// Google's human-readable explanation, present on some error statuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Plus Code for the request as a whole — returned for reverse geocoding, where the
    /// query is a coordinate rather than an address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plus_code: Option<PlusCode>,
    /// Address descriptor for the request as a whole: landmarks and areas near the queried
    /// point. Only for reverse geocoding with
    /// [`address_descriptors(true)`](crate::GeocodeRequest::address_descriptors), and only
    /// in supported regions.
    ///
    /// Kept as raw JSON on purpose: the feature is in beta and its shape can still change,
    /// which would otherwise mean a breaking release of this crate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_descriptor: Option<serde_json::Value>,
}

/// One matched place: where it is, how precisely, and what it is made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeocodeResult {
    /// The address as Google would print it, in the requested `language`.
    pub formatted_address: String,
    /// Coordinates, accuracy and the recommended map viewport.
    pub geometry: Geometry,
    /// Stable Google Place ID. Unlike coordinates, this may be stored indefinitely and is
    /// the right key to keep in your own database.
    pub place_id: String,
    /// What kind of place this is: `street_address`, `route`, `locality`, `postal_code`, …
    #[serde(default)]
    pub types: Vec<String>,
    /// The address broken into parts (country, prefecture, city, block, …). Prefer the
    /// helpers — [`postal_code`](GeocodeResult::postal_code),
    /// [`prefecture`](GeocodeResult::prefecture), [`locality`](GeocodeResult::locality),
    /// [`country`](GeocodeResult::country), [`component`](GeocodeResult::component) — over
    /// walking this list yourself.
    #[serde(default)]
    pub address_components: Vec<AddressComponent>,
    /// Plus Code (Open Location Code) for this result, when Google supplies one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plus_code: Option<PlusCode>,
    /// For a postal code that spans several place names, all of those names. Only present
    /// when the query was a postal code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postcode_localities: Vec<String>,
    /// Landmarks and areas near this result. Only with
    /// [`address_descriptors(true)`](crate::GeocodeRequest::address_descriptors), and only
    /// in supported regions. Raw JSON while the feature is in beta — see
    /// [`GeocodeResponse::address_descriptor`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_descriptor: Option<serde_json::Value>,
    /// `true` when Google matched something other than exactly what was asked for — a
    /// misspelling, an incomplete address, or one of several equally good candidates.
    /// Worth surfacing before treating the coordinates as confirmed.
    #[serde(default)]
    pub partial_match: bool,
}

/// A Plus Code, Google's [Open Location Code](https://plus.codes/).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlusCode {
    /// The globally unique code, e.g. `"8Q7XJP00+00"`.
    pub global_code: String,
    /// The short form relative to a nearby place, e.g. `"MP2H+8Q 渋谷区"`. Not always present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compound_code: Option<String>,
}

/// Where a result sits on the map, and how much to trust it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    /// The coordinates themselves.
    pub location: LatLng,
    /// How precise [`location`](Geometry::location) is — check this before treating a
    /// coordinate as a building. See [`LocationType`].
    pub location_type: LocationType,
    /// The viewport Google recommends when displaying this result on a map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Bounds>,
    /// The box that fully contains the result, for results that have an extent (a city, a
    /// park). Absent for point-like results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
}

/// A latitude/longitude pair in decimal degrees (WGS 84).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    /// Latitude in degrees, −90 to 90.
    pub lat: f64,
    /// Longitude in degrees, −180 to 180.
    pub lng: f64,
}

/// A rectangle, given by two opposite corners.
///
/// Doubles as the [viewport bias](crate::GeocodeRequest::bounds) on a request and as
/// [`Geometry::viewport`] / [`Geometry::bounds`] on a result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    /// North-east corner.
    pub northeast: LatLng,
    /// South-west corner.
    pub southwest: LatLng,
}

/// One piece of a structured address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressComponent {
    /// Full name, e.g. `"東京都"` or `"United States"`.
    pub long_name: String,
    /// Abbreviated name, e.g. `"Tokyo"` or `"US"`.
    pub short_name: String,
    /// What this component is: `country`, `administrative_area_level_1`, `locality`,
    /// `postal_code`, `route`, `premise`, … The list Google may return is documented under
    /// [address types](https://developers.google.com/maps/documentation/geocoding/requests-geocoding#Types).
    #[serde(default)]
    pub types: Vec<String>,
}

impl GeocodeResult {
    /// The coordinates of this result — shorthand for `self.geometry.location`.
    pub fn location(&self) -> LatLng {
        self.geometry.location
    }

    /// The first address component carrying the given type, or `None` if the address has
    /// no such part.
    ///
    /// This is the general form of the named helpers below; use it for the types they do
    /// not cover, such as `route`, `premise` or `sublocality_level_1`.
    ///
    /// ```
    /// # use uiuifree_gmaps_geocoder::GeocodeResult;
    /// # fn f(r: &GeocodeResult) {
    /// let ward = r.component("sublocality_level_1").map(|c| c.long_name.as_str());
    /// let street = r.component("route").map(|c| c.long_name.as_str());
    /// # let _ = (ward, street);
    /// # }
    /// ```
    pub fn component(&self, type_: &str) -> Option<&AddressComponent> {
        self.address_components
            .iter()
            .find(|c| c.types.iter().any(|t| t == type_))
    }

    /// The postal code (`postal_code`), e.g. `"150-0043"`.
    pub fn postal_code(&self) -> Option<&str> {
        self.component("postal_code").map(|c| c.long_name.as_str())
    }

    /// The country component, kept whole so you can pick between `long_name` (`"日本"`) and
    /// `short_name`, which is the ISO 3166-1 alpha-2 code (`"JP"`).
    pub fn country(&self) -> Option<&AddressComponent> {
        self.component("country")
    }

    /// The full name of the top-level administrative area
    /// (`administrative_area_level_1`): a prefecture in Japan, a state in the US.
    pub fn prefecture(&self) -> Option<&str> {
        self.component("administrative_area_level_1")
            .map(|c| c.long_name.as_str())
    }

    /// The full name of the city or town (`locality`).
    ///
    /// Not every address has one — for a Tokyo special ward the city-level name arrives as
    /// `locality`, but in other countries the nearest equivalent may only appear as
    /// `sublocality` or `postal_town`, reachable via
    /// [`component`](GeocodeResult::component).
    pub fn locality(&self) -> Option<&str> {
        self.component("locality").map(|c| c.long_name.as_str())
    }
}

/// The most precise result of a set, by [`LocationType::precision_rank`]
/// (`ROOFTOP` > `RANGE_INTERPOLATED` > `GEOMETRIC_CENTER` > `APPROXIMATE`).
///
/// Ties keep Google's ordering: among equally precise results the earliest one wins,
/// because Google returns results in descending order of relevance. Returns `None` for an
/// empty slice, which is what a `ZERO_RESULTS` lookup gives you.
///
/// ```no_run
/// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient) -> Result<(), uiuifree_gmaps_geocoder::Error> {
/// use uiuifree_gmaps_geocoder::most_precise;
///
/// let results = client.geocode("東京都渋谷区道玄坂1-1-1").await?;
/// if let Some(best) = most_precise(&results) {
///     println!("{:?} ({})", best.location(), best.geometry.location_type);
/// }
/// # Ok(())
/// # }
/// ```
pub fn most_precise(results: &[GeocodeResult]) -> Option<&GeocodeResult> {
    results
        .iter()
        .enumerate()
        // max_by_key keeps the *last* maximum, so rank ties by reversed index to keep the first.
        .max_by_key(|(index, result)| {
            (
                result.geometry.location_type.precision_rank(),
                std::cmp::Reverse(*index),
            )
        })
        .map(|(_, result)| result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result_with(location_type: LocationType, address: &str) -> GeocodeResult {
        GeocodeResult {
            formatted_address: address.to_string(),
            geometry: Geometry {
                location: LatLng { lat: 0.0, lng: 0.0 },
                location_type,
                viewport: None,
                bounds: None,
            },
            place_id: String::new(),
            types: Vec::new(),
            address_components: Vec::new(),
            plus_code: None,
            postcode_localities: Vec::new(),
            address_descriptor: None,
            partial_match: false,
        }
    }

    #[test]
    fn most_preciseは精度が最大の結果を返す() {
        let results = [
            result_with(LocationType::Approximate, "approximate"),
            result_with(LocationType::Rooftop, "rooftop"),
            result_with(LocationType::GeometricCenter, "center"),
        ];
        assert_eq!(
            most_precise(&results).map(|r| r.formatted_address.as_str()),
            Some("rooftop")
        );
    }

    #[test]
    fn 同じ精度ならgoogleの並び順どおり先頭を返す() {
        let results = [
            result_with(LocationType::Rooftop, "first"),
            result_with(LocationType::Rooftop, "second"),
        ];
        assert_eq!(
            most_precise(&results).map(|r| r.formatted_address.as_str()),
            Some("first")
        );
    }

    #[test]
    fn 空の結果はnoneになる() {
        assert!(most_precise(&[]).is_none());
    }
}
