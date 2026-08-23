//! Async Rust client for the [Google Maps Geocoding API][geocoding-api] — and nothing else.
//!
//! One endpoint (`/maps/api/geocode/json`), three lookups:
//!
//! | Lookup | Method |
//! | --- | --- |
//! | Address → coordinates (*forward geocoding*) | [`GeocodingClient::geocode`] |
//! | Coordinates → address (*reverse geocoding*) | [`GeocodingClient::reverse_geocode`] |
//! | Place ID → address and coordinates | [`GeocodingClient::geocode_place_id`] |
//! | Any of the three, with options | [`GeocodeRequest`] passed to [`GeocodingClient::send`] |
//!
//! Results are typed ([`GeocodeResult`], [`LocationType`], [`GeocodeStatus`]), failures come
//! back classified instead of retried, and the API key never reaches a log line.
//!
//! # Quick start
//!
//! ```no_run
//! use uiuifree_gmaps_geocoder::GeocodingClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), uiuifree_gmaps_geocoder::Error> {
//!     let client = GeocodingClient::new(std::env::var("GOOGLE_MAPS_API_KEY").unwrap());
//!
//!     // Address → coordinates
//!     for r in client.geocode("1600 Amphitheatre Parkway, Mountain View, CA").await? {
//!         let loc = r.location();
//!         println!("{} => {}, {}", r.formatted_address, loc.lat, loc.lng);
//!     }
//!
//!     // Coordinates → address
//!     for r in client.reverse_geocode(35.6580, 139.6994).await? {
//!         println!("{}", r.formatted_address);
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Requests with options
//!
//! [`GeocodeRequest`] is a borrowing builder that covers every parameter the endpoint
//! accepts: [`language`](GeocodeRequest::language), [`region`](GeocodeRequest::region),
//! [`bounds`](GeocodeRequest::bounds), [`components`](GeocodeRequest::component),
//! [`result_type`](GeocodeRequest::result_type),
//! [`location_type`](GeocodeRequest::location_type) and
//! [`extra_computations`](GeocodeRequest::address_descriptors).
//!
//! ```no_run
//! # async fn run() -> Result<(), uiuifree_gmaps_geocoder::Error> {
//! use uiuifree_gmaps_geocoder::{GeocodeRequest, GeocodingClient, LocationType};
//!
//! let client = GeocodingClient::new("YOUR_API_KEY");
//!
//! // Japanese defaults in one call: language=ja, region=jp, components=country:JP.
//! let in_japan = client.send(GeocodeRequest::japan("東京都渋谷区渋谷2-24-12")).await?;
//!
//! // Reverse geocoding, narrowed to rooftop-accurate street addresses.
//! let rooftop = client
//!     .send(
//!         GeocodeRequest::reverse(35.658, 139.6994)
//!             .result_type("street_address")
//!             .location_type(LocationType::Rooftop),
//!     )
//!     .await?;
//! # let _ = (in_japan, rooftop);
//! # Ok(())
//! # }
//! ```
//!
//! # Reading a result
//!
//! Every field Google returns is on [`GeocodeResult`]; the common ones have helpers.
//!
//! ```
//! use uiuifree_gmaps_geocoder::{most_precise, GeocodeResult};
//!
//! fn summarize(results: &[GeocodeResult]) {
//!     // Most accurate hit: ROOFTOP > RANGE_INTERPOLATED > GEOMETRIC_CENTER > APPROXIMATE.
//!     let Some(best) = most_precise(results) else {
//!         return; // ZERO_RESULTS: no match, and that is not an error
//!     };
//!     let loc = best.location();
//!     println!("{}, {}", loc.lat, loc.lng);
//!     println!("{:?} {:?}", best.prefecture(), best.locality());
//!     println!("{:?}", best.postal_code());
//!     println!("{:?}", best.component("route"));
//!
//!     // Did we resolve to a building, or only to a city/postal-code centroid?
//!     if !best.geometry.location_type.is_street_level() {
//!         println!("approximate only: {}", best.geometry.location_type);
//!     }
//! }
//! ```
//!
//! # Errors
//!
//! | Google `status` / transport | This crate |
//! | --- | --- |
//! | `OK` | `Ok(results)` |
//! | `ZERO_RESULTS` | `Ok(vec![])` — no match is not an error |
//! | `OVER_QUERY_LIMIT`, `UNKNOWN_ERROR` | [`Error::Api`] — worth retrying |
//! | HTTP 429/5xx, timeout, connection failure | [`Error::Http`] — worth retrying |
//! | `REQUEST_DENIED`, `INVALID_REQUEST`, `OVER_DAILY_LIMIT` | [`Error::Api`] — retrying will not help |
//!
//! **This crate has no retry policy.** Every failure above reaches you on the first attempt:
//! backoff, jitter, queueing and quota control belong to the caller, which is where the
//! context to make those choices lives. [`Error::is_retryable`] classifies an error so you
//! can drive that decision without matching on transport details yourself.
//!
//! One exception sits below this crate: `reqwest` replays a request the server refused at
//! the protocol level (an HTTP/2 NACK) before processing it. That is transport plumbing,
//! not a policy — nothing the server acted on is ever sent twice. Pass a client built with
//! `reqwest::retry::never()` to [`GeocodingClient::with_http_client`] to turn even that off.
//!
//! The API key is stripped from every error ([`Error::Http`] carries no URL) and printed as
//! `[REDACTED]` by `GeocodingClient`'s `Debug`, so errors are safe to log.
//!
//! # Deliberate omissions
//!
//! No retrying, no caching (Google allows at most 30 days anyway), no
//! `extra_computations=BUILDING_AND_ENTRANCES`, and no other Google Maps Platform API.
//! Quota, billing and request de-duplication stay with the caller.
//!
//! # Google Maps Platform terms
//!
//! Using this crate means calling Google's API under the [Maps Platform terms][terms]:
//! geocoding results may be cached for at most 30 days (`place_id` may be stored),
//! and displaying results generally requires a Google map and attribution. A library
//! cannot enforce that for you.
//!
//! [geocoding-api]: https://developers.google.com/maps/documentation/geocoding/overview
//! [terms]: https://cloud.google.com/maps-platform/terms
#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]
mod client;
mod error;
mod model;
mod types;

pub use client::{GeocodeRequest, GeocodingClient};
pub use error::Error;
pub use model::{
    most_precise, AddressComponent, Bounds, GeocodeResponse, GeocodeResult, Geometry, LatLng,
    PlusCode,
};
pub use types::{Component, GeocodeStatus, LocationType};
