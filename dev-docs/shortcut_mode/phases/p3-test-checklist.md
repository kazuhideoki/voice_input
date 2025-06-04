# Phase 3 テストチェックリスト

## 準備
```bash
cargo build --release
./target/release/voice_inputd  # Terminal 1
# Terminal 2で以下のテストを実行
```

## 基本機能テスト

### 1. UI表示とガイド
- [ ] `voice_input stack-mode on` → UIウィンドウ表示
- [ ] キーボードショートカットガイド表示確認
- [ ] "🟢 Stack Mode ON" 表示確認

### 2. ESCキー
- [ ] ESCキー押下 → UI即座に閉じる
- [ ] `voice_input status` → "disabled" 確認

### 3. ハイライト（3秒）
- [ ] スタックモードON、3つスタック作成
- [ ] Cmd+2 → スタック2が緑色ハイライト
- [ ] 3秒後に自動的にハイライト解除
- [ ] Cmd+1 → ハイライトがスタック1に移動

### 4. 10個以上のスタック
- [ ] 12個のスタック作成
- [ ] スタック1-9: "Cmd+1"〜"Cmd+9" 表示
- [ ] スタック10-12: 番号のみ表示
- [ ] 警告メッセージ表示確認

### 5. パフォーマンス
- [ ] アイドル時CPU < 5%
- [ ] ハイライト時CPU < 10%
- [ ] UI更新60fps（カクつきなし）

## クイックテストコマンド

```bash
# CPU使用率確認
top -pid $(pgrep voice_input_ui)

# メモリ使用量
ps aux | grep voice_input_ui | awk '{print $5/1024 " MB"}'

# パフォーマンステスト実行
./scripts/test-phase3-performance.sh
```

## 問題があった場合

1. アクセシビリティ権限確認
2. `pkill -f voice_input` で全プロセス終了
3. voice_inputd再起動

## 完了確認

- [ ] 全項目テスト完了
- [ ] パフォーマンス基準達成
- [ ] 実使用で問題なし