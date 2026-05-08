#!/usr/bin/env python3
"""Compare streamed GPT-4o transcription with GPT Realtime Whisper.

The script writes a full run directory with logs, raw streamed events, final
transcripts, and simple latency/accuracy metrics.
"""

from __future__ import annotations

import argparse
import base64
import csv
import json
import logging
import math
import os
import ssl
import struct
import sys
import threading
import time
import uuid
import wave
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen


OPENAI_TRANSCRIPTIONS_URL = "https://api.openai.com/v1/audio/transcriptions"
OPENAI_REALTIME_TRANSCRIPTION_URL = "wss://api.openai.com/v1/realtime?intent=transcription"
REALTIME_SAMPLE_RATE = 24_000


@dataclass
class BenchmarkResult:
    provider: str
    model: str
    audio: str
    repetition: int
    success: bool
    elapsed_seconds: float
    audio_duration_seconds: float
    first_delta_seconds: float | None
    audio_sent_seconds: float | None
    source_audio_sent_seconds: float | None
    tail_seconds: float | None
    tail_after_source_audio_sent_seconds: float | None
    real_time_factor: float | None
    audio_tail_rms_dbfs: float | None
    audio_tail_peak_dbfs: float | None
    final_silence_ms: int
    likely_truncated_audio: bool
    transcript: str
    wer: float | None
    cer: float | None
    error: str


class HttpResponseError(RuntimeError):
    def __init__(self, status: int, body: str) -> None:
        super().__init__(f"{status}: {body}")
        self.status = status
        self.body = body


class RawEventWriter:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._file = self.path.open("a", encoding="utf-8")

    def write(self, record: dict[str, Any]) -> None:
        with self._lock:
            self._file.write(json.dumps(record, ensure_ascii=False) + "\n")
            self._file.flush()

    def close(self) -> None:
        self._file.close()


@dataclass(frozen=True)
class AudioTailDiagnostics:
    tail_rms_dbfs: float | None
    tail_peak_dbfs: float | None
    final_silence_ms: int
    likely_truncated_audio: bool


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def setup_logging(output_dir: Path, verbose: bool) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    level = logging.DEBUG if verbose else logging.INFO
    formatter = logging.Formatter("%(asctime)s %(levelname)s %(message)s")

    root = logging.getLogger()
    root.setLevel(level)
    root.handlers.clear()

    console = logging.StreamHandler()
    console.setLevel(level)
    console.setFormatter(formatter)
    root.addHandler(console)

    file_handler = logging.FileHandler(output_dir / "benchmark.log", encoding="utf-8")
    file_handler.setLevel(logging.DEBUG)
    file_handler.setFormatter(formatter)
    root.addHandler(file_handler)


def audio_duration_seconds(path: Path) -> float:
    with wave.open(str(path), "rb") as wav:
        return wav.getnframes() / float(wav.getframerate())


def wav_samples_mono(path: Path) -> tuple[tuple[int, ...], int]:
    with wave.open(str(path), "rb") as wav:
        if wav.getcomptype() != "NONE":
            raise ValueError(f"compressed WAV is unsupported: {wav.getcomptype()}")
        if wav.getsampwidth() != 2:
            raise ValueError(f"only 16-bit PCM WAV is supported, got {wav.getsampwidth() * 8}-bit")

        channels = wav.getnchannels()
        source_rate = wav.getframerate()
        frames = wav.readframes(wav.getnframes())

    samples = struct.unpack(f"<{len(frames) // 2}h", frames)
    if channels <= 1:
        return samples, source_rate

    mono_samples = []
    for index in range(0, len(samples), channels):
        mono_samples.append(round(sum(samples[index : index + channels]) / channels))
    return tuple(mono_samples), source_rate


def read_wav_as_pcm16_mono(path: Path, target_rate: int) -> bytes:
    samples, source_rate = wav_samples_mono(path)
    resampled = resample_pcm16(samples, source_rate, target_rate)
    return struct.pack(f"<{len(resampled)}h", *resampled)


