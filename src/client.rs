//! The client, its retry policy, and the request builder.
use crate::error::Error;
use crate::model::{Bounds, GeocodeResponse, GeocodeResult};
use crate::types::{Component, GeocodeStatus, LocationType};
use std::time::Duration;

/// The JSON endpoint of the Geocoding API.
const DEFAULT_BASE_URL: &str = "https://maps.googleapis.com/maps/api/geocode/json";

/// How the client retries failures that look temporary.
///
/// The delay before attempt `n` is `base_delay * 2^n`, capped at `max_delay` — with the
/// defaults that is 500 ms, 1 s, 2 s. Retries cover `OVER_QUERY_LIMIT`, `UNKNOWN_ERROR`,
/// HTTP 429 and 5xx, timeouts and connection failures; see [`Error::is_retryable`].
///
/// There is no jitter and the `Retry-After` header is ignored, both on purpose.
///
/// ```
/// use std::time::Duration;
/// use uiuifree_gmaps_geocoder::RetryConfig;
///
/// // Defaults: 3 retries, 500ms doubling, capped at 8s.
/// let default = RetryConfig::default();
/// assert_eq!(default.max_retries, 3);
///
/// // More patient, and quicker off the mark.
/// let patient = RetryConfig {
///     max_retries: 5,
///     base_delay: Duration::from_millis(300),
///     ..RetryConfig::default()
/// };
///
/// // Or fail on the first error.
/// assert_eq!(RetryConfig::disabled().max_retries, 0);
/// # let _ = patient;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    /// How many *extra* attempts to make after the first one. `0` disables retrying.
    pub max_retries: u32,
    /// How long to wait before the first retry. Each further retry doubles it.
    pub base_delay: Duration,
    /// Ceiling for the doubling, so a long run of failures cannot stall a request forever.
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
        }
    }
}

impl RetryConfig {
    /// Never retry: fail on the first error, whatever it is.
    ///
    /// Use it when the caller already has its own retry or queueing layer, or when a
    /// request is on a latency budget that a backoff would blow.
    pub fn disabled() -> Self {
        Self {
            max_retries: 0,
            ..Self::default()
        }
    }

    /// `attempt` 回目（0始まり）の待機時間 = `base_delay * 2^attempt`（`max_delay` で頭打ち）。
    fn delay_for(&self, attempt: u32) -> Duration {
        // シフトは u32 に収まる 31 まで（それ以上は saturating_mul と max_delay が頭打ちにする）。
        // 16 で止めると、max_delay が十分大きい設定で 16 回目以降だけ倍々が止まってしまう。
        let mult = 1u32 << attempt.min(31);
        self.base_delay.saturating_mul(mult).min(self.max_delay)
    }
}

/// An async client for the Google Maps Geocoding API.
///
/// Build one per API key and share it: it wraps a `reqwest::Client`, so `clone` is cheap
/// and clones share the same connection pool. Retrying is on by default ([`RetryConfig`]).
///
/// The three shorthands — [`geocode`](Self::geocode),
/// [`reverse_geocode`](Self::reverse_geocode), [`geocode_place_id`](Self::geocode_place_id)
/// — cover the common cases; [`send`](Self::send) takes a [`GeocodeRequest`] when you need
/// language, region, bias or filters.
///
/// `Debug` prints the API key as `[REDACTED]`.
///
/// ```no_run
/// # async fn run() -> Result<(), uiuifree_gmaps_geocoder::Error> {
/// use uiuifree_gmaps_geocoder::GeocodingClient;
/// let client = GeocodingClient::new("YOUR_API_KEY");
/// let results = client.geocode("東京都渋谷区道玄坂1-1-1").await?;
/// if let Some(first) = results.first() {
///     let loc = first.location();
///     println!("{}, {} ({:?})", loc.lat, loc.lng, first.prefecture());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct GeocodingClient {
    api_key: String,
    http: reqwest::Client,
    base_url: String,
    retry: RetryConfig,
}

