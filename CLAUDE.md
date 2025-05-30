# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Architecture

### Core Components

- **voice_input**: CLI client that communicates with daemon via Unix socket
- **voice_inputd**: Background daemon handling audio recording, transcription, and text input
- **Audio Processing**: Memory-only processing (no temporary files)
- **IPC**: JSON communication over Unix Domain Socket (`/tmp/voice_input.sock`)

### Layered Architecture

```
Application Layer   - Business logic and use cases (StackService)
Domain Layer       - Business rules and entities (Stack, StackInfo)
Infrastructure Layer - External dependencies (Audio, OpenAI, UI)
```

- **Data Management**: In-memory only (no persistence by design)
- **Separation of Concerns**: Clear boundaries between layers
- **Testability**: Each layer can be tested independently

### Audio Data Flow

```
[User] → [CLI] → [UDS] → [Daemon] → [Audio Recording] → [Memory Buffer] → [WAV Generation] → [OpenAI API] → [Text Output]
```

## Build and Run Commands

- Build: `cargo build`
- Run: `cargo run`
- Release build: `cargo build --release`
- Check: `cargo check`
- Format code: `cargo fmt`
- Lint: `cargo clippy`
- Test: `cargo test`
- Test single file: `cargo test --test <test_name>` or `cargo test <test_module::test_name>`
- Test with ignored tests (manual): `cargo test -- --ignored --nocapture`
- Performance test: `cargo test --test performance_test -- --ignored --nocapture`

## Code Style Guidelines

- Use Rust 2024 edition
- Follow Rust naming conventions: snake_case for variables/functions, CamelCase for types/structs
- Error handling: Use Result type for functions that can fail, with descriptive error messages
- Comments: Use doc comments (///) for public API and regular comments (//) for implementation details
- Organize imports: std first, then external crates, then local modules
- Prefer Arc/Mutex for shared state in multithreaded contexts
- Use proper error propagation with ? operator
- Use async/await for async operations with tokio
- Keep functions focused and small when possible
- Format code with rustfmt

## Crate

- **MUST NOT USE** anyhow

## Workflows

- After Edit, MUST run `cargo check` for type checking unless otherwise instructed
- Before commit, run `cargo clippy -- -D warnings` to ensure no clippy warnings
- Run `cargo fmt` to format code before commit

## Testing

### Local Development

- Run all tests: `cargo test`
- Run specific test: `cargo test test_name`
- Run tests with output: `cargo test -- --nocapture`
- Run ignored tests: `cargo test -- --ignored`

### CI Environment

- CI uses `cargo test --features ci-test` to skip environment-dependent tests
- Tests requiring audio devices, daemon processes, or GUI are marked with `#[cfg_attr(feature = "ci-test", ignore)]`
- This ensures CI runs only tests that can execute reliably in GitHub Actions environment

### Test Categories

1. **Unit tests**: Always run in CI
2. **Integration tests**: Run if they don't require external resources
3. **E2E tests**: Skipped in CI if they require:
   - Audio input devices
   - Running daemon process
   - GUI/accessibility permissions
   - macOS-specific features

## Version Management

The project uses `rust-toolchain.toml` to ensure consistent Rust version across environments:

- Rust version: 1.86.0
- Includes rustfmt and clippy components
- Both local and CI environments use the same version

## CI/CD Pipeline

GitHub Actions workflow (`.github/workflows/ci.yml`) performs:

1. Code formatting check (`cargo fmt -- --check`)
2. Clippy analysis with all warnings as errors (`cargo clippy -- -D warnings`)
3. Test execution with CI-safe tests only (`cargo test --features ci-test`)
4. Build caching for faster CI runs

### Common CI Issues and Solutions

1. **Clippy version differences**:

   - Use `rust-toolchain.toml` to pin Rust version
   - Ensures local and CI use same clippy rules

2. **Test failures in CI**:

   - Use `ci-test` feature to skip environment-dependent tests
   - Mark flaky tests with `#[cfg_attr(feature = "ci-test", ignore)]`

3. **Build performance**:
   - Use `Swatinem/rust-cache@v2` for dependency caching
   - Share cache across PRs with `shared-key`
