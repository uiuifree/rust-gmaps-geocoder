//! HTTP 結合テスト（wiremock）。
//!
//! 1回の呼び出しが**ちょうど1リクエスト**であること・エラーの分類・クエリパラメータが
//! Google 互換の形でサーバに届くか（`|` のパーセントエンコードがデコードされるか）を、
//! モックサーバ視点で固定化する。
use uiuifree_gmaps_geocoder::{
    Bounds, Component, Error, GeocodeRequest, GeocodeStatus, GeocodingClient, LatLng, LocationType,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// テスト用の最小 OK レスポンス。
fn ok_body() -> serde_json::Value {
    serde_json::json!({
        "status": "OK",
        "results": [{
            "formatted_address": "日本、東京都渋谷区",
            "place_id": "p1",
            "geometry": {
                "location": {"lat": 35.658, "lng": 139.6994},
                "location_type": "ROOFTOP"
            }
        }]
    })
}

/// モックサーバ向けクライアント。
fn client(server: &MockServer) -> GeocodingClient {
    GeocodingClient::new("TEST_KEY").with_base_url(server.uri())
}

/// 常に同じ応答を返すモックを1つ立てる。
async fn mount_always(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(response)
        .mount(server)
        .await;
}

#[tokio::test]
async fn 一度の呼び出しはちょうど1リクエストで再試行しない() {
    let server = MockServer::start().await;
    // OVER_QUERY_LIMIT は「待てば直る」種類のエラーだが、それでも自動では投げ直さない。
    mount_always(
        &server,
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"status": "OVER_QUERY_LIMIT", "results": []})),
    )
    .await;

    let err = client(&server).geocode("渋谷").await.unwrap_err();
    assert!(
        err.is_retryable(),
        "OVER_QUERY_LIMIT は呼び出し側が再試行を判断できる分類のはず"
    );
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "ライブラリが勝手に投げ直している"
    );
}

#[tokio::test]
async fn request_deniedは再試行しても無駄だと分かる() {
    let server = MockServer::start().await;
    mount_always(
        &server,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "REQUEST_DENIED",
            "results": [],
            "error_message": "The provided API key is invalid."
        })),
    )
    .await;

    let err = client(&server).geocode("渋谷").await.unwrap_err();
    assert!(!err.is_retryable());
    match err {
        Error::Api { status, message } => {
            assert_eq!(status, GeocodeStatus::RequestDenied);
            assert_eq!(message.as_deref(), Some("The provided API key is invalid."));
        }
        other => panic!("Error::Api を期待: {other:?}"),
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn http500は再試行可能なhttpエラーになる() {
    let server = MockServer::start().await;
    mount_always(&server, ResponseTemplate::new(500)).await;

    let err = client(&server).geocode("渋谷").await.unwrap_err();
    assert!(matches!(err, Error::Http(_)), "Error::Http を期待: {err:?}");
    assert!(err.is_retryable(), "5xx は再試行可能扱いのはず");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // エラーに API キーが漏れていない（without_url() の検証）
    assert!(
        !format!("{err}").contains("TEST_KEY"),
        "エラー表示にAPIキーが漏れている: {err}"
    );
    assert!(
        !format!("{err:?}").contains("TEST_KEY"),
        "エラーDebugにAPIキーが漏れている: {err:?}"
    );
}

#[tokio::test]
async fn http429は再試行可能なhttpエラーになる() {
    let server = MockServer::start().await;
    mount_always(&server, ResponseTemplate::new(429)).await;

    let err = client(&server).geocode("渋谷").await.unwrap_err();
    assert!(matches!(err, Error::Http(_)), "Error::Http を期待: {err:?}");
    assert!(err.is_retryable(), "429 は再試行可能扱いのはず");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn 呼び出し側で再試行すれば回復する() {
    let server = MockServer::start().await;
    // 1回目だけ 500、以後は OK。呼び出し側が投げ直す前提の形。
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    let err = client.geocode("渋谷").await.unwrap_err();
    assert!(err.is_retryable(), "ここで再試行の可否を判断する");

    let results = client
        .geocode("渋谷")
        .await
        .expect("投げ直せば成功するはず");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].place_id, "p1");
}

#[tokio::test]
async fn forwardの全パラメータがgoogle互換の形でサーバに届く() {
    let server = MockServer::start().await;
    // wiremock はパーセントエンコードをデコードして比較する。
    // `components=country:JP|postal_code:150-0043` が `%7C`/`%3A` 経由でも正しく届くことを固定化。
    Mock::given(method("GET"))
        .and(path("/"))
        .and(query_param("key", "TEST_KEY"))
        .and(query_param("address", "道玄坂"))
        .and(query_param("language", "ja"))
        .and(query_param("region", "jp"))
        .and(query_param("bounds", "35.5,139.5|35.8,139.9"))
        .and(query_param("components", "country:JP|postal_code:150-0043"))
        .and(query_param("extra_computations", "ADDRESS_DESCRIPTORS"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
        .expect(1)
        .mount(&server)
        .await;

    let req = GeocodeRequest::address("道玄坂")
        .language("ja")
        .region("jp")
        .bounds(Bounds {
            southwest: LatLng {
                lat: 35.5,
                lng: 139.5,
            },
            northeast: LatLng {
                lat: 35.8,
                lng: 139.9,
            },
        })
        .component(Component::Country("JP"))
        .component(Component::PostalCode("150-0043"))
        .address_descriptors(true);
    let results = client(&server)
        .send(req)
        .await
        .expect("全パラメータ付きで成功するはず");
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn reverseのフィルタがサーバに届く() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .and(query_param("latlng", "35.658,139.6994"))
        .and(query_param("result_type", "street_address|locality"))
        .and(query_param("location_type", "ROOFTOP"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
        .expect(1)
        .mount(&server)
        .await;

    let req = GeocodeRequest::reverse(35.658, 139.6994)
        .result_type("street_address")
        .result_type("locality")
        .location_type(LocationType::Rooftop);
    let results = client(&server)
        .send(req)
        .await
        .expect("reverse フィルタ付きで成功するはず");
    assert_eq!(results.len(), 1);
}
