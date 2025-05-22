# osascript排除 - 詳細設計と手順

## Phase 1: 効果音のネイティブ実装

### 1.0 完了条件とスコープ

#### 完了条件
- [ ] NativeSoundPlayer構造体が正常に動作
- [ ] システム音声ファイル（Ping, Purr, Glass）をネイティブ再生可能
- [ ] 既存afplayコマンドからのフォールバック機構動作
- [ ] 基本的な単体テストが通過
- [ ] `cargo build`と`cargo check`が通る

#### やらないこと
- カスタム音声ファイルの対応（システム音声のみ）
- 複数音声の同時再生
- 音量調整機能
- 音声エフェクト（リバーブ等）
- 他のオーディオフォーマット対応（AIFF/WAVのみ）

### 1.1 ファイル構成
```
src/
├── infrastructure/
│   ├── audio/
│   │   ├── sound_player.rs  # 新規作成
│   │   ├── cpal_backend.rs  # 既存
│   │   └── mod.rs           # 更新
│   └── external/
│       └── sound.rs         # 段階的に更新
```

### 1.2 実装手順

#### Step 1: NativeSoundPlayer構造体作成
```rust
// src/infrastructure/audio/sound_player.rs
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound;
use std::sync::Arc;

pub struct NativeSoundPlayer {
    device: cpal::Device,
    config: cpal::StreamConfig,
    ping_data: Arc<Vec<f32>>,
    purr_data: Arc<Vec<f32>>,
    glass_data: Arc<Vec<f32>>,
}

impl NativeSoundPlayer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // 1. cpalデバイス初期化
        // 2. システム音声ファイル読み込み
        // 3. データ変換（i16 → f32）
    }
    
    pub fn play_ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.play_sound(self.ping_data.clone())
    }
    
    pub fn play_purr(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.play_sound(self.purr_data.clone())
    }
    
    pub fn play_glass(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.play_sound(self.glass_data.clone())
    }
    
    fn play_sound(&self, data: Arc<Vec<f32>>) -> Result<(), Box<dyn std::error::Error>> {
        // cpalストリーム作成・再生
    }
}
```

#### Step 2: ファイル読み込み機能
```rust
fn load_system_sound(path: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let samples: Result<Vec<i16>, _> = reader.samples().collect();
    let samples = samples?;
    
    // i16 → f32 変換 (-1.0 to 1.0)
    Ok(samples.iter().map(|&s| s as f32 / i16::MAX as f32).collect())
}
```

#### Step 3: sound.rs更新（段階的移行）
```rust
// src/infrastructure/external/sound.rs
use crate::infrastructure::audio::sound_player::NativeSoundPlayer;

pub enum SoundBackend {
    Native(NativeSoundPlayer),
    Command, // 既存実装（フォールバック用）
}

pub struct SoundPlayer {
    backend: SoundBackend,
}

impl SoundPlayer {
    pub fn new() -> Self {
        match NativeSoundPlayer::new() {
            Ok(native) => Self { backend: SoundBackend::Native(native) },
            Err(_) => Self { backend: SoundBackend::Command },
        }
    }
    
    pub fn play_ping(&self) {
        match &self.backend {
            SoundBackend::Native(player) => {
                if let Err(_) = player.play_ping() {
                    // フォールバック
                    self.play_ping_command();
                }
            },
            SoundBackend::Command => self.play_ping_command(),
        }
    }
    
    fn play_ping_command(&self) {
        // 既存のafplayコマンド実装
    }
}
```

## Phase 2: Apple Music制御のswift-bridge実装

### 2.0 完了条件とスコープ

#### 完了条件
- [ ] swift-bridgeビルド設定が正常に動作
- [ ] Apple Music一時停止・再開機能が動作
- [ ] MediaPlayer権限チェック・要求機能が動作
- [ ] 既存osascriptコマンドからのフォールバック機構動作
- [ ] Info.plist権限説明文が適切に設定
- [ ] macOS以外のプラットフォームでビルドエラーなし

#### やらないこと
- 他の音楽アプリ（Spotify等）への対応
- 楽曲情報取得機能
- プレイリスト操作
- 音量制御
- シャッフル・リピート制御
- 楽曲検索機能

### 2.1 ファイル構成
```
src/
├── native/
│   ├── mod.rs               # 新規作成
│   ├── MusicController.swift # 新規作成
│   └── bridge.rs            # 新規作成
├── infrastructure/external/
│   └── sound.rs             # Apple Music機能追加
├── build.rs                 # 新規作成
└── Info.plist              # 新規作成
```

### 2.2 実装手順

#### Step 1: build.rs設定
```rust
// build.rs
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        swift_bridge_build::parse_bridges(vec!["src/native/mod.rs"])
            .write_all_concatenated(
                std::env::var("OUT_DIR").unwrap(),
                env!("CARGO_PKG_NAME")
            );
    }
}
```

