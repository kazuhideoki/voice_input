# GitHub Actions CI 実装設計書

## 全体設計

### Why - 概要、目的

本プロジェクトの継続的インテグレーション（CI）環境を構築し、コード品質の維持と自動化されたテストの実行を実現する。現在のテストスイートにはローカル環境に依存するテストが含まれているため、CI環境では明示的に指定したテストのみを実行する仕組みを導入する。

**設計方針:**
- ローカルでは従来通り`cargo test`で全テストを実行
- CI環境では`cargo test --features ci-test`等で環境非依存テストのみ実行
- 既存のテスト実行体験を変更しない

**課題:**
- 多くのテストがmacOS固有の機能（pbcopy/pbpaste、osascript、アクセシビリティ権限）に依存
- デーモンプロセスやUnixドメインソケットを使用するテストがCI環境で実行不可
- 環境依存テストと純粋なユニットテストが混在

### What - 成果物（機能、非機能）

**機能要件:**
1. GitHub Actions ワークフローによる自動ビルド・テスト
2. CI環境用のテスト実行設定（feature flag）
3. 環境非依存テストの明示的な分類
4. テスト結果の可視化とレポート

**非機能要件:**
1. CI実行時間の最適化（5分以内）
2. 失敗時の詳細なエラー情報
3. 並列実行によるパフォーマンス向上
4. キャッシュによるビルド時間短縮
5. ローカル開発体験の維持

### How - フェーズ分割

| Phase | 目的 | 成果物 | 完了条件 | 除外項目 |
|-------|------|--------|----------|----------|
| Phase 1 | Feature flagの導入 | `Cargo.toml`更新<br>`tests/common/mod.rs` | - `ci-test` featureが定義済み<br>- 条件付きコンパイルが可能 | - 既存テストの修正<br>- GitHub Actions設定 |
| Phase 2 | CI安全テストの分類 | 各テストファイル | - 環境非依存テストに`#[cfg_attr]`適用<br>- CI環境で実行可能なテストを明確化 | - テストロジックの変更<br>- 新規テストの追加 |
| Phase 3 | GitHub Actions設定 | `.github/workflows/ci.yml` | - push/PR時に自動実行<br>- CI用テストのみ実行 | - デプロイ処理<br>- リリース自動化 |
| Phase 4 | 最適化とレポート | ワークフロー更新 | - キャッシュによる高速化<br>- テスト結果の可視化 | - 外部サービス連携<br>- 通知機能 |

---

## フェーズ詳細設計

### Phase 1: Feature flagの導入

#### 目的
CI環境専用のテスト実行設定を導入し、ローカルとCIで異なるテストセットを実行可能にする。

#### 成果物

**`Cargo.toml`の更新:**
```toml
[features]
default = []
ci-test = []  # CI環境で安全に実行できるテストのみを有効化
```

**`tests/common/mod.rs`:**
```rust
// CI環境で実行可能なテストを示すマーカー
#[cfg(feature = "ci-test")]
pub const CI_TEST_MODE: bool = true;

#[cfg(not(feature = "ci-test"))]
pub const CI_TEST_MODE: bool = false;
```

#### 完了条件
- [ ] `cargo test`で全テストが実行される（従来通り）
- [ ] `cargo test --features ci-test`でCI用設定が有効になる
- [ ] featureの有無で条件分岐が可能

#### 除外項目
- 複雑なfeature組み合わせ
- 実行時の動的な切り替え

---

### Phase 2: CI安全テストの分類

#### 目的
各テストファイルで環境非依存のテストを識別し、CI環境でのみそれらを実行するよう設定する。

#### 成果物

**環境依存度による分類:**

1. **完全にCI安全なテストファイル:**
   - `ipc_compatibility_test.rs`
   - `ipc_serialization_test.rs`
   - `cli_args_test.rs`（一部）

2. **部分的にCI安全なテストファイル:**
   - `cli_integration.rs`
   - `voice_inputd_direct_input_test.rs`

3. **CI実行不可なテストファイル:**
   - `e2e_direct_input_test.rs`
   - `integration_test.rs`
   - `performance_test.rs`

**実装方法:**

```rust
// CI安全なテスト（常に実行）
#[test]
fn test_serialization() {
    // テストコード
}

// 環境依存テスト（CI環境ではスキップ）
#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_clipboard_integration() {
    // クリップボード操作を含むテスト
}

// または、ファイル全体をCI環境で除外
#[cfg(not(feature = "ci-test"))]
mod integration_tests {
    // 環境依存テスト群
}
```

#### 完了条件
- [ ] 全テストファイルで環境依存度を評価完了
- [ ] CI安全なテストに適切な属性を付与
- [ ] `cargo test --features ci-test`で環境依存テストがスキップされる
- [ ] `cargo test`で全テストが実行される

#### 除外項目
- テストコードのリファクタリング
- モック化による環境非依存化

---

### Phase 3: GitHub Actions設定

#### 目的
自動ビルド・テスト環境を構築し、CI環境で安全なテストのみを実行する。

#### 成果物

**`.github/workflows/ci.yml`:**
```yaml
name: CI

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        rust: [stable]
    
    steps:
    - uses: actions/checkout@v4
    
    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: ${{ matrix.rust }}
        override: true
        components: rustfmt, clippy
    
    - name: Check format
      run: cargo fmt -- --check
    
    - name: Clippy
      run: cargo clippy -- -D warnings
    
    - name: Build
      run: cargo build --verbose
    
    - name: Run CI-safe tests
      run: cargo test --features ci-test --verbose
    
    - name: Show skipped tests count
      run: |
        echo "Environment-dependent tests were skipped in CI"
        cargo test --features ci-test -- --list | grep -c "ignored" || true
```

#### 完了条件
- [ ] push/PR時にワークフローが自動実行
- [ ] Ubuntu/macOS両環境でビルド成功
- [ ] CI安全なテストのみが実行される
- [ ] スキップされたテスト数が表示される

#### 除外項目
- Windows環境のサポート
- ナイトリービルドのテスト
- 全テストの強制実行オプション

---

### Phase 4: 最適化とレポート

#### 目的
CI実行時間を短縮し、テスト結果の可視性を向上させる。

#### 成果物

**ワークフロー更新内容:**

1. **依存関係キャッシュ:**
```yaml
- uses: Swatinem/rust-cache@v2
  with:
    cache-on-failure: true
```

2. **テスト結果レポート:**
```yaml
- name: Generate test report
  run: |
    cargo test --features ci-test --no-fail-fast -- -Z unstable-options --format json > test-results.json || true
    
- name: Upload test results
  uses: actions/upload-artifact@v3
  if: always()
  with:
    name: test-results-${{ matrix.os }}
    path: test-results.json
```

3. **並列実行最適化:**
```yaml
- name: Run tests with parallelism
  run: cargo test --features ci-test --jobs 4
```

#### 完了条件
- [ ] 2回目以降のビルドが50%以上高速化
- [ ] テスト結果がArtifactsとして保存
- [ ] 失敗時に詳細なログが確認可能
- [ ] PR上でテスト結果サマリーが表示

#### 除外項目
- 外部サービス（Codecov等）との連携
- Slack/Discord通知
- ベンチマーク結果の追跡

---

## 実装における注意事項

1. **後方互換性の維持**
   - `cargo test`の動作を変更しない
   - 既存の`#[ignore]`属性との共存

2. **ドキュメント**
   - READMEにCI実行方法を追記
   - 各テストファイルに環境依存性をコメント

3. **段階的な移行**
   - まずfeature flagを導入
   - 次に明確にCI安全なテストから適用
   - 最後に部分的に安全なテストを整理