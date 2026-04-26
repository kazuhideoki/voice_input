#!/usr/bin/env python3
"""Benchmark OpenAI transcription and pyannoteAI STT orchestration."""

from __future__ import annotations

import argparse
import csv
import json
import os
import statistics
import time
import uuid
import wave
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen


OPENAI_TRANSCRIPTIONS_URL = "https://api.openai.com/v1/audio/transcriptions"
PYANNOTE_BASE_URL = "https://api.pyannote.ai/v1"
TERMINAL_STATUSES = {"succeeded", "failed", "canceled"}


@dataclass
class BenchmarkResult:
    provider: str
    model: str
    audio: str
    repetition: int
    success: bool
    elapsed_seconds: float
    text: str
    error: str
    status: str
    job_id: str


class HttpResponseError(RuntimeError):
    def __init__(self, status: int, body: str) -> None:
        super().__init__(f"{status}: {body}")
        self.status = status
        self.body = body


def http_request(
    method: str,
    url: str,
    headers: dict[str, str] | None = None,
    body: bytes | None = None,
    timeout: int = 60,
) -> tuple[int, bytes]:
    request = Request(url, data=body, headers=headers or {}, method=method)
    try:
        with urlopen(request, timeout=timeout) as response:
            return response.status, response.read()
    except HTTPError as error:
        raise HttpResponseError(error.code, error.read().decode("utf-8", errors="replace")) from error


def post_json(url: str, headers: dict[str, str], payload: dict[str, Any], timeout: int = 60) -> tuple[int, dict[str, Any]]:
    status, response_body = http_request(
        "POST",
        url,
        headers=headers,
        body=json.dumps(payload).encode("utf-8"),
        timeout=timeout,
    )
    return status, json.loads(response_body)


def get_json(url: str, headers: dict[str, str], timeout: int = 60) -> dict[str, Any]:
    _, response_body = http_request("GET", url, headers=headers, timeout=timeout)
    return json.loads(response_body)


def multipart_body(fields: dict[str, str], file_field: str, file_path: Path, content_type: str) -> tuple[str, bytes]:
    boundary = f"----voice-input-benchmark-{uuid.uuid4().hex}"
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
            (
                f'Content-Disposition: form-data; name="{file_field}"; '
                f'filename="{file_path.name}"\r\n'
            ).encode(),
            f"Content-Type: {content_type}\r\n\r\n".encode(),
            file_path.read_bytes(),
            b"\r\n",
            f"--{boundary}--\r\n".encode(),
        ]
    )
    return boundary, b"".join(chunks)


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        os.environ.setdefault(key, value)


def audio_duration_seconds(path: Path) -> float:
    with wave.open(str(path), "rb") as wav:
        return wav.getnframes() / float(wav.getframerate())


def transcribe_openai(audio_path: Path, model: str, api_key: str) -> tuple[str, dict[str, Any]]:
    boundary, body = multipart_body(
        {
            "model": model,
            "language": "ja",
            "response_format": "json",
        },
        "file",
        audio_path,
        "audio/wav",
    )
    _, response_body = http_request(
        "POST",
        OPENAI_TRANSCRIPTIONS_URL,
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": f"multipart/form-data; boundary={boundary}",
        },
        body=body,
        timeout=180,
    )
    payload = json.loads(response_body)
    return payload.get("text", ""), payload


def pyannote_headers(api_key: str, content_type: bool = True) -> dict[str, str]:
    headers = {"Authorization": f"Bearer {api_key}"}
    if content_type:
        headers["Content-Type"] = "application/json"
    return headers


def upload_to_pyannote(audio_path: Path, api_key: str, run_id: str) -> str:
    media_url = f"media://asr-benchmark/{run_id}/{audio_path.name}"
    status, payload = post_json(
        f"{PYANNOTE_BASE_URL}/media/input",
        headers=pyannote_headers(api_key),
        payload={"url": media_url},
        timeout=60,
    )
    if status != 201:
        raise RuntimeError(f"pyannote media URL failed: {status}: {payload}")
    upload_url = payload["url"]

    http_request(
        "PUT",
        upload_url,
        headers={"Content-Type": "audio/wav"},
        body=audio_path.read_bytes(),
        timeout=180,
    )
    return media_url


def submit_pyannote_job(media_url: str, api_key: str, model: str) -> str:
    _, payload = post_json(
        f"{PYANNOTE_BASE_URL}/diarize",
        headers=pyannote_headers(api_key),
        payload={
            "url": media_url,
            "model": "precision-2",
            "transcription": True,
            "transcriptionConfig": {"model": model},
        },
        timeout=60,
    )
    return payload["jobId"]


def poll_pyannote_job(job_id: str, api_key: str, poll_seconds: int, timeout_seconds: int) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    while True:
        payload = get_json(
            f"{PYANNOTE_BASE_URL}/jobs/{job_id}",
            headers=pyannote_headers(api_key, content_type=False),
            timeout=60,
        )
        status = payload.get("status", "")
        if status in TERMINAL_STATUSES:
            return payload
        if time.monotonic() >= deadline:
            raise TimeoutError(f"pyannote job timed out: {job_id} status={status}")
        time.sleep(poll_seconds)


def pyannote_text(output: dict[str, Any]) -> str:
    turns = output.get("turnLevelTranscription") or []
    if turns:
        return "\n".join(turn.get("text", "").strip() for turn in turns if turn.get("text"))
    words = output.get("wordLevelTranscription") or []
    return "".join(word.get("text", "") for word in words).strip()


