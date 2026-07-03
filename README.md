# muster

Agent-aware project switcher for [herdr](https://herdr.dev/). One keypress opens
a fuzzy list: **open** projects show their agent state (blocked / working / done
/ idle) with the blocked ones on top; **dormant** projects sit below, one press
from a fresh workspace. A project always maps to one workspace — identity is
stored when muster creates it, not guessed from a pane's directory.

## Install (local dev)

    cargo build --release
    herdr plugin link /home/kichelm/dev/herdr-muster

## Configure

    herdr plugin config-dir kichel.muster   # prints the config dir
    # copy config.toml.example there as config.toml and edit

- `paths`      — directories always listed
- `roots`      — scanned one level deep for git repos
- `use_zoxide` — merge `zoxide query -l` when zoxide is installed

## Keybind

Add to your herdr `config.toml`, then `herdr server reload-config`:

    [[keys.command]]
    key = "prefix+m"
    type = "plugin_action"
    command = "kichel.muster.open"

## Keys (in the switcher)

- type to fuzzy filter · ↑/↓ move
- Enter — jump (focus if open, muster a workspace if dormant)
- Ctrl-N — force a new workspace for the selected dir
- Ctrl-X — close the selected open workspace
- Esc / Ctrl-C — cancel
