# herdr-muster Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `muster`, a native-Rust herdr plugin: a keybind opens an agent-aware fuzzy switcher that focuses a project's workspace (showing its agent state) or musters a new one, with project↔workspace identity stored in a muster-owned registry.

**Architecture:** A single Rust binary run as a herdr plugin **pane**. Pure modules (`config`, `sources`, `registry`, `model`) are unit-tested; a trait-fronted `herdr` module wraps the `herdr` CLI and is mocked in tests. `picker` is a ratatui + nucleo grouped switcher returning an `Outcome`; `main` owns all side effects and the close→refresh loop.

**Tech Stack:** Rust 2021, ratatui + crossterm, nucleo-matcher, serde/serde_json, toml, dirs, tempfile (dev).

## Global Constraints

- Plugin `id = "kichel.muster"`; binary `herdr-muster`; display name `Muster`.
- `min_herdr_version = "0.7.0"`; verified against installed **0.7.1**.
- Platforms `["linux", "macos"]`.
- All herdr calls via `$HERDR_BIN_PATH` (fallback literal `"herdr"`).
- No external `fzf`. `zoxide` optional at runtime (skip silently if absent).
- Identity is stored, never inferred from pane cwd (ADR 0001).
- Meta line for open rows is `<agent> · <state>` only — herdr exposes no message/elapsed (verified).
- Verified CLI JSON shapes (herdr 0.7.1):
  - `workspace list` → `{"result":{"workspaces":[{"workspace_id":"w5","label":"~","agent_status":"working"}]}}`
  - `agent list` → `{"result":{"agents":[{"agent":"claude","agent_status":"working","workspace_id":"w5","pane_id":"w5:p1","cwd":"/home/x"}]}}`
  - `agent_status` ∈ `idle|working|blocked|done|unknown`
  - `workspace create --cwd P --label L --focus` → `{"result":{"workspace":{"workspace_id":"w6"},"root_pane":{"cwd":"P"}}}`
  - `workspace focus <id>` / `workspace close <id>` / `pane close <id>` → `{"result":{"type":"ok"}}`

## File structure

```
herdr-muster/
  Cargo.toml
  herdr-plugin.toml
  config.toml.example
  README.md
  src/
    main.rs        # env wiring, act-loop, self-close
    config.rs      # Config, load, ~ expansion
    sources.rs     # gather/dedup dormant dirs; basename, collapse_home (pure)
    registry.rs    # state.json load/save; bind/unbind/workspace_for/reconcile
    herdr.rs       # Herdr trait, CliHerdr, JSON parsers, Workspace/Agent structs
    model.rs       # AgentState/Kind/Row, assemble(), sort (pure)
    picker.rs      # ratatui + nucleo grouped switcher, Outcome
```

Each pure `src/*.rs` carries an inline `#[cfg(test)] mod tests`.

---

### Task 1: Scaffold crate

**Files:** Create `Cargo.toml`, `src/main.rs`, `.gitignore`
**Interfaces:** Produces a compiling `herdr-muster` binary with deps resolved.

- [ ] **Step 1: `.gitignore`**

```
/target
```

- [ ] **Step 2: `Cargo.toml`**

```toml
[package]
name = "herdr-muster"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "herdr-muster"
path = "src/main.rs"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
nucleo-matcher = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
dirs = "5"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: placeholder `src/main.rs`**

```rust
fn main() {
    eprintln!("herdr-muster: not yet wired");
}
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: downloads deps, `Finished`, no errors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs .gitignore
git commit -m "chore: scaffold herdr-muster crate"
```

---

### Task 2: Config loading

**Files:** Create `src/config.rs`; modify `src/main.rs` (`mod config;`)
**Interfaces:** Produces `Config { paths, roots, use_zoxide }`, `expand_tilde`, `Config::load` (missing file → default, `use_zoxide == true`).

- [ ] **Step 1: add `mod config;` at top of `src/main.rs`**

- [ ] **Step 2: write `src/config.rs` (tests + impl together)**

```rust
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub paths: Vec<String>,
    pub roots: Vec<String>,
    pub use_zoxide: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { paths: Vec::new(), roots: Vec::new(), use_zoxide: true }
    }
}

pub fn expand_tilde(s: &str) -> PathBuf {
    if s == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(s));
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| format!("bad config {}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("read {}: {e}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_yields_default_with_zoxide_on() {
        let cfg = Config::load(Path::new("/no/such/file.toml")).unwrap();
        assert!(cfg.paths.is_empty() && cfg.roots.is_empty() && cfg.use_zoxide);
    }

    #[test]
    fn parses_fields() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "paths=[\"~/dev/api\"]\nroots=[\"~/dev\"]\nuse_zoxide=false\n").unwrap();
        let cfg = Config::load(f.path()).unwrap();
        assert_eq!(cfg.paths, vec!["~/dev/api".to_string()]);
        assert_eq!(cfg.roots, vec!["~/dev".to_string()]);
        assert!(!cfg.use_zoxide);
    }

    #[test]
    fn malformed_is_error() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "paths = not-an-array").unwrap();
        assert!(Config::load(f.path()).is_err());
    }

    #[test]
    fn expand_tilde_cases() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/dev"), home.join("dev"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test config`
