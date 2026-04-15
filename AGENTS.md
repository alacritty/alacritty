<!-- crag:auto-start -->
# AGENTS.md

> Generated from governance.md by crag. Regenerate: `crag compile --target agents-md`

## Project: alacritty


## Quality Gates

All changes must pass these checks before commit:

### Lint
1. `cargo clippy -- -D warnings`
2. `cargo fmt --check`

### Test
1. `cargo test`

### Ci (inferred from workflow)
1. `cargo test -p alacritty_terminal --no-default-features`
2. `cargo clippy --all-targets`
3. `cargo build --target=x86_64-apple-darwin`
4. `cargo test --release --target=x86_64-apple-darwin`
5. `cargo build --release --target=aarch64-apple-darwin`
6. `make dmg-universal`
7. `cargo test --release`

## Coding Standards

- Stack: rust
- Follow project commit conventions

## Architecture

- Type: monorepo (cargo)

## Key Directories

- `.github/` — CI/CD
- `docs/` — documentation
- `scripts/` — tooling

## Testing

- Framework: cargo test
- Layout: flat

## Code Style

- Indent: 4 spaces
- Formatter: rustfmt

## Anti-Patterns

Do not:
- Do not use `unwrap()` in library code — return `Result` instead
- Do not `clone()` without justification — prefer borrowing
- Do not use `unsafe` without a safety comment explaining invariants

## Security

- No hardcoded secrets — grep for sk_live, AKIA, password= before commit

## Workflow

1. Read `governance.md` at the start of every session — it is the single source of truth.
2. Run all mandatory quality gates before committing.
3. If a gate fails, fix the issue and re-run only the failed gate.
4. Use the project commit conventions for all changes.

<!-- crag:auto-end -->
