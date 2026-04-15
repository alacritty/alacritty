<!-- crag:auto-start -->
# CLAUDE.md — alacritty

> Generated from governance.md by crag. Regenerate: `crag compile --target claude`



**Stack:** rust
**Runtimes:** rust

## Quality Gates

Run these in order before committing. Stop on first MANDATORY failure:

- `cargo clippy -- -D warnings`
- `cargo fmt --check`
- `cargo test`
- `cargo test -p alacritty_terminal --no-default-features`
- `cargo clippy --all-targets`
- `cargo build --target=x86_64-apple-darwin`
- `cargo test --release --target=x86_64-apple-darwin`
- `cargo build --release --target=aarch64-apple-darwin`
- `make dmg-universal`
- `cargo test --release`

## Rules

1. Read `governance.md` at the start of every session — it is the single source of truth.
2. Run all mandatory quality gates before committing.
3. If a gate fails, attempt an automatic fix (lint/format) with bounded retry (max 2 attempts). If it still fails, escalate to the user.
4. Never modify files outside this repository.
5. Never run destructive system commands (`rm -rf /`, `DROP TABLE`, force-push to main).
- Follow project commit conventions

## Security

- No hardcoded secrets — grep for sk_live, AKIA, password= before commit

## Tool Context

This project uses **crag** (https://www.npmjs.com/package/@whitehatd/crag) as its governance engine. The `governance.md` file is the authoritative source. Run `crag audit` to detect drift and `crag compile --target all` to recompile all targets.

<!-- crag:auto-end -->
