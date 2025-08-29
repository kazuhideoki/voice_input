# コーディング規約と設計方針（要点）

## 品質基準
- ゼロ警告ポリシー: `cargo clippy -- -D warnings` を徹底（CI では `--all-targets --features ci-test`）
- フォーマット統一: `cargo fmt`（CI/PR 前に実行）
- 型安全: `cargo check` を適宜実行
- ドキュメント: 公開APIには doc コメントを付与

## テスト戦略
- ローカル: `cargo test` で全テスト
- CI: `cargo test --features ci-test` で環境依存テストを除外
- デバイス/デーモン/GUI 依存テスト: `#[cfg_attr(feature = "ci-test", ignore)]` を付与

## アーキテクチャ
- `voice_inputd` は単一スレッド Tokio ランタイム前提
  - 共有は `Rc` を優先（`Arc` 避ける）
  - `spawn_local` を活用
  - 不要な同期化を避ける
- エラーハンドリング
  - `anyhow` は避け、`thiserror` などで独自型を用意
  - `?` による適切な伝搬、説明的なメッセージ

## パフォーマンス
- 直接入力がデフォルト（クリップボード方式より高速）
- クリップボード操作最小化
- 適切なデータ構造選択
- 音声データはメモリ上で処理（I/O 削減）

## 変更フロー
1. 計画: 仕様/テスト戦略を検討
2. 実装: 既存パターンに合わせ、必要なテスト追加
3. テスト: `cargo test` / `--features ci-test` / clippy / fmt を実行
4. 事前チェックリストを満たして PR

## 命名/その他
- Rust 標準の命名規則に準拠（`snake_case`, `CamelCase` 等）
- 公開 API は意味の通る名前・ドキュメントを付与
- モジュール境界をまたぐ型/エラーは明確に定義
