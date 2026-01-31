# パフォーマンス改善計画（優先順）

## 目的
- 「停止→結果入力」までの体感時間を短縮する
- API待ちが支配的でも、クライアント側の無駄を削る
- 変更前後で数値比較できるようにする

## 現状サマリ（計測ログから）
- OpenAI API待ちが最大要因（約1.2〜1.9s）
- `text_input_subprocess` が短文で大きい（200〜500ms）
- 音声処理（trim + FLAC）も30秒では200ms超
- 辞書処理は小さい（10ms未満）

## 計測指標（変更前後の比較に使う）
- `transcription.handle`（停止→入力完了）
- `openai.send`（API待ち）
- `audio.stop_recording`（trim + encode）
- `text_input.subprocess`（直接入力の起動/実行）

## 優先順位（高→低）

### P0: 音声データを小さくする（効果最大）
**狙い**: API待ちと送信量を減らす  
**理由**: API時間が支配的で、入力サイズ削減が最も効きやすい  

やること
- 収録後の `trim_silence` の後に **モノラル化 + リサンプリング** を挿入
- 目標値: 16kHz / 1ch
- 変更点の計測: `audio.stop_recording` / `openai.send` / `openai.transcribe_total`

期待効果
- 送信サイズの大幅減（理論上 1/3〜1/6）
- API待ちの短縮（効果の程度は検証が必要）

### P1: enigo_helper 常駐化（短文の体感改善）
**狙い**: `text_input.subprocess` の起動コスト削減  
**理由**: 短文での占有率が高い（200〜500ms）

やること
- `enigo_helper` を常駐プロセスにし、stdin/IPC でテキストを送信
- 失敗時は現行方式にフォールバック（理由: 安定性維持）
- 計測: `text_input.subprocess` がどれだけ縮むか確認

期待効果
- 200〜500ms 程度の短縮（短文ほど効果が大きい）

### P2: ブロッキング要素の削減（体感の引き上げ）
**狙い**: シングルスレッドの停止を避ける  
**理由**: current_thread で `std::thread::sleep` / `osascript` が止まりやすい

やること
- `std::thread::sleep(100ms)` を `tokio::time::sleep` へ置換
- 可能なら `osascript` を spawn で非同期化
- 計測: `transcription.handle` の尾部と体感反応速度

期待効果
- 体感の「引っかかり」減少（数十〜100ms）

### P3: 辞書の高速化（将来のスケール対策）
**狙い**: 辞書サイズ増加時の劣化を防ぐ  

やること
- `apply_replacements` の事前構造化（surface_chars のキャッシュ）
- 必要なら Aho-Corasick の導入検討
- 計測: `transcription.dict` の増減

期待効果
- 辞書規模増大時の遅延抑制

## 実施順（提案）
1) P0: モノラル化 + 16kHz 化  
2) P1: enigo_helper 常駐化  
3) P2: ブロッキング要素削減  
4) P3: 辞書の高速化

## 補足
- `VOICE_INPUT_PROFILE=1` を有効にして計測を継続する
- P0/P1 で十分な体感改善が得られたら、P2/P3 は後回しでも良い