Expected: 4 pass.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: config loading with ~ expansion"
```

---

### Task 3: Directory sources

**Files:** Create `src/sources.rs`; modify `src/main.rs` (`mod sources;`)
**Interfaces:**
- `pub struct Candidate { pub path: PathBuf, pub display: String }`
- `pub fn basename(p: &Path) -> String`
- `pub fn collapse_home(p: &Path) -> String`
- `pub fn git_repos_under(root: &Path) -> Vec<PathBuf>`
- `pub fn gather(cfg: &Config, zoxide_lines: &[String]) -> Vec<Candidate>` (canonical, deduped, existing dirs only)

- [ ] **Step 1: add `mod sources;` to `src/main.rs`**

- [ ] **Step 2: write `src/sources.rs`**

```rust
use crate::config::{expand_tilde, Config};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub display: String,
}

pub fn basename(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

pub fn collapse_home(p: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = p.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.display());
        }
    }
    p.display().to_string()
}

pub fn git_repos_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join(".git").exists() {
            out.push(p);
        }
    }
    out
}

fn finalize(raw: Vec<PathBuf>) -> Vec<Candidate> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for p in raw {
        let Ok(canon) = std::fs::canonicalize(&p) else { continue };
        if !canon.is_dir() {
            continue;
        }
        if seen.insert(canon.clone()) {
            out.push(Candidate { display: collapse_home(&canon), path: canon });
        }
    }
    out
}

pub fn gather(cfg: &Config, zoxide_lines: &[String]) -> Vec<Candidate> {
    let mut raw: Vec<PathBuf> = Vec::new();
    for p in &cfg.paths {
        raw.push(expand_tilde(p));
    }
    for r in &cfg.roots {
        raw.extend(git_repos_under(&expand_tilde(r)));
    }
    if cfg.use_zoxide {
        for l in zoxide_lines {
            raw.push(PathBuf::from(l));
        }
    }
    finalize(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn git_repos_under_finds_only_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("proj");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("plain")).unwrap();
        assert_eq!(git_repos_under(tmp.path()), vec![repo]);
    }

    #[test]
    fn gather_dedups_and_drops_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        fs::create_dir_all(&a).unwrap();
        let cfg = Config {
            paths: vec![a.to_string_lossy().to_string(), a.to_string_lossy().to_string()],
            roots: vec![],
            use_zoxide: true,
        };
        let z = vec![
            a.to_string_lossy().to_string(),
            tmp.path().join("ghost").to_string_lossy().to_string(),
        ];
        let got = gather(&cfg, &z);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, fs::canonicalize(&a).unwrap());
    }

    #[test]
    fn basename_and_collapse() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(basename(Path::new("/x/y/proj")), "proj");
        assert_eq!(collapse_home(&home.join("dev")), "~/dev");
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test sources`
Expected: 3 pass.

- [ ] **Step 4: Commit**

```bash
git add src/sources.rs src/main.rs
git commit -m "feat: gather and dedup candidate directories"
```

---

### Task 4: Identity registry

**Files:** Create `src/registry.rs`; modify `src/main.rs` (`mod registry;`)
**Interfaces:**
- `pub struct Registry { map: HashMap<String,String> }` (canonical-dir-string → workspace_id), `#[derive(Default)]`
- `pub fn load(path: &Path) -> Registry` (default on missing/corrupt — never panics)
- `pub fn save(&self, path: &Path) -> std::io::Result<()>`
- `pub fn workspace_for(&self, dir: &Path) -> Option<&String>`
- `pub fn bind(&mut self, dir: &Path, ws: &str)` / `pub fn unbind(&mut self, dir: &Path)`
- `pub fn reconcile(&mut self, live: &HashSet<String>) -> bool` (retain only live ws ids; returns whether anything was dropped)
- `pub fn live_map(&self) -> HashMap<PathBuf,String>`

Caller passes **canonical** dirs.

- [ ] **Step 1: add `mod registry;` to `src/main.rs`**

