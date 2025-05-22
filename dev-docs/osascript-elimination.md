# osascript依存排除の実装方法

## 目的
実行の安定性向上のため、外部コマンド依存を最小化する

## 現状分析

`src/infrastructure/external/sound.rs`で以下の外部コマンド依存:
- 効果音: `afplay`コマンド（→ ネイティブ実装へ変更）
- Apple Music制御: `osascript`コマンド（→ swift-bridge実装へ変更）

## 実装方法

### 1. 効果音再生 - ネイティブ実装（採用）

#### 現在の実装
```rust
Command::new("afplay")
    .arg("/System/Library/Sounds/Ping.aiff")
    .spawn();
```

#### ネイティブ実装アプローチ（既存cpal活用）
```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;

pub struct NativeSoundPlayer {
    // システム音声ファイルを事前ロード
    ping_data: Arc<Vec<f32>>,
    purr_data: Arc<Vec<f32>>, 
    glass_data: Arc<Vec<f32>>,
}

impl NativeSoundPlayer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // AIFFファイルを読み込んでf32サンプルデータに変換
        // /System/Library/Sounds/*.aiff を hound で読み込み
    }
    
    pub fn play_sound(&self, sound_data: Arc<Vec<f32>>) {
        // cpalを使ってオーディオストリームで再生
    }
}
```

### 2. Apple Music制御 - swift-bridge実装（採用）

#### 現在の実装（排除対象）
```rust
Command::new("osascript")
    .arg("-e")
    .arg(applescript_code)
    .output()
```

#### swift-bridge実装アプローチ

**必要な設定**:
- `build.rs`でSwiftコンパイル設定
- `Info.plist`に`NSAppleMusicUsageDescription`追加（権限説明）

**実装**:

1. ビルド設定 (`build.rs`):
```rust
fn main() {
    swift_bridge_build::parse_bridges(vec!["src/native/mod.rs"])
        .write_all_concatenated(out_dir, env!("CARGO_PKG_NAME"));
}
```

2. Swift側実装 (`src/native/MusicController.swift`):
```swift
import MediaPlayer
import Foundation

@_cdecl("pause_apple_music_native")
public func pauseAppleMusicNative() -> Bool {
    // 権限チェック
    let authStatus = MPMediaLibrary.authorizationStatus()
    if authStatus != .authorized {
        MPMediaLibrary.requestAuthorization { _ in }
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
public func resumeAppleMusicNative() {
    let authStatus = MPMediaLibrary.authorizationStatus()
    guard authStatus == .authorized else { return }
    
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    musicPlayer.play()
}
```

3. Rust側実装:
```rust
extern "C" {
    fn pause_apple_music_native() -> bool;
    fn resume_apple_music_native();
}

pub fn pause_apple_music() -> bool {
    unsafe { pause_apple_music_native() }
}

pub fn resume_apple_music() {
    unsafe { resume_apple_music_native() }
}
```

### 3. 実装方針

#### Phase 1: 効果音のネイティブ実装
1. `hound`クレートでAIFFファイル読み込み
2. 既存`cpal`でネイティブ再生
3. 非同期対応でUIブロック回避

#### Phase 2: Apple Music制御のswift-bridge実装
1. `swift-bridge`でSwift/MediaPlayer統合
2. フォールバック機構（API失敗時の処理）
3. 権限チェック（Privacy設定対応）

#### Phase 3: 統合テスト
1. 動作確認とパフォーマンス測定
2. エラーハンドリング強化

## 依存関係変更

### 追加が必要なクレート
```toml
[dependencies]
# 効果音用
hound = "3.5.1"  # 既に存在

# Apple Music制御用
swift-bridge = "0.1"
```

### ビルド設定変更
```toml
[build-dependencies]
swift-bridge-build = "0.1"
```

## 移行手順

1. **段階的移行**: 既存コードを残しつつ新実装を並行開発
2. **動作確認**: 新実装の動作テスト完了後
3. **完全移行**: 外部コマンド依存コード削除

## 期待効果

- **安定性向上**: 外部コマンド依存排除による実行安定性改善
- **起動速度向上**: 外部プロセス起動オーバーヘッド削除
- **保守性向上**: システム環境依存の減少