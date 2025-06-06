# ショートカットキーモード設計案

## 概要

StackingMode有効時に自動でショートカットキー制御を連動させ、スタック操作の効率性を劇的に向上させる設計案。スタッキングモード時の課題に特化したソリューションを提供する。

## 現状の課題分析

### 現在のコマンド構造

StackingModeでは以下のコマンドが頻繁に使用される：

1. `voice_input stack-mode on/off` - スタックモードの有効/無効
2. `voice_input start` - 録音開始（スタック保存）
3. `voice_input paste <number>` - スタック番号でペースト
4. `voice_input list-stacks` - スタック一覧表示
5. `voice_input clear-stacks` - 全スタッククリア

### 課題

- **多数のキーバインディングが必要**: 各コマンドを個別にランチャーアプリに設定する必要
- **効率性の問題**: 特に `paste <number>` コマンドでは複数の番号に対応した設定が必要
- **ワークフロー断続**: スタック一覧確認とペースト操作が分離している

### 解決後の簡素化

**現在**: 5つのコマンド × 複数の番号 = 多数のキーバインド設定
**提案後**: `voice_input stack-mode on` の1つのキーバインド設定のみ（ショートカットキーが自動連動）

## ショートカットキーモード連動の提案

### 設計コンセプト（問題特化型アプローチ）

**核心思想**: スタッキングモード時にキー操作が増加する問題を、その発生時点で自動解決

1. **自動連動**: スタックモード有効時にショートカットキーも自動有効化
2. **問題特化**: 通常音声入力は既存ワークフローを維持
3. **操作一元化**: 必要な時に必要な機能が自動提供される

### Why: 根本課題の解決

**従来の問題**:

- スタッキングモード時のみキー操作増加
- 2つのモード管理（スタック + ショートカット）が認知負荷

**解決アプローチ**:

- 課題発生時点（スタック有効化）で自動的に解決策提供
- ユーザーは「スタッキングモード」のみを意識
- 通常音声入力への影響ゼロ

### What: 自動連動システム

#### 操作構造（簡素化）

```bash
# 通常音声入力（変更なし）
voice_input toggle

# スタッキング（ショートカットキー自動連動）
voice_input stack-mode on/off

# フォールバック（既存CLI併用可能）
voice_input paste <number>
```

#### ショートカットキー（スタックモード有効時に自動有効化）

**自動連動**: スタックモード有効時のみ以下のキーが機能

- `cmd + R` : 録音開始/停止（トグル）
- `cmd + S` : スタック一覧表示オーバーレイ
- `cmd + 1-9` : 対応する番号のスタックを直接ペースト
- `cmd + C` : 全スタッククリア

**利点**:

- **操作削減**: 2操作 → 1操作（`stack-mode on` + `shortcut-mode toggle` → `stack-mode on`のみ）
- **認知軽減**: スタッキングモードのみ管理、ショートカット意識不要
- **自動最適化**: 必要な時に必要な機能が自動提供

**キー上書きの制限**:

- macOSシステムレベルのショートカット（`Cmd+Tab`, `Cmd+Space`等）は上書き不可
- アプリケーションレベルでのキャプチャに制限あり
- 完全な上書きではなく、優先度に基づく処理となる

### 技術的実装アプローチ

#### アーキテクチャ変更

```
現在: [外部ランチャー] → [CLI] → [IPC] → [Daemon]
提案: [Global Key Hook] → [Daemon] → [Direct Action]
```

#### 実装候補技術

##### 1. Rust native approach

- **ライブラリ**: `rdev` または `device_query`
- **インターフェース**:

  ```rust
  // StackServiceでの自動連動
  impl StackService {
      pub fn enable_stack_mode(&mut self) -> bool {
          self.mode_enabled = true;
          self.hotkey_manager.enable_shortcuts(); // 自動連動
          true
      }

      pub fn disable_stack_mode(&mut self) -> bool {
          self.mode_enabled = false;
          self.hotkey_manager.disable_shortcuts(); // 自動連動
          true
      }
  }
  ```

- **仕組み**: OSの低レベルキーボードAPIを直接呼び出してグローバルキーイベントをキャプチャ
- **pros**: 単一バイナリ、最高性能、他プロセス不要
- **cons**: macOSアクセシビリティ権限必須、実装が複雑、権限エラー時の対応が困難

##### 2. AppleScript integration

- **実装**: AppleScript経由でのグローバルホットキー
- **pros**: macOS標準、比較的簡単
- **cons**: 制限が多い、パフォーマンス問題

##### 3. Hybrid approach (推奨)

- **組み合わせ**: Rustベースの軽量キーフック + 既存IPC
- **インターフェース**:

  ```rust
  // HotkeyManagerトレイト
  trait HotkeyManager {
      fn enable_shortcuts(&mut self) -> Result<(), HotkeyError>;
      fn disable_shortcuts(&mut self) -> Result<(), HotkeyError>;
      fn is_enabled(&self) -> bool;
  }

  // IPCとの連携
  fn handle_shortcut_key(key: Key) {
      match key {
          Key::R => send_cmd(&IpcCmd::Toggle { /* ... */ }),
          Key::S => show_stack_overlay(),
          Key::Num(n) => send_cmd(&IpcCmd::PasteStack { number: n }),
          _ => {}
      }
  }
  ```