def dbfs_from_amplitude(amplitude: float) -> float | None:
    if amplitude <= 0:
        return None
    return 20.0 * math.log10(amplitude / 32768.0)


def audio_tail_diagnostics(
    path: Path,
    tail_window_ms: int = 500,
    silence_threshold_dbfs: float = -45.0,
    min_final_silence_ms: int = 250,
) -> AudioTailDiagnostics:
    samples, sample_rate = wav_samples_mono(path)
    if not samples:
        return AudioTailDiagnostics(None, None, 0, False)

    tail_sample_count = max(1, min(len(samples), int(sample_rate * tail_window_ms / 1000)))
    tail = samples[-tail_sample_count:]
    rms = math.sqrt(sum(sample * sample for sample in tail) / len(tail))
    peak = max(abs(sample) for sample in tail)
    tail_rms_dbfs = dbfs_from_amplitude(rms)
    tail_peak_dbfs = dbfs_from_amplitude(float(peak))

    silence_threshold = 32768.0 * (10.0 ** (silence_threshold_dbfs / 20.0))
    window_sample_count = max(1, int(sample_rate * 20 / 1000))
    final_silent_samples = 0
    for end in range(len(samples), 0, -window_sample_count):
        start = max(0, end - window_sample_count)
        window = samples[start:end]
        window_rms = math.sqrt(sum(sample * sample for sample in window) / len(window))
        if window_rms > silence_threshold:
            break
        final_silent_samples += len(window)

    final_silence_ms = round(final_silent_samples * 1000 / sample_rate)
    likely_truncated_audio = final_silence_ms < min_final_silence_ms
    return AudioTailDiagnostics(tail_rms_dbfs, tail_peak_dbfs, final_silence_ms, likely_truncated_audio)


def resample_pcm16(samples: tuple[int, ...], source_rate: int, target_rate: int) -> tuple[int, ...]:
    if source_rate == target_rate:
        return samples
    if not samples:
        return ()

    output_length = max(1, round(len(samples) * target_rate / source_rate))
    output: list[int] = []
    for index in range(output_length):
        source_position = index * source_rate / target_rate
        left = int(math.floor(source_position))
        right = min(left + 1, len(samples) - 1)
        fraction = source_position - left
        value = samples[left] * (1.0 - fraction) + samples[right] * fraction
        output.append(max(-32768, min(32767, round(value))))
    return tuple(output)


def chunk_pcm16(pcm: bytes, sample_rate: int, chunk_ms: int) -> list[bytes]:
    bytes_per_chunk = max(2, int(sample_rate * chunk_ms / 1000) * 2)
    bytes_per_chunk -= bytes_per_chunk % 2
    return [pcm[index : index + bytes_per_chunk] for index in range(0, len(pcm), bytes_per_chunk)]


def multipart_body(fields: dict[str, str], file_field: str, file_path: Path, content_type: str) -> tuple[str, bytes]:
    boundary = f"----voice-input-survey-{uuid.uuid4().hex}"
    chunks: list[bytes] = []
    for name, value in fields.items():
        chunks.extend(
            [
                f"--{boundary}\r\n".encode(),
                f'Content-Disposition: form-data; name="{name}"\r\n\r\n'.encode(),
                value.encode("utf-8"),
                b"\r\n",
            ]
        )
    chunks.extend(
        [
            f"--{boundary}\r\n".encode(),
            f'Content-Disposition: form-data; name="{file_field}"; filename="{file_path.name}"\r\n'.encode(),
            f"Content-Type: {content_type}\r\n\r\n".encode(),
            file_path.read_bytes(),
            b"\r\n",
            f"--{boundary}--\r\n".encode(),
        ]
    )
    return boundary, b"".join(chunks)


def open_http(request: Request, timeout: int) -> Any:
    try:
        return urlopen(request, timeout=timeout, context=ssl.create_default_context())
    except HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        raise HttpResponseError(error.code, body) from error


def extract_delta(event: dict[str, Any]) -> str:
    for key in ("delta", "text", "transcript"):
        value = event.get(key)
        if isinstance(value, str):
            return value
    return ""


