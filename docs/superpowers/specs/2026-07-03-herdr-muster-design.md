# herdr-muster — design

**Date:** 2026-07-03
**Status:** approved (wider scope: agent-aware switcher + identity registry)

## Summary

`muster` is a plugin for [herdr](https://herdr.dev/) — the terminal
agent-multiplexer. A keybind opens an overlay pane holding a fuzzy
**switcher + launcher**:

- **Open** projects (those with a live workspace) appear first, each showing
  its agent state (blocked / working / done / idle) and agent name.
  Blocked floats to the top — the switcher doubles as a herd-status board.
- **Dormant** projects (known dirs with no workspace) appear below; selecting
  one musters a new workspace and enters it.

Identity — "which workspace belongs to project P" — is **assigned at creation**
and stored in a muster-owned registry, so a project always maps to one
workspace even as its panes `cd` around (see ADR 0001).

Name is the livestock-roundup verb *muster*: round up your projects into
workspaces. `herdr` + `muster` = "muster the herd."

## Goals / Non-goals

**Goals**
- One keypress to jump to any project's workspace (focus if open, else create).
- Never two workspaces for one project directory.
- See, in the switcher, which open agent is blocked / working / done / idle.
- Native Rust TUI, no external picker dependency.

**Non-goals (v1)**
- A human "why blocked" message or elapsed timer — herdr exposes neither
  (verified). Meta line is `<agent> · <state>` only.
- Managing workspaces created outside muster (not in the registry).
- Renaming workspaces, multi-select, remote sessions.

## Plugin surface (`herdr-plugin.toml`)

```toml
id = "kichel.muster"
name = "Muster"
version = "0.1.0"
min_herdr_version = "0.7.0"
description = "Agent-aware project switcher — muster your projects into workspaces"
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

The picker is an interactive TUI → a **pane** entrypoint. The `open` action
(bound to a keybind) opens it. On finishing, the binary closes its own pane via
`herdr pane close $HERDR_PANE_ID`.

### Keybinding (user config, documented in README)

```toml
[[keys.command]]
key = "prefix+m"
type = "plugin_action"
command = "kichel.muster.open"
```

## Configuration

User-edited `$HERDR_PLUGIN_CONFIG_DIR/config.toml`:

```toml
paths      = ["~/dev/api", "~/notes"]   # static, always a project
roots      = ["~/dev"]                  # scanned one level for git repos
use_zoxide = true                       # merge `zoxide query -l` if present
```

All optional. Missing file → empty config (`use_zoxide` defaults true). `~`/`$HOME` expanded.

## In-picker keys

| Key | Action |
|---|---|
| type | fuzzy filter (name + path) |
| ↑ / ↓ | move selection |
| Enter | **jump** — focus (open) or create+enter (dormant) |
| Ctrl-N | **force new** — create a fresh workspace for the selected dir even if one is open (rebinds registry to the new one) |
| Ctrl-X | **close** — close the selected open workspace, unbind it, stay in picker |
| Esc / Ctrl-C | cancel |

`●`/glyph column encodes agent state (see below).

## Data flow

1. **Load** config (`config.rs`) and registry (`registry.rs`, `state.json`).
2. **Gather dormant candidates** (`sources.rs`): static `paths` + git repos under
   `roots` + `zoxide query -l` (if enabled/installed). Merge, canonicalize, dedup,
   drop nonexistent.
3. **Query herdr** (`herdr.rs`): `workspace list` → `[{workspace_id, label, agent_status}]`;
   `agent list` → `[{agent, workspace_id, agent_status}]` (best-effort; empty on failure).
4. **Reconcile** registry against live workspace ids (drop dead entries); persist if changed.
5. **Assemble rows** (`model.rs`, pure):
   - each live-and-registered dir → **Open** row `{workspace_id, state, agent name}`
     (state from `workspace list`; agent name joined from `agent list` by `workspace_id`),
   - each dormant candidate not already open → **Dormant** row,
   - sort: open before dormant; open by state rank (blocked < working < done < idle < unknown)
     then name; dormant by name.
6. **Pick** (`picker.rs`): ratatui + nucleo fuzzy over the assembled rows, grouped
   headers, glyph column, meta line `<agent> · <state>` for open rows.
7. **Act** (`main.rs`, loop):
   - Enter on Open → `workspace focus <id>`; on Dormant → `workspace create --cwd D --label <basename> --focus`, bind `D→new_id` in registry.
   - Ctrl-N → create + rebind (overwrite).
   - Ctrl-X → `workspace close <id>`, unbind, re-assemble, re-open picker.
   - Esc → nothing.
8. **Finish**: persist registry, `herdr pane close $HERDR_PANE_ID`.

## Modules

| Module | Responsibility | Tested |
|---|---|---|
| `config.rs` | Parse `config.toml`, expand `~`, defaults | unit |
| `sources.rs` | Gather/canonicalize/dedup dormant candidate dirs | unit (pure) |
| `registry.rs` | `state.json` load/save; `bind`/`unbind`/`workspace_for`/`reconcile(live_ids)` | unit (pure + tmpfile IO) |
| `herdr.rs` | `Herdr` trait + `CliHerdr`; JSON parsers for `workspace list`, `agent list`, `create`; `AgentState` | unit (parse fixtures + mock) |
| `model.rs` | `Row`/`Kind`/`AgentState`, `assemble(...)`, sort | unit (pure) |
| `picker.rs` | ratatui + nucleo grouped switcher TUI; returns `Outcome` | manual |
| `main.rs` | env wiring, act-loop, self-close | manual |

Side-effecting herdr calls stay behind the `Herdr` trait; `assemble`, `sources`,
`registry`, and parsers are pure/mocked and unit-tested. The picker returns an
`Outcome { Cancel | Jump(idx) | ForceNew(idx) | Close(idx) }`; `main` owns all
side effects and the close→refresh loop, keeping the TUI decoupled.

## Runtime environment (from herdr)

- `HERDR_BIN_PATH` — herdr binary for all CLI calls (fallback literal `"herdr"`).
- `HERDR_PLUGIN_CONFIG_DIR` — holds `config.toml`.
- `HERDR_PLUGIN_STATE_DIR` — holds `state.json` (the registry).
- `HERDR_PANE_ID` — the picker's own pane; used to self-close.

### CLI JSON shapes (herdr 0.7.1, verified)

- `workspace list` → `{"result":{"workspaces":[{"workspace_id","label","agent_status"}]}}`
- `agent list` → `{"result":{"agents":[{"agent","agent_status","workspace_id","pane_id","cwd",…}]}}`
  (join key = `workspace_id`; `agent` is the name; **no** message/elapsed fields exist)
- `agent_status` ∈ `idle | working | blocked | done | unknown`
- `workspace create --cwd P --label L --focus` → `{"result":{"workspace":{"workspace_id"},"root_pane":{"cwd"}}}`
- `workspace focus <id>` → `{"result":{"type":"ok"}}`
- `workspace close <id>` → `{"result":{"type":"ok"}}`
- `pane close <id>` → `{"result":{"type":"ok"}}`

## Error handling

| Case | Behavior |
|---|---|
| `zoxide` absent / disabled | skip that source silently |
| `roots` entry missing | skip |
| `workspace list` fails | treat as no live workspaces → all projects dormant (still usable) |
| `agent list` fails | omit agent names; glyph/state still from `workspace list` |
| `state.json` missing/corrupt | start empty registry (worst case: one duplicate workspace) |
| no candidate projects | hint: "no projects — edit `<config path>`" |
| malformed `config.toml` | stderr + nonzero exit (herdr logs it) |
| `workspace create` fails | show error in pane, stay open |

## Testing

- `config`: `~` expansion, defaults, malformed error.
- `sources`: merge/dedup/nonexistent filtering (pure).
- `registry`: bind/unbind/workspace_for, reconcile prunes dead ids, round-trip save/load.
- `herdr`: parse `workspace list` / `agent list` / `create` fixtures; mock-driven trait dispatch.
- `model`: `assemble` open/dormant classification, agent-name join, blocked-first sort.
- `picker`/`main`: manual.

## Dependencies

`ratatui`, `crossterm`, `nucleo-matcher`, `serde`+`serde_json`, `toml`, `dirs`;
dev `tempfile`. No external `fzf`; `zoxide` optional at runtime.

## Open questions

None. All CLI shapes verified against herdr 0.7.1 (including the live `agent list`
schema). Identity mechanism recorded in ADR 0001. Verify-in-code item:
`herdr pane close $HERDR_PANE_ID` tears the picker pane down cleanly (fallback:
process exit).
