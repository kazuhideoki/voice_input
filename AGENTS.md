# Repository Guidelines

## Project Structure & Module Organization
- `src/` holds the Rust library and binaries; `src/bin/` contains the CLI (`voice_input`) and daemon (`voice_inputd`) entrypoints.
- `src/application/`, `src/domain/`, and `src/infrastructure/` follow a layered architecture; shared utilities live in `src/utils/`.
- Integration and E2E tests live in `tests/` (e.g., `tests/e2e/`, `tests/unit/`). Benchmarks are in `benches/`.
- Developer scripts are in `scripts/`, and additional docs are in `README.md`

## Build, Test, and Development Commands
- `cargo build` / `cargo build --release`: debug or optimized builds.
- `cargo test`: runs all tests (may require audio devices/permissions).
- `cargo test --features ci-test`: CI-safe tests only; use for environment-independent runs.
- `cargo fmt -- --check`: formatting check; run before commits.
- `cargo clippy -- -D warnings`: lint with warnings as errors.
- `cargo check`: fast type-check after changes.
- `./scripts/quality-check.sh`: local CI-style checks; add `--bench` or `--memory` as needed.

## Coding Style & Naming Conventions
- Rust formatting is enforced by `rustfmt`; do not hand-format.
- Clippy warnings must be fixed (zero-warnings policy).
- Prefer explicit error types (avoid `anyhow`); use `?` for propagation.
- Single-threaded runtime: prefer `Rc` over `Arc` and `spawn_local` where applicable.
- Add doc comments for public APIs.

## Testing Guidelines
- Tests are written with Rust’s built-in test framework; name files `*_test.rs` and place shared helpers in `tests/common/`.
- Mark device/daemon-dependent tests with `#[cfg_attr(feature = "ci-test", ignore)]`.
- Use `cargo test --features ci-test` for CI parity.
- Test function names must not use `test_` / `_test` prefixes or suffixes (e.g., `test_*`, `*_test`).
- Test function names should clearly describe the verification intent.
- Every test must include a Japanese doc comment that explains the specification.
- Example:
  - `/// データが登録出来る`
  - `enable_data_recording()`

## Commit & Pull Request Guidelines
- Commit messages are short and descriptive (often Japanese), sometimes prefixed with verbs like `fix`, `delete`, or `doc`. Keep them concise and action-oriented.
- PRs should include a brief summary, testing commands run (e.g., `cargo test --features ci-test`), and any relevant logs or screenshots if behavior changes are user-visible.

## Configuration & Environment
- Rust version is pinned in `rust-toolchain.toml` (use the same toolchain locally).
- Configure API keys and device priority via `.env` (see `.env.example`).
