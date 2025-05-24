# macOS権限設定の効率化提案

## 現状の課題

現在、`voice_inputd`の再ビルド時に以下の手動作業が必要：

1. システム設定でアクセシビリティ権限を再設定
2. LaunchAgentの再読み込み（`launchctl unload/load`）
3. 初回実行時の各種権限許可

これらの作業は開発効率を低下させ、リリース時のユーザー体験も損なう。

## 試行した解決策と結果

### 1. ❌ Ad-hocコード署名 + ビルドスクリプト

**試行内容：**
```bash
codesign -s - -f target/release/voice_inputd
launchctl unload/load ~/Library/LaunchAgents/com.user.voiceinputd.plist
```

**結果：** 失敗。バイナリのハッシュ値が変わるため、アクセシビリティ権限が無効になる。

### 2. ❌ シンボリックリンク方式

**試行内容：**
```bash
sudo ln -s /Users/kazuhideoki/voice_input/target/release/voice_inputd /usr/local/bin/voice_inputd
# LaunchAgentでシンボリックリンクを参照
```

**結果：** 失敗。macOSはシンボリックリンクの実体（ターゲット）のハッシュ値を検証するため、効果なし。

### 3. ❌ TCCキャッシュクリア

**試行内容：**
```bash
sudo killall tccd
```

**結果：** 一時的な効果のみ。根本的な解決にならず。

## 問題の本質

macOSのTCC（Transparency, Consent, and Control）システムは、以下の要素でアプリを識別：

1. **実行ファイルのハッシュ値**（SHA-256）
2. **Bundle ID**（.appバンドルの場合）
3. **コード署名情報**

再ビルドするとハッシュ値が変わるため、TCCデータベースの既存エントリと一致しなくなる。

## 効果的な解決策

### 1. **ラッパースクリプト方式**（最も実用的）

実行ファイルを直接登録せず、変更されないラッパースクリプトを介して実行：

```bash
#!/bin/bash
# /usr/local/bin/voice_inputd_wrapper
exec /Users/kazuhideoki/voice_input/target/release/voice_inputd "$@"
```

**実装手順：**
1. ラッパースクリプトを作成し、実行権限を付与
2. LaunchAgentでラッパーを指定
3. システム設定でラッパーにアクセシビリティ権限を付与

**メリット：**
- ラッパーのハッシュ値は変わらない
- 再ビルド後も権限が維持される
- 簡単に実装可能

### 2. **アプリケーションバンドル化**（推奨）

`.app`形式にすることで、Bundle IDベースの権限管理が可能：

```
VoiceInputDaemon.app/
├── Contents/
│   ├── Info.plist
│   ├── MacOS/
│   │   └── launcher.sh  # 固定のラッパースクリプト
│   └── Resources/
│       └── voice_inputd  # 実際のバイナリ
```

**launcher.sh:**
```bash
#!/bin/bash
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
exec "$DIR/../Resources/voice_inputd" "$@"
```

**Info.plist:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.kazuhideoki.voice-inputd</string>
    <key>CFBundleName</key>
    <string>VoiceInputDaemon</string>
    <key>CFBundleExecutable</key>
    <string>launcher.sh</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
```

### 3. **デーモン分離アーキテクチャ**

権限が必要な操作を別プロセスに分離：

```
voice_inputd (メインデーモン・頻繁に再ビルド)
    ↓ IPC
voice_input_helper (権限実行用・めったに変更しない)
```

**メリット：**
- helperのみに権限を付与
- メインデーモンは自由に再ビルド可能
- セキュリティ的にも優れている

## 即座に実装可能な対策

### 開発用セットアップスクリプト

```bash
#!/bin/bash
# scripts/setup-dev-env.sh

echo "📦 Setting up development environment..."

# 1. ラッパースクリプトを作成
cat > /tmp/voice_inputd_wrapper << 'EOF'
#!/bin/bash
exec /Users/kazuhideoki/voice_input/target/release/voice_inputd "$@"
EOF

# 2. 適切な場所に配置
sudo mv /tmp/voice_inputd_wrapper /usr/local/bin/
sudo chmod +x /usr/local/bin/voice_inputd_wrapper

# 3. LaunchAgentを更新
sed -i.bak 's|target/release/voice_inputd|/usr/local/bin/voice_inputd_wrapper|g' \
    ~/Library/LaunchAgents/com.user.voiceinputd.plist

echo "✅ Setup complete!"
echo ""
echo "⚠️  システム設定で以下に権限を付与してください："
echo "   /usr/local/bin/voice_inputd_wrapper"
```

### 開発用ビルドスクリプト（改良版）

```bash
#!/bin/bash
# scripts/dev-build.sh

echo "🔨 Building voice_input..."
cargo build --release || exit 1

echo "🔄 Restarting daemon..."
launchctl unload ~/Library/LaunchAgents/com.user.voiceinputd.plist 2>/dev/null
launchctl load ~/Library/LaunchAgents/com.user.voiceinputd.plist

echo "✅ Build complete!"
```

## 推奨される実装順序

1. **即座に：** ラッパースクリプト方式を実装
2. **短期的：** アプリケーションバンドル化
3. **中期的：** デーモン分離アーキテクチャ
4. **長期的：** Apple Developer Programによる正式署名

## まとめ

macOSのセキュリティ制約により、バイナリの再ビルド時に権限が失われるのは避けられない。しかし、ラッパースクリプトやアプリケーションバンドル化により、開発効率を大幅に改善できる。特にラッパースクリプト方式は、5分で実装可能な実用的な解決策である。