- [ ] **Step 2: write `src/registry.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    map: HashMap<String, String>,
}

fn key(dir: &Path) -> String {
    dir.to_string_lossy().to_string()
}

impl Registry {
    pub fn load(path: &Path) -> Registry {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Registry::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(path, text)
    }

    pub fn workspace_for(&self, dir: &Path) -> Option<&String> {
        self.map.get(&key(dir))
    }

    pub fn bind(&mut self, dir: &Path, ws: &str) {
        self.map.insert(key(dir), ws.to_string());
    }

    pub fn unbind(&mut self, dir: &Path) {
        self.map.remove(&key(dir));
    }

    pub fn reconcile(&mut self, live: &HashSet<String>) -> bool {
        let before = self.map.len();
        self.map.retain(|_, ws| live.contains(ws));
        before != self.map.len()
    }

    pub fn live_map(&self) -> HashMap<PathBuf, String> {
        self.map.iter().map(|(k, v)| (PathBuf::from(k), v.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_unbind_lookup() {
        let mut r = Registry::default();
        r.bind(Path::new("/a"), "w1");
        assert_eq!(r.workspace_for(Path::new("/a")).map(String::as_str), Some("w1"));
        r.unbind(Path::new("/a"));
        assert!(r.workspace_for(Path::new("/a")).is_none());
    }

    #[test]
    fn reconcile_drops_dead() {
        let mut r = Registry::default();
        r.bind(Path::new("/a"), "w1");
        r.bind(Path::new("/b"), "w2");
        let live: HashSet<String> = ["w2".to_string()].into_iter().collect();
        assert!(r.reconcile(&live));
        assert!(r.workspace_for(Path::new("/a")).is_none());
        assert_eq!(r.workspace_for(Path::new("/b")).map(String::as_str), Some("w2"));
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sub").join("state.json");
        let mut r = Registry::default();
        r.bind(Path::new("/a"), "w1");
        r.save(&path).unwrap();
        let r2 = Registry::load(&path);
        assert_eq!(r2.workspace_for(Path::new("/a")).map(String::as_str), Some("w1"));
    }

    #[test]
    fn load_missing_or_corrupt_is_default() {
        assert!(Registry::load(Path::new("/no/such")).live_map().is_empty());
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "{not json").unwrap();
        assert!(Registry::load(tmp.path()).live_map().is_empty());
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test registry`
Expected: 4 pass.

- [ ] **Step 4: Commit**

```bash
git add src/registry.rs src/main.rs
git commit -m "feat: muster-owned identity registry"
```

---

### Task 5: herdr CLI wrapper + parsers

**Files:** Create `src/herdr.rs`; modify `src/main.rs` (`mod herdr;`)
**Interfaces:**
- `pub struct Workspace { pub workspace_id: String, pub label: String, pub agent_status: String }`
- `pub struct Agent { pub agent: String, pub workspace_id: String }`
- `pub trait Herdr { fn list_workspaces(&self)->Result<Vec<Workspace>,String>; fn list_agents(&self)->Result<Vec<Agent>,String>; fn create_workspace(&self,cwd:&str,label:&str)->Result<String,String>; fn focus_workspace(&self,id:&str)->Result<(),String>; fn close_workspace(&self,id:&str)->Result<(),String>; fn close_pane(&self,id:&str)->Result<(),String>; }`
- `pub struct CliHerdr { pub bin: String }` implementing `Herdr`
- `pub fn parse_workspaces(json:&str)->Result<Vec<Workspace>,String>`
- `pub fn parse_agents(json:&str)->Result<Vec<Agent>,String>`
- `pub fn parse_created_id(json:&str)->Result<String,String>`

- [ ] **Step 1: add `mod herdr;` to `src/main.rs`**

- [ ] **Step 2: write `src/herdr.rs`**