def transcribe_pyannote(
    media_url: str,
    model: str,
    api_key: str,
    poll_seconds: int,
    timeout_seconds: int,
) -> tuple[str, dict[str, Any], str, str]:
    job_id = submit_pyannote_job(media_url, api_key, model)
    payload = poll_pyannote_job(job_id, api_key, poll_seconds, timeout_seconds)
    status = payload.get("status", "")
    if status != "succeeded":
        raise RuntimeError(f"pyannote job {job_id} ended with {status}: {json.dumps(payload, ensure_ascii=False)}")
    output = payload.get("output") or {}
    return pyannote_text(output), payload, job_id, status


def write_outputs(results: list[BenchmarkResult], raw_payloads: list[dict[str, Any]], output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "results.json").write_text(
        json.dumps([asdict(result) for result in results], ensure_ascii=False, indent=2)
    )
    (output_dir / "raw_payloads.jsonl").write_text(
        "\n".join(json.dumps(payload, ensure_ascii=False) for payload in raw_payloads) + "\n"
    )

    with (output_dir / "results.csv").open("w", newline="") as csv_file:
        writer = csv.DictWriter(csv_file, fieldnames=list(asdict(results[0]).keys()))
        writer.writeheader()
        for result in results:
            writer.writerow(asdict(result))

    summary_lines = ["# ASR benchmark summary", ""]
    for provider in sorted({result.provider for result in results}):
        provider_results = [result for result in results if result.provider == provider]
        ok = [result for result in provider_results if result.success]
        elapsed = [result.elapsed_seconds for result in ok]
        summary_lines.append(f"## {provider}")
        summary_lines.append(f"- success: {len(ok)}/{len(provider_results)}")
        if elapsed:
            summary_lines.append(f"- mean seconds: {statistics.mean(elapsed):.3f}")
            summary_lines.append(f"- median seconds: {statistics.median(elapsed):.3f}")
            summary_lines.append(f"- min seconds: {min(elapsed):.3f}")
            summary_lines.append(f"- max seconds: {max(elapsed):.3f}")
        summary_lines.append("")
    (output_dir / "summary.md").write_text("\n".join(summary_lines))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--audio-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=Path(".tmp/asr-benchmark"))
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--openai-model", default="gpt-4o-transcribe")
    parser.add_argument("--pyannote-model", default="parakeet-tdt-0.6b-v3")
    parser.add_argument("--pyannote-api-key", default="sk_a28eae16b86d40c0a5d09ada4419b30d")
    parser.add_argument("--poll-seconds", type=int, default=10)
    parser.add_argument("--pyannote-timeout-seconds", type=int, default=900)
    parser.add_argument("--providers", default="openai,pyannote")
    parser.add_argument("--audio-start", type=int, default=1)
    parser.add_argument("--audio-end", type=int, default=6)
    args = parser.parse_args()

    load_dotenv(Path(".env"))
    openai_api_key = os.environ.get("OPENAI_API_KEY")
    if not openai_api_key:
        raise SystemExit("OPENAI_API_KEY is missing")

    audio_paths = [args.audio_dir / f"capture-{index:06}.wav" for index in range(args.audio_start, args.audio_end + 1)]
    missing = [path for path in audio_paths if not path.exists()]
    if missing:
        raise SystemExit(f"missing audio files: {missing}")

    run_id = uuid.uuid4().hex
    output_dir = args.output_dir / time.strftime("%Y%m%dT%H%M%S")
    manifest = {
        "run_id": run_id,
        "audio_files": [
            {"path": str(path), "bytes": path.stat().st_size, "duration_seconds": audio_duration_seconds(path)}
            for path in audio_paths
        ],
        "repetitions": args.repetitions,
        "openai_model": args.openai_model,
        "pyannote_model": args.pyannote_model,
    }
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2))

    results: list[BenchmarkResult] = []
    raw_payloads: list[dict[str, Any]] = []

    providers = [provider.strip() for provider in args.providers.split(",") if provider.strip()]
    pyannote_media_urls = {}
    if "pyannote" in providers:
        pyannote_media_urls = {
            path: upload_to_pyannote(path, args.pyannote_api_key, run_id) for path in audio_paths
        }

    for provider in providers:
        for audio_path in audio_paths:
            for repetition in range(1, args.repetitions + 1):
                started = time.monotonic()
                text = ""
                error = ""
                status = ""
                job_id = ""
                raw: dict[str, Any] = {}
                try:
                    if provider == "openai":
                        text, raw = transcribe_openai(audio_path, args.openai_model, openai_api_key)
                        model = args.openai_model
                        status = "succeeded"
                    else:
                        text, raw, job_id, status = transcribe_pyannote(
                            pyannote_media_urls[audio_path],
                            args.pyannote_model,
                            args.pyannote_api_key,
                            args.poll_seconds,
                            args.pyannote_timeout_seconds,
                        )
                        model = args.pyannote_model
                    success = True
                except Exception as exc:  # noqa: BLE001 - benchmark should keep collecting failures.
                    model = args.openai_model if provider == "openai" else args.pyannote_model
                    success = False
                    error = str(exc)
                elapsed = time.monotonic() - started
                result = BenchmarkResult(
                    provider=provider,
                    model=model,
                    audio=audio_path.name,
                    repetition=repetition,
                    success=success,
                    elapsed_seconds=elapsed,
                    text=text,
                    error=error,
                    status=status,
                    job_id=job_id,
                )
                print(json.dumps(asdict(result), ensure_ascii=False), flush=True)
                results.append(result)
                raw_payloads.append(
                    {
                        "provider": provider,
                        "audio": audio_path.name,
                        "repetition": repetition,
                        "elapsed_seconds": elapsed,
                        "payload": raw,
                    }
                )
                write_outputs(results, raw_payloads, output_dir)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