def parse_sse_blocks(response: Any) -> Any:
    event_name = ""
    data_lines: list[str] = []
    for raw_line in response:
        line = raw_line.decode("utf-8", errors="replace").rstrip("\r\n")
        if not line:
            if data_lines:
                yield event_name, "\n".join(data_lines)
                event_name = ""
                data_lines = []
            continue
        if line.startswith("event:"):
            event_name = line.removeprefix("event:").strip()
        elif line.startswith("data:"):
            data_lines.append(line.removeprefix("data:").strip())
        elif line.startswith("{"):
            yield "", line
    if data_lines:
        yield event_name, "\n".join(data_lines)


def stream_gpt4o_transcribe(
    audio_path: Path,
    model: str,
    language: str,
    api_key: str,
    raw_events: RawEventWriter,
    timeout_seconds: int,
) -> tuple[str, float | None, dict[str, Any]]:
    boundary, body = multipart_body(
        {
            "model": model,
            "language": language,
            "stream": "true",
        },
        "file",
        audio_path,
        "audio/wav",
    )
    request = Request(
        OPENAI_TRANSCRIPTIONS_URL,
        data=body,
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": f"multipart/form-data; boundary={boundary}",
        },
        method="POST",
    )

    started = time.monotonic()
    first_delta: float | None = None
    transcript_parts: list[str] = []
    done_text = ""
    event_count = 0

    logging.info("gpt4o_stream request started model=%s audio=%s", model, audio_path)
    with open_http(request, timeout_seconds) as response:
        for event_name, data in parse_sse_blocks(response):
            event_count += 1
            if data == "[DONE]":
                break
            try:
                payload = json.loads(data)
            except json.JSONDecodeError:
                payload = {"data": data}

            event_type = payload.get("type") or event_name
            now = time.monotonic()
            raw_events.write(
                {
                    "provider": "gpt4o_stream",
                    "received_seconds": now - started,
                    "event": event_type,
                    "payload": payload,
                }
            )

            if event_type == "transcript.text.delta":
                delta = extract_delta(payload)
                if delta and first_delta is None:
                    first_delta = now - started
                if delta:
                    transcript_parts.append(delta)
                    logging.debug("gpt4o_stream delta=%s", delta)
            elif event_type == "transcript.text.done":
                done_text = extract_delta(payload)
                logging.info("gpt4o_stream completed at %.3fs", now - started)

    transcript = done_text or "".join(transcript_parts)
    metadata = {"event_count": event_count}
    return transcript, first_delta, metadata


