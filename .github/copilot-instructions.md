<!-- crag:auto-start -->
# Copilot Instructions — alacritty

> Generated from governance.md by crag. Regenerate: `crag compile --target copilot`



**Stack:** rust

## Runtimes

rust

## Quality Gates

When you propose changes, the following checks must pass before commit:

- **lint**: `cargo clippy -- -D warnings`
- **lint**: `cargo fmt --check`
- **test**: `cargo test`
- **ci (inferred from workflow)**: `cargo test -p alacritty_terminal --no-default-features`
- **ci (inferred from workflow)**: `cargo clippy --all-targets`
- **ci (inferred from workflow)**: `cargo build --target=x86_64-apple-darwin`
- **ci (inferred from workflow)**: `cargo test --release --target=x86_64-apple-darwin`
- **ci (inferred from workflow)**: `cargo build --release --target=aarch64-apple-darwin`
- **ci (inferred from workflow)**: `make dmg-universal`
- **ci (inferred from workflow)**: `cargo test --release`

## Expectations for AI-Assisted Code

1. **Run gates before suggesting a commit.** If you cannot run them (no shell access), explicitly remind the human to run them.
2. **Respect classifications.** `MANDATORY` gates must pass. `OPTIONAL` gates should pass but may be overridden with a note. `ADVISORY` gates are informational only.
3. **Respect workspace paths.** When a gate is scoped to a subdirectory, run it from that directory.
4. **No hardcoded secrets.** - No hardcoded secrets — grep for sk_live, AKIA, password= before commit
5. Follow project commit conventions.
6. **Conservative changes.** Do not rewrite unrelated files. Do not add new dependencies without explaining why.

## Tool Context

This project uses **crag** (https://www.npmjs.com/package/@whitehatd/crag) as its AI-agent governance layer. The `governance.md` file is the authoritative source. If you have shell access, run `crag check` to verify the infrastructure and `crag diff` to detect drift.

<!-- crag:auto-end -->
