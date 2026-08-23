# uiuifree-gmaps-geocoder

[![crates.io](https://img.shields.io/crates/v/uiuifree-gmaps-geocoder.svg)](https://crates.io/crates/uiuifree-gmaps-geocoder)
[![docs.rs](https://img.shields.io/docsrs/uiuifree-gmaps-geocoder)](https://docs.rs/uiuifree-gmaps-geocoder)
[![CI](https://github.com/uiuifree/rust-gmaps-geocoder/actions/workflows/ci.yml/badge.svg)](https://github.com/uiuifree/rust-gmaps-geocoder/actions/workflows/ci.yml)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://blog.rust-lang.org/)
[![license: MIT](https://img.shields.io/crates/l/uiuifree-gmaps-geocoder.svg)](LICENSE)

**Google Maps Geocoding API の非同期 Rust クライアント。** 住所から緯度経度（forward）、
座標から住所（reverse）、Place ID から両方を引く。結果は型付き、一時的な失敗は指数バックオフで
自動再試行、API キーはログに出ない。

[English README](README.md) · [APIドキュメント](https://docs.rs/uiuifree-gmaps-geocoder) · [llms.txt](llms.txt) · [変更履歴](CHANGELOG.md)

**Geocoding だけ**を扱う。`/maps/api/geocode/json` の1エンドポイントに絞っており、
Places / Routes / Static Maps・地図描画は入れない（Google Maps Platform 全体を使いたいなら
別のクレートのほうが合う）。

## 目次

- [このクレートの中身](#このクレートの中身)
- [インストール](#インストール)
- [使い方](#使い方)
- [レシピ集](#レシピ集)
- [API 早見表](#api-早見表)
- [挙動: ステータス・エラー・再試行](#挙動-ステータスエラー再試行)
- [Google の規約（呼び出し側の責務）](#google-の規約呼び出し側の責務)
- [未対応（意図的な割り切り）](#未対応意図的な割り切り)
- [使わないほうがいい場合](#使わないほうがいい場合)
- [よくある質問](#よくある質問)
- [開発](#開発)

## このクレートの中身

**Geocoding API の機能は網羅し、それ以外は持たない。**

- **3つの引き方** — 住所→座標（forward）／座標→住所（reverse）／Place ID→住所・座標
- **全リクエストパラメータ** — `language` / `region` / `bounds`（範囲バイアス）/
  `components`（絞り込み）/ `result_type`・`location_type`（reverse フィルタ）/
  `extra_computations=ADDRESS_DESCRIPTORS`（ベータ）
- **全レスポンス項目** — 座標・精度・viewport/bounds・`address_components`・`types`・
  `place_id`・`plus_code`（結果／トップレベル両方）・`postcode_localities`・`partial_match`・
  `address_descriptor`
- **文字列でなく型** — `GeocodeStatus` / `LocationType` は enum。未知の値は `Other(String)` に
  落ちるので、Google 側に新しい値が増えてもパースは壊れない
- **住所要素ヘルパー** — `postal_code()` / `prefecture()` / `locality()` / `country()` /
  `component("route")`、加えて一番正確な1件を選ぶ `most_precise(&results)`
- **再試行を標準装備** — `OVER_QUERY_LIMIT` / `UNKNOWN_ERROR` / HTTP 429・5xx /
  タイムアウト・接続失敗を指数バックオフで自動再試行
- **API キーがログに出ない** — エラーからは URL ごと除去され、`Debug` は `[REDACTED]` 表示
- **依存が薄い** — `reqwest`(rustls) / `serde`・`serde_json` / `thiserror` / `tokio`（`time` のみ）。
  OpenSSL も重量級フレームワークも要らない
- **結果を保存できる** — 全モデルが `Serialize` + `Deserialize`。取得結果をそのまま保存・再送できる
- **日本の住所に強い** — `GeocodeRequest::japan(address)` 一発で
  `language=ja` / `region=jp` / `components=country:JP`

## インストール

```sh
cargo add uiuifree-gmaps-geocoder tokio --features tokio/macros,tokio/rt-multi-thread
```

`Cargo.toml` に直接書く場合:

```toml
[dependencies]
uiuifree-gmaps-geocoder = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**Geocoding API を有効にした** Google Maps Platform の API キーが要る
（[取得手順](https://developers.google.com/maps/documentation/geocoding/get-api-key)）。
リクエストは Google の従量課金対象。

## 使い方

```rust
use uiuifree_gmaps_geocoder::GeocodingClient;

#[tokio::main]
async fn main() -> Result<(), uiuifree_gmaps_geocoder::Error> {
    let client = GeocodingClient::new(std::env::var("GOOGLE_MAPS_API_KEY").unwrap());

    // 住所 → 座標
    for r in client.geocode("東京都渋谷区道玄坂1-1-1").await? {
        let loc = r.location();
        println!(
            "{} => {}, {} [{}] 〒{:?} / {:?}",
            r.formatted_address,
            loc.lat,
            loc.lng,
            r.geometry.location_type, // ROOFTOP / RANGE_INTERPOLATED / ...
            r.postal_code(),
            r.prefecture(),
        );
    }

    // 座標 → 住所
    for r in client.reverse_geocode(35.6580, 139.6994).await? {
        println!("{}", r.formatted_address);
    }
    Ok(())
}
```

`geocode` は Google の関連度順に並んだ `Vec<GeocodeResult>` を返す。
**該当なしはエラーにしない**（`ZERO_RESULTS` は空の `Vec`）。

## レシピ集

### 3つの引き方

```rust
client.geocode("東京都渋谷区道玄坂1-1-1").await?;               // 住所 → 座標
client.reverse_geocode(35.6580, 139.6994).await?;               // 座標 → 住所
client.geocode_place_id("ChIJN1t_tDeuEmsRUsoyG83frY4").await?;  // Place ID → 住所
```

### 一番正確な1件を選ぶ

```rust
use uiuifree_gmaps_geocoder::most_precise;

// ROOFTOP > RANGE_INTERPOLATED > GEOMETRIC_CENTER > APPROXIMATE の順で最良を1件
//（同順位なら Google の並び順どおり先頭が勝つ）
let best = most_precise(&results);

// 番地レベルまで特定できたか。市区町村や郵便番号の中心止まりなら false
let street_level = results.iter().any(|r| r.geometry.location_type.is_street_level());
```

### 日本の住所

```rust
use uiuifree_gmaps_geocoder::GeocodeRequest;

// language=ja + region=jp + components=country:JP をまとめて適用
let results = client.send(GeocodeRequest::japan("東京都渋谷区渋谷2-24-12")).await?;
let r = &results[0];
(r.prefecture(), r.locality(), r.postal_code()); // ("東京都", "渋谷区", "150-0002")
```

### 全パラメータ

```rust
use uiuifree_gmaps_geocoder::{Bounds, Component, GeocodeRequest, GeocodingClient, LatLng, LocationType};

let client = GeocodingClient::new("YOUR_API_KEY");

// forward: 言語・地域バイアス・範囲バイアス・コンポーネント絞り込み
let req = GeocodeRequest::address("道玄坂")
    .language("ja")
    .region("jp")
    .bounds(Bounds {
        southwest: LatLng { lat: 35.5, lng: 139.5 },
        northeast: LatLng { lat: 35.8, lng: 139.9 },
    })
    .component(Component::Country("JP")); // 複数呼ぶと AND
let results = client.send(req).await?;

// reverse: 結果種別・精度で絞る
let req = GeocodeRequest::reverse(35.658, 139.6994)
    .result_type("street_address")
    .location_type(LocationType::Rooftop);
let results = client.send(req).await?;

// トップレベルの plus_code など、レスポンス全体が要るとき
let resp = client.send_response(GeocodeRequest::reverse(35.658, 139.6994)).await?;
println!("{:?}", resp.plus_code);
```

### 再試行の調整

```rust
use std::time::Duration;
use uiuifree_gmaps_geocoder::{GeocodingClient, RetryConfig};

// 既定は「最大3回・初回500ms・上限8s」
let client = GeocodingClient::new("YOUR_API_KEY").with_retry(RetryConfig {
    max_retries: 5,
    base_delay: Duration::from_millis(300),
    ..RetryConfig::default()
});

let no_retry = GeocodingClient::new("YOUR_API_KEY").with_retry(RetryConfig::disabled());
```

### HTTP クライアントを差し替える

```rust
// 既定は全体タイムアウト10秒。プロキシ・タイムアウト・接続プールを変えたいときはこちら
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
let client = GeocodingClient::with_http_client("YOUR_API_KEY", http);
```

### エラーを扱う

```rust
use uiuifree_gmaps_geocoder::{Error, GeocodeStatus};

match client.geocode(address).await {
    Ok(results) if results.is_empty() => { /* ZERO_RESULTS: 該当なし。エラーではない */ }
    Ok(results) => { /* 結果を使う */ }
    Err(Error::Api { status: GeocodeStatus::RequestDenied, message }) => {
        // キー不正・API 未有効化・課金停止。再試行しても直らないので設定を直す
        eprintln!("denied: {message:?}");
    }
    Err(e) if e.is_retryable() => { /* 内蔵の再試行を使い切った状態。さらに待つか諦める */ }
    Err(e) => return Err(e),
}
```

## API 早見表

| 項目 | 役割 |
| --- | --- |
| `GeocodingClient::new(key)` | 全体タイムアウト10秒・既定の再試行つきクライアント |
| `::with_http_client(key, reqwest::Client)` | `reqwest::Client` を自前で用意する |
| `.with_retry(RetryConfig)` | 再試行設定を差し替える（`RetryConfig::disabled()` で無効化） |
| `.with_base_url(url)` | 接続先を差し替える（テスト・プロキシ） |
| `.geocode(address)` | 住所 → 座標 → `Result<Vec<GeocodeResult>, Error>` |
| `.reverse_geocode(lat, lng)` | 座標 → 住所 → `Result<Vec<GeocodeResult>, Error>` |
| `.geocode_place_id(place_id)` | Place ID → 住所 → `Result<Vec<GeocodeResult>, Error>` |
| `.send(GeocodeRequest)` | オプション付きリクエストを送る |
| `.send_response(GeocodeRequest)` | 同上。レスポンス全体（`GeocodeResponse`）を返す |
| `GeocodeRequest::address / japan / reverse / place_id` | リクエストの作り方4種 |
| `.language / .region / .bounds / .component / .result_type / .location_type / .address_descriptors` | オプション |
| `GeocodeResult::location()` | 結果の `LatLng` |
| `.postal_code() / .prefecture() / .locality() / .country() / .component(type)` | 住所要素ヘルパー |
| `LocationType::precision_rank() / .is_street_level()` | 精度の比較・判定 |
| `most_precise(&results)` | 一番正確な1件 |
| `Error::is_retryable()` | 待って再試行する価値があるか |

## 挙動: ステータス・エラー・再試行

| Google の `status` / 通信 | このクレートの扱い |
| --- | --- |
| `OK` | `Ok(results)` |
| `ZERO_RESULTS` | `Ok(vec![])` — 該当なしはエラーにしない |
| `OVER_QUERY_LIMIT`・`UNKNOWN_ERROR` | 指数バックオフで再試行 → 使い切ったら `Err(Error::Api)` |
| HTTP 429・5xx・タイムアウト・接続失敗 | 指数バックオフで再試行 → 使い切ったら `Err(Error::Http)` |
| `REQUEST_DENIED`・`INVALID_REQUEST`・`OVER_DAILY_LIMIT` | 再試行せず即 `Err(Error::Api)` |

- 待ち時間は `base_delay * 2^attempt`（`max_delay` で頭打ち。既定は 500ms→8s・最大3回）
- `Error::is_retryable()` は内蔵の再試行とまったく同じ基準で分類する
- **API キーはエラーに出ない。** `Error` は `reqwest::Error` から URL（`key=` を含む）を
  除去して保持し、`GeocodingClient` の `Debug` は `api_key: "[REDACTED]"` と表示する。
  エラーはそのままログに流してよい

## Google の規約（呼び出し側の責務）

ライブラリでは強制できないので、利用者が守る必要がある。

- ジオコーディング結果のキャッシュは**最大30日**まで。`place_id` は保存してよい。
  このクレートが一切キャッシュを持たないのはこのため
- 結果の表示には原則として Google 地図の併用と帰属表示が要る
- 判断は[最新の規約](https://cloud.google.com/maps-platform/terms)で確認すること
  （ここの要約は法的助言ではない）

## 未対応（意図的な割り切り）

- バックオフのジッター・`Retry-After` ヘッダの解釈
- `extra_computations=BUILDING_AND_ENTRANCES`（建物外形・入口のベータ）
- キャッシュ・リクエストの重複排除（同一住所の再問い合わせ抑制・課金管理は呼び出し側の責務）
- Geocoding 以外の Google Maps Platform API（Places / Routes / Time Zone / Elevation …）

## 使わないほうがいい場合

- **オフラインで動かしたい** — 内蔵データ型のクレートを使う（日本なら郵便番号→町名は
  `jp-address-search`、座標→町名は `japan-reverse-geocoder`）
- **Geocoding 以外の Google Maps Platform も使う** — `google_maps` のような
  プラットフォーム全体をカバーするクレートのほうが合う
- **プロバイダ非依存にしたい**（Nominatim / OpenCage など）— このクレートは Google の
  レスポンス形状を直接扱う設計
- **座標を無期限に保存したい** — Google の規約が許していない

## よくある質問

### 住所から緯度経度を取るには？

`GeocodingClient::new(api_key).geocode("<住所>").await?` が `Vec<GeocodeResult>` を返す。
`result.location()` で `LatLng { lat, lng }`。[使い方](#使い方)を参照。

### 座標から住所（逆ジオコーディング）は？

`client.reverse_geocode(lat, lng).await?` の `formatted_address` を読む。
`.result_type("locality")` や `.location_type(LocationType::Rooftop)` で絞り込める。

### 存在しない住所を渡したら？

`Ok(vec![])` が返る（Google の `ZERO_RESULTS`）。`Err` になるのは本当の失敗だけ。

### `OVER_QUERY_LIMIT`（レート上限）は再試行される？

される。既定で最大3回・指数バックオフ（500ms→1s→2s、上限8s）。`UNKNOWN_ERROR`・
HTTP 429・5xx・タイムアウト・接続失敗も同じ。`with_retry(RetryConfig { .. })` で調整、
`RetryConfig::disabled()` で無効化。

### ログから API キーが漏れない？

漏れない。エラーは URL（`key=` が乗る場所）を持たず、クライアントの `Debug` は
`[REDACTED]` を表示する。キーの取得元は呼び出し側が決める（`String` を渡すだけ）。

### 結果が十分な精度か判断するには？

`result.geometry.location_type` を見る。`is_street_level()` は `ROOFTOP` /
`RANGE_INTERPOLATED` で true、市区町村や郵便番号の中心では false。
複数結果から最良を選ぶなら `most_precise(&results)`。

### キャッシュはある？

無い。意図的に持っていない（キャッシュ・重複排除・課金管理は呼び出し側の責務。
そもそも Google の規約でも30日までしか持てない）。

### 必要な非同期ランタイムは？

Tokio。`reqwest` が要求し、再試行の待機にも `tokio::time` を使う。

### 対応する Rust の下限バージョンは？

1.88。`rust-version` で宣言し、CI で検査している。

### Google を叩かずにテストするには？

モックサーバーに向ける: `GeocodingClient::new(key).with_base_url(mock.uri())`。
このクレート自身の `tests/http.rs` がまさにその方式（wiremock・ネットワーク不要）。

## 開発

```sh
cargo test                                     # 単体＋HTTP結合テスト。ネットワーク不要
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
GOOGLE_MAPS_API_KEY=xxxx cargo run --example geocode -- "東京都渋谷区道玄坂1-1-1"
```

テストはパース・ステータス解釈・住所要素ヘルパー・バックオフ計算・クエリ組み立て・
`Serialize` 往復に加え、wiremock による HTTP 結合テストで再試行ループの実挙動
（回復・非再試行・枯渇・クエリエンコードの Google 互換性・キー非漏洩）を検証する。

### ライブテスト（実 API）

`tests/live.rs` は実際の Google API を叩く。全て `#[ignore]` なので `cargo test` でも CI でも
走らない。実サービスに対して確かめたいときだけ明示的に実行する（1回でおよそ十数リクエストの課金）:

```sh
GOOGLE_MAPS_API_KEY=xxxx cargo test --test live -- --ignored --nocapture
```

モックでは絶対に落ちないところを見ている: 組み立てたクエリを Google が受け付けるか、
`japan()` で本当に日本語の住所要素が返るか、Place ID で引き直すと同じ場所に戻るか、
本物の `REQUEST_DENIED` でもキーがエラーに出ないか、そして**レスポンスに未対応の項目が
増えていないか**。

コントリビューターと AI エージェント向けの手引きは [AGENTS.md](AGENTS.md)、
機械可読の要約は [llms.txt](llms.txt) にある。

## ライセンス

MIT（[LICENSE](LICENSE)）。

このプロジェクトは Google と提携も後援もされていない。"Google Maps" は Google LLC の商標。