#### Step 2: Swift実装
```swift
// src/native/MusicController.swift
import MediaPlayer
import Foundation

@_cdecl("pause_apple_music_native")
public func pauseAppleMusicNative() -> Bool {
    let authStatus = MPMediaLibrary.authorizationStatus()
    guard authStatus == .authorized else {
        // 権限要求
        MPMediaLibrary.requestAuthorization { status in
            // 非同期処理
        }
        return false
    }
    
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    if musicPlayer.playbackState == .playing {
        musicPlayer.pause()
        return true
    }
    return false
}

@_cdecl("resume_apple_music_native")
public func resumeAppleMusicNative() -> Bool {
    let authStatus = MPMediaLibrary.authorizationStatus()
    guard authStatus == .authorized else { return false }
    
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    musicPlayer.play()
    return true
}

@_cdecl("get_music_playback_state")
public func getMusicPlaybackState() -> Int32 {
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    return Int32(musicPlayer.playbackState.rawValue)
}
```

#### Step 3: Rust bridge
```rust
// src/native/bridge.rs
extern "C" {
    fn pause_apple_music_native() -> bool;
    fn resume_apple_music_native() -> bool;
    fn get_music_playback_state() -> i32;
}

pub struct NativeMusicController;

impl NativeMusicController {
    pub fn pause_apple_music() -> Result<bool, Box<dyn std::error::Error>> {
        Ok(unsafe { pause_apple_music_native() })
    }
    
    pub fn resume_apple_music() -> Result<bool, Box<dyn std::error::Error>> {
        Ok(unsafe { resume_apple_music_native() })
    }
    
    pub fn is_playing() -> Result<bool, Box<dyn std::error::Error>> {
        let state = unsafe { get_music_playback_state() };
        Ok(state == 1) // MPMusicPlaybackState.playing = 1
    }
}
```

#### Step 4: Info.plist権限設定
```xml
<!-- Info.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSAppleMusicUsageDescription</key>
    <string>音声入力中にApple Musicを一時停止・再開するために使用します</string>
</dict>
</plist>
```

## Phase 3: 統合テスト

### 3.0 完了条件とスコープ

#### 完了条件
- [ ] 全ての単体テストが通過
- [ ] 統合テストが通過（効果音再生＋Apple Music制御）
- [ ] パフォーマンステストで既存実装以上の性能確認
- [ ] フォールバック機構のテストが通過
- [ ] CI/CD環境での自動テストが通過
- [ ] `cargo test`が全て成功

#### やらないこと
- 手動でのUIテスト（自動化可能な範囲のみ）
- 他のDAW（Logic Pro等）との連携テスト
- 長時間実行のストレステスト
- メモリリークの詳細解析
- セキュリティ監査
- 異なるmacOSバージョンでの互換性テスト

### 3.1 テスト項目
```rust
// tests/sound_integration.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_native_sound_playback() {
        // 効果音再生テスト
    }
    
    #[test] 
    fn test_music_control() {
        // Apple Music制御テスト
    }
    
    #[test]
    fn test_fallback_mechanism() {
        // フォールバック機構テスト
    }
}
```

### 3.2 パフォーマンス測定
```rust
// benchmarks/performance.rs
use std::time::Instant;

fn benchmark_sound_latency() {
    let start = Instant::now();
    // 音声再生処理
    let duration = start.elapsed();
    println!("Sound latency: {:?}", duration);
}
```

## 依存関係更新

### Cargo.toml（現在の設定）
```toml
[dependencies]
# 既存
cpal = "0.15"
hound = "3.5.1"

# Apple Music制御用（macOSのみ・段階的移行中のため現在は無効）
# [target.'cfg(target_os = "macos")'.dependencies]
# swift-bridge = { version = "0.1", optional = true }

# [build-dependencies]
# Apple Music制御用ビルド設定（macOSのみ・段階的移行中のため現在は無効）
# [target.'cfg(target_os = "macos")'.build-dependencies]
# swift-bridge-build = { version = "0.1", optional = true }

# [features]
# default = ["native-music"]
# native-music = ["swift-bridge", "swift-bridge-build"]
```

### 将来の完全版Cargo.toml
```toml
[dependencies]
# 既存
cpal = "0.15"
hound = "3.5.1"

# Apple Music制御用（macOSのみ）
[target.'cfg(target_os = "macos")'.dependencies]
swift-bridge = { version = "0.1", optional = true }

[build-dependencies]
# Apple Music制御用ビルド設定（macOSのみ）
[target.'cfg(target_os = "macos")'.build-dependencies]
swift-bridge-build = { version = "0.1", optional = true }

[features]
default = ["native-music"]
native-music = ["swift-bridge", "swift-bridge-build"]
```

## 実装状況（2025年12月更新）

### Phase 1: 効果音のネイティブ実装 ✅ **完了**
- ✅ NativeSoundPlayer実装完了 (`src/infrastructure/audio/sound_player.rs`)
- ✅ フォールバック機構付き段階的移行完了 (`src/infrastructure/external/sound.rs`)
- ✅ 全テスト通過確認済み (15/15 tests passed)
- ✅ `cargo check`/`cargo build`で問題なし