def stream_realtime_whisper(
    audio_path: Path,
    model: str,
    language: str,
    api_key: str,
    raw_events: RawEventWriter,
    realtime_url: str,
    chunk_ms: int,
    send_interval_scale: float,
    timeout_after_send_seconds: int,
    use_vad: bool,
) -> tuple[str, float | None, float, float, dict[str, Any]]:
    try:
        import websocket  # type: ignore[import-not-found]
    except ImportError as error:
        raise RuntimeError("websocket-client is required. Install with: python3 -m pip install websocket-client") from error

    pcm = read_wav_as_pcm16_mono(audio_path, REALTIME_SAMPLE_RATE)
    chunks = chunk_pcm16(pcm, REALTIME_SAMPLE_RATE, chunk_ms)
    started = time.monotonic()
    first_delta: float | None = None
    completed_transcripts: list[str] = []
    partial_parts: list[str] = []
    receiver_error: list[str] = []
    audio_sent_seconds: float | None = None
    source_audio_sent_seconds: float | None = None
    completed_seconds: float | None = None
    completed = threading.Event()
    session_ready = threading.Event()
    stop_receiver = threading.Event()
    event_count = 0
    event_count_lock = threading.Lock()

    logging.info(
        "realtime_whisper connecting model=%s audio=%s chunks=%d chunk_ms=%d",
        model,
        audio_path,
        len(chunks),
        chunk_ms,
    )
    ws = websocket.WebSocket()
    ws.connect(
        realtime_url,
        header=[
            f"Authorization: Bearer {api_key}",
            "OpenAI-Safety-Identifier: voice-input-survey",
        ],
        timeout=30,
    )
    ws.settimeout(1)

    def send_json(payload: dict[str, Any]) -> None:
        logging.debug("realtime_whisper send=%s", payload.get("type"))
        ws.send(json.dumps(payload))

    def receive_loop() -> None:
        nonlocal completed_seconds, event_count, first_delta
        while not stop_receiver.is_set():
            try:
                message = ws.recv()
            except websocket.WebSocketTimeoutException:
                continue
            except Exception as exc:  # noqa: BLE001 - preserve receiver errors in benchmark output.
                if not stop_receiver.is_set():
                    receiver_error.append(str(exc))
                return

            now = time.monotonic()
            try:
                payload = json.loads(message)
            except json.JSONDecodeError:
                payload = {"data": message}
            event_type = payload.get("type", "")
            with event_count_lock:
                event_count += 1
            raw_events.write(
                {
                    "provider": "realtime_whisper",
                    "received_seconds": now - started,
                    "event": event_type,
                    "payload": payload,
                }
            )

            if event_type == "conversation.item.input_audio_transcription.delta":
                delta = extract_delta(payload)
                if delta and first_delta is None:
                    first_delta = now - started
                if delta:
                    partial_parts.append(delta)
                    print(delta, end="", file=sys.stderr, flush=True)
                    logging.debug("realtime_whisper delta=%s", delta)
            elif event_type == "conversation.item.input_audio_transcription.completed":
                transcript = extract_delta(payload)
                if transcript:
                    completed_transcripts.append(transcript)
                    if not partial_parts:
                        print(transcript, end="", file=sys.stderr, flush=True)
                    print("", file=sys.stderr, flush=True)
                completed_seconds = now - started
                if audio_sent_seconds is None:
                    logging.info("realtime_whisper completed at %.3fs before audio_sent marker", completed_seconds)
                else:
                    tail_after_source = None
                    if source_audio_sent_seconds is not None:
                        tail_after_source = completed_seconds - source_audio_sent_seconds
                    logging.info(
                        "realtime_whisper completed at %.3fs tail_after_audio_sent=%.3fs tail_after_source_audio_sent=%s",
                        completed_seconds,
                        completed_seconds - audio_sent_seconds,
                        None if tail_after_source is None else f"{tail_after_source:.3f}s",
                    )
                completed.set()
            elif event_type == "error":
                receiver_error.append(json.dumps(payload, ensure_ascii=False))
                completed.set()
                session_ready.set()
            elif event_type == "session.updated":
                session_ready.set()

    receiver = threading.Thread(target=receive_loop, name="realtime-whisper-receiver", daemon=True)
    receiver.start()

    transcription: dict[str, str] = {"model": model}
    if language:
        transcription["language"] = language

    turn_detection: dict[str, Any] | None = None
    if use_vad:
        turn_detection = {
            "type": "server_vad",
            "threshold": 0.5,
            "prefix_padding_ms": 300,
            "silence_duration_ms": 500,
        }

    try:
        send_json(
            {
                "type": "session.update",
                "session": {
                    "type": "transcription",
                    "audio": {
                        "input": {
                            "format": {"type": "audio/pcm", "rate": REALTIME_SAMPLE_RATE},
                            "transcription": transcription,
                            "turn_detection": turn_detection,
                        }
                    },
                },
            }
        )
        if not session_ready.wait(timeout=10):
            raise TimeoutError("realtime session.update was not acknowledged within 10s")
        if receiver_error:
            raise RuntimeError("; ".join(receiver_error))

        sleep_seconds = chunk_ms / 1000.0 * send_interval_scale
        logging.info(
            "realtime_whisper sending audio send_interval_scale=%.3f estimated_send_seconds=%.3f",
            send_interval_scale,
            len(chunks) * sleep_seconds,
        )
        last_progress_log = time.monotonic()
        for index, chunk in enumerate(chunks, start=1):
            send_json(
                {
                    "type": "input_audio_buffer.append",
                    "audio": base64.b64encode(chunk).decode("ascii"),
                }
            )
            now = time.monotonic()
            if index == len(chunks) or now - last_progress_log >= 1.0:
                logging.info(
                    "realtime_whisper sent chunks=%d/%d elapsed=%.3fs",
                    index,
                    len(chunks),
                    now - started,
                )
                last_progress_log = now
            if sleep_seconds > 0:
                time.sleep(sleep_seconds)

        source_audio_sent_seconds = time.monotonic() - started
        logging.info("realtime_whisper source audio sent at %.3fs", source_audio_sent_seconds)

        audio_sent_seconds = time.monotonic() - started
        logging.info("realtime_whisper audio sent at %.3fs", audio_sent_seconds)
        if not use_vad:
            send_json({"type": "input_audio_buffer.commit"})

        completed.wait(timeout=timeout_after_send_seconds)
    finally:
        stop_receiver.set()
        try:
            ws.close()
        finally:
            receiver.join(timeout=2)

    if receiver_error:
        raise RuntimeError("; ".join(receiver_error))
    if not completed.is_set():
        raise TimeoutError(f"realtime transcription did not complete within {timeout_after_send_seconds}s after sending")

    transcript = "\n".join(completed_transcripts).strip() or "".join(partial_parts).strip()
    metadata = {
        "event_count": event_count,
        "chunks": len(chunks),
        "sample_rate": REALTIME_SAMPLE_RATE,
    }
    return transcript, first_delta, source_audio_sent_seconds or audio_sent_seconds, audio_sent_seconds, metadata