```rust
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    pub agent_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub agent: String,
    pub workspace_id: String,
}

pub trait Herdr {
    fn list_workspaces(&self) -> Result<Vec<Workspace>, String>;
    fn list_agents(&self) -> Result<Vec<Agent>, String>;
    fn create_workspace(&self, cwd: &str, label: &str) -> Result<String, String>;
    fn focus_workspace(&self, id: &str) -> Result<(), String>;
    fn close_workspace(&self, id: &str) -> Result<(), String>;
    fn close_pane(&self, id: &str) -> Result<(), String>;
}

// ---- JSON shapes (herdr 0.7.1); unknown fields ignored ----

#[derive(Deserialize)]
struct WsResp { result: WsResult }
#[derive(Deserialize)]
struct WsResult { workspaces: Vec<WsItem> }
#[derive(Deserialize)]
struct WsItem {
    workspace_id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    agent_status: String,
}

#[derive(Deserialize)]
struct AgResp { result: AgResult }
#[derive(Deserialize)]
struct AgResult { agents: Vec<AgItem> }
#[derive(Deserialize)]
struct AgItem { agent: String, workspace_id: String }

#[derive(Deserialize)]
struct CrResp { result: CrResult }
#[derive(Deserialize)]
struct CrResult { workspace: CrWs }
#[derive(Deserialize)]
struct CrWs { workspace_id: String }

pub fn parse_workspaces(json: &str) -> Result<Vec<Workspace>, String> {
    let r: WsResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.workspaces.into_iter().map(|w| Workspace {
        workspace_id: w.workspace_id,
        label: w.label,
        agent_status: if w.agent_status.is_empty() { "unknown".into() } else { w.agent_status },
    }).collect())
}

pub fn parse_agents(json: &str) -> Result<Vec<Agent>, String> {
    let r: AgResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.agents.into_iter().map(|a| Agent {
        agent: a.agent,
        workspace_id: a.workspace_id,
    }).collect())
}

pub fn parse_created_id(json: &str) -> Result<String, String> {
    let r: CrResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.workspace.workspace_id)
}

pub struct CliHerdr {
    pub bin: String,
}

impl CliHerdr {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new(&self.bin)
            .args(args)
            .output()
            .map_err(|e| format!("spawn {}: {e}", self.bin))?;
        if !out.status.success() {
            return Err(format!("herdr {:?}: {}", args, String::from_utf8_lossy(&out.stderr)));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

impl Herdr for CliHerdr {
    fn list_workspaces(&self) -> Result<Vec<Workspace>, String> {
        parse_workspaces(&self.run(&["workspace", "list"])?)
    }
    fn list_agents(&self) -> Result<Vec<Agent>, String> {
        parse_agents(&self.run(&["agent", "list"])?)
    }
    fn create_workspace(&self, cwd: &str, label: &str) -> Result<String, String> {
        parse_created_id(&self.run(&["workspace", "create", "--cwd", cwd, "--label", label, "--focus"])?)
    }
    fn focus_workspace(&self, id: &str) -> Result<(), String> {
        self.run(&["workspace", "focus", id]).map(|_| ())
    }
    fn close_workspace(&self, id: &str) -> Result<(), String> {
        self.run(&["workspace", "close", id]).map(|_| ())
    }
    fn close_pane(&self, id: &str) -> Result<(), String> {
        self.run(&["pane", "close", id]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WS: &str = r#"{"result":{"type":"workspace_list","workspaces":[{"workspace_id":"w5","label":"~","agent_status":"working"},{"workspace_id":"w6","label":"/tmp","agent_status":""}]}}"#;
    const AG: &str = r#"{"result":{"type":"agent_list","agents":[{"agent":"claude","agent_status":"working","workspace_id":"w5","pane_id":"w5:p1","cwd":"/home/x"}]}}"#;
    const CR: &str = r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"cwd":"/p"},"type":"workspace_created"}}"#;

    #[test]
    fn parses_workspaces_with_status_default() {
        let ws = parse_workspaces(WS).unwrap();
        assert_eq!(ws[0].workspace_id, "w5");
        assert_eq!(ws[0].agent_status, "working");
        assert_eq!(ws[1].agent_status, "unknown"); // empty -> unknown
    }

    #[test]
    fn parses_agents_join_field() {
        let ag = parse_agents(AG).unwrap();
        assert_eq!(ag, vec![Agent { agent: "claude".into(), workspace_id: "w5".into() }]);
    }

    #[test]
    fn parses_created_id() {
        assert_eq!(parse_created_id(CR).unwrap(), "w9");
    }

    #[test]
    fn bad_json_errors() {
        assert!(parse_workspaces("nope").is_err());
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test herdr`
Expected: 4 pass.

- [ ] **Step 4: Commit**

```bash
git add src/herdr.rs src/main.rs
git commit -m "feat: herdr CLI wrapper and JSON parsers"
```

---

### Task 6: Row model + assembly + sort

**Files:** Create `src/model.rs`; modify `src/main.rs` (`mod model;`)
**Interfaces:**
- `pub enum AgentState { Blocked, Working, Done, Idle, Unknown }` with `from_str(&str)->Self`, `rank(&self)->u8`, `glyph(&self)->&'static str`, `word(&self)->&'static str`
- `pub enum Kind { Open { workspace_id: String, state: AgentState, agent: Option<String> }, Dormant }`
- `pub struct Row { pub name: String, pub path: PathBuf, pub display: String, pub kind: Kind }`
- `pub fn assemble(bound: &HashMap<PathBuf,String>, workspaces: &[Workspace], agents: &[Agent], dormant: &[Candidate]) -> Vec<Row>`
  (`bound` = live dir→ws map; open rows carry state+agent; dormant excludes any dir already open; sort: open before dormant, open by state rank then name, dormant by name.)

- [ ] **Step 1: add `mod model;` to `src/main.rs`**

- [ ] **Step 2: write `src/model.rs`**

