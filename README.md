# uiuifree-gmaps-geocoder

[![crates.io](https://img.shields.io/crates/v/uiuifree-gmaps-geocoder.svg)](https://crates.io/crates/uiuifree-gmaps-geocoder)
[![docs.rs](https://img.shields.io/docsrs/uiuifree-gmaps-geocoder)](https://docs.rs/uiuifree-gmaps-geocoder)
[![CI](https://github.com/uiuifree/rust-gmaps-geocoder/actions/workflows/ci.yml/badge.svg)](https://github.com/uiuifree/rust-gmaps-geocoder/actions/workflows/ci.yml)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://blog.rust-lang.org/)
[![license: MIT](https://img.shields.io/crates/l/uiuifree-gmaps-geocoder.svg)](LICENSE)

**Async Rust client for the Google Maps Geocoding API.** Convert an address into
latitude/longitude (forward geocoding), coordinates into an address (reverse geocoding),
or a Place ID into both — with typed responses, errors you can act on, and an API key that
never leaks into logs.

[日本語 README](README.ja.md) · [API documentation](https://docs.rs/uiuifree-gmaps-geocoder) · [llms.txt](llms.txt) · [Changelog](CHANGELOG.md)

Geocoding **only**: this crate covers the single `/maps/api/geocode/json` endpoint and
nothing else — no Places, Routes, Static Maps, or map rendering. If you want the whole
Google Maps Platform surface, use a full-platform client instead.

```rust
let client = GeocodingClient::new(std::env::var("GOOGLE_MAPS_API_KEY")?);
let results = client.geocode("1600 Amphitheatre Parkway, Mountain View, CA").await?;
let loc = results[0].location();
println!("{}, {}", loc.lat, loc.lng); // 37.4224764, -122.0842499
```

## Contents

- [Why this crate](#why-this-crate)
- [Install](#install)
- [Quick start](#quick-start)
- [Cookbook](#cookbook)
- [API reference at a glance](#api-reference-at-a-glance)
- [Behavior: statuses and errors](#behavior-statuses-and-errors)
- [Google Maps Platform terms you handle yourself](#google-maps-platform-terms-you-handle-yourself)
- [Not covered, on purpose](#not-covered-on-purpose)
- [When *not* to use this crate](#when-not-to-use-this-crate)
- [FAQ](#faq)
- [Development](#development)

## Why this crate

**Complete coverage of the Geocoding API, and nothing else.**

- **Three lookups** — address → coordinates (forward), coordinates → address (reverse),
  Place ID → address & coordinates.
- **Every request parameter** — `language`, `region`, `bounds` (viewport bias),
  `components` (filtering), `result_type` / `location_type` (reverse filters),
  `extra_computations=ADDRESS_DESCRIPTORS` (beta).
- **Every response field** — coordinates, accuracy, viewport/bounds, `address_components`,
  `types`, `place_id`, `plus_code` (per result *and* top level), `postcode_localities`,
  `partial_match`, `address_descriptor`.
- **Typed, not stringly-typed** — `GeocodeStatus` and `LocationType` are enums; unknown
  values fall back to `Other(String)`, so a new value from Google never breaks parsing.
- **Address-component helpers** — `postal_code()`, `prefecture()`, `locality()`,
  `country()`, `component("route")`, plus `most_precise(&results)` to pick the best hit.
- **Errors you can act on** — no retry policy of its own; `Error::is_retryable()` tells you
  whether a failure is worth another attempt. Backoff and queueing stay yours.
- **API keys stay out of logs** — the key is stripped from error URLs and printed as
  `[REDACTED]` in `Debug`.
- **Small dependency tree** — `reqwest` (rustls), `serde`/`serde_json`, `thiserror`.
  No OpenSSL, no proc-macro-heavy framework.
- **Serializable results** — every model implements `Serialize` + `Deserialize`, so you
  can store a response and replay it later.
- **First-class Japanese addresses** — `GeocodeRequest::japan(address)` applies
  `language=ja` / `region=jp` / `components=country:JP` in one call.

## Install

```sh
cargo add uiuifree-gmaps-geocoder tokio --features tokio/macros,tokio/rt-multi-thread
```

Or in `Cargo.toml`:

```toml
[dependencies]
uiuifree-gmaps-geocoder = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

You need a Google Maps Platform API key with the **Geocoding API** enabled
([get one here](https://developers.google.com/maps/documentation/geocoding/get-api-key)).
Requests are billed by Google per request.

## Quick start

```rust
use uiuifree_gmaps_geocoder::GeocodingClient;

#[tokio::main]
async fn main() -> Result<(), uiuifree_gmaps_geocoder::Error> {
    let client = GeocodingClient::new(std::env::var("GOOGLE_MAPS_API_KEY").unwrap());

    // Address -> coordinates
    for r in client.geocode("1-1-1 Dogenzaka, Shibuya, Tokyo").await? {
        let loc = r.location();
        println!(
            "{} => {}, {} [{}] postal={:?} area={:?}",
            r.formatted_address,
            loc.lat,
            loc.lng,
            r.geometry.location_type, // ROOFTOP / RANGE_INTERPOLATED / ...
            r.postal_code(),
            r.prefecture(),
        );
    }

    // Coordinates -> address
    for r in client.reverse_geocode(35.6580, 139.6994).await? {
        println!("{}", r.formatted_address);
    }
    Ok(())
}
```

`geocode` returns `Vec<GeocodeResult>` ordered by Google's own relevance ranking.
No match is **not** an error: `ZERO_RESULTS` comes back as an empty `Vec`.

## Cookbook

### The three lookups

```rust
client.geocode("1600 Amphitheatre Parkway, Mountain View, CA").await?; // address -> coordinates
client.reverse_geocode(35.6580, 139.6994).await?;                      // coordinates -> address
client.geocode_place_id("ChIJN1t_tDeuEmsRUsoyG83frY4").await?;         // Place ID -> address
```

### Pick the most accurate result

```rust
use uiuifree_gmaps_geocoder::most_precise;

// Best of ROOFTOP > RANGE_INTERPOLATED > GEOMETRIC_CENTER > APPROXIMATE
// (ties keep Google's ordering, so the first one wins).
let best = most_precise(&results);

// Did we actually resolve down to a street address, or only to a city centroid?
let street_level = results.iter().any(|r| r.geometry.location_type.is_street_level());
```

### All request parameters

```rust
use uiuifree_gmaps_geocoder::{Bounds, Component, GeocodeRequest, GeocodingClient, LatLng, LocationType};

let client = GeocodingClient::new("YOUR_API_KEY");

// Forward: language, region bias, viewport bias, component filtering
let req = GeocodeRequest::address("Dogenzaka")
    .language("ja")
    .region("jp")
    .bounds(Bounds {
        southwest: LatLng { lat: 35.5, lng: 139.5 },
        northeast: LatLng { lat: 35.8, lng: 139.9 },
    })
    .component(Component::Country("JP")); // call repeatedly to AND filters
let results = client.send(req).await?;

// Reverse: keep only the result types / accuracy you care about
let req = GeocodeRequest::reverse(35.658, 139.6994)
    .result_type("street_address")
    .location_type(LocationType::Rooftop);
let results = client.send(req).await?;

// Need the whole response (top-level `plus_code`, `address_descriptor`)?
let resp = client.send_response(GeocodeRequest::reverse(35.658, 139.6994)).await?;
println!("{:?}", resp.plus_code);
```

### Japanese addresses

```rust
// language=ja + region=jp + components=country:JP in one call
let results = client.send(GeocodeRequest::japan("東京都渋谷区渋谷2-24-12")).await?;
let r = &results[0];
(r.prefecture(), r.locality(), r.postal_code()); // ("東京都", "渋谷区", "150-0002")
```

### Retry it yourself

The crate never retries for you. `Error::is_retryable()` does the classification so your
own loop stays short:

```rust
use std::time::Duration;

let mut delay = Duration::from_millis(500);
let results = loop {
    match client.geocode(address).await {
        Ok(results) => break results,
        Err(e) if e.is_retryable() && delay < Duration::from_secs(8) => {
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        Err(e) => return Err(e),
    }
};
```

Doing it here rather than inside the crate is deliberate: only the caller knows the request
budget, whether jitter matters, and whether a failure should go back on a queue instead of
blocking the task.

### Bring your own HTTP client

```rust
// The default client has a 10s total timeout; override it (proxy, timeouts, pool) here.
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
let client = GeocodingClient::with_http_client("YOUR_API_KEY", http);
```

### Handle errors

```rust
use uiuifree_gmaps_geocoder::{Error, GeocodeStatus};

match client.geocode(address).await {
    Ok(results) if results.is_empty() => { /* ZERO_RESULTS: no match, not an error */ }
    Ok(results) => { /* use results */ }
    Err(Error::Api { status: GeocodeStatus::RequestDenied, message }) => {
        // bad key / API not enabled / billing off — fix the setup, do not retry
        eprintln!("denied: {message:?}");
    }
    Err(e) if e.is_retryable() => { /* temporary — back off and try again on your schedule */ }
    Err(e) => return Err(e),
}
```

## API reference at a glance

| Item | What it does |
| --- | --- |
| `GeocodingClient::new(key)` | Client with a 10s-timeout HTTP client |
| `::with_http_client(key, reqwest::Client)` | Supply your own `reqwest::Client` |
| `.with_base_url(url)` | Point at another endpoint (tests, proxy) |
| `.geocode(address)` | Forward geocoding → `Result<Vec<GeocodeResult>, Error>` |
| `.reverse_geocode(lat, lng)` | Reverse geocoding → `Result<Vec<GeocodeResult>, Error>` |
| `.geocode_place_id(place_id)` | Place ID lookup → `Result<Vec<GeocodeResult>, Error>` |
| `.send(GeocodeRequest)` | Any request with options → results |
| `.send_response(GeocodeRequest)` | Same, but returns the full `GeocodeResponse` |
| `GeocodeRequest::address / japan / reverse / place_id` | Request builders |
| `.language / .region / .bounds / .component / .result_type / .location_type / .address_descriptors` | Request options |
| `GeocodeResult::location()` | `LatLng` of the result |
| `.postal_code() / .prefecture() / .locality() / .country() / .component(type)` | Address-component helpers |
| `LocationType::precision_rank() / .is_street_level()` | Compare accuracy |
| `most_precise(&results)` | The most accurate single result |
| `Error::is_retryable()` | Whether backing off and trying again could help |

## Behavior: statuses and errors

| Google `status` / transport | This crate |
| --- | --- |
| `OK` | `Ok(results)` |
| `ZERO_RESULTS` | `Ok(vec![])` — no match is not an error |
| `OVER_QUERY_LIMIT`, `UNKNOWN_ERROR` | `Err(Error::Api)`, `is_retryable() == true` |
| HTTP 429, HTTP 5xx, timeout, connection failure | `Err(Error::Http)`, `is_retryable() == true` |
| `REQUEST_DENIED`, `INVALID_REQUEST`, `OVER_DAILY_LIMIT` | `Err(Error::Api)`, `is_retryable() == false` — retrying cannot fix these |

- **No retry policy.** Nothing is retried, queued or cached on your behalf; every failure
  reaches you on the first attempt. (`reqwest` below this crate still replays a request the
  server refused at the protocol level before processing it — transport plumbing, not
  policy. Pass a client built with `reqwest::retry::never()` to turn even that off.)
- `Error::is_retryable()` is the classification hook for whatever retry layer you already
  have — it covers both API statuses and transport failures, so you never match on
  `reqwest` internals yourself.
- **The API key never appears in an error.** `Error` strips the URL (which carries `key=`)
  from the underlying `reqwest::Error`, and `GeocodingClient`'s `Debug` prints
  `api_key: "[REDACTED]"`. Logging an `Error` is safe.

## Google Maps Platform terms you handle yourself

A library cannot enforce these — they are yours to honor:

- Geocoding results may be cached for **up to 30 days**; `place_id` may be stored
  indefinitely. This crate deliberately caches nothing.
- Displaying results generally requires a Google map and proper attribution.
- Check the [current terms](https://cloud.google.com/maps-platform/terms) yourself; the
  summary above is not legal advice.

## Not covered, on purpose

- Retrying, backoff and the `Retry-After` header — `Error::is_retryable()` classifies,
  the caller decides.
- `extra_computations=BUILDING_AND_ENTRANCES` (building outlines/entrances, beta).
- Caching or request de-duplication — quota and billing control belong to the caller.
- Any other Google Maps Platform API (Places, Routes, Time Zone, Elevation, …).

## When *not* to use this crate

- **You must work offline / without network calls.** Use an embedded-dataset crate
  (for Japan: `jp-address-search` for postal code → town, `japan-reverse-geocoder` for
  coordinates → town).
- **You need more of Google Maps Platform than geocoding.** A full-platform client such
  as `google_maps` covers Places, Routes and friends in one crate.
- **You want a provider-agnostic geocoder** (Nominatim/OpenCage/…). This crate speaks
  Google's response shape directly, on purpose.
- **You need to store coordinates indefinitely.** Google's terms do not allow it.

## FAQ

### How do I convert an address to latitude and longitude in Rust?

`GeocodingClient::new(api_key).geocode("<address>").await?` returns `Vec<GeocodeResult>`;
`result.location()` gives you `LatLng { lat, lng }`. See [Quick start](#quick-start).

### How do I do reverse geocoding (coordinates to address)?

`client.reverse_geocode(lat, lng).await?`, then read `formatted_address`, or narrow the
answer with `.result_type("locality")` / `.location_type(LocationType::Rooftop)`.

### What happens when the address does not exist?

You get `Ok(vec![])` — Google's `ZERO_RESULTS`. Only real failures become `Err`.

### Does it retry on `OVER_QUERY_LIMIT` / rate limiting?

No — one call is one request. It classifies the failure for you: `Error::is_retryable()`
returns `true` for `OVER_QUERY_LIMIT`, `UNKNOWN_ERROR`, HTTP 429/5xx, timeouts and
connection failures, and `false` for a bad key, a malformed request or an exhausted daily
quota. See [Retry it yourself](#retry-it-yourself) for the loop.

### Can my API key leak through logs?

No. Errors carry no URL (that is where `key=` lives) and `Debug` for the client prints
`[REDACTED]`. You still choose where the key comes from — the crate only takes a `String`.

### How do I know whether a result is precise enough?

Check `result.geometry.location_type`: `is_street_level()` is true for `ROOFTOP` and
`RANGE_INTERPOLATED`, false for a city or postal-code centroid. `most_precise(&results)`
picks the best result of a set.

### Does it cache results?

No, and that is deliberate — caching, deduplication and quota control are the caller's
responsibility (and Google caps caching at 30 days anyway).

### Which async runtime does it need?

Tokio — `reqwest` requires it. The crate itself does not depend on `tokio` directly.

### What is the minimum supported Rust version?

1.88, declared via `rust-version` and checked in CI.

### How do I test code that uses this crate without calling Google?

Point the client at a mock server: `GeocodingClient::new(key).with_base_url(mock.uri())`.
That is exactly how this crate's own `tests/http.rs` works (wiremock, no network).

## Development

```sh
cargo test                                     # unit + HTTP integration tests, no network needed
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
GOOGLE_MAPS_API_KEY=xxxx cargo run --example geocode -- "1-1-1 Dogenzaka, Shibuya, Tokyo"
```

Tests cover parsing, status interpretation, address-component helpers, query building and
`Serialize` round-trips, plus wiremock-backed integration tests (one call really is one
request, error classification, Google-compatible query encoding, key redaction).

### Live tests

`tests/live.rs` talks to the real API. Those tests are `#[ignore]`d, so `cargo test` and CI
never call Google; run them deliberately when you want to confirm the crate against the
live service (roughly a dozen billed requests):

```sh
GOOGLE_MAPS_API_KEY=xxxx cargo test --test live -- --ignored --nocapture
```

They check what a mock cannot: that Google accepts the queries this crate builds, that
`japan()` really returns Japanese components, that a Place ID round-trips to the same spot,
that a real `REQUEST_DENIED` keeps the key out of the error — and that no new response field
has appeared that the model does not cover.

Contributor and AI-agent notes live in [AGENTS.md](AGENTS.md); a condensed machine-readable
summary is in [llms.txt](llms.txt).

## License

MIT — see [LICENSE](LICENSE).

This project is not affiliated with or endorsed by Google. "Google Maps" is a trademark of
Google LLC.
