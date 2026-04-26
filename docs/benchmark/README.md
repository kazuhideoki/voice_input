# ASR benchmark results

This directory contains benchmark results for Japanese ASR requests using the
audio files `capture-000001.wav` through `capture-000006.wav` from:

`/Users/kazuhideoki/diarize-log-storage/storage/runs/20260421T150358_264+0900/audios`

Each target model was requested 5 times per audio file, for 30 requests per
model.

## Summary

| Target | Requests | Mean latency | Median latency | Notes |
|---|---:|---:|---:|---|
| OpenAI `gpt-4o-transcribe` | 30/30 | 6.001s | 6.259s | Japanese language was specified with `language=ja`. |
| pyannote `parakeet-tdt-0.6b-v3` | 30/30 | 14.636s | 14.315s | Default pyannote STT model. Japanese audio was transcribed as English/romaji-like text. |
| pyannote `faster-whisper-large-v3-turbo` | 30/30 | 16.145s | 14.302s | Japanese transcription improved significantly, but output has many spaces and occasional hallucinated endings. |

## Directory layout

- `openai-gpt4o-and-pyannote-parakeet-initial/`
  - Initial run: all OpenAI requests and the first half of pyannote Parakeet requests.
- `pyannote-parakeet-continuation/`
  - Continuation run for the remaining pyannote Parakeet requests.
- `openai-vs-pyannote-parakeet/`
  - Combined comparison for OpenAI `gpt-4o-transcribe` and pyannote `parakeet-tdt-0.6b-v3`.
- `pyannote-whisper/`
  - Rerun of pyannote using `faster-whisper-large-v3-turbo`.
- `pyannote-whisper-comparison/`
  - Comparison of OpenAI, pyannote Parakeet, and pyannote Whisper results.

## Files

Per-run directories contain:

- `manifest.json`: input files, duration, repetitions, and model configuration.
- `results.json`: normalized benchmark results.
- `results.csv`: CSV version of normalized benchmark results.
- `raw_payloads.jsonl`: raw API responses for each request.
- `summary.md`: latency summary for that run.

The benchmark script used to generate these results is `scripts/asr_benchmark.py`.