/// `api_key` を含めない手動実装（derive すると Debug 出力にキーが漏れる）。
impl std::fmt::Debug for GeocodingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeocodingClient")
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl GeocodingClient {
    /// A client for the given [API key][key], with a default HTTP client and default
    /// retry policy.
    ///
    /// The default HTTP client carries a **10 second total timeout**, because without one
    /// a stalled connection would hang forever and the retry loop would never get its
    /// turn. Pass your own client to [`with_http_client`](Self::with_http_client) to
    /// change that, or anything else (proxy, connection pool, headers).
    ///
    /// Where the key comes from is up to you — this crate only stores the string, and
    /// keeps it out of `Debug` output and error messages.
    ///
    /// [key]: https://developers.google.com/maps/documentation/geocoding/get-api-key
    ///
    /// # Panics
    ///
    /// If the platform TLS stack cannot be initialised at all, which is the only way
    /// building the default `reqwest::Client` can fail.
    pub fn new(api_key: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("building the default HTTP client cannot fail");
        Self::with_http_client(api_key, http)
    }

    /// A client that uses a `reqwest::Client` you configured yourself — custom timeouts, a
    /// proxy, a shared connection pool, or your own middleware stack.
    ///
    /// Unlike [`new`](Self::new), no timeout is imposed here; set one on the client you
    /// pass in, or a hung connection will never reach the retry loop.
    pub fn with_http_client(api_key: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            api_key: api_key.into(),
            http,
            base_url: DEFAULT_BASE_URL.to_string(),
            retry: RetryConfig::default(),
        }
    }

    /// Replace the retry policy — see [`RetryConfig`], and `RetryConfig::disabled()` to
    /// turn retrying off.
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Send requests somewhere other than Google: a mock server in tests, or a proxy that
    /// speaks the same JSON.
    ///
    /// ```no_run
    /// # use uiuifree_gmaps_geocoder::GeocodingClient;
    /// # let mock_server_uri = "http://127.0.0.1:8080";
    /// // Point the client at a wiremock/httpmock server and no request leaves the machine.
    /// let client = GeocodingClient::new("TEST_KEY").with_base_url(mock_server_uri);
    /// ```
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Address → coordinates (*forward geocoding*).
    ///
    /// Results come back in Google's relevance order; `ZERO_RESULTS` is an empty `Vec`,
    /// not an error. For language, region bias or component filters, build a
    /// [`GeocodeRequest`] and use [`send`](Self::send) — or
    /// [`GeocodeRequest::japan`] for Japanese addresses.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] for any status other than `OK`/`ZERO_RESULTS`, [`Error::Http`] for
    /// transport failures — in both cases only after the retries are exhausted.
    ///
    /// ```no_run
    /// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient) -> Result<(), uiuifree_gmaps_geocoder::Error> {
    /// let results = client.geocode("1600 Amphitheatre Parkway, Mountain View, CA").await?;
    /// let Some(best) = uiuifree_gmaps_geocoder::most_precise(&results) else {
    ///     return Ok(()); // no match
    /// };
    /// println!("{}, {}", best.location().lat, best.location().lng);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn geocode(&self, address: &str) -> Result<Vec<GeocodeResult>, Error> {
        self.send(GeocodeRequest::address(address)).await
    }

    /// Coordinates → address (*reverse geocoding*).
    ///
    /// Google answers from the most specific address outwards to the containing areas, so
    /// the first result is usually the one you want. Narrow it further with
    /// [`GeocodeRequest::result_type`] / [`GeocodeRequest::location_type`]. No address at
    /// those coordinates is an empty `Vec`, not an error.
    ///
    /// # Errors
    ///
    /// Same as [`geocode`](Self::geocode).
    ///
    /// ```no_run
    /// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient) -> Result<(), uiuifree_gmaps_geocoder::Error> {
    /// for r in client.reverse_geocode(35.6580, 139.6994).await? {
    ///     println!("{}", r.formatted_address);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn reverse_geocode(&self, lat: f64, lng: f64) -> Result<Vec<GeocodeResult>, Error> {
        self.send(GeocodeRequest::reverse(lat, lng)).await
    }

    /// Place ID → address and coordinates.
    ///
    /// The way to refresh a place you stored earlier: `place_id` is the one field Google's
    /// terms let you keep indefinitely, so store it and re-resolve the address later
    /// instead of caching coordinates.
    ///
    /// # Errors
    ///
    /// Same as [`geocode`](Self::geocode); an unknown or retired Place ID comes back as
    /// [`Error::Api`] with `INVALID_REQUEST`.
    pub async fn geocode_place_id(&self, place_id: &str) -> Result<Vec<GeocodeResult>, Error> {
        self.send(GeocodeRequest::place_id(place_id)).await
    }

    /// Send a request built with [`GeocodeRequest`] — the full-control entry point, with
    /// retrying — and return its results.
    ///
    /// # Errors
    ///
    /// Same as [`geocode`](Self::geocode).
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), uiuifree_gmaps_geocoder::Error> {
    /// use uiuifree_gmaps_geocoder::{Component, GeocodeRequest, GeocodingClient};
    /// let client = GeocodingClient::new("YOUR_API_KEY");
    /// let req = GeocodeRequest::address("道玄坂")
    ///     .language("ja")
    ///     .component(Component::Country("JP"));
    /// let results = client.send(req).await?;
    /// # let _ = results;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(&self, req: GeocodeRequest<'_>) -> Result<Vec<GeocodeResult>, Error> {
        Ok(self.send_response(req).await?.results)
    }

    /// Like [`send`](Self::send), but returns the whole [`GeocodeResponse`].
    ///
    /// Use it when you need what sits beside the results: the top-level
    /// [`plus_code`](GeocodeResponse::plus_code) of a reverse geocode, or the top-level
    /// [`address_descriptor`](GeocodeResponse::address_descriptor).
    ///
    /// # Errors
    ///
    /// Same as [`geocode`](Self::geocode).
    pub async fn send_response(&self, req: GeocodeRequest<'_>) -> Result<GeocodeResponse, Error> {
        let params = req.to_params(&self.api_key);
        let mut attempt = 0u32;
        loop {
            match self.try_once(&params).await {
                Ok(resp) => return Ok(resp),
                Err(e) if attempt < self.retry.max_retries && e.is_retryable() => {
                    tokio::time::sleep(self.retry.delay_for(attempt)).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 1回だけ HTTP を叩いて結果を解釈する（再試行しない）。
    async fn try_once(&self, params: &[(&str, String)]) -> Result<GeocodeResponse, Error> {
        let response: GeocodeResponse = self
            .http
            .get(&self.base_url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        interpret(response)
    }
}

/// レスポンスの `status` を解釈する。`OK` はそのまま、`ZERO_RESULTS` は空 results、
/// それ以外は [`Error::Api`]。
fn interpret(response: GeocodeResponse) -> Result<GeocodeResponse, Error> {
    match response.status {
        GeocodeStatus::Ok | GeocodeStatus::ZeroResults => Ok(response),
        other => Err(Error::Api {
            status: other,
            message: response.error_message,
        }),
    }
}

/// A geocoding request: what to look up, plus any options.
///
/// Start from one of the four constructors — [`address`](Self::address),
/// [`japan`](Self::japan), [`reverse`](Self::reverse), [`place_id`](Self::place_id) —
/// chain the options you need, and hand it to [`GeocodingClient::send`].
///
/// It borrows its inputs rather than copying them, so it is built and sent in one
/// expression; `clone` it if you want to keep one around as a template. Options that do
/// not apply to a lookup (a `bounds` bias on a reverse geocode, say) are simply ignored by
/// Google.
///
/// ```no_run
/// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient) -> Result<(), uiuifree_gmaps_geocoder::Error> {
/// use uiuifree_gmaps_geocoder::{Bounds, Component, GeocodeRequest, LatLng};
///
/// let tokyo = Bounds {
///     southwest: LatLng { lat: 35.5, lng: 139.5 },
///     northeast: LatLng { lat: 35.8, lng: 139.9 },
/// };
/// let req = GeocodeRequest::address("道玄坂")
///     .language("ja")
///     .region("jp")
///     .bounds(tokyo)                        // prefer hits inside this box
///     .component(Component::Country("JP")); // and only Japanese ones
/// let results = client.send(req).await?;
/// # let _ = results;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct GeocodeRequest<'a> {
    query: Query<'a>,
    language: Option<&'a str>,
    region: Option<&'a str>,
    bounds: Option<Bounds>,
    components: Vec<Component<'a>>,
    result_types: Vec<&'a str>,
    location_types: Vec<LocationType>,
    address_descriptors: bool,
}

/// 検索キー（住所 / 座標 / Place ID）。
#[derive(Debug, Clone)]
enum Query<'a> {
    Address(&'a str),
    LatLng(f64, f64),
    PlaceId(&'a str),
}

impl<'a> GeocodeRequest<'a> {
    /// Look up an address (*forward geocoding*).
    ///
    /// Google parses free-form input, but a complete, correctly formatted address gets the
    /// best match — check [`partial_match`](crate::GeocodeResult::partial_match) when the
    /// input may be rough.
    pub fn address(address: &'a str) -> Self {
        Self::new(Query::Address(address))
    }

    /// [`address`](Self::address) with Japanese defaults applied: `language=ja`,
    /// `region=jp` and `components=country:JP`.
    ///
    /// Results come back in Japanese, ambiguous names resolve to the Japanese place, and
    /// same-spelled places abroad are filtered out.
    ///
    /// ```no_run
    /// # async fn run(client: &uiuifree_gmaps_geocoder::GeocodingClient) -> Result<(), uiuifree_gmaps_geocoder::Error> {
    /// use uiuifree_gmaps_geocoder::GeocodeRequest;
    /// let results = client.send(GeocodeRequest::japan("東京都渋谷区渋谷2-24-12")).await?;
    /// # let _ = results;
    /// # Ok(())
    /// # }
    /// ```
    pub fn japan(address: &'a str) -> Self {
        Self::address(address)
            .language("ja")
            .region("jp")
            .component(Component::Country("JP"))
    }

    /// Look up coordinates (*reverse geocoding*), in decimal degrees.
    pub fn reverse(lat: f64, lng: f64) -> Self {
        Self::new(Query::LatLng(lat, lng))
    }

    /// Look up a Google Place ID.
    pub fn place_id(place_id: &'a str) -> Self {
        Self::new(Query::PlaceId(place_id))
    }

    fn new(query: Query<'a>) -> Self {
        Self {
            query,
            language: None,
            region: None,
            bounds: None,
            components: Vec::new(),
            result_types: Vec::new(),
            location_types: Vec::new(),
            address_descriptors: false,
        }
    }

    /// The language results are returned in, as a [language code][langs] such as `"ja"` or
    /// `"en"`. Google falls back to the address's own locale when omitted.
    ///
    /// [langs]: https://developers.google.com/maps/faq#languagesupport
    pub fn language(mut self, language: &'a str) -> Self {
        self.language = Some(language);
        self
    }

    /// Region bias for forward geocoding: a ccTLD such as `"jp"` or `"uk"`, which tilts
    /// ambiguous names towards that country without excluding others.
    ///
    /// To *exclude* other countries instead, filter with
    /// [`Component::Country`](crate::Component::Country).
    pub fn region(mut self, region: &'a str) -> Self {
        self.region = Some(region);
        self
    }

    /// Viewport bias for forward geocoding: prefer results inside this box.
    ///
    /// A bias, not a filter — a strong match outside the box can still be returned.
    pub fn bounds(mut self, bounds: Bounds) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Add a [`Component`] filter. Call it repeatedly to require several at once (the API
    /// ANDs them).
    ///
    /// Unlike [`region`](Self::region) this really does restrict the results, which makes
    /// `Component::Country("JP")` the reliable way to keep a lookup inside Japan.
    pub fn component(mut self, component: Component<'a>) -> Self {
        self.components.push(component);
        self
    }

    /// Restrict a reverse geocode to certain [address types][types] — `"street_address"`,
    /// `"locality"`, `"postal_code"`, … Call it repeatedly to accept any of them.
    ///
    /// Handy when you want "the city this point is in" rather than the full address.
    ///
    /// [types]: https://developers.google.com/maps/documentation/geocoding/requests-reverse-geocoding#reverse-restricted
    pub fn result_type(mut self, result_type: &'a str) -> Self {
        self.result_types.push(result_type);
        self
    }

    /// Restrict a reverse geocode to certain accuracies — pass
    /// [`LocationType::Rooftop`] to accept only building-level answers. Call it repeatedly
    /// to accept any of them.
    ///
    /// Note that filtering can leave you with nothing (`ZERO_RESULTS`, i.e. an empty
    /// `Vec`) where an unfiltered request would have returned an approximate answer.
    pub fn location_type(mut self, location_type: LocationType) -> Self {
        self.location_types.push(location_type);
        self
    }

    /// Ask for [address descriptors][descriptors] (`extra_computations=ADDRESS_DESCRIPTORS`):
    /// nearby landmarks and areas describing the place.
    ///
    /// Beta, and only available in some regions. The payload arrives as raw JSON in
    /// [`GeocodeResult::address_descriptor`](crate::GeocodeResult::address_descriptor) and
    /// [`GeocodeResponse::address_descriptor`](crate::GeocodeResponse::address_descriptor)
    /// because its shape can still change.
    ///
    /// [descriptors]: https://developers.google.com/maps/documentation/geocoding/address-descriptors/requests-address-descriptors
    pub fn address_descriptors(mut self, enable: bool) -> Self {
        self.address_descriptors = enable;
        self
    }

    /// クエリパラメータへ展開する。
    fn to_params(&self, api_key: &str) -> Vec<(&'static str, String)> {
        let mut params: Vec<(&'static str, String)> = Vec::with_capacity(8);
        params.push(("key", api_key.to_string()));
        match self.query {
            Query::Address(a) => params.push(("address", a.to_string())),
            Query::LatLng(lat, lng) => params.push(("latlng", format!("{lat},{lng}"))),
            Query::PlaceId(id) => params.push(("place_id", id.to_string())),
        }
        if let Some(v) = self.language {
            params.push(("language", v.to_string()));
        }
        if let Some(v) = self.region {
            params.push(("region", v.to_string()));
        }
        if let Some(b) = self.bounds {
            // bounds=southwest_lat,southwest_lng|northeast_lat,northeast_lng
            params.push((
                "bounds",
                format!(
                    "{},{}|{},{}",
                    b.southwest.lat, b.southwest.lng, b.northeast.lat, b.northeast.lng
                ),
            ));
        }
        if !self.components.is_empty() {
            params.push((
                "components",
                join_pipe(self.components.iter().map(|c| c.to_param())),
            ));
        }
        if !self.result_types.is_empty() {
            params.push((
                "result_type",
                join_pipe(self.result_types.iter().map(|t| t.to_string())),
            ));
        }
        if !self.location_types.is_empty() {
            params.push((
                "location_type",
                join_pipe(self.location_types.iter().map(|t| t.as_str().to_string())),
            ));
        }
        if self.address_descriptors {
            params.push(("extra_computations", "ADDRESS_DESCRIPTORS".to_string()));
        }
        params
    }
}

/// Google のリスト系パラメータ区切り（`|`）で連結する。
fn join_pipe(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LocationType;

    fn sample_ok() -> &'static str {
        r#"{
          "status": "OK",
          "results": [{
            "formatted_address": "日本、〒150-0043 東京都渋谷区道玄坂１丁目１−１",
            "place_id": "ChIJ_abc123",
            "types": ["street_address"],
            "plus_code": {"global_code": "8Q7XMP2H+8Q", "compound_code": "MP2H+8Q 渋谷区"},
            "address_components": [
              {"long_name": "1-1", "short_name": "1-1", "types": ["premise"]},
              {"long_name": "渋谷区", "short_name": "渋谷区", "types": ["locality", "political"]},
              {"long_name": "東京都", "short_name": "Tokyo", "types": ["administrative_area_level_1", "political"]},
              {"long_name": "日本", "short_name": "JP", "types": ["country", "political"]},
              {"long_name": "150-0043", "short_name": "150-0043", "types": ["postal_code"]}
            ],
            "geometry": {
              "location": {"lat": 35.6580, "lng": 139.6994},
              "location_type": "ROOFTOP",
              "viewport": {
                "northeast": {"lat": 35.66, "lng": 139.70},
                "southwest": {"lat": 35.65, "lng": 139.69}
              }
            }
          }]
        }"#
    }

    fn results_of(json: &str) -> Vec<GeocodeResult> {
        interpret(serde_json::from_str(json).unwrap())
            .unwrap()
            .results
    }

    #[test]
    fn ok応答をパースして座標と住所要素とpluscodeを取り出す() {
        let results = results_of(sample_ok());
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(
            r.location(),
            crate::LatLng {
                lat: 35.6580,
                lng: 139.6994
            }
        );
        assert_eq!(r.geometry.location_type, LocationType::Rooftop);
        assert_eq!(r.prefecture(), Some("東京都"));
        assert_eq!(r.locality(), Some("渋谷区"));
        assert_eq!(r.postal_code(), Some("150-0043"));
        assert_eq!(r.country().map(|c| c.short_name.as_str()), Some("JP"));
        assert_eq!(
            r.component("premise").map(|c| c.long_name.as_str()),
            Some("1-1")
        );
        assert!(r.component("route").is_none());
        // Plus Code
        assert_eq!(r.plus_code.as_ref().unwrap().global_code, "8Q7XMP2H+8Q");
    }

    #[test]
    fn 郵便番号のpostcode_localitiesとトップレベルpluscodeを拾う() {
        let json = r#"{
          "status": "OK",
          "plus_code": {"global_code": "8Q7XMP00+00"},
          "results": [{
            "formatted_address": "〒150-0043 東京都渋谷区",
            "place_id": "ChIJ_zip",
            "types": ["postal_code"],
            "postcode_localities": ["渋谷", "神泉"],
            "address_components": [],
            "geometry": {"location": {"lat": 35.65, "lng": 139.69}, "location_type": "APPROXIMATE"}
          }]
        }"#;
        let resp: GeocodeResponse = interpret(serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(resp.plus_code.as_ref().unwrap().global_code, "8Q7XMP00+00");
        assert_eq!(resp.results[0].postcode_localities, vec!["渋谷", "神泉"]);
    }

    #[test]
    fn 未知のstatusはotherに落ちる() {
        let resp: GeocodeResponse =
            serde_json::from_str(r#"{"status":"BRAND_NEW_STATUS","results":[]}"#).unwrap();
        assert_eq!(resp.status, GeocodeStatus::Other("BRAND_NEW_STATUS".into()));
    }

    #[test]
    fn zero_resultsは空の結果になる() {
        assert!(results_of(r#"{"status":"ZERO_RESULTS","results":[]}"#).is_empty());
    }

    #[test]
    fn エラーステータスはapiエラーになり再試行判定できる() {
        let resp: GeocodeResponse = serde_json::from_str(
            r#"{"status":"REQUEST_DENIED","results":[],"error_message":"The provided API key is invalid."}"#,
        )
        .unwrap();
        match interpret(resp) {
            Err(e @ Error::Api { .. }) => {
                assert!(!e.is_retryable());
                let Error::Api { status, message } = e else {
                    unreachable!()
                };
                assert_eq!(status, GeocodeStatus::RequestDenied);
                assert_eq!(message.as_deref(), Some("The provided API key is invalid."));
            }
            other => panic!("Apiエラーを期待: {other:?}"),
        }
    }

    #[test]
    fn over_query_limitは再試行対象() {
        let resp: GeocodeResponse =
            serde_json::from_str(r#"{"status":"OVER_QUERY_LIMIT","results":[]}"#).unwrap();
        assert!(interpret(resp).unwrap_err().is_retryable());
    }

    #[test]
    fn バックオフは指数で伸び上限で頭打ち() {
        let cfg = RetryConfig {
            max_retries: 5,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
        };
        assert_eq!(cfg.delay_for(0), Duration::from_millis(500));
        assert_eq!(cfg.delay_for(1), Duration::from_secs(1));
        assert_eq!(cfg.delay_for(2), Duration::from_secs(2));
        assert_eq!(cfg.delay_for(3), Duration::from_secs(4));
        assert_eq!(cfg.delay_for(4), Duration::from_secs(8));
        assert_eq!(cfg.delay_for(20), Duration::from_secs(8));
    }

    #[test]
    fn 上限が大きければ16回目以降も倍々で伸びる() {
        let cfg = RetryConfig {
            max_retries: 40,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_secs(300),
        };
        assert_eq!(cfg.delay_for(16), Duration::from_millis(65_536));
        assert_eq!(cfg.delay_for(17), Duration::from_millis(131_072));
        // 最後は max_delay で頭打ちになる。
        assert_eq!(cfg.delay_for(40), Duration::from_secs(300));
    }

    #[test]
    fn forwardの全オプションをクエリ化する() {
        let sw = crate::LatLng {
            lat: 35.5,
            lng: 139.5,
        };
        let ne = crate::LatLng {
            lat: 35.8,
            lng: 139.9,
        };
        let req = GeocodeRequest::address("道玄坂")
            .language("ja")
            .region("jp")
            .bounds(Bounds {
                southwest: sw,
                northeast: ne,
            })
            .component(Component::Country("JP"))
            .component(Component::PostalCode("150-0043"))
            .address_descriptors(true);
        let params = req.to_params("KEY");
        let get = |k: &str| {
            params
                .iter()
                .find(|(pk, _)| *pk == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("address"), Some("道玄坂"));
        assert_eq!(get("language"), Some("ja"));
        assert_eq!(get("region"), Some("jp"));
        assert_eq!(get("bounds"), Some("35.5,139.5|35.8,139.9"));
        assert_eq!(get("components"), Some("country:JP|postal_code:150-0043"));
        assert_eq!(get("extra_computations"), Some("ADDRESS_DESCRIPTORS"));
    }

    #[test]
    fn reverseとフィルタとplace_idをクエリ化する() {
        // reverse + result_type/location_type フィルタ
        let params = GeocodeRequest::reverse(35.658, 139.6994)
            .result_type("street_address")
            .result_type("locality")
            .location_type(LocationType::Rooftop)
            .to_params("KEY");
        let get = |k: &str| {
            params
                .iter()
                .find(|(pk, _)| *pk == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("latlng"), Some("35.658,139.6994"));
        assert_eq!(get("result_type"), Some("street_address|locality"));
        assert_eq!(get("location_type"), Some("ROOFTOP"));

        // place_id
        let p2 = GeocodeRequest::place_id("ChIJ_abc").to_params("KEY");
        assert_eq!(
            p2.iter()
                .find(|(k, _)| *k == "place_id")
                .map(|(_, v)| v.as_str()),
            Some("ChIJ_abc")
        );
    }

    #[test]
    fn 結果はserializeで往復できる() {
        let resp: GeocodeResponse = serde_json::from_str(sample_ok()).unwrap();
        let json = serde_json::to_string(&resp).unwrap();
        let again: GeocodeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(again.status, GeocodeStatus::Ok);
        assert_eq!(
            again.results[0].geometry.location_type,
            LocationType::Rooftop
        );
        assert_eq!(
            again.results[0].plus_code.as_ref().unwrap().global_code,
            "8Q7XMP2H+8Q"
        );
    }
}
