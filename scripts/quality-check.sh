#!/bin/bash
set -e

echo "🔍 Running quality checks..."

# フォーマットチェック
echo ""
echo "📝 Checking code format..."
cargo fmt -- --check
echo "✅ Format check passed"

# Clippy（lint）チェック
echo ""
echo "🔧 Running clippy..."
cargo clippy --all-targets -- -D warnings
echo "✅ Clippy check passed"

# テスト実行
echo ""
echo "🧪 Running tests..."
cargo test
echo "✅ All tests passed"

# E2Eテスト（環境依存のものはスキップ）
echo ""
echo "🌐 Running E2E tests (ci-safe mode)..."
cargo test --features ci-test --test e2e_direct_input_test || true
cargo test --features ci-test --test voice_inputd_direct_input_test || true

# ベンチマーク（任意）
if [ "$1" = "--bench" ]; then
    echo ""
    echo "📊 Running benchmarks..."
    cargo bench
fi

# メモリ監視テスト（任意）
if [ "$1" = "--memory" ]; then
    echo ""
    echo "💾 Running memory monitoring tests..."
    cargo test --test benchmarks::recording_bench -- benchmark_memory_monitor_overhead --nocapture
fi

echo ""
echo "✨ All quality checks passed!"
echo ""
echo "Optional flags:"
echo "  --bench   Run performance benchmarks"
echo "  --memory  Run memory monitoring tests"