def normalize_for_error_rate(text: str) -> str:
    return " ".join(text.strip().split())


def levenshtein_distance(left: list[str], right: list[str]) -> int:
    previous = list(range(len(right) + 1))
    for left_index, left_value in enumerate(left, start=1):
        current = [left_index]
        for right_index, right_value in enumerate(right, start=1):
            insert = current[right_index - 1] + 1
            delete = previous[right_index] + 1
            replace = previous[right_index - 1] + (left_value != right_value)
            current.append(min(insert, delete, replace))
        previous = current
    return previous[-1]


def error_rates(reference: str | None, hypothesis: str) -> tuple[float | None, float | None]:
    if not reference:
        return None, None

    normalized_reference = normalize_for_error_rate(reference)
    normalized_hypothesis = normalize_for_error_rate(hypothesis)
    reference_words = normalized_reference.split()
    hypothesis_words = normalized_hypothesis.split()

    wer: float | None = None
    if reference_words:
        wer = levenshtein_distance(reference_words, hypothesis_words) / len(reference_words)

    reference_chars = list(normalized_reference.replace(" ", ""))
    hypothesis_chars = list(normalized_hypothesis.replace(" ", ""))
    cer: float | None = None
    if reference_chars:
        cer = levenshtein_distance(reference_chars, hypothesis_chars) / len(reference_chars)
    return wer, cer


