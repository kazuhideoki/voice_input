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

## 短尺音声での速度比較

実利用では 30 秒より短い入力が多い想定なので、元音声から開始位置を変えた 3 パターンを切り出し、それぞれ 2 秒、5 秒、10 秒で比較した。さらに各秒数で 5 セットずつ追加実行し、各秒数 `n=8` で平均を取り直した。Realtime 側は実発話に近づけるため `--send-interval-scale 1` で送信した。

| 音声長 | provider | n | 初回表示 avg | 話し終わり後の最終待ち avg | 最終待ちの range |
|---:|---|---:|---:|---:|---:|
| 2s | `realtime_whisper` | 8 | 2.238s | 0.892s | 0.722-1.124s |
| 2s | `gpt4o_stream` | 8 | 1.260s | 1.330s 相当 | 0.721-2.011s |
| 5s | `realtime_whisper` | 8 | 1.814s | 0.820s | 0.709-0.988s |
| 5s | `gpt4o_stream` | 8 | 0.942s | 1.123s 相当 | 0.907-1.471s |
| 10s | `realtime_whisper` | 8 | 1.854s | 0.869s | 0.683-1.011s |
| 10s | `gpt4o_stream` | 8 | 1.007s | 1.303s 相当 | 1.039-1.595s |

ここでの `gpt4o_stream` の「話し終わり後の最終待ち」は、録音停止後にファイルを送った場合の API 完了時間として扱った。`realtime_whisper` は、音声を送り終えてから `conversation.item.input_audio_transcription.completed` を受け取るまでの時間。

この検証では、`realtime_whisper` は停止後の最終待ちが概ね 0.7-1.1 秒に収まった。`gpt4o_stream` は短尺でも 0.7-2.0 秒程度の揺れがあり、平均では `realtime_whisper` の方が停止後待ちは短かった。

一方で、最終テキスト品質は `realtime_whisper` が常に上とは言えない。例として、元音声の「ウェイクを検知」が `realtime_whisper` で「結構検知」に寄るケースがあった。`gpt4o_stream` も英字表記への補正や補完は入るため、どちらが望ましいかは voice-input の用途次第。

検証に使った追加サンプルは `survey/realtime_whisper_api/samples/` に配置した。切り出し位置は `segment_manifest.csv` に記録している。

主な run:

- `survey/realtime_whisper_api/runs/20260508T121038`
- `survey/realtime_whisper_api/runs/20260508T121045`
- `survey/realtime_whisper_api/runs/20260508T121055`
- `survey/realtime_whisper_api/runs/20260508T121110`
- `survey/realtime_whisper_api/runs/20260508T121118`
- `survey/realtime_whisper_api/runs/20260508T121128`
- `survey/realtime_whisper_api/runs/20260508T121144`
- `survey/realtime_whisper_api/runs/20260508T121151`
- `survey/realtime_whisper_api/runs/20260508T121201`
- `survey/realtime_whisper_api/runs/20260508T132547` から `survey/realtime_whisper_api/runs/20260508T132818` までの追加 15 run

## 再現用コマンド

```sh
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py \
  --audio survey/realtime_whisper_api/sample.wav \
  --providers realtime_whisper \
  --send-interval-scale 1

.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py \
  --audio survey/realtime_whisper_api/sample.wav \
  --providers gpt4o_stream

.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py \
  --audio survey/realtime_whisper_api/samples/p2_5s.wav \
  --providers realtime_whisper,gpt4o_stream \
  --send-interval-scale 1
```
