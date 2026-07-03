# CONTEXT — herdr-muster

Glossary for the `muster` plugin. Definitions only — no implementation detail.

## Terms

- **Project** — a directory muster can open, sourced from the user's config
  (`paths`, git repos under `roots`) or `zoxide`. A project exists whether or
  not it currently has a workspace.

- **Workspace** — herdr's unit of running work (its own tabs/panes). muster
  binds exactly one **project** to a workspace at the moment it creates that
  workspace. Herdr itself has no notion of a workspace's directory; muster
  supplies that meaning.

- **Open project** — a project that currently has a bound workspace. Shown in
  the picker's *open* group with its agent state.

- **Dormant project** — a known project with no workspace yet. Selecting one
  musters (creates) a workspace for it.

- **Agent state** — herdr's per-workspace status, surfaced by muster:
  *blocked* (waiting on the user), *working* (running), *done* (finished,
  awaiting review), *idle* (no agent activity).

- **Muster** (verb) — to jump to a project: focus its workspace if open, else
  create one and enter it. Named for the livestock-roundup sense of the word.

- **Identity** — the fixed association between a project and its workspace.
  Assigned when muster creates the workspace, so a pane later moved to another
  directory still belongs to its original project. Stored in a muster-owned
  registry; see ADR 0001.

- **Force new** — create a fresh workspace for a project even though one is
  already open, rebinding identity to the new one (Ctrl-N).

- **Close** — end an open project's workspace and unbind it (Ctrl-X).
