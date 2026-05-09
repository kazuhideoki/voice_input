# Realtime Whisper logprobs survey

調査日: 2026-05-09

## 結論

`gpt-realtime-whisper` は Realtime transcription session の `include` に
`item.input_audio_transcription.logprobs` を指定しても、今回の実API検証では
`conversation.item.input_audio_transcription.delta` / `completed` のどちらにも
`logprobs` を返さなかった。

同じ検証スクリプト・同じ音声・同じ Realtime transcription 経路で
`gpt-4o-transcribe` を使うと `logprobs` が返ったため、少なくとも今回の観測では
「include 指定や検出コードが誤っている」のではなく、`gpt-realtime-whisper` 側で
logprobs が実効していない可能性が高い。

## 公式 docs の前提

- Realtime transcription guide は、`gpt-realtime-whisper` を低遅延の streaming transcription
  model として案内している。
  - https://developers.openai.com/api/docs/guides/realtime-transcription
- 同 guide は、log probabilities が利用可能な場合は
  `include: ["item.input_audio_transcription.logprobs"]` で要求すると説明しており、
  例の model も `gpt-realtime-whisper` になっている。
  - https://developers.openai.com/api/docs/guides/realtime-transcription#handle-confidence-timestamps-and-diarization
- ただし同じ節に「選択した model / endpoint が optional field を support するか検証し、
  unavailable な field の fallback を持つべき」という注意もある。

## 検証方法

`samples/1.wav` と `samples/2.wav` を 24kHz mono PCM16 に変換し、Realtime API の
WebSocket transcription session に送った。`turn_detection` は `null` にして、音声送信後に
`input_audio_buffer.commit` を送る。

検証スクリプト:

```sh
python3 survey/realtime_whisper_logprobs_20260509/realtime_whisper_logprobs_probe.py \
  --audio samples/1.wav \
  --model gpt-realtime-whisper

python3 survey/realtime_whisper_logprobs_20260509/realtime_whisper_logprobs_probe.py \
  --audio samples/2.wav \
  --model gpt-realtime-whisper

python3 survey/realtime_whisper_logprobs_20260509/realtime_whisper_logprobs_probe.py \
  --audio samples/1.wav \
  --model gpt-4o-transcribe \
  --skip-no-include
```

スクリプトは Python 標準ライブラリのみを使う。`OPENAI_API_KEY` または
`TRANSCRIPTION_API_KEY` は `.env` または環境変数から読む。

## 実行結果

### `gpt-realtime-whisper`

Run:

- `survey/realtime_whisper_logprobs_20260509/runs/20260509T112653Z`
- `survey/realtime_whisper_logprobs_20260509/runs/20260509T112725Z`

| audio | include | success | completed logprobs | delta events with logprobs |
|---|---:|---:|---:|---:|
| `samples/1.wav` | true | true | 0 | 0 |
| `samples/1.wav` | false | true | 0 | 0 |
| `samples/2.wav` | true | true | 0 | 0 |
| `samples/2.wav` | false | true | 0 | 0 |

`include=true` の run では `session.updated` に以下が反映されていた。

```json
{
  "session": {
    "audio": {
      "input": {
        "transcription": {
          "model": "gpt-realtime-whisper",
          "language": "ja",
          "prompt": null
        },
        "turn_detection": null
      }
    },
    "include": ["item.input_audio_transcription.logprobs"]
  }
}
```

しかし受信した transcription event は `delta` と `completed` に transcript だけを含み、
`logprobs` field は含まれなかった。

### `gpt-4o-transcribe` control

Run:

- `survey/realtime_whisper_logprobs_20260509/runs/20260509T112716Z`

| audio | include | success | completed logprobs | delta events with logprobs | avg logprob |
|---|---:|---:|---:|---:|---:|
| `samples/1.wav` | true | true | 32 | 32 | -0.1722 |

この control run では、`conversation.item.input_audio_transcription.delta` と
`completed` の両方で `logprobs` が返った。

## voice-input への含意

現状の `src/infrastructure/external/realtime_whisper_adapter.rs` は
`include: ["item.input_audio_transcription.logprobs"]` を送っており、API が返せば
`TranscriptionOutput.tokens` に mapping できる形になっている。

ただし、今回の実APIでは `gpt-realtime-whisper` から token logprobs が返らなかったため、
Realtime Whisper 経路では低信頼範囲選択を logprobs 前提で動かすことはできない。
実装上は `tokens` が空でも成立する fallback を維持する必要がある。

## 指定方法の追加確認

公式 docs / API reference / SDK 型定義 / 公開事例を確認した範囲では、Realtime
transcription event の logprobs を要求する公開指定は
`session.include: ["item.input_audio_transcription.logprobs"]` だった。

指定場所やフィールド名の誤りを疑い、`samples/1.wav` で以下の variant も実API確認した。

| variant | 結果 |
|---|---|
| `session.include` | API は受理するが、`gpt-realtime-whisper` では logprobs 0 件 |
| `session.audio.input.transcription.logprobs = true` | `unknown_parameter` |
| event root の `include` | `unknown_parameter` |
| `session.audio.input.include` | `unknown_parameter` |
| 旧Realtime風の `session.input_audio_format` / `session.input_audio_transcription` | `unknown_parameter` |

追加 run:

- `survey/realtime_whisper_logprobs_20260509/runs/20260509T121715Z`
- `survey/realtime_whisper_logprobs_20260509/runs/20260509T121726Z`

このため、今回の不達は「`include` の置き場所や名前を間違えた」可能性よりも、
`gpt-realtime-whisper` の optional logprobs が現時点の実APIでは返らない、と見る方が妥当。

なお、OpenAI Python SDK の Audio Transcriptions 型定義では、Audio API の `logprobs` は
`gpt-4o-transcribe` / `gpt-4o-mini-transcribe` / `gpt-4o-mini-transcribe-2025-12-15`
かつ `response_format=json` のみと記載されており、ここにも `gpt-realtime-whisper`
の logprobs 対応は出てこない。

## 再検証ポイント

- OpenAI 側で `gpt-realtime-whisper` の optional field support が変わる可能性があるため、
  model 更新時はこの survey script を再実行する。
- API が logprobs を返すようになった場合、`completed` だけでなく `delta` 側にも入るかを確認する。
- 本調査では `turn_detection=null` の manual commit のみを確認した。
