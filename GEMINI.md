<!-- crag:auto-start -->
# GEMINI.md

> Generated from governance.md by crag. Regenerate: `crag compile --target gemini`

## Project Context

- **Name:** alacritty
- **Stack:** rust
- **Runtimes:** rust

## Rules

### Quality Gates

Run these checks in order before committing any changes:

1. [lint] `cargo clippy -- -D warnings`
2. [lint] `cargo fmt --check`
3. [test] `cargo test`
4. [ci (inferred from workflow)] `cargo test -p alacritty_terminal --no-default-features`
5. [ci (inferred from workflow)] `cargo clippy --all-targets`
6. [ci (inferred from workflow)] `cargo build --target=x86_64-apple-darwin`
7. [ci (inferred from workflow)] `cargo test --release --target=x86_64-apple-darwin`
8. [ci (inferred from workflow)] `cargo build --release --target=aarch64-apple-darwin`
9. [ci (inferred from workflow)] `make dmg-universal`
10. [ci (inferred from workflow)] `cargo test --release`

### Security

- No hardcoded secrets — grep for sk_live, AKIA, password= before commit

### Workflow

- Follow project commit conventions
- Run quality gates before committing
- Review security implications of all changes

<!-- crag:auto-end -->