```rust
use crate::herdr::{Agent, Workspace};
use crate::sources::{basename, collapse_home, Candidate};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Blocked,
    Working,
    Done,
    Idle,
    Unknown,
}

impl AgentState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "blocked" => AgentState::Blocked,
            "working" => AgentState::Working,
            "done" => AgentState::Done,
            "idle" => AgentState::Idle,
            _ => AgentState::Unknown,
        }
    }
    pub fn rank(&self) -> u8 {
        match self {
            AgentState::Blocked => 0,
            AgentState::Working => 1,
            AgentState::Done => 2,
            AgentState::Idle => 3,
            AgentState::Unknown => 4,
        }
    }
    pub fn glyph(&self) -> &'static str {
        match self {
            AgentState::Blocked => "⛔",
            AgentState::Working => "◐",
            AgentState::Done => "✓",
            AgentState::Idle => "○",
            AgentState::Unknown => "·",
        }
    }
    pub fn word(&self) -> &'static str {
        match self {
            AgentState::Blocked => "blocked",
            AgentState::Working => "working",
            AgentState::Done => "done",
            AgentState::Idle => "idle",
            AgentState::Unknown => "unknown",
        }
    }
}

#[derive(Debug)]
pub enum Kind {
    Open { workspace_id: String, state: AgentState, agent: Option<String> },
    Dormant,
}

#[derive(Debug)]
pub struct Row {
    pub name: String,
    pub path: PathBuf,
    pub display: String,
    pub kind: Kind,
}

pub fn assemble(
    bound: &HashMap<PathBuf, String>,
    workspaces: &[Workspace],
    agents: &[Agent],
    dormant: &[Candidate],
) -> Vec<Row> {
    let status: HashMap<&str, &str> =
        workspaces.iter().map(|w| (w.workspace_id.as_str(), w.agent_status.as_str())).collect();
    let agent_of: HashMap<&str, &str> =
        agents.iter().map(|a| (a.workspace_id.as_str(), a.agent.as_str())).collect();

    let mut rows = Vec::new();
    let mut open_dirs = HashSet::new();
    for (dir, ws) in bound {
        open_dirs.insert(dir.clone());
        let state = AgentState::from_str(status.get(ws.as_str()).copied().unwrap_or("unknown"));
        let agent = agent_of.get(ws.as_str()).map(|s| s.to_string());
        rows.push(Row {
            name: basename(dir),
            display: collapse_home(dir),
            path: dir.clone(),
            kind: Kind::Open { workspace_id: ws.clone(), state, agent },
        });
    }
    for c in dormant {
        if open_dirs.contains(&c.path) {
            continue;
        }
        rows.push(Row {
            name: basename(&c.path),
            display: c.display.clone(),
            path: c.path.clone(),
            kind: Kind::Dormant,
        });
    }
    rows.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    rows
}

fn sort_key(r: &Row) -> (u8, u8, String) {
    match &r.kind {
        Kind::Open { state, .. } => (0, state.rank(), r.name.to_lowercase()),
        Kind::Dormant => (1, 0, r.name.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: &str, st: &str) -> Workspace {
        Workspace { workspace_id: id.into(), label: String::new(), agent_status: st.into() }
    }
    fn cand(p: &str) -> Candidate {
        Candidate { path: PathBuf::from(p), display: p.into() }
    }

    #[test]
    fn assembles_open_and_dormant_with_sort_and_join() {
        let mut bound = HashMap::new();
        bound.insert(PathBuf::from("/dev/web"), "w1".to_string());   // working
        bound.insert(PathBuf::from("/dev/api"), "w2".to_string());   // blocked
        let workspaces = vec![ws("w1", "working"), ws("w2", "blocked")];
        let agents = vec![Agent { agent: "codex".into(), workspace_id: "w1".into() }];
        // /dev/api is open, so it must NOT appear as dormant even if listed
        let dormant = vec![cand("/dev/api"), cand("/dev/zeta"), cand("/dev/alpha")];

        let rows = assemble(&bound, &workspaces, &agents, &dormant);

        // order: blocked(api), working(web), then dormant alpha, zeta
        assert_eq!(rows[0].name, "api");
        assert!(matches!(rows[0].kind, Kind::Open { state: AgentState::Blocked, .. }));
        assert_eq!(rows[1].name, "web");
        match &rows[1].kind {
            Kind::Open { agent, .. } => assert_eq!(agent.as_deref(), Some("codex")),
            _ => panic!(),
        }
        assert_eq!(rows[2].name, "alpha");
        assert!(matches!(rows[2].kind, Kind::Dormant));
        assert_eq!(rows[3].name, "zeta");
        assert_eq!(rows.len(), 4); // api not duplicated
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test model`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src/model.rs src/main.rs
git commit -m "feat: row assembly with agent state and blocked-first sort"
```

---

### Task 7: Switcher TUI

**Files:** Create `src/picker.rs`; modify `src/main.rs` (`mod picker;`)
**Interfaces:**
- `pub enum Outcome { Cancel, Jump(usize), ForceNew(usize), Close(usize) }` (index into the `rows` slice)
- `pub fn run(rows: &[crate::model::Row]) -> std::io::Result<Outcome>`

Group headers show only when the query is empty; a typed query yields a flat, score-ranked list. Ctrl-X returns `Close` only on an `Open` row.

- [ ] **Step 1: add `mod picker;` to `src/main.rs`**

- [ ] **Step 2: write `src/picker.rs`**

```rust
use crate::model::{AgentState, Kind, Row};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::io::stdout;

