# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Run Commands

- Build: `cargo build`
- Run: `cargo run`
- Release build: `cargo build --release`
- Check: `cargo check`
- Format code: `rustfmt src/*.rs`
- Lint: `cargo clippy`
- Test: `cargo test`
- Test single file: `cargo test --test <test_name>` or `cargo test <test_module::test_name>`

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

## Workflows

- After Edit, MUST run `cargo check` for type checking unless otherwise instructed
