# 0001 — Workspace identity via a muster-owned registry

**Status:** accepted
**Date:** 2026-07-03

## Context

muster's core promise is "one project, one workspace": selecting a project
focuses its existing workspace or creates one — it must never open a second
workspace for a directory that already has one. This requires a stable answer
to *"which workspace belongs to project P?"*

Herdr does not store a directory on a workspace. A workspace owns panes; only
**panes** carry `cwd` (launch dir) and `foreground_cwd` (live shell dir).
Neither `workspace list` nor `workspace get` returns any directory
(verified, herdr 0.7.1).

The obvious approach — infer a workspace's project from its panes' cwds — is
unstable: a pane can `cd` elsewhere, and split panes can be launched in
unrelated directories. Under inference, opening a shell in directory B inside
project A's workspace would make muster think A is now B, breaking dedup and
spawning duplicate workspaces. The user explicitly wants the opposite: a pane
opened on B inside A's workspace still belongs to A.

## Decision

muster owns the project↔workspace mapping. When muster creates a workspace for
a project, it records `canonical_project_dir → workspace_id` in a registry file
(`state.json`) under `HERDR_PLUGIN_STATE_DIR`. Identity is **assigned at
creation**, never derived from runtime pane state.

On each run muster reconciles the registry against `herdr workspace list` and
drops entries whose workspace no longer exists. A project is "open" iff its
registry entry points to a live workspace.

## Alternatives considered

- **Infer from pane cwd** (match a workspace by any pane's `cwd`). Zero state,
  but exactly the drift/duplicate problem above. Rejected.
- **Encode the dir in the workspace `label`.** No state file and survives pane
  movement, but the label is user-visible (path pollutes the sidebar) and a
  user rename silently breaks identity. Rejected.

## Consequences

- **Positive:** identity survives `cd` and splits; dedup is a map lookup, not
  an N+1 sweep of `pane list`; the registry is invisible to the herdr UI.
- **Negative:** muster only knows about workspaces it created. A workspace made
  through herdr's own UI is not in the registry, so muster may create a second
  workspace for that directory. Accepted — muster is the intended entry point
  for project workspaces.
- **Negative:** the registry can drift (workspace closed outside muster). Handled
  by pruning against `workspace list` every run.
- State lives at `HERDR_PLUGIN_STATE_DIR/state.json`; losing it degrades
  gracefully to "everything looks dormant" (worst case: one duplicate
  workspace), never a crash.