pub enum Outcome {
    Cancel,
    Jump(usize),
    ForceNew(usize),
    Close(usize),
}

fn state_color(s: AgentState) -> Color {
    match s {
        AgentState::Blocked => Color::Red,
        AgentState::Working => Color::Cyan,
        AgentState::Done => Color::Green,
        AgentState::Idle => Color::DarkGray,
        AgentState::Unknown => Color::DarkGray,
    }
}

/// Returns original row indices, ranked. Empty query keeps assembled order.
fn filter(rows: &[Row], query: &str, matcher: &mut Matcher) -> Vec<usize> {
    if query.is_empty() {
        return (0..rows.len()).collect();
    }
    let pat = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let hay_str = format!("{} {}", r.name, r.display);
            let hay = Utf32Str::new(&hay_str, &mut buf);
            pat.score(hay, matcher).map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn row_line(r: &Row) -> Line<'static> {
    match &r.kind {
        Kind::Open { state, agent, .. } => {
            let color = state_color(*state);
            let meta = match agent {
                Some(a) => format!("{a} · {}", state.word()),
                None => state.word().to_string(),
            };
            Line::from(vec![
                Span::styled(format!("{} ", state.glyph()), Style::default().fg(color)),
                Span::styled(format!("{:<18} ", r.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:<28} ", r.display), Style::default().fg(Color::DarkGray)),
                Span::styled(meta, Style::default().fg(color)),
            ])
        }
        Kind::Dormant => Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{:<18} ", r.name), Style::default().fg(Color::Gray)),
            Span::styled(r.display.clone(), Style::default().fg(Color::DarkGray)),
        ]),
    }
}

fn header(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM),
    )))
}

/// Build list items (with headers when browsing) and the list position of the
/// currently-selected filtered row.
fn build(rows: &[Row], filtered: &[usize], sel: usize, show_headers: bool) -> (Vec<ListItem<'static>>, usize) {
    let mut items = Vec::new();
    let mut sel_pos = 0;
    let mut last_group: Option<u8> = None;
    for (fi, &ri) in filtered.iter().enumerate() {
        let r = &rows[ri];
        let group = match r.kind { Kind::Open { .. } => 0u8, Kind::Dormant => 1u8 };
        if show_headers && last_group != Some(group) {
            items.push(header(if group == 0 { "  OPEN" } else { "  PROJECTS" }));
            last_group = Some(group);
        }
        if fi == sel {
            sel_pos = items.len();
        }
        items.push(ListItem::new(row_line(r)));
    }
    (items, sel_pos)
}

pub fn run(rows: &[Row]) -> std::io::Result<Outcome> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), terminal::EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);
    let mut query = String::new();
    let mut sel: usize = 0;
    let mut outcome = Outcome::Cancel;

    loop {
        let filtered = filter(rows, &query, &mut matcher);
        if sel >= filtered.len() {
            sel = filtered.len().saturating_sub(1);
        }
        let (items, sel_pos) = build(rows, &filtered, sel, query.is_empty());

        term.draw(|f| {
            let v = Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
                .split(f.area());
            let prompt = Paragraph::new(format!("muster> {query}"))
                .block(Block::default().borders(Borders::ALL).title("pick project"));
            f.render_widget(prompt, v[0]);

            let mut st = ListState::default();
            if !filtered.is_empty() {
                st.select(Some(sel_pos));
            }
            let list = List::new(items)
                .highlight_style(Style::default().bg(Color::Rgb(60, 50, 30)).add_modifier(Modifier::BOLD))
                .highlight_symbol("▌");
            f.render_stateful_widget(list, v[1], &mut st);

            let help = Paragraph::new("↵ jump   ^n new   ^x close   esc cancel")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(help, v[2]);
        })?;

        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            match k.code {
                KeyCode::Esc => break,
                KeyCode::Char('c') if ctrl => break,
                KeyCode::Enter => {
                    if let Some(&ri) = filtered.get(sel) {
                        outcome = Outcome::Jump(ri);
                        break;
                    }
                }
                KeyCode::Char('n') if ctrl => {
                    if let Some(&ri) = filtered.get(sel) {
                        outcome = Outcome::ForceNew(ri);
                        break;
                    }
                }
                KeyCode::Char('x') if ctrl => {
                    if let Some(&ri) = filtered.get(sel) {
                        if matches!(rows[ri].kind, Kind::Open { .. }) {
                            outcome = Outcome::Close(ri);
                            break;
                        }
                    }
                }
                KeyCode::Up => sel = sel.saturating_sub(1),
                KeyCode::Down => {
                    if sel + 1 < filtered.len() {
                        sel += 1;
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    sel = 0;
                }
                KeyCode::Char(c) if !ctrl => {
                    query.push(c);
                    sel = 0;
                }
                _ => {}
            }
        }
    }

    execute!(stdout(), terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    Ok(outcome)
}
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles clean.

