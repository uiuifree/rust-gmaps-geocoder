# AGENTS.md

AIエージェント・コントリビューター向けの開発ガイド。

## プロジェクト概要

Google Maps Platform の Geocoding API（住所→座標・座標→住所・Place ID→住所）だけを扱う非同期クライアント。
ほかの Maps API（Places / Routes / Static Maps 等）は入れない。

- 依存は `reqwest`（rustls）/ `serde` / `serde_json` / `thiserror` / `tokio`（time = 再試行の待機）に留める
- 公開 API は `GeocodingClient` + `GeocodeRequest`（ビルダ）+ 結果型（`GeocodeResult` ほか）に集約する
- `status` が `OK` / `ZERO_RESULTS` 以外は `Error` にする（成功扱いにしない）。再試行対象は `is_retryable()` で判定
- API キーはログ・`Debug`・エラー文面に出さない（`[REDACTED]`・URL から `key=` を除去）
- 未知の `status` / `location_type` は `Other(String)` で受ける（前方互換）
- 既定のテストはネットワーク不要（`wiremock` のモックサーバー + JSON のパース）。
  実 API を叩くのは `examples/` と `tests/live.rs`（`#[ignore]` 付き）だけで、CI では走らせない

## コマンド

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
GOOGLE_MAPS_API_KEY=... cargo run --example geocode   # 実 API の確認
GOOGLE_MAPS_API_KEY=... cargo test --test live -- --ignored --nocapture   # 実 API のライブテスト（課金あり）
```

## 構成

- `src/lib.rs` — crate ドキュメント・再エクスポート
- `src/client.rs` — `GeocodingClient`（HTTP・再試行）と `GeocodeRequest`（ビルダ。`japan()` = 日本向け既定）
- `src/types.rs` — `GeocodeStatus` / `LocationType`（`precision_rank` / `is_street_level`）/ `Component`
- `src/model.rs` — レスポンスの型・住所要素ヘルパー・`most_precise`
- `src/error.rs` — `Error`（`is_retryable`）
- `tests/http.rs` — モックサーバーでの結合テスト
- `tests/live.rs` — 実 API に対するライブテスト（全て `#[ignore]`。`--ignored` を付けたときだけ走る）
- `examples/geocode.rs` — 実 API の動作確認
- `README.md`（英語・正）/ `README.ja.md`（日本語）/ `llms.txt`（AI 向け要約）

## ドキュメントの言語

- **rustdoc（`///` と `//!`）と `examples/` の出力は英語**。docs.rs は英語圏の検索・AI が読む面なので、
  公開 API の説明はここに英語で書く
- **実装コメント（`//`）とテスト名は日本語のまま**。公開されないので、書きやすさを優先する
- README は英語が正（`README.md`）で、日本語版（`README.ja.md`）は同じ構成を保つ。
  **どちらか片方だけ直さない**

## 変更時の注意

- Google の規約（結果のキャッシュは30日まで・Google 地図との併用・帰属表示）はクレートで強制できない。README の注意書きを消さない
- 公開 API を足す・変えるときは rustdoc（`# Errors` / 例つき）・両方の README・`llms.txt`・`CHANGELOG.md` を揃える
- crates.io のメタデータ（`description` / `keywords` / `categories`）は検索の入口。名前を変えたら合わせて見直す
