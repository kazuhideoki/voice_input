# GPT Realtime Whisper findings

このメモは、公式ドキュメントだけでは判断しにくかった実API検証の観測結果を残すものです。

## Realtime API の運用上の注意

`gpt-realtime-whisper` では `server_vad` が使えなかった。実APIでは `session.audio.input.turn_detection` に対して `Turn detection is not supported for this transcription model.` が返った。

そのため、今回の検証スクリプトでは `turn_detection=null` にして、音声送信後に `input_audio_buffer.commit` を送る。

また、`session.update` 直後に音声を送り始めるのではなく、`session.updated` を受け取ってから送信開始する方が安全。

## 評価軸

voice-input で採用判断するときは、次の軸を分ける。

- 体感速度: 長文では `gpt-realtime-whisper` の逐次表示が有利
- 短文の一括入力: `gpt-4o-transcribe` のファイル処理でも十分速い可能性がある
- 実装複雑度: `gpt-realtime-whisper` は WebSocket とイベント制御が必要で、`gpt-4o-transcribe` より扱う状態が多い

## 再現用コマンド

```sh
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py \
  --audio survey/realtime_whisper_api/sample.wav \
  --providers realtime_whisper \
  --send-interval-scale 1

.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py \
  --audio survey/realtime_whisper_api/sample.wav \
  --providers gpt4o_stream
```