- [ ] **Step 4: Manual smoke**

Temporarily add to `main()` (remove after):

```rust
// TEMP
use crate::model::{Kind, AgentState, Row};
let rows = vec![
    Row { name: "api".into(), path: "/dev/api".into(), display: "~/dev/api".into(),
        kind: Kind::Open { workspace_id: "w1".into(), state: AgentState::Blocked, agent: Some("claude".into()) } },
    Row { name: "infra".into(), path: "/dev/infra".into(), display: "~/dev/infra".into(), kind: Kind::Dormant },
];
eprintln!("{:?}", match picker::run(&rows).unwrap() {
    picker::Outcome::Jump(i) => format!("jump {i}"),
    picker::Outcome::ForceNew(i) => format!("new {i}"),
    picker::Outcome::Close(i) => format!("close {i}"),
    picker::Outcome::Cancel => "cancel".into(),
});
```

Run: `cargo run`
Expected: switcher shows OPEN (⛔ api · claude · blocked) and PROJECTS (infra); typing filters flat; ↵/^n/^x/esc print the matching outcome; ^x on `infra` (dormant) does nothing. Then delete the TEMP block.

- [ ] **Step 5: Commit**

```bash
git add src/picker.rs src/main.rs
git commit -m "feat: agent-aware grouped switcher TUI"
```

---

### Task 8: Wire `main.rs`

**Files:** Modify `src/main.rs` (final orchestration)
**Interfaces:** Produces finished behavior — load, assemble, pick, act, close→refresh loop, self-close.

- [ ] **Step 1: write final `src/main.rs`**

```rust
mod config;
mod herdr;
mod model;
mod picker;
mod registry;
mod sources;

use herdr::Herdr;
use model::{Kind, Row};
use registry::Registry;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn zoxide_lines(enabled: bool) -> Vec<String> {
    if !enabled {
        return Vec::new();
    }
    let Ok(out) = std::process::Command::new("zoxide").args(["query", "-l"]).output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout).lines().map(|s| s.to_string()).collect()
}

fn config_path() -> PathBuf {
    match std::env::var("HERDR_PLUGIN_CONFIG_DIR") {
        Ok(d) => PathBuf::from(d).join("config.toml"),
        Err(_) => PathBuf::from("config.toml"),
    }
}

fn state_path() -> PathBuf {
    match std::env::var("HERDR_PLUGIN_STATE_DIR") {
        Ok(d) => PathBuf::from(d).join("state.json"),
        Err(_) => PathBuf::from("state.json"),
    }
}

fn create_and_bind<H: Herdr>(h: &H, reg: &mut Registry, dir: &Path) -> Result<(), String> {
    let cwd = dir.to_string_lossy().to_string();
    let id = h.create_workspace(&cwd, &sources::basename(dir))?;
    reg.bind(dir, &id);
    Ok(())
}

fn run() -> Result<(), String> {
    let bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let client = herdr::CliHerdr { bin };

    let cfg = config::Config::load(&config_path())?;
    let dormant = sources::gather(&cfg, &zoxide_lines(cfg.use_zoxide));

    let reg_path = state_path();
    let mut reg = Registry::load(&reg_path);
    let mut dirty = false;

    let result = (|| -> Result<(), String> {
        loop {
            let workspaces = client.list_workspaces().unwrap_or_default();
            let agents = client.list_agents().unwrap_or_default();
            let live: HashSet<String> =
                workspaces.iter().map(|w| w.workspace_id.clone()).collect();
            if reg.reconcile(&live) {
                dirty = true;
            }
            let bound = reg.live_map();
            let rows: Vec<Row> = model::assemble(&bound, &workspaces, &agents, &dormant);
            if rows.is_empty() {
                return Err(format!("no projects — edit {}", config_path().display()));
            }

            match picker::run(&rows).map_err(|e| e.to_string())? {
                picker::Outcome::Cancel => return Ok(()),
                picker::Outcome::Jump(i) => {
                    match &rows[i].kind {
                        Kind::Open { workspace_id, .. } => client.focus_workspace(workspace_id)?,
                        Kind::Dormant => {
                            create_and_bind(&client, &mut reg, &rows[i].path)?;
                            dirty = true;
                        }
                    }
                    return Ok(());
                }
                picker::Outcome::ForceNew(i) => {
                    create_and_bind(&client, &mut reg, &rows[i].path)?;
                    dirty = true;
                    return Ok(());
                }
                picker::Outcome::Close(i) => {
                    if let Kind::Open { workspace_id, .. } = &rows[i].kind {
                        client.close_workspace(workspace_id)?;
                        reg.unbind(&rows[i].path);
                        dirty = true;
                    }
                    continue; // re-assemble and re-open
                }
            }
        }
    })();

    if dirty {
        let _ = reg.save(&reg_path);
    }
    if let Ok(pane) = std::env::var("HERDR_PANE_ID") {
        let _ = client.close_pane(&pane);
    }
    result
}

fn main() {
    if let Err(e) = run() {
        eprintln!("herdr-muster: {e}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: Build + full test suite**

Run: `cargo build && cargo test`
Expected: compiles; all unit tests pass (config 4, sources 3, registry 4, herdr 4, model 1).

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire switcher with registry, agents, and act-loop"
```