def write_outputs(results: list[BenchmarkResult], output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    result_dicts = [asdict(result) for result in results]
    (output_dir / "results.json").write_text(json.dumps(result_dicts, ensure_ascii=False, indent=2), encoding="utf-8")

    if results:
        with (output_dir / "results.csv").open("w", newline="", encoding="utf-8") as csv_file:
            writer = csv.DictWriter(csv_file, fieldnames=list(asdict(results[0]).keys()))
            writer.writeheader()
            writer.writerows(result_dicts)

    summary_lines = ["# Realtime Whisper survey", ""]
    for provider in sorted({result.provider for result in results}):
        provider_results = [result for result in results if result.provider == provider]
        ok = [result for result in provider_results if result.success]
        elapsed = [result.elapsed_seconds for result in ok]
        first_delta = [result.first_delta_seconds for result in ok if result.first_delta_seconds is not None]
        tail = [result.tail_seconds for result in ok if result.tail_seconds is not None]
        summary_lines.append(f"## {provider}")
        summary_lines.append(f"- success: {len(ok)}/{len(provider_results)}")
        if elapsed:
            summary_lines.append(f"- elapsed seconds mean: {sum(elapsed) / len(elapsed):.3f}")
            summary_lines.append(f"- elapsed seconds min/max: {min(elapsed):.3f}/{max(elapsed):.3f}")
        if first_delta:
            summary_lines.append(f"- first delta seconds mean: {sum(first_delta) / len(first_delta):.3f}")
        if tail:
            summary_lines.append(f"- realtime tail seconds mean: {sum(tail) / len(tail):.3f}")
        summary_lines.append("")
    (output_dir / "summary.md").write_text("\n".join(summary_lines), encoding="utf-8")

    transcript_dir = output_dir / "transcripts"
    transcript_dir.mkdir(exist_ok=True)
    for result in results:
        name = f"{result.provider}-rep{result.repetition:02d}.txt"
        (transcript_dir / name).write_text(result.transcript, encoding="utf-8")


def read_reference(args: argparse.Namespace) -> str | None:
    if args.expected_text:
        return args.expected_text
    if args.expected_file:
        return args.expected_file.read_text(encoding="utf-8")
    return None


def run_provider(
    provider: str,
    args: argparse.Namespace,
    audio_duration: float,
    diagnostics: AudioTailDiagnostics,
    api_key: str,
    raw_events: RawEventWriter,
    reference: str | None,
    repetition: int,
) -> BenchmarkResult:
    started = time.monotonic()
    transcript = ""
    first_delta: float | None = None
    source_audio_sent_seconds: float | None = None
    audio_sent_seconds: float | None = None
    model = args.gpt4o_model if provider == "gpt4o_stream" else args.realtime_model
    error = ""
    success = False

    try:
        if provider == "gpt4o_stream":
            transcript, first_delta, metadata = stream_gpt4o_transcribe(
                args.audio,
                args.gpt4o_model,
                args.language,
                api_key,
                raw_events,
                args.http_timeout_seconds,
            )
            logging.info("gpt4o_stream metadata=%s", metadata)
        elif provider == "realtime_whisper":
            transcript, first_delta, source_audio_sent_seconds, audio_sent_seconds, metadata = stream_realtime_whisper(
                args.audio,
                args.realtime_model,
                args.language,
                api_key,
                raw_events,
                args.realtime_url,
                args.chunk_ms,
                args.send_interval_scale,
                args.timeout_after_send_seconds,
                args.realtime_vad,
            )
            logging.info("realtime_whisper metadata=%s", metadata)
        else:
            raise ValueError(f"unknown provider: {provider}")
        success = True
    except Exception as exc:  # noqa: BLE001 - benchmark should record failures and continue.
        error = str(exc)
        logging.exception("%s failed", provider)

    elapsed = time.monotonic() - started
    wer, cer = error_rates(reference, transcript)
    tail_seconds = None
    if audio_sent_seconds is not None:
        tail_seconds = elapsed - audio_sent_seconds
    tail_after_source_audio_sent_seconds = None
    if source_audio_sent_seconds is not None:
        tail_after_source_audio_sent_seconds = elapsed - source_audio_sent_seconds
    real_time_factor = elapsed / audio_duration if audio_duration else None

    return BenchmarkResult(
        provider=provider,
        model=model,
        audio=str(args.audio),
        repetition=repetition,
        success=success,
        elapsed_seconds=elapsed,
        audio_duration_seconds=audio_duration,
        first_delta_seconds=first_delta,
        audio_sent_seconds=audio_sent_seconds,
        source_audio_sent_seconds=source_audio_sent_seconds,
        tail_seconds=tail_seconds,
        tail_after_source_audio_sent_seconds=tail_after_source_audio_sent_seconds,
        real_time_factor=real_time_factor,
        audio_tail_rms_dbfs=diagnostics.tail_rms_dbfs,
        audio_tail_peak_dbfs=diagnostics.tail_peak_dbfs,
        final_silence_ms=diagnostics.final_silence_ms,
        likely_truncated_audio=diagnostics.likely_truncated_audio,
        transcript=transcript,
        wer=wer,
        cer=cer,
        error=error,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audio", type=Path, default=Path("sample.wav"))
    parser.add_argument("--output-dir", type=Path, default=Path("survey/runs"))
    parser.add_argument("--providers", default="gpt4o_stream,realtime_whisper")
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--language", default="ja")
    parser.add_argument("--gpt4o-model", default="gpt-4o-transcribe")
    parser.add_argument("--realtime-model", default="gpt-realtime-whisper")
    parser.add_argument("--realtime-url", default=OPENAI_REALTIME_TRANSCRIPTION_URL)
    parser.add_argument("--chunk-ms", type=int, default=100)
    parser.add_argument("--send-interval-scale", type=float, default=0.0)
    parser.add_argument("--timeout-after-send-seconds", type=int, default=60)
    parser.add_argument("--http-timeout-seconds", type=int, default=300)
    parser.add_argument("--realtime-vad", action="store_true")
    parser.add_argument("--expected-text")
    parser.add_argument("--expected-file", type=Path)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    load_dotenv(Path(".env"))
    api_key = os.environ.get("OPENAI_API_KEY") or os.environ.get("TRANSCRIPTION_API_KEY")
    if not api_key:
        raise SystemExit("OPENAI_API_KEY or TRANSCRIPTION_API_KEY is missing")
    if not args.audio.exists():
        raise SystemExit(f"audio file is missing: {args.audio}")
    if args.repetitions < 1:
        raise SystemExit("--repetitions must be >= 1")
    if args.chunk_ms < 20:
        raise SystemExit("--chunk-ms must be >= 20")

    run_dir = args.output_dir / time.strftime("%Y%m%dT%H%M%S")
    setup_logging(run_dir, args.verbose)
    raw_events = RawEventWriter(run_dir / "raw_events.jsonl")
    reference = read_reference(args)
    audio_duration = audio_duration_seconds(args.audio)
    diagnostics = audio_tail_diagnostics(args.audio)
    providers = [provider.strip() for provider in args.providers.split(",") if provider.strip()]

    manifest = {
        "audio": str(args.audio),
        "audio_duration_seconds": audio_duration,
        "audio_tail": asdict(diagnostics),
        "providers": providers,
        "repetitions": args.repetitions,
        "language": args.language,
        "gpt4o_model": args.gpt4o_model,
        "realtime_model": args.realtime_model,
        "realtime_url": args.realtime_url,
        "chunk_ms": args.chunk_ms,
        "send_interval_scale": args.send_interval_scale,
        "realtime_vad": args.realtime_vad,
        "has_reference": reference is not None,
    }
    (run_dir / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
    logging.info("run started output_dir=%s", run_dir)
    logging.info(
        "audio tail diagnostics tail_rms_dbfs=%s tail_peak_dbfs=%s final_silence_ms=%d likely_truncated_audio=%s",
        None if diagnostics.tail_rms_dbfs is None else f"{diagnostics.tail_rms_dbfs:.1f}",
        None if diagnostics.tail_peak_dbfs is None else f"{diagnostics.tail_peak_dbfs:.1f}",
        diagnostics.final_silence_ms,
        diagnostics.likely_truncated_audio,
    )
    if diagnostics.likely_truncated_audio:
        logging.warning(
            "audio appears to end without enough trailing silence; final words may be cut off in realtime transcription"
        )

    results: list[BenchmarkResult] = []
    try:
        for repetition in range(1, args.repetitions + 1):
            for provider in providers:
                result = run_provider(
                    provider,
                    args,
                    audio_duration,
                    diagnostics,
                    api_key,
                    raw_events,
                    reference,
                    repetition,
                )
                results.append(result)
                logging.info(
                    "result provider=%s success=%s elapsed=%.3fs first_delta=%s transcript_chars=%d",
                    result.provider,
                    result.success,
                    result.elapsed_seconds,
                    None if result.first_delta_seconds is None else f"{result.first_delta_seconds:.3f}s",
                    len(result.transcript),
                )
                print(json.dumps(asdict(result), ensure_ascii=False), flush=True)
                write_outputs(results, run_dir)
    finally:
        raw_events.close()
        write_outputs(results, run_dir)
        logging.info("run finished output_dir=%s", run_dir)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
