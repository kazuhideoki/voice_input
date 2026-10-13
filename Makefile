.DEFAULT_GOAL := help

.PHONY: help setup install restart uninstall build build-debug check test test-local fmt fmt-check lint quality quality-bench bench clean

help: ## 操作一覧を表示（make の既定動作）
	@printf '使い方: make <target>\n\n'
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

setup: ## 初回準備: LaunchAgent を設定（続けて make install）
	./scripts/setup-app-bundle.sh

install: ## リリースビルド・アプリ配置・署名・daemon 再起動
	./scripts/build-app-bundle.sh

restart: ## 再ビルドせず daemon を再起動（権限付与後など）
	./scripts/restart-app-bundle.sh

uninstall: ## アプリ・LaunchAgent を削除し、macOS の権限設定をリセット
	./scripts/cleanup-app-bundle.sh

build: ## Rust バイナリのリリースビルド（アプリへの反映なし）
	cargo build --release

build-debug: ## Rust バイナリの開発ビルド
	cargo build

check: ## Rust の型チェック
	cargo check

test: ## 環境依存のテストを除いて実行（CI と同じ設定）
	cargo test --features ci-test

test-local: ## 音声デバイス・daemon・GUI に依存するテストも実行
	cargo test

fmt: ## Rust コードをフォーマット
	cargo fmt

fmt-check: ## Rust コードのフォーマットを確認
	cargo fmt -- --check

lint: ## 全ターゲットの Clippy チェック（警告をエラー扱い）
	cargo clippy --all-targets -- -D warnings

quality: ## 既存のローカル品質チェック（環境依存テストを含む）
	./scripts/quality-check.sh

quality-bench: ## ローカル品質チェックとベンチマーク
	./scripts/quality-check.sh --bench

bench: ## Rust ベンチマークを実行
	cargo bench

clean: ## Cargo のビルド生成物を削除
	cargo clean
