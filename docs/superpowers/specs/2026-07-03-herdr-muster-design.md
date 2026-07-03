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

[[actions]]
id = "open"
title = "Muster: pick project"
contexts = ["workspace"]
command = ["herdr", "plugin", "pane", "open", "--plugin", "kichel.muster", "--entrypoint", "picker"]
```

The picker is an **interactive TUI**, so it runs as a **pane** entrypoint. `zoomed` placement gives it the full terminal while open. The `open` action (bound to the keybind) opens that pane. On select/quit the binary closes its own pane via `herdr pane close $HERDR_PANE_ID`.

### Keybinding (user config, documented in README)

Bind `prefix+m` to a plugin **action** that opens the picker pane (herdr exposes no direct pane-keybind type; `plugin_action` is the confirmed mechanism):

```toml
[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "kichel.muster.open"
```

The manifest declares a matching `[[actions]]` `open` whose command opens the pane:
`["herdr", "plugin", "pane", "open", "--plugin", "kichel.muster", "--entrypoint", "picker"]`.
This returns immediately after opening; the pane then runs the picker binary.

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
3. **Query live workspaces** (`herdr.rs`): run `$HERDR_BIN_PATH workspace list` → `result.workspaces[].workspace_id`. `workspace list`/`get` do **not** expose cwd, so for each workspace run `pane list --workspace <id>` and take the first pane's `cwd` (`result.panes[0].cwd` = workspace root dir). Build `cwd → workspace_id` map. Annotate each candidate dir with a `● live` marker when a workspace already exists for it.
4. **Pick** (`picker.rs`): ratatui + nucleo fuzzy matcher. Left = filtered candidate list (live-marker shown); right = **preview pane** for the highlighted entry: `git status -s` (if repo) plus a shallow directory tree.
5. **Act** on `Enter`:
   - dir has workspace → `herdr workspace focus <id>`,
   - else → `herdr workspace create --cwd <dir> --label <dir> --focus`, read `result.workspace.workspace_id` from JSON (already focused).
   Then close own picker pane: `herdr pane close $HERDR_PANE_ID`.
6. **Exit** → picker pane closed, herdr shows the focused workspace.

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
- `HERDR_PANE_ID` — the picker's own pane id; used to self-close after acting.
- `HERDR_PLUGIN_STATE_DIR` — available; unused in v1 (no durable state needed).
- `HERDR_SOCKET_PATH` — available; v1 uses CLI, not raw socket.

CLI JSON shapes (herdr 0.7.1, verified):
- `workspace list` → `{"result":{"workspaces":[{"workspace_id","label",…}]}}`
- `pane list --workspace <id>` → `{"result":{"panes":[{"cwd","pane_id",…}]}}`
- `workspace create … --focus` → `{"result":{"workspace":{"workspace_id"},"root_pane":{"cwd"}}}`
- `workspace focus <id>` → `{"result":{"type":"ok"}}` (or focuses)

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

None. CLI shapes and the pane-open keybind mechanism verified against installed herdr 0.7.1. Remaining verify-in-code item: confirm `herdr pane close $HERDR_PANE_ID` cleanly tears down the picker pane (fallback: process exit).