- **仕組み**: 最小限のキー検出機能 + 既存コマンドシステムの流用
- **pros**: 既存アーキテクチャを活用、段階的実装可能、フォールバック確保
- **cons**: 若干のオーバーヘッド

### 実装ファイル構成案

```
src/
├── hotkey/                      # 新規モジュール
│   ├── mod.rs
│   ├── key_handler.rs          # グローバルキーフック
│   ├── overlay_ui.rs           # スタック一覧オーバーレイ
│   └── shortcut_mode.rs        # ショートカットモード管理
├── bin/
│   └── voice_input_hotkey.rs   # 新規バイナリ
└── ...
```

### UIコンポーネント設計（最小限）

#### スタック一覧オーバーレイ（シンプル版）

```
┌─────────────────────────────────┐
│ [1] Hello, world!               │
│ [2] Meeting notes...            │
│ [3] Code snippet: fn...         │
├─────────────────────────────────┤
│ Press 1-3 to paste, ESC to exit │
└─────────────────────────────────┘
```

**設計方針**:

- スタック内容のみを表示（時刻、件数表示は省略）
- 最小限の操作ガイドのみ
- 軽量で高速な表示を優先

## 実現可能性評価

### 技術的実現可能性: ★★★☆☆ (中程度)

**課題:**

- macOSのアクセシビリティ権限管理
- グローバルキーフックの安定性
- 既存アーキテクチャとの統合

**解決策:**

- 段階的実装（最初はシンプルなキーフック）
- フォールバック機能（CLIコマンドは併用）
- 十分なエラーハンドリング

### ユーザビリティ向上効果: ★★★★★ (高)

**期待効果:**

- **操作数削減**: 5回のキー操作 → 2回
- **ワークフロー統合**: 一覧表示とペーストがシームレス
- **視覚的フィードバック**: 現在の状態が明確

### 開発コスト: ★★★☆☆ (中程度)

**新規開発が必要な機能:**

1. グローバルキーフック (2-3日)
2. オーバーレイUI (3-4日)
3. ショートカットモード管理 (1-2日)
4. 既存システムとの統合 (2-3日)
5. テストとデバッグ (3-5日)

**合計推定**: 11-17日

## 実装方針

### 自動連動アーキテクチャ

**核心設計**:

1. **StackService拡張**: スタックモード切り替え時にショートカットキーを自動制御
2. **透明性**: ユーザーからはスタッキングモードのみが見える
3. **フォールバック**: 既存CLIコマンドとの併用可能

### 状態管理

```rust
// 統合状態管理
pub struct VoiceInputState {
    stack_mode: bool,
    shortcuts_enabled: bool, // stack_modeに自動連動
    recording: bool,
}

impl VoiceInputState {
    fn enable_stack_mode(&mut self) {
        self.stack_mode = true;
        self.shortcuts_enabled = true; // 自動連動
    }
}
```

## リスク評価

### 高リスク

- **権限問題**: macOSアクセシビリティ権限の要求とUX影響
- **パフォーマンス**: グローバルキーフックによるシステム負荷

### 中リスク

- **キーコンフリクト**: 他アプリケーションとのキーバインド競合
- **安定性**: キーフック機能のクラッシュ耐性

### 低リスク

- **実装複雑度**: 既存アーキテクチャとの統合は十分可能

## 代替案検討

### 案1: 専用ランチャーアプリ連携

- Alfred/Raycast向けの専用プラグイン開発
- **pros**: 高い統合性、UX良好
- **cons**: 特定ツールへの依存

### 案2: メニューバー常駐アプリ

- メニューバーからのスタック操作UI
- **pros**: macOS標準UI、安定
- **cons**: マウス操作必要、効率性劣る

### 案3: Webベースインターフェース

- ローカルWebサーバーでの操作UI
- **pros**: 高いカスタマイズ性
- **cons**: 起動コスト、セキュリティ考慮

## 結論: 問題特化型ソリューション

**核心価値**: スタッキング時の課題に特化した根本解決

### 設計の優位性

1. **問題の本質解決**: キー操作増加の発生時点で自動的に解決策提供
2. **操作効率の劇的改善**: 2操作 → 1操作への簡素化
3. **認知負荷軽減**: スタッキングモードのみ管理、ショートカット意識不要
4. **通常音声入力への影響ゼロ**: 既存ワークフローを完全保持

### 推奨実装アプローチ

**自動連動システム**:

- StackServiceでの統合状態管理
- Hybrid approach（Rust + 既存IPC）
- 透明な自動制御とフォールバック確保

### 期待効果

**ユーザビリティ**: ★★★★★

- スタッキングモード使用体験の劇的改善
- 学習コスト最小化（既存概念の拡張）
- 直感的な問題解決

**技術的価値**: ★★★★☆

- 既存アーキテクチャとの自然な統合
- 段階的実装による安全性確保
- voice_inputの差別化要素強化

この自動連動設計により、スタッキングモードが実用的で魅力的な機能となり、voice_inputの付加価値を大幅に向上させることができる。
