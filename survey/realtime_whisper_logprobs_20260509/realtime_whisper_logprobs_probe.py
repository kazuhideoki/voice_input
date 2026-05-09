#!/usr/bin/env python3
"""Probe whether Realtime Whisper returns transcription logprobs.

This script intentionally uses only Python's standard library so the survey can
be rerun without installing websocket dependencies.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import math
import os
import secrets
import socket
import ssl
import struct
import time
import wave
from dataclasses import asdict, dataclass
from pathlib import Path
from select import select
from typing import Any
from urllib.parse import urlparse


OPENAI_REALTIME_TRANSCRIPTION_URL = "wss://api.openai.com/v1/realtime?intent=transcription"
REALTIME_SAMPLE_RATE = 24_000
DEFAULT_OUTPUT_DIR = Path(__file__).resolve().parent / "runs"


@dataclass
class ProbeResult:
    audio: str
    model: str
    payload_variant: str
    include_logprobs: bool
    success: bool
    elapsed_seconds: float
    transcript: str
    completed_event_has_logprobs: bool
    delta_events_with_logprobs: int
    completed_logprobs_count: int
    first_logprobs_event: str | None
    min_logprob: float | None
    max_logprob: float | None
    avg_logprob: float | None
    sample_tokens: list[dict[str, Any]]
    event_counts: dict[str, int]
    error: str


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip("\"").strip("'"))


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
    if channels == 1:
        return samples, source_rate

    mono = []
    for index in range(0, len(samples), channels):
        mono.append(round(sum(samples[index : index + channels]) / channels))
    return tuple(mono), source_rate


def resample_pcm16(samples: tuple[int, ...], source_rate: int, target_rate: int) -> tuple[int, ...]:
    if source_rate == target_rate:
        return samples
    if not samples:
        return ()

    output_length = max(1, round(len(samples) * target_rate / source_rate))
    output = []
    for index in range(output_length):
        source_position = index * source_rate / target_rate
        left = int(math.floor(source_position))
        right = min(left + 1, len(samples) - 1)
        fraction = source_position - left
        value = samples[left] * (1.0 - fraction) + samples[right] * fraction
        output.append(max(-32768, min(32767, round(value))))
    return tuple(output)


def read_wav_as_pcm16_mono(path: Path, target_rate: int) -> bytes:
    samples, source_rate = wav_samples_mono(path)
    resampled = resample_pcm16(samples, source_rate, target_rate)
    return struct.pack(f"<{len(resampled)}h", *resampled)


def chunk_pcm16(pcm: bytes, sample_rate: int, chunk_ms: int) -> list[bytes]:
    bytes_per_chunk = max(2, int(sample_rate * chunk_ms / 1000) * 2)
    bytes_per_chunk -= bytes_per_chunk % 2
    return [pcm[index : index + bytes_per_chunk] for index in range(0, len(pcm), bytes_per_chunk)]


class MinimalWebSocket:
    def __init__(self, url: str, headers: dict[str, str], timeout: float = 30.0) -> None:
        parsed = urlparse(url)
        if parsed.scheme != "wss":
            raise ValueError("only wss:// URLs are supported")
        self.host = parsed.hostname or ""
        self.port = parsed.port or 443
        self.path = parsed.path or "/"
        if parsed.query:
            self.path += f"?{parsed.query}"
        self.timeout = timeout
        self.sock = self._connect(headers)

    def _connect(self, headers: dict[str, str]) -> ssl.SSLSocket:
        raw_sock = socket.create_connection((self.host, self.port), timeout=self.timeout)
        context = ssl.create_default_context()
        sock = context.wrap_socket(raw_sock, server_hostname=self.host)
        sock.settimeout(self.timeout)

        ws_key = base64.b64encode(secrets.token_bytes(16)).decode("ascii")
        request_lines = [
            f"GET {self.path} HTTP/1.1",
            f"Host: {self.host}",
            "Upgrade: websocket",
            "Connection: Upgrade",
            f"Sec-WebSocket-Key: {ws_key}",
            "Sec-WebSocket-Version: 13",
        ]
        request_lines.extend(f"{key}: {value}" for key, value in headers.items())
        request = "\r\n".join(request_lines) + "\r\n\r\n"
        sock.sendall(request.encode("utf-8"))

        response = bytearray()
        while b"\r\n\r\n" not in response:
            chunk = sock.recv(4096)
            if not chunk:
                break
            response.extend(chunk)

        header_text = bytes(response).split(b"\r\n\r\n", 1)[0].decode("iso-8859-1", errors="replace")
        if " 101 " not in header_text.splitlines()[0]:
            raise RuntimeError(f"websocket handshake failed: {header_text}")

        accept_source = (ws_key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode("ascii")
        expected_accept = base64.b64encode(hashlib.sha1(accept_source).digest()).decode("ascii")
        if f"Sec-WebSocket-Accept: {expected_accept}".lower() not in header_text.lower():
            raise RuntimeError("websocket handshake did not include the expected accept key")

        return sock

    def send_json(self, payload: dict[str, Any]) -> None:
        self._send_frame(json.dumps(payload, ensure_ascii=False).encode("utf-8"), opcode=0x1)

    def close(self) -> None:
        try:
            self._send_frame(b"", opcode=0x8)
        except OSError:
            pass
        self.sock.close()

    def receive_json(self, timeout: float) -> dict[str, Any] | None:
        readable, _, _ = select([self.sock], [], [], timeout)
        if not readable:
            return None

        message = self._receive_frame()
        if message is None:
            return None
        opcode, payload = message
        if opcode == 0x8:
            raise RuntimeError("websocket closed by server")
        if opcode == 0x9:
            self._send_frame(payload, opcode=0xA)
            return None
        if opcode != 0x1:
            return None
        return json.loads(payload.decode("utf-8"))

    def _send_frame(self, payload: bytes, opcode: int) -> None:
        first_byte = 0x80 | opcode
        length = len(payload)
        header = bytearray([first_byte])
        mask_bit = 0x80
        if length < 126:
            header.append(mask_bit | length)
        elif length < 65536:
            header.append(mask_bit | 126)
            header.extend(struct.pack("!H", length))
        else:
            header.append(mask_bit | 127)
            header.extend(struct.pack("!Q", length))

        mask = secrets.token_bytes(4)
        masked = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        self.sock.sendall(bytes(header) + mask + masked)

    def _receive_frame(self) -> tuple[int, bytes] | None:
        header = self._recv_exact(2)
        if not header:
            return None
        first, second = header
        opcode = first & 0x0F
        length = second & 0x7F
        if length == 126:
            length = struct.unpack("!H", self._recv_exact(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self._recv_exact(8))[0]

        masked = bool(second & 0x80)
        mask = self._recv_exact(4) if masked else b""
        payload = self._recv_exact(length)
        if masked:
            payload = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        return opcode, payload

    def _recv_exact(self, length: int) -> bytes:
        data = bytearray()
        while len(data) < length:
            chunk = self.sock.recv(length - len(data))
            if not chunk:
                raise RuntimeError("socket closed while reading websocket frame")
            data.extend(chunk)
        return bytes(data)


def build_session_update(
    model: str,
    language: str,
    include_logprobs: bool,
    payload_variant: str,
) -> dict[str, Any]:
    transcription: dict[str, Any] = {"model": model}
    if language:
        transcription["language"] = language

    if payload_variant == "transcription_logprobs_true":
        transcription["logprobs"] = True

    session: dict[str, Any] = {
        "type": "transcription",
        "audio": {
            "input": {
                "format": {"type": "audio/pcm", "rate": REALTIME_SAMPLE_RATE},
                "transcription": transcription,
                "turn_detection": None,
            }
        },
    }
    if include_logprobs:
        session["include"] = ["item.input_audio_transcription.logprobs"]

    if payload_variant in ("current", "transcription_logprobs_true"):
        return {"type": "session.update", "session": session}

    if payload_variant == "legacy_realtime_fields":
        legacy_session: dict[str, Any] = {
            "type": "transcription",
            "input_audio_format": "pcm16",
            "input_audio_transcription": transcription,
            "turn_detection": None,
        }
        if include_logprobs:
            legacy_session["include"] = ["item.input_audio_transcription.logprobs"]
        return {"type": "session.update", "session": legacy_session}

    if payload_variant == "include_on_event":
        payload = {"type": "session.update", "session": session}
        session.pop("include", None)
        if include_logprobs:
            payload["include"] = ["item.input_audio_transcription.logprobs"]
        return payload

    if payload_variant == "include_under_input":
        session.pop("include", None)
        if include_logprobs:
            session["audio"]["input"]["include"] = ["item.input_audio_transcription.logprobs"]
        return {"type": "session.update", "session": session}

    raise ValueError(f"unknown payload variant: {payload_variant}")


def collect_event(
    payload: dict[str, Any],
    raw_events_path: Path,
    started: float,
    event_counts: dict[str, int],
) -> None:
    event_type = str(payload.get("type", ""))
    event_counts[event_type] = event_counts.get(event_type, 0) + 1
    with raw_events_path.open("a", encoding="utf-8") as raw_events:
        raw_events.write(
            json.dumps(
                {
                    "received_seconds": time.monotonic() - started,
                    "event": event_type,
                    "payload": payload,
                },
                ensure_ascii=False,
            )
            + "\n"
        )


def run_probe(
    api_key: str,
    audio_path: Path,
    output_dir: Path,
    model: str,
    language: str,
    include_logprobs: bool,
    chunk_ms: int,
    send_interval_scale: float,
    timeout_after_commit: float,
    realtime_url: str,
    payload_variant: str,
) -> ProbeResult:
    started = time.monotonic()
    raw_events_path = (
        output_dir / f"raw_events_{audio_path.stem}_{payload_variant}_{'include' if include_logprobs else 'no_include'}.jsonl"
    )
    raw_events_path.write_text("", encoding="utf-8")
    event_counts: dict[str, int] = {}

    try:
        ws = MinimalWebSocket(
            realtime_url,
            {
                "Authorization": f"Bearer {api_key}",
                "OpenAI-Safety-Identifier": "voice-input-logprobs-survey",
            },
        )
        try:
            ws.send_json(build_session_update(model, language, include_logprobs, payload_variant))

            session_ready = False
            session_deadline = time.monotonic() + 10.0
            while time.monotonic() < session_deadline:
                payload = ws.receive_json(timeout=1.0)
                if payload is None:
                    continue
                collect_event(payload, raw_events_path, started, event_counts)
                event_type = payload.get("type")
                if event_type == "session.updated":
                    session_ready = True
                    break
                if event_type == "error":
                    raise RuntimeError(json.dumps(payload, ensure_ascii=False))
            if not session_ready:
                raise TimeoutError("session.updated was not received within 10s")

            pcm = read_wav_as_pcm16_mono(audio_path, REALTIME_SAMPLE_RATE)
            chunks = chunk_pcm16(pcm, REALTIME_SAMPLE_RATE, chunk_ms)
            sleep_seconds = chunk_ms / 1000.0 * send_interval_scale
            for chunk in chunks:
                ws.send_json(
                    {
                        "type": "input_audio_buffer.append",
                        "audio": base64.b64encode(chunk).decode("ascii"),
                    }
                )
                drain_available_events(ws, raw_events_path, started, event_counts)
                if sleep_seconds > 0:
                    time.sleep(sleep_seconds)

            ws.send_json({"type": "input_audio_buffer.commit"})

            completed_payload: dict[str, Any] | None = None
            deadline = time.monotonic() + timeout_after_commit
            while time.monotonic() < deadline:
                payload = ws.receive_json(timeout=1.0)
                if payload is None:
                    continue
                collect_event(payload, raw_events_path, started, event_counts)
                event_type = payload.get("type")
                if event_type == "error":
                    raise RuntimeError(json.dumps(payload, ensure_ascii=False))
                if event_type == "conversation.item.input_audio_transcription.completed":
                    completed_payload = payload
                    break
            if completed_payload is None:
                raise TimeoutError("completed event was not received after commit")

            all_logprobs = extract_logprobs(raw_events_path)
            completed_logprobs = completed_payload.get("logprobs")
            completed_has_logprobs = isinstance(completed_logprobs, list) and len(completed_logprobs) > 0
            delta_events_with_logprobs = count_delta_events_with_logprobs(raw_events_path)
            values = [float(item["logprob"]) for item in all_logprobs if isinstance(item.get("logprob"), int | float)]
            transcript = str(completed_payload.get("transcript") or "")
            return ProbeResult(
                audio=str(audio_path),
                model=model,
                payload_variant=payload_variant,
                include_logprobs=include_logprobs,
                success=True,
                elapsed_seconds=time.monotonic() - started,
                transcript=transcript,
                completed_event_has_logprobs=completed_has_logprobs,
                delta_events_with_logprobs=delta_events_with_logprobs,
                completed_logprobs_count=len(completed_logprobs) if isinstance(completed_logprobs, list) else 0,
                first_logprobs_event=first_logprobs_event(raw_events_path),
                min_logprob=min(values) if values else None,
                max_logprob=max(values) if values else None,
                avg_logprob=sum(values) / len(values) if values else None,
                sample_tokens=all_logprobs[:12],
                event_counts=event_counts,
                error="",
            )
        finally:
            ws.close()
    except Exception as exc:  # noqa: BLE001 - survey output should preserve API errors.
        return ProbeResult(
            audio=str(audio_path),
            model=model,
            payload_variant=payload_variant,
            include_logprobs=include_logprobs,
            success=False,
            elapsed_seconds=time.monotonic() - started,
            transcript="",
            completed_event_has_logprobs=False,
            delta_events_with_logprobs=0,
            completed_logprobs_count=0,
            first_logprobs_event=None,
            min_logprob=None,
            max_logprob=None,
            avg_logprob=None,
            sample_tokens=[],
            event_counts=event_counts,
            error=str(exc),
        )


def drain_available_events(
    ws: MinimalWebSocket,
    raw_events_path: Path,
    started: float,
    event_counts: dict[str, int],
) -> None:
    while True:
        payload = ws.receive_json(timeout=0.0)
        if payload is None:
            return
        collect_event(payload, raw_events_path, started, event_counts)


def extract_logprobs(raw_events_path: Path) -> list[dict[str, Any]]:
    logprobs: list[dict[str, Any]] = []
    for line in raw_events_path.read_text(encoding="utf-8").splitlines():
        record = json.loads(line)
        payload = record.get("payload", {})
        items = payload.get("logprobs")
        if isinstance(items, list):
            for item in items:
                if isinstance(item, dict):
                    logprobs.append(item)
    return logprobs


def first_logprobs_event(raw_events_path: Path) -> str | None:
    for line in raw_events_path.read_text(encoding="utf-8").splitlines():
        record = json.loads(line)
        payload = record.get("payload", {})
        if isinstance(payload.get("logprobs"), list) and payload["logprobs"]:
            return str(payload.get("type", ""))
    return None


def count_delta_events_with_logprobs(raw_events_path: Path) -> int:
    count = 0
    for line in raw_events_path.read_text(encoding="utf-8").splitlines():
        record = json.loads(line)
        payload = record.get("payload", {})
        if (
            payload.get("type") == "conversation.item.input_audio_transcription.delta"
            and isinstance(payload.get("logprobs"), list)
            and payload["logprobs"]
        ):
            count += 1
    return count


def write_outputs(results: list[ProbeResult], output_dir: Path) -> None:
    result_dicts = [asdict(result) for result in results]
    (output_dir / "results.json").write_text(json.dumps(result_dicts, ensure_ascii=False, indent=2), encoding="utf-8")

    lines = [
        "# Realtime Whisper logprobs probe",
        "",
        "| audio | include | success | completed logprobs | delta events with logprobs | avg logprob | transcript | error |",
        "|---|---:|---:|---:|---:|---:|---|---|",
    ]
    for result in results:
        avg = "" if result.avg_logprob is None else f"{result.avg_logprob:.4f}"
        transcript = result.transcript.replace("|", "\\|")
        error = result.error.replace("|", "\\|")
        lines.append(
            f"| `{Path(result.audio).name}` / `{result.payload_variant}` | {result.include_logprobs} | {result.success} | "
            f"{result.completed_logprobs_count} | {result.delta_events_with_logprobs} | {avg} | "
            f"{transcript} | {error} |"
        )
    (output_dir / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--audio", action="append", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--model", default="gpt-realtime-whisper")
    parser.add_argument("--language", default="ja")
    parser.add_argument("--chunk-ms", type=int, default=100)
    parser.add_argument("--send-interval-scale", type=float, default=0.0)
    parser.add_argument("--timeout-after-commit", type=float, default=45.0)
    parser.add_argument("--realtime-url", default=OPENAI_REALTIME_TRANSCRIPTION_URL)
    parser.add_argument("--skip-no-include", action="store_true")
    parser.add_argument(
        "--payload-variant",
        choices=[
            "current",
            "legacy_realtime_fields",
            "transcription_logprobs_true",
            "include_on_event",
            "include_under_input",
        ],
        default="current",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[2]
    load_dotenv(repo_root / ".env")

    api_key = os.environ.get("OPENAI_API_KEY") or os.environ.get("TRANSCRIPTION_API_KEY")
    if not api_key:
        raise SystemExit("OPENAI_API_KEY or TRANSCRIPTION_API_KEY is missing")

    audios = args.audio or [repo_root / "samples" / "1.wav", repo_root / "samples" / "2.wav"]
    run_dir = args.output_dir / time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir.mkdir(parents=True, exist_ok=True)

    results: list[ProbeResult] = []
    for audio_path in audios:
        include_modes = [True] if args.skip_no_include else [True, False]
        for include_logprobs in include_modes:
            result = run_probe(
                api_key=api_key,
                audio_path=audio_path,
                output_dir=run_dir,
                model=args.model,
                language=args.language,
                include_logprobs=include_logprobs,
                chunk_ms=args.chunk_ms,
                send_interval_scale=args.send_interval_scale,
                timeout_after_commit=args.timeout_after_commit,
                realtime_url=args.realtime_url,
                payload_variant=args.payload_variant,
            )
            results.append(result)
            print(
                f"{audio_path} include_logprobs={include_logprobs} "
                f"success={result.success} completed_logprobs={result.completed_logprobs_count} "
                f"error={result.error}",
                flush=True,
            )

    write_outputs(results, run_dir)
    print(run_dir)


if __name__ == "__main__":
    main()
