# Changelog

このプロジェクトの注目すべき変更はこのファイルに記録する。
形式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に、
バージョニングは [Semantic Versioning](https://semver.org/lang/ja/) に準拠する。

## [Unreleased]

### Added

- `GeocodeRequest::japan(address)`（日本向け既定 = `language=ja` / `region=jp` / `components=country:JP`）
- `LocationType::precision_rank()` / `is_street_level()`（番地レベルまで特定できたかの判定）
- `most_precise(&results)`（一番正確な1件を選ぶ）
- `AGENTS.md` / `llms.txt`
- 英語の `README.md` と日本語の `README.ja.md`（相互リンク・同じ構成）
- 実 API に対するライブテスト `tests/live.rs`（全て `#[ignore]`。
  `GOOGLE_MAPS_API_KEY=... cargo test --test live -- --ignored` で実行）。
  クエリの受理・`japan()` の実結果・Place ID の往復・実 `REQUEST_DENIED` でのキー非漏洩・
  レスポンス項目の取りこぼし検出を見る

### Fixed

- `most_precise()` が同じ精度の結果が並んだとき最後の1件を返していた。
  ドキュメントどおり Google の並び順を尊重して先頭を返す

### Changed

- rustdoc を全面的に英語で書き直し（`# Errors` / `# Panics` / 実行できる例つき）。
  実装コメントとテスト名は日本語のまま
- crates.io 向けメタデータを整理（`description` / `keywords` / `categories` /
  `homepage` / `documentation`、`.github` と `.idea` を配布物から除外）
- `llms.txt` を llms.txt 形式に沿って書き直し（使いどころ・API 一覧・挙動・リンク）

## [0.1.0] - 2026-07-18

### Added

- Google Maps Geocoding API の async クライアント `GeocodingClient`
  - forward（住所→座標）/ reverse（座標→住所）/ Place ID 引き
  - 全リクエストパラメータ対応: `language` / `region` / `bounds` / `components` /
    `result_type` / `location_type` / `extra_computations=ADDRESS_DESCRIPTORS`
  - 全レスポンス項目対応: 座標・精度・viewport/bounds・住所要素・`place_id`・
    `plus_code`（結果／トップレベル）・`postcode_localities`・`partial_match`・
    `address_descriptor`（結果／トップレベル、生 JSON 保持）
- リクエストビルダ `GeocodeRequest`（借用ベース・`Debug`/`Clone`）
- 型付き enum `GeocodeStatus` / `LocationType`（未知値は `Other(String)` で前方互換）
- 住所要素ヘルパー `postal_code()` / `prefecture()` / `locality()` / `country()` / `component()`
- 指数バックオフ再試行（既定: 最大3回・初回500ms・上限8s）。対象は
  `OVER_QUERY_LIMIT` / `UNKNOWN_ERROR` / HTTP 429・5xx / タイムアウト・接続失敗。
  `RetryConfig` で調整、`RetryConfig::disabled()` で無効化
- 既定 HTTP クライアントに全体タイムアウト10秒（`with_http_client` で上書き可）
- API キー漏洩対策: `Error` は `reqwest::Error` から URL（`key=` クエリ込み）を除去して保持。
  `GeocodingClient` の `Debug` はキーを `[REDACTED]` 表示
- 全レスポンスモデルが `Serialize`/`Deserialize`（保存・再送可能）
- wiremock による HTTP 結合テスト（再試行回復・非再試行・枯渇・クエリエンコード固定化）

### Known limitations

- バックオフにジッターなし・`Retry-After` ヘッダ未解釈（意図的な割り切り）
- `extra_computations=BUILDING_AND_ENTRANCES`（建物外形・入口ベータ）未対応
- キャッシュなし（同一住所の再問い合わせ抑制・課金管理は呼び出し側の責務）

[Unreleased]: https://github.com/uiuifree/rust-gmaps-geocoder/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/uiuifree/rust-gmaps-geocoder/releases/tag/v0.1.0
