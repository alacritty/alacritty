# Alacritty AI chat panel

A native AI assistant docked below the terminal. Toggle it with **`Ctrl+Shift+A`**.
It can read the visible screen and recent scrollback, talk to an OpenAI-compatible API,
and run shell commands on your behalf.

This document covers **storing your API key** and the related configuration.

---

## How the key is stored

Your API key is **never** written to `alacritty.toml`, logs, the chat transcript, or the
shell environment. It is stored in your operating system's secret store (keyring) and read
into memory only while a request is in flight:

| Platform | Backend                         |
| -------- | ------------------------------- |
| Linux    | Secret Service (libsecret)      |
| macOS    | Keychain                        |
| Windows  | Credential Manager              |

Each entry is identified by a **service** and **user** name:

- `keyring_service` — default `alacritty-ai`
- `keyring_user` — default derived from the host of `base_url` (e.g. `api.openai.com`)

---

## 1. Enable the panel

Add an `[ai]` section to your config (`~/.config/alacritty/alacritty.toml` on Linux,
`~/Library/Application Support/alacritty/alacritty.toml` on macOS,
`%APPDATA%\alacritty\alacritty.toml` on Windows):

```toml
[ai]
enabled = true
base_url = "https://api.openai.com/v1"   # default
model = "gpt-4o-mini"                     # default
```

The panel is **disabled by default**; it does nothing until `enabled = true`.

## 2. Store your API key

Run the `set-key` subcommand. It prompts for the key and reads it **without echoing** it to
the terminal:

```sh
alacritty ai set-key
# API key: ········
# API key stored in keyring (service: alacritty-ai, user: api.openai.com).
```

You can also pipe it in (useful from a password manager), in which case nothing is echoed:

```sh
pass show openai/api-key | alacritty ai set-key
# or
echo "$OPENAI_API_KEY" | alacritty ai set-key
```

> **Important:** if you set a custom `keyring_service`/`keyring_user`, or a non-default
> `base_url`, pass the **same config** so `set-key` stores the key under the identity the
> running terminal will look up:
>
> ```sh
> alacritty --config-file /path/to/alacritty.toml ai set-key
> ```

## 3. Use it

1. Open a terminal and press `Ctrl+Shift+A`.
2. Type a request (e.g. *"what's taking up disk space in this directory?"*) and press Enter.

If the panel shows **`No API key — run alacritty ai set-key`**, the key for the configured
service/user wasn't found — re-run step 2 with the matching config.

## Remove the key

```sh
alacritty ai delete-key
# API key removed from keyring (service: alacritty-ai, user: api.openai.com).
```

---

## Provider examples

### OpenAI (default)

```toml
[ai]
enabled = true
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
```

```sh
alacritty ai set-key   # paste your sk-... key
```

### Local model via Ollama (OpenAI-compatible endpoint)

Ollama usually needs no key, but the store entry can be any placeholder:

```toml
[ai]
enabled = true
base_url = "http://localhost:11434/v1"
model = "llama3.1"
keyring_user = "ollama"
```

```sh
echo "ollama" | alacritty ai set-key
```

### Any OpenAI-compatible proxy/gateway

```toml
[ai]
enabled = true
base_url = "https://your-gateway.example.com/v1"
model = "your-model"
keyring_user = "work-gateway"   # a stable label of your choosing
```

```sh
alacritty --config-file ~/.config/alacritty/alacritty.toml ai set-key
```

Using `keyring_user` to give each provider a distinct label lets you keep **multiple keys**
in the keyring at once and switch providers by editing `base_url`/`keyring_user`.

---

## Verifying the key is stored (and not in plaintext)

The key should be in the secret store and **not** in any file. On Linux with libsecret:

```sh
secret-tool search service alacritty-ai     # shows the stored entry
grep -ri "sk-" ~/.config/alacritty/          # should find nothing
```

In a running session, confirm the key is not exposed to your shell or to AI-run commands:

```sh
echo "$OPENAI_API_KEY"   # empty — Alacritty never puts the key in the child environment
```

---

## All `[ai]` settings

| Key                        | Default                       | Meaning                                                                 |
| -------------------------- | ----------------------------- | ----------------------------------------------------------------------- |
| `enabled`                  | `false`                       | Turn the panel on.                                                      |
| `base_url`                 | `https://api.openai.com/v1`   | OpenAI-compatible API base URL.                                        |
| `model`                    | `gpt-4o-mini`                 | Model passed to the chat-completions endpoint.                         |
| `keyring_service`          | `alacritty-ai`                | Secret-store service name for the key.                                 |
| `keyring_user`             | host of `base_url`            | Secret-store user/account name for the key.                            |
| `execution_mode`           | `Smart`                       | `TypeOnly`, `Smart`, or `Yolo` (see below).                            |
| `auto_approve`             | `[]`                          | Regex patterns of commands to always auto-run.                         |
| `deny`                     | `[]`                          | Regex patterns of commands to always treat as destructive.            |
| `panel_lines`              | `12`                          | Panel height in terminal rows.                                         |
| `context_scrollback_lines` | `200`                         | Lines of scrollback sent to the model as context.                     |
| `output_idle_ms`           | `400`                         | Idle window used to detect a command has finished before capturing.   |

### Execution modes

- **`TypeOnly`** — inserts the command at the prompt but never runs it; you press Enter.
- **`Smart`** (default) — auto-runs commands judged safe or matching `auto_approve`, and
  **asks for confirmation** before destructive ones (`rm -rf`, `dd`, `mkfs`, `shutdown`,
  `curl … | sh`, etc.).
- **`Yolo`** — runs every command immediately, without confirmation. Use with care.

The destructive-command classifier is a heuristic safety net, not a guarantee. Keep `Smart`
unless you have a specific reason to change it.

### Interactive commands (sudo, prompts)

When a command the agent runs blocks on an interactive prompt — a `sudo` password, a
package manager's `[Y/n]`, an `ssh` host-key confirmation, etc. — the panel detects it,
shows **"⏸ respond in the terminal above"**, and hands keyboard control to you. Type your
answer (e.g. your password) directly in the terminal as usual; when the command finishes,
the agent resumes automatically and continues the task.

Notes and limits:

- Detection is heuristic (it pattern-matches common prompts). Unusual prompts may not be
  detected, in which case the prompt text is simply handed back to the model as output.
- The model is also told to prefer non-interactive flags (`--noconfirm`, `-y`) and to flag
  commands that may need input, so chained prompts (password → `[Y/n]`) are handled.
- For a fully unattended `sudo`, run `sudo -v` first to cache your credentials, or use a
  `NOPASSWD` sudoers rule — but be aware that lets the agent run privileged commands without
  a password.

---

## Troubleshooting

- **`No API key — run alacritty ai set-key`** — no entry for the configured
  `keyring_service`/`keyring_user`. Re-run `set-key` with the same config file.
- **`keyring unavailable: …`** — the OS secret store couldn't be reached. On Linux you need
  a running Secret Service provider (GNOME Keyring, KWallet, or KeePassXC with Secret
  Service enabled). Make sure your keyring is unlocked.
- **Nothing happens on `Ctrl+Shift+A`** — confirm `[ai] enabled = true` and that the
  binding isn't overridden in your config.