### Phase 2: Apple Music制御のswift-bridge実装 ✅ **基盤完了・一時無効化中**
- ✅ ファイル構成完了
  - `src/native/MusicController.swift`: Swift実装完了
  - `src/native/bridge.rs`: Rustブリッジ完了
  - `src/native/mod.rs`: モジュール統合完了
  - `Info.plist`: 権限設定完了
  - `build.rs`: ビルド設定完了（無効化中）
- ✅ sound.rsに統合済み（フォールバック機構付き）
- ⚠️ **段階的移行のため現在は無効化**
  - swift-bridge依存関係はコメントアウト
  - NativeMusicControllerは一時的にダミー実装
  - osascriptフォールバックが正常動作中

### Phase 2.1: 技術調査・準備 🔍 **未実施**

#### 🤖 Claude実行可能（Web調査・文献調査）
- [ ] swift-bridgeクレートの文献調査
  - [ ] 最新バージョン確認（Crates.io）
  - [ ] 公式ドキュメント・README調査
  - [ ] GitHub issues/PRでの既知問題調査
  - [ ] 依存関係とシステム要件の文献調査
- [ ] 代替手段の技術調査
  - [ ] Objective-C FFI（objc crate等）の調査
  - [ ] bindgen + cc crateでのフレームワーク直接呼び出し調査
  - [ ] 他のRust-Swift/Rust-ObjC連携手法調査
  - [ ] 既存プロジェクトでの事例調査
- [ ] Apple MediaPlayerフレームワーク調査
  - [ ] 公式ドキュメントでのAPI仕様確認
  - [ ] 権限要件・制限事項調査
  - [ ] バージョン互換性情報調査
- [ ] 技術比較・評価
  - [ ] 各手法のメリット・デメリット整理
  - [ ] 保守性・可読性の比較
  - [ ] 学習コスト・導入コストの評価

#### 👤 手動実行必要（ビルド・実行・環境確認）
- [ ] 実際のビルド環境確認
  - [ ] 現在のmacOS/Xcodeバージョン確認
  - [ ] swift-bridgeクレートの実際のインストール試行
  - [ ] ビルドエラーの確認と対処
- [ ] 最小限のPoC作成・実行
  - [ ] "Hello World"レベルのSwift-Rustブリッジ
  - [ ] MediaPlayerフレームワークへの最小限アクセス
  - [ ] 実際の権限ダイアログ動作確認
- [ ] パフォーマンス測定
  - [ ] 実際のレイテンシ測定
  - [ ] メモリ使用量確認
  - [ ] osascript vs ネイティブの性能比較
- [ ] 開発環境セットアップ確認
  - [ ] CI/CD環境での自動ビルド可能性確認
  - [ ] 他の開発者環境での再現性確認

#### 📋 協力作業（Claude調査 → 手動検証）
- [ ] 技術選択の最終決定
  - [ ] Claude調査結果の報告
  - [ ] 手動検証結果との照合
  - [ ] 実装方針の確定
- [ ] 実装計画の詳細化
  - [ ] Phase 2.5以降の具体的タスク定義
  - [ ] リスク評価と対策計画
  - [ ] スケジュール調整

### Phase 2.5: swift-bridge有効化 🔄 **未実施**
- [ ] Cargo.tomlでswift-bridge依存関係を有効化
- [ ] build.rsでswift-bridgeビルドを有効化
- [ ] NativeMusicControllerの実装を実際のSwift呼び出しに戻す
- [ ] ビルド・テスト確認

### Phase 3: 統合テスト・完全移行 📋 **計画中**
- [ ] swift-bridge有効化後の動作確認
- [ ] Apple Music権限テスト
- [ ] パフォーマンステスト
- [ ] osascriptフォールバック削除
- [ ] CI/CD対応

## 現在の動作状況

### 効果音（Phase 1）
- ✅ ネイティブ実装優先、afplayフォールバック
- ✅ 完全に動作中

### Apple Music制御（Phase 2）
- ✅ osascript実装で完全動作中
- ⚠️ swift-bridge実装は準備完了だが無効化中
- 📋 将来のPhase 2.5で有効化予定

## マイルストーン

### ✅ Phase 1完了
- ✅ NativeSoundPlayer実装
- ✅ 段階的移行開始
- ✅ 基本テスト実行

### ✅ Phase 2基盤完了（一時無効化）
- ✅ swift-bridge設定（無効化中）
- ✅ Apple Music制御実装（ダミー化中）
- ✅ 権限処理実装

### 📋 Phase 2.1: 技術調査（次回作業）
- [ ] 🤖 Claude: swift-bridge文献調査
- [ ] 🤖 Claude: 代替手段技術調査  
- [ ] 🤖 Claude: 技術比較・評価
- [ ] 👤 手動: 実際のビルド環境確認
- [ ] 👤 手動: 最小限PoC作成・実行
- [ ] 📋 協力: 技術選択の最終決定

### 📋 Phase 2.5: 有効化（調査後）
- [ ] swift-bridge依存関係有効化
- [ ] 実装の有効化
- [ ] 動作確認

### 📋 Phase 3: 完全移行
- [ ] 統合テスト
- [ ] パフォーマンス最適化
- [ ] osascriptフォールバック削除