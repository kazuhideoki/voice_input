# 開発でよく使うコマンド（Quick Reference）

## 初期セットアップ
- `.env` 作成: `cp .env.example .env`
- ラッパー+LaunchAgent設定: `./scripts/setup-dev-env.sh`

## ビルド/実行
- 開発ビルド: `cargo build`
- リリースビルド: `cargo build --release`
- デーモン（バックグラウンド）起動: `cargo run --bin voice_inputd &`
- CLI 実行例: `cargo run --bin voice_input -- --list-devices`
- 開発ビルド+デーモン再起動: `./scripts/dev-build.sh`

## テスト/品質
- すべてのテスト: `cargo test`
- CI相当テスト: `cargo test --features ci-test`
- フォーマットチェック: `cargo fmt -- --check`
- Lint（警告をエラー扱い）: `cargo clippy -- -D warnings`
- CI 厳格 Lint: `cargo clippy --all-targets --features ci-test -- -D warnings`
- 品質一括チェック: `./scripts/quality-check.sh [--bench|--memory]`

## ランタイム操作
- 録音開始/停止: `voice_input start` / `voice_input stop`
- 1回で録音→転写→直接入力: `voice_input toggle`
- 入力デバイス一覧: `voice_input --list-devices`
- クリップボード方式: `voice_input start --copy-and-paste`
- ヘルスチェック: `voice_input health`

## 辞書操作
- 追加/更新: `voice_input dict add "誤変換" "正しい語"`
- 削除: `voice_input dict remove "誤変換"`
- 一覧: `voice_input dict list`
- 保存先変更: `voice_input config set dict-path /path/to/dictionary.json`

## macOS運用/トラブルシュート
- LaunchAgent 再起動: `launchctl kickstart -k user/$(id -u)/com.user.voiceinputd`
- デーモン手動起動: `nohup /usr/local/bin/voice_inputd_wrapper > /tmp/voice_inputd.out 2> /tmp/voice_inputd.err &`
- エラーログ監視: `tail -f /tmp/voice_inputd.err`
