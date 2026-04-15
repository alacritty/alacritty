---
trigger: always_on
description: Governance rules for alacritty — compiled from governance.md by crag
---

# Windsurf Rules — alacritty

Generated from governance.md by crag. Regenerate: `crag compile --target windsurf`

## Project

(No description)

**Stack:** rust

## Runtimes

rust

## Cascade Behavior

When Windsurf's Cascade agent operates on this project:

- **Always read governance.md first.** It is the single source of truth for quality gates and policies.
- **Run all mandatory gates before proposing changes.** Stop on first failure.
- **Respect classifications.** OPTIONAL gates warn but don't block. ADVISORY gates are informational.
- **Respect path scopes.** Gates with a `path:` annotation must run from that directory.
- **No destructive commands.** Never run rm -rf, dd, DROP TABLE, force-push to main, curl|bash, docker system prune.
- - No hardcoded secrets — grep for sk_live, AKIA, password= before commit
- Follow the project commit conventions.

## Quality Gates (run in order)

1. `cargo clippy -- -D warnings`
2. `cargo fmt --check`
3. `cargo test`
4. `cargo test -p alacritty_terminal --no-default-features`
5. `cargo clippy --all-targets`
6. `cargo build --target=x86_64-apple-darwin`
7. `cargo test --release --target=x86_64-apple-darwin`
8. `cargo build --release --target=aarch64-apple-darwin`
9. `make dmg-universal`
10. `cargo test --release`

## Rules of Engagement

1. **Minimal changes.** Don't rewrite files that weren't asked to change.
2. **No new dependencies** without explicit approval.
3. **Prefer editing** existing files over creating new ones.
4. **Always explain** non-obvious changes in commit messages.
5. **Ask before** destructive operations (delete, rename, migrate schema).

---

**Tool:** crag — https://www.npmjs.com/package/@whitehatd/crag
