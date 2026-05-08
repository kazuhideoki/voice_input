# GPT Realtime Whisper survey

`sample.wav` を使って、以下の2経路の文字起こし速度と結果を比較するための調査スクリプトです。

- `gpt-4o-transcribe`: Audio API `/v1/audio/transcriptions` に `stream=true` を付けて SSE の delta/done を記録
- `gpt-realtime-whisper`: Realtime API の transcription session に WAV を 24kHz PCM16 mono として chunk 送信

実APIで観測した注意点は `survey/realtime_whisper_api/FINDINGS.md` に記録しています。

## Setup

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r survey/realtime_whisper_api/requirements.txt
```

API key は `.env` または環境変数で設定します。

```sh
export OPENAI_API_KEY=...
```

## Run

```sh
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py
```

出力は `survey/realtime_whisper_api/runs/YYYYMMDDTHHMMSS/` に保存されます。

- `benchmark.log`: 実行ログ
- `raw_events.jsonl`: ストリームで受信した raw event
- `results.json` / `results.csv`: 速度、first delta、最終 transcript、任意の WER/CER
- `summary.md`: provider 別の簡易サマリ
- `transcripts/*.txt`: provider ごとの最終文字起こし

## Accuracy reference

正解テキストがある場合は CER/WER を出せます。

```sh
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py \
  --audio survey/realtime_whisper_api/sample.wav \
  --expected-file expected.txt
```

日本語のように空白区切りでない言語では、WER より CER を主に見てください。

## Useful options

```sh
# 複数回実行
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py --repetitions 3

# デフォルトはファイル検証向けに最速送信。
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py --send-interval-scale 0

# 実時間ストリームに近い速度で送る。
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py --send-interval-scale 1

# Realtime セッション設定の反映は待ってから音声送信される。
# `gpt-realtime-whisper` は server VAD 非対応のため、既定では手動 commit します。

# Realtime 側で server VAD を使う。`gpt-realtime-whisper` は非対応なので、
# `--realtime-model gpt-4o-transcribe` など対応モデルの確認用。
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py --realtime-vad

# 片方だけ実行
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py --providers gpt4o_stream
.venv/bin/python survey/realtime_whisper_api/realtime_whisper_benchmark.py --providers realtime_whisper
```

`sample.wav` は現在 16kHz mono PCM なので、Realtime API 用の 24kHz PCM16 mono 変換はスクリプト内で行います。