---

### Task 9: Manifest, docs, live smoke test

**Files:** Create `herdr-plugin.toml`, `config.toml.example`, `README.md`
**Interfaces:** Produces an installable plugin exercised end-to-end in herdr 0.7.1.

- [ ] **Step 1: `herdr-plugin.toml`**

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

- [ ] **Step 2: `config.toml.example`**

```toml
# Copy to the dir printed by:  herdr plugin config-dir kichel.muster
# then rename to config.toml
paths      = ["~/dev/api", "~/notes"]
roots      = ["~/dev"]
use_zoxide = true
```

- [ ] **Step 3: `README.md`**

```markdown
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
```

- [ ] **Step 4: Build release**

Run: `cargo build --release`
Expected: `target/release/herdr-muster` exists.

- [ ] **Step 5: Link + verify listed**

Run: `herdr plugin link /home/kichelm/dev/herdr-muster && herdr plugin list`
Expected: `kichel.muster` appears.

- [ ] **Step 6: Open the switcher**

Run: `herdr plugin pane open --plugin kichel.muster --entrypoint picker`
Expected: a zoomed pane opens the switcher (with a valid `config.toml`; otherwise the "no projects" hint confirms wiring).

- [ ] **Step 7: End-to-end**

With a valid `config.toml`:
1. Select a **dormant** project → new workspace created + focused, picker pane closes; `herdr workspace list` shows the new id.
2. Re-open muster → that project now appears under **OPEN** with its state; `cd` inside its pane, re-open muster → still one row, still Open (identity held).
3. Select it → focuses the existing workspace (no duplicate).
4. Ctrl-X on it → workspace closes, row returns to PROJECTS.
5. Confirm `state.json` under `herdr plugin config-dir`-adjacent state dir tracks the binding (or is pruned after close).

- [ ] **Step 8: Commit**

```bash
git add herdr-plugin.toml config.toml.example README.md
git commit -m "feat: plugin manifest, config example, and README"
```

---

## Self-Review

- **Spec coverage:** switcher open/dormant groups + state glyph + blocked-first sort (Task 6, 7) ✓; agent-name join (Task 5, 6) ✓; identity registry + reconcile + persist (Task 4, 8, ADR 0001) ✓; focus-else-create + Ctrl-N force-new + Ctrl-X close→refresh loop (Task 7, 8) ✓; config paths/roots/zoxide (Task 2, 3) ✓; self-close via `HERDR_PANE_ID` (Task 8) ✓; manifest pane+action+build, keybind docs (Task 9) ✓; error paths — zoxide absent / `workspace list` fail (`unwrap_or_default`) / `agent list` fail (names omitted) / corrupt registry (default) / empty projects (hint) / bad config (Task 2) ✓.
- **Placeholder scan:** only intentional temporary code is Task 7 Step 4's TEMP block, removed before Task 8 rewrites `main.rs`. No TBD/TODO.
- **Type consistency:** `Workspace`/`Agent` fields identical across `herdr` parsers, `Herdr` trait, and `model::assemble`; `AgentState`/`Kind`/`Row` identical across `model`, `picker`, `main`; `Outcome` variants match between Task 7 and Task 8; `Registry` methods (`load`/`save`/`bind`/`unbind`/`reconcile`/`live_map`/`workspace_for`) consistent in Task 4 and Task 8.

## Notes / risks

- `herdr pane close $HERDR_PANE_ID` teardown verified live in Task 9 Step 7; fallback = rely on process exit (drop the `close_pane` call).
- `agent list` schema is confirmed against a live agent (herdr 0.7.1). If a future herdr renames `agent`/`workspace_id`, only `parse_agents` changes; agent names degrade to absent, glyph/state unaffected.
- nucleo-matcher 0.3 `Utf32Str::new(&str, &mut Vec<char>)` + `Pattern::score` used in `picker`; `cargo build` surfaces any minor API drift at Task 7.
```
