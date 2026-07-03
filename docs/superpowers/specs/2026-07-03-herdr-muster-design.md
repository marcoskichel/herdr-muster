# herdr-muster — design

**Date:** 2026-07-03
**Status:** approved

## Summary

`muster` is a plugin for [herdr](https://herdr.dev/) — the terminal agent-multiplexer. It adds a fuzzy project picker (a `sesh`-equivalent, native to herdr).

A keybind opens an overlay pane with a fuzzy finder over the user's project directories. On select:

- if a herdr workspace already exists for that directory → **focus** it,
- otherwise → **create** a workspace rooted at that directory, then focus it.

Name comes from the livestock-roundup verb *muster* — the picker rounds up your projects into workspaces. Fits herdr's herd motif (`herdr` + `muster` = "muster the herd").

## Goals / Non-goals

**Goals**
- One keypress from anywhere in herdr to jump to any project's workspace.
- No duplicate workspaces for the same directory (focus-existing-else-create).
- Zero external picker dependency — native Rust TUI.

**Non-goals (v1)**
- Renaming / deleting / managing existing workspaces (herdr CLI already does this).
- Session (not workspace) switching.
- Fuzzy over arbitrary filesystem — only configured sources.

## Plugin surface (`herdr-plugin.toml`)

```toml
id = "kichel.muster"
name = "Muster"
version = "0.1.0"
min_herdr_version = "0.7.0"
description = "Fuzzy project picker — muster your projects into workspaces"
platforms = ["linux", "macos"]

[[build]]
command = ["cargo", "build", "--release"]

[[panes]]
id = "picker"
placement = "zoomed"
command = ["target/release/herdr-muster"]
```

The picker is an **interactive TUI**, so it runs as a **pane** entrypoint (not a fire-and-forget action). `zoomed` placement gives it the full terminal while open; it closes on select/quit.

### Keybinding (user config, documented in README)

Bind `prefix+m` to open the picker pane. herdr's docs show `plugin_action` keybinds clearly; opening a **pane** entrypoint from a keybind is the one mechanism to confirm against the installed herdr version during implementation. Two forms, in order of preference:

```toml
# Preferred — if herdr supports a direct pane keybind type:
[[keys.command]]
key = "prefix+m"
type = "plugin_pane"          # VERIFY exact type name against herdr version
command = "kichel.muster.picker"
```

```toml
# Safe fallback — a thin action that opens the pane via CLI:
[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "kichel.muster.open"
```

The fallback needs a matching `[[actions]]` in the manifest whose command is
`["herdr", "plugin", "pane", "open", "--plugin", "kichel.muster", "--entrypoint", "picker"]`
(using `$HERDR_BIN_PATH` in practice). First implementation step verifies which form herdr accepts and drops the other.

## Configuration

User-edited file at `$HERDR_PLUGIN_CONFIG_DIR/config.toml`:

```toml
paths      = ["~/dev/api", "~/notes"]   # static entries, always listed
roots      = ["~/dev"]                  # scanned one level deep for git repos
use_zoxide = true                       # merge `zoxide query -l` if binary present
```

All fields optional. Missing file → empty config → picker shows a hint to populate it.
`~` and `$HOME` are expanded.

## Data flow

1. **Load config** (`config.rs`). Expand `~`. Defaults applied for missing fields.
2. **Gather sources** (`sources.rs`):
   - static `paths`,
   - git repos found directly under each `roots` entry (one level, dir contains `.git`),
   - `zoxide query -l` output if `use_zoxide` and the `zoxide` binary is on `PATH`.
   Merge → canonicalize → dedup by absolute path → drop paths that don't exist.
3. **Query live workspaces** (`herdr.rs`): run `$HERDR_BIN_PATH workspace list`, parse JSON, build `cwd → workspace_id` map. Annotate each candidate dir with a `● live` marker when a workspace already exists for it.
4. **Pick** (`picker.rs`): ratatui + nucleo fuzzy matcher. Left = filtered candidate list (live-marker shown); right = **preview pane** for the highlighted entry: `git status -s` (if repo) plus a shallow directory tree.
5. **Act** on `Enter`:
   - dir has workspace → `herdr workspace focus <id>`,
   - else → `herdr workspace create --cwd <dir> --label <basename>`, parse `.id` from JSON, then `herdr workspace focus <new_id>`.
6. **Exit** → pane closes, herdr shows the focused workspace.

`Esc` / `q` / `Ctrl-C` → quit without action.

## Modules

| Module | Responsibility | Depends on | Tested |
|---|---|---|---|
| `config.rs` | Parse `config.toml`, expand `~`, apply defaults | toml, dirs | unit (parse + expand) |
| `sources.rs` | Gather/merge/canonicalize/dedup candidate dirs | std fs, config | unit (pure merge/dedup) |
| `herdr.rs` | Wrap `HERDR_BIN_PATH` CLI: `list`/`focus`/`create`; parse JSON. Trait-fronted for mocking. | serde_json | unit (JSON parse fixtures) |
| `picker.rs` | ratatui + nucleo TUI + preview | ratatui, nucleo | manual |
| `main.rs` | Read env, wire modules, exit codes | all | — |

Design keeps side-effecting CLI calls behind a `herdr.rs` trait so `sources` and selection logic stay pure and unit-testable.

## Runtime environment (from herdr)

Injected when the pane command runs:
- `HERDR_BIN_PATH` — path to herdr binary (used for all CLI calls; portable).
- `HERDR_PLUGIN_CONFIG_DIR` — where `config.toml` lives.
- `HERDR_PLUGIN_STATE_DIR` — available; unused in v1 (no durable state needed).
- `HERDR_SOCKET_PATH` — available; v1 uses CLI, not raw socket.

## Error handling

| Case | Behavior |
|---|---|
| `zoxide` not installed / `use_zoxide=false` | Skip that source silently. |
| `roots` entry missing | Skip, continue. |
| `workspace list` fails / unparsable | Proceed with empty live-map → everything takes the create path. |
| No candidate dirs | Show hint: "no projects — edit `<config path>`". |
| Malformed `config.toml` | Print error to stderr, exit nonzero (herdr logs it). |
| `workspace create` fails | Show error in pane, stay open. |

## Testing

- `sources`: merge + dedup + nonexistent-path filtering (pure, table-driven).
- `herdr`: parse `workspace list` and `workspace create` JSON from fixtures; verify focus/create dispatch via mock trait.
- `config`: `~`/`$HOME` expansion, defaults for missing fields, malformed-file error.
- TUI (`picker`): manual verification.

## Dependencies

`ratatui`, `nucleo`, `serde` + `serde_json`, `toml`, `dirs`. No external `fzf`/`zoxide` *required* (zoxide optional at runtime).

## Open questions

- **Pane-open keybind mechanism** — confirm whether herdr exposes a direct pane keybind type or requires the action-wrapper fallback (see Keybinding). Resolved in first implementation step against the installed herdr version.

All other items resolved during brainstorming (name `muster`, location `~/dev/herdr-muster`, full scope including roots-scan + preview).
