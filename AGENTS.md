# AGENTS.md

This file provides development guidelines and best practices for AI agents and developers working on this project.

## Development Philosophy

### 1. Test-Driven Development
- Write tests before implementing features when possible
- Ensure all tests pass locally before pushing
- Use `cargo test` for full test suite, `cargo test --features ci-test` for CI-safe tests

### 2. Code Quality Standards
- **Zero Warnings Policy**: All clippy warnings must be fixed
- **Format Consistency**: Run `cargo fmt` before every commit
- **Type Safety**: Run `cargo check` after code changes
- **Documentation**: Add doc comments for public APIs

### 3. CI/CD Best Practices

#### Environment Parity
- Use `rust-toolchain.toml` to ensure version consistency
- Local development should mirror CI environment
- Test with same flags as CI: `--features ci-test`

#### Test Strategy
```bash
# Local: Run all tests including those requiring hardware/permissions
cargo test

# CI: Run only environment-independent tests
cargo test --features ci-test
```

#### Common Issues Resolution

1. **"Works on my machine" syndrome**:
   - Always verify with `rust-toolchain.toml` version
   - Run clippy with `-D warnings` flag
   - Test with CI feature flags

2. **Audio device tests**:
   - Mark with `#[cfg_attr(feature = "ci-test", ignore)]`
   - Provide mock implementations where possible

3. **Daemon process tests**:
   - Use `#[cfg_attr(feature = "ci-test", ignore)]`
   - Consider unit testing individual components

### 4. Architecture Principles

#### Single-threaded Tokio Runtime (voice_inputd)
- Use `Rc` instead of `Arc` for single-threaded contexts
- Leverage `spawn_local` for local tasks
- Avoid unnecessary synchronization overhead

#### Error Handling
- Use custom error types, avoid `anyhow`
- Provide descriptive error messages
- Proper error propagation with `?` operator

#### Performance Considerations
- Direct input is default (85% faster than clipboard)
- Minimize clipboard operations
- Use efficient data structures

### 5. Feature Development Workflow

1. **Planning**:
   - Document feature in issue/PR
   - Consider CI limitations early
   - Plan test strategy

2. **Implementation**:
   - Follow existing code patterns
   - Add appropriate tests
   - Document public APIs

3. **Testing**:
   ```bash
   # Full local test
   cargo test
   
   # CI simulation
   cargo test --features ci-test
   
   # Clippy check
   cargo clippy -- -D warnings
   
   # Format check
   cargo fmt -- --check
   ```

4. **Pre-commit Checklist**:
   - [ ] All tests pass locally
   - [ ] CI tests pass with `--features ci-test`
   - [ ] No clippy warnings
   - [ ] Code is formatted
   - [ ] Documentation updated

### 6. Debugging CI Failures

1. **Version mismatch**:
   - Check `rust-toolchain.toml`
   - Update local Rust: `rustup update`

2. **Test failures**:
   - Run with CI flags: `cargo test --features ci-test`
   - Check test output in Actions logs

3. **Clippy failures**:
   - Run exact CI command: `cargo clippy --all-targets --features ci-test -- -D warnings`
   - Fix all warnings, even minor ones

### 7. Contributing Guidelines

- Keep PRs focused and small
- Write descriptive commit messages
- Update tests for bug fixes
- Add tests for new features
- Document breaking changes

## Quick Reference

```bash
# Development commands
cargo build                    # Debug build
cargo build --release          # Release build
cargo test                     # All tests
cargo test --features ci-test  # CI-safe tests
cargo clippy -- -D warnings    # Lint with warnings as errors
cargo fmt                      # Format code

# CI simulation
./scripts/ci-local.sh          # Run full CI pipeline locally (if available)
```

## Related Documentation

- [README.md](./README.md) - User documentation and setup
- [CLAUDE.md](./CLAUDE.md) - AI agent specific guidelines
- [dev-docs/](./dev-docs/) - Detailed development documentation