//! Live tests against the real Google Geocoding API.
//!
//! Every test here is `#[ignore]`d: `cargo test` skips them, so CI and everyday runs stay
//! offline and free. Run them deliberately, with a key that has the Geocoding API enabled:
//!
//! ```sh
//! GOOGLE_MAPS_API_KEY=xxxx cargo test --test live -- --ignored --nocapture
//! ```
//!
//! They cost roughly a dozen billed requests per full run. What they check is exactly what
//! a mock server cannot: that the queries this crate builds are accepted by Google, that
//! the responses still fit the model, and that a real `REQUEST_DENIED` keeps the key out of
//! the error.
use uiuifree_gmaps_geocoder::{
    most_precise, Component, Error, GeocodeRequest, GeocodeResponse, GeocodeStatus,
    GeocodingClient, RetryConfig,
};

/// 実 API のキー。無ければ「実行の仕方を間違えた」ので落とす。
fn api_key() -> String {
    std::env::var("GOOGLE_MAPS_API_KEY").expect(
        "live tests need a real key: GOOGLE_MAPS_API_KEY=xxxx cargo test --test live -- --ignored",
    )
}

/// 既定設定のテスト用クライアント。
fn client() -> GeocodingClient {
    GeocodingClient::new(api_key())
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn 住所から座標を引ける() {
    let results = client()
        .geocode("1600 Amphitheatre Parkway, Mountain View, CA")
        .await
        .expect("forward geocoding failed");

    let best = most_precise(&results).expect("expected at least one result");
    let loc = best.location();
    println!(
        "{} => {}, {} [{}]",
        best.formatted_address, loc.lat, loc.lng, best.geometry.location_type
    );

    // Googleplex 周辺。桁・符号・緯度経度の取り違えはここで落ちる。
    assert!(
        (37.3..37.5).contains(&loc.lat) && (-122.2..-121.9).contains(&loc.lng),
        "coordinates are not near the Googleplex: {loc:?}"
    );
    assert!(
        best.geometry.location_type.precision_rank() >= 1,
        "expected a reasonably precise result, got {}",
        best.geometry.location_type
    );
    assert!(!best.place_id.is_empty(), "place_id should be present");
    assert_eq!(best.country().map(|c| c.short_name.as_str()), Some("US"));
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn japanの既定で日本語の住所要素が返る() {
    let results = client()
        .send(GeocodeRequest::japan("東京都渋谷区道玄坂1-1-1"))
        .await
        .expect("forward geocoding failed");

    let best = most_precise(&results).expect("expected at least one result");
    println!(
        "{} / 〒{:?} / {:?} {:?} [{}]",
        best.formatted_address,
        best.postal_code(),
        best.prefecture(),
        best.locality(),
        best.geometry.location_type
    );

    // language=ja が効いていれば都道府県は日本語で返る。
    assert_eq!(best.prefecture(), Some("東京都"));
    assert_eq!(best.locality(), Some("渋谷区"));
    // components=country:JP が効いていれば同名の海外の地名は混ざらない。
    assert_eq!(best.country().map(|c| c.short_name.as_str()), Some("JP"));
    assert!(best.postal_code().is_some(), "expected a postal code");
    assert!(
        best.geometry.location_type.is_street_level(),
        "expected a street-level match, got {}",
        best.geometry.location_type
    );
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn 座標から住所を引ける() {
    let results = client()
        .send(GeocodeRequest::reverse(35.6580, 139.6994).language("ja"))
        .await
        .expect("reverse geocoding failed");

    assert!(!results.is_empty(), "expected at least one result");
    for r in results.iter().take(3) {
        println!("{} [{}]", r.formatted_address, r.geometry.location_type);
    }
    // 渋谷駅前の座標。もっとも詳しい結果から順に返るので先頭に区名が入る。
    assert!(
        results[0].formatted_address.contains("渋谷"),
        "unexpected address for Shibuya coordinates: {}",
        results[0].formatted_address
    );
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn place_idで引き直すと同じ場所に戻る() {
    let client = client();
    let first = client
        .send(GeocodeRequest::japan("東京都渋谷区渋谷2-24-12"))
        .await
        .expect("forward geocoding failed");
    let origin = first.first().expect("expected at least one result");

    let again = client
        .geocode_place_id(&origin.place_id)
        .await
        .expect("place_id lookup failed");
    let round_tripped = again.first().expect("place_id returned no result");

    println!(
        "{} => {} ({})",
        origin.formatted_address, round_tripped.formatted_address, origin.place_id
    );
    let (a, b) = (origin.location(), round_tripped.location());
    assert!(
        (a.lat - b.lat).abs() < 0.01 && (a.lng - b.lng).abs() < 0.01,
        "place_id lookup landed somewhere else: {a:?} vs {b:?}"
    );
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn componentフィルタで国を絞れる() {
    // フィルタ無しなら France が返る名前を、US に限定して引く。
    let results = client()
        .send(GeocodeRequest::address("Paris").component(Component::Country("US")))
        .await
        .expect("forward geocoding failed");

    let best = results.first().expect("expected at least one result");
    println!("{}", best.formatted_address);
    assert_eq!(
        best.country().map(|c| c.short_name.as_str()),
        Some("US"),
        "components=country:US did not restrict the result"
    );
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn 存在しない住所はzero_resultsで空になる() {
    let results = client()
        .geocode("zzqqxx nonexistent place 8f3k2 qqzz")
        .await
        .expect("ZERO_RESULTS must not be an error");
    assert!(results.is_empty(), "expected no results, got {results:#?}");
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn 不正なキーはrequest_deniedになりキーがエラーに出ない() {
    const BAD_KEY: &str = "AIza-this-key-is-not-valid-0123456789";
    // 再試行しても直らないので待たない。
    let client = GeocodingClient::new(BAD_KEY).with_retry(RetryConfig::disabled());

    let err = client
        .geocode("Mountain View, CA")
        .await
        .expect_err("an invalid key must be an error");
    println!("{err}");

    match &err {
        Error::Api { status, .. } => assert_eq!(*status, GeocodeStatus::RequestDenied),
        other => panic!("expected Error::Api, got {other:?}"),
    }
    assert!(!err.is_retryable(), "REQUEST_DENIED must not be retryable");
    // 本番の失敗経路でもキーが漏れないこと（モックでは通ってもここで落ちうる）。
    let rendered = format!("{err} {err:?}");
    assert!(
        !rendered.contains(BAD_KEY),
        "the API key leaked into the error: {rendered}"
    );
}

#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn 実レスポンスはserializeで往復できる() {
    let resp = client()
        .send_response(GeocodeRequest::japan("東京都渋谷区道玄坂1-1-1"))
        .await
        .expect("forward geocoding failed");
    assert_eq!(resp.status, GeocodeStatus::Ok);

    let json = serde_json::to_string(&resp).expect("serialize failed");
    let again: GeocodeResponse = serde_json::from_str(&json).expect("deserialize failed");
    assert_eq!(resp, again, "a real response did not survive a round trip");
}

/// Google が返すフィールドをこのクレートが取りこぼしていないかを見る。
///
/// モックでは絶対に落ちないテスト。API 側に項目が増えたらここで気づける。
#[tokio::test]
#[ignore = "calls the real Google API (billed); needs GOOGLE_MAPS_API_KEY"]
async fn レスポンスに未対応のフィールドが増えていないか確認する() {
    const TOP_LEVEL: &[&str] = &[
        "status",
        "results",
        "error_message",
        "plus_code",
        "address_descriptor",
    ];
    const RESULT: &[&str] = &[
        "formatted_address",
        "geometry",
        "place_id",
        "types",
        "address_components",
        "plus_code",
        "postcode_localities",
        "address_descriptor",
        "partial_match",
    ];
    const GEOMETRY: &[&str] = &["location", "location_type", "viewport", "bounds"];

    let key = api_key();
    let raw: serde_json::Value = reqwest::Client::new()
        .get("https://maps.googleapis.com/maps/api/geocode/json")
        .query(&[
            ("key", key.as_str()),
            ("address", "東京都渋谷区道玄坂1-1-1"),
            ("language", "ja"),
            ("region", "jp"),
        ])
        .send()
        .await
        // 失敗時の panic 出力に URL（key= を含む）を出さない。
        .map_err(reqwest::Error::without_url)
        .expect("raw request failed")
        .json()
        .await
        .map_err(reqwest::Error::without_url)
        .expect("raw response was not JSON");

    let unknown = |value: &serde_json::Value, known: &[&str]| -> Vec<String> {
        value
            .as_object()
            .map(|map| {
                map.keys()
                    .filter(|k| !known.contains(&k.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut missing = unknown(&raw, TOP_LEVEL);
    if let Some(result) = raw.get("results").and_then(|r| r.get(0)) {
        missing.extend(
            unknown(result, RESULT)
                .into_iter()
                .map(|k| format!("results[].{k}")),
        );
        if let Some(geometry) = result.get("geometry") {
            missing.extend(
                unknown(geometry, GEOMETRY)
                    .into_iter()
                    .map(|k| format!("results[].geometry.{k}")),
            );
        }
    }

    assert!(
        missing.is_empty(),
        "Google returned fields this crate does not model: {missing:?}. \
         Add them to src/model.rs and to the lists in this test."
    );
}
