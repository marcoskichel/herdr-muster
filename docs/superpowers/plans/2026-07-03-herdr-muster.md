# herdr-muster Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `muster`, a native-Rust herdr plugin: a keybind opens a fuzzy project picker that focuses a directory's herdr workspace, or creates one if none exists.

**Architecture:** A single Rust binary run as a herdr plugin **pane** entrypoint. Pure modules (`config`, `sources`, selection logic) are unit-tested; a trait-fronted `herdr` module wraps the `herdr` CLI (invoked via `$HERDR_BIN_PATH`) and is mocked in tests. `picker` is a ratatui + nucleo TUI verified manually.

**Tech Stack:** Rust 2021, ratatui + crossterm (TUI), nucleo-matcher (fuzzy), serde/serde_json (parse herdr JSON), toml + dirs (config), tempfile (test fixtures).

## Global Constraints

- Plugin `id = "kichel.muster"`; binary name `herdr-muster`; display name `Muster`.
- Target herdr: `min_herdr_version = "0.7.0"`; verified against installed **0.7.1**.
- Platforms: `["linux", "macos"]`.
- All herdr CLI calls go through `$HERDR_BIN_PATH` (fallback literal `"herdr"`), never a hardcoded path.
- No external `fzf` dependency. `zoxide` is optional at runtime (skip silently if absent).
- Verified CLI JSON shapes (herdr 0.7.1):
  - `workspace list` → `{"result":{"workspaces":[{"workspace_id":"w2","label":"~",…}]}}`
  - `pane list --workspace <id>` → `{"result":{"panes":[{"cwd":"/home/x","pane_id":"w2:p1",…}]}}`
  - `workspace create --cwd P --label L --focus` → `{"result":{"workspace":{"workspace_id":"w3"},"root_pane":{"cwd":"P"}}}`
  - `workspace focus <id>` → `{"result":{"type":"ok"}}`
  - `pane close <pane_id>` → `{"result":{"type":"ok"}}`
- Dedup key = first pane's `cwd` from `pane list --workspace <id>` (workspace objects carry no cwd).

---

## File structure

```
herdr-muster/
  Cargo.toml                 # crate + deps
  herdr-plugin.toml          # plugin manifest (build, pane, action)
  config.toml.example        # sample user config
  README.md                  # install + keybind docs
  src/
    main.rs                  # env wiring, orchestration, self-close
    config.rs                # Config struct, load, ~ expansion
    sources.rs               # gather/merge/dedup candidate dirs (pure)
    herdr.rs                 # Herdr trait, JSON parsers, CliHerdr, cwd-map, decide()
    picker.rs                # ratatui + nucleo TUI + preview
```

Each `src/*.rs` (except `picker`/`main`) carries an inline `#[cfg(test)] mod tests`.

---

### Task 1: Scaffold crate

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.gitignore`

**Interfaces:**
- Produces: a compiling binary `herdr-muster` with all deps resolved.

- [ ] **Step 1: Write `.gitignore`**

```
/target
```

- [ ] **Step 2: Write `Cargo.toml`**

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

- [ ] **Step 3: Write placeholder `src/main.rs`**

```rust
fn main() {
    eprintln!("herdr-muster: not yet wired");
}
```

- [ ] **Step 4: Build to resolve deps**

Run: `cargo build`
Expected: compiles, downloads deps, prints `Finished`. No errors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs .gitignore
git commit -m "chore: scaffold herdr-muster crate"
```

---

### Task 2: Config loading

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)

**Interfaces:**
- Produces:
  - `pub struct Config { pub paths: Vec<String>, pub roots: Vec<String>, pub use_zoxide: bool }`
  - `pub fn expand_tilde(s: &str) -> std::path::PathBuf`
  - `impl Config { pub fn load(path: &std::path::Path) -> Result<Config, String> }`
  - Missing file → `Ok(Config::default())` with `use_zoxide == true`.

- [ ] **Step 1: Declare the module in `src/main.rs`**

Add at top of `src/main.rs`:

```rust
mod config;
```

- [ ] **Step 2: Write failing tests in `src/config.rs`**

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
        assert!(cfg.paths.is_empty());
        assert!(cfg.roots.is_empty());
        assert!(cfg.use_zoxide);
    }

    #[test]
    fn parses_fields() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "paths = [\"~/dev/api\"]\nroots = [\"~/dev\"]\nuse_zoxide = false\n").unwrap();
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
    fn expand_tilde_root_and_child() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/dev"), home.join("dev"));
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
    }
}
```

- [ ] **Step 3: Run tests to verify they pass** (implementation is already inline with the tests)

Run: `cargo test --lib config`
Expected: 4 tests pass. If the binary crate exposes no `--lib`, run `cargo test config` — same tests execute.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: config loading with ~ expansion"
```

---

### Task 3: Directory sources (gather / dedup)

**Files:**
- Create: `src/sources.rs`
- Modify: `src/main.rs` (add `mod sources;`)

**Interfaces:**
- Consumes: `config::Config`, `config::expand_tilde`.
- Produces:
  - `pub struct Candidate { pub path: std::path::PathBuf, pub display: String, pub live: Option<String> }`
    (`path` = canonical absolute; `display` = home-collapsed; `live` = `Some(workspace_id)` when a workspace exists, set later by caller — starts `None`.)
  - `pub fn git_repos_under(root: &Path) -> Vec<PathBuf>`
  - `pub fn collapse_home(p: &Path) -> String`
  - `pub fn gather(cfg: &Config, zoxide_lines: &[String]) -> Vec<Candidate>`

- [ ] **Step 1: Declare the module in `src/main.rs`**

Add near the other `mod` lines:

```rust
mod sources;
```

- [ ] **Step 2: Write tests + implementation in `src/sources.rs`**

```rust
use crate::config::{expand_tilde, Config};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub display: String,
    pub live: Option<String>,
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

/// Direct child directories of `root` that contain a `.git` entry.
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

/// Canonicalize, drop nonexistent, dedup by absolute path (stable order).
fn finalize(raw: Vec<PathBuf>) -> Vec<Candidate> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for p in raw {
        let Ok(canon) = std::fs::canonicalize(&p) else { continue };
        if !canon.is_dir() {
            continue;
        }
        if seen.insert(canon.clone()) {
            out.push(Candidate {
                display: collapse_home(&canon),
                path: canon,
                live: None,
            });
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
        let found = git_repos_under(tmp.path());
        assert_eq!(found, vec![repo]);
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
        // one real dup + one nonexistent zoxide line
        let z = vec![
            a.to_string_lossy().to_string(),
            tmp.path().join("ghost").to_string_lossy().to_string(),
        ];
        let got = gather(&cfg, &z);
        let canon_a = fs::canonicalize(&a).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, canon_a);
        assert!(got[0].live.is_none());
    }

    #[test]
    fn collapse_home_shortens() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(collapse_home(&home), "~");
        assert_eq!(collapse_home(&home.join("dev")), "~/dev");
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test sources`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/sources.rs src/main.rs
git commit -m "feat: gather and dedup candidate directories"
```

---

### Task 4: herdr CLI wrapper + JSON parsers + cwd map

**Files:**
- Create: `src/herdr.rs`
- Modify: `src/main.rs` (add `mod herdr;`)

**Interfaces:**
- Produces:
  - `pub trait Herdr { fn list_workspace_ids(&self) -> Result<Vec<String>, String>; fn pane_cwds(&self, ws: &str) -> Result<Vec<String>, String>; fn create_workspace(&self, cwd: &str, label: &str) -> Result<String, String>; fn focus_workspace(&self, ws: &str) -> Result<(), String>; fn close_pane(&self, pane: &str) -> Result<(), String>; }`
  - `pub struct CliHerdr { pub bin: String }` implementing `Herdr`.
  - `pub fn parse_workspace_ids(json: &str) -> Result<Vec<String>, String>`
  - `pub fn parse_pane_cwds(json: &str) -> Result<Vec<String>, String>`
  - `pub fn parse_created_id(json: &str) -> Result<String, String>`
  - `pub fn build_cwd_map<H: Herdr>(h: &H) -> std::collections::HashMap<std::path::PathBuf, String>`
    (canonical dir → workspace_id; skips workspaces whose panes fail or whose cwd can't canonicalize.)

- [ ] **Step 1: Declare the module in `src/main.rs`**

```rust
mod herdr;
```

- [ ] **Step 2: Write tests + implementation in `src/herdr.rs`**

```rust
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub trait Herdr {
    fn list_workspace_ids(&self) -> Result<Vec<String>, String>;
    fn pane_cwds(&self, workspace_id: &str) -> Result<Vec<String>, String>;
    /// Creates a focused workspace, returns its workspace_id.
    fn create_workspace(&self, cwd: &str, label: &str) -> Result<String, String>;
    fn focus_workspace(&self, workspace_id: &str) -> Result<(), String>;
    fn close_pane(&self, pane_id: &str) -> Result<(), String>;
}

// ---- JSON shapes (herdr 0.7.1) ----

#[derive(Deserialize)]
struct WsListResp { result: WsListResult }
#[derive(Deserialize)]
struct WsListResult { workspaces: Vec<WsItem> }
#[derive(Deserialize)]
struct WsItem { workspace_id: String }

#[derive(Deserialize)]
struct PaneListResp { result: PaneListResult }
#[derive(Deserialize)]
struct PaneListResult { panes: Vec<PaneItem> }
#[derive(Deserialize)]
struct PaneItem { cwd: String }

#[derive(Deserialize)]
struct CreateResp { result: CreateResult }
#[derive(Deserialize)]
struct CreateResult { workspace: CreatedWs }
#[derive(Deserialize)]
struct CreatedWs { workspace_id: String }

pub fn parse_workspace_ids(json: &str) -> Result<Vec<String>, String> {
    let r: WsListResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.workspaces.into_iter().map(|w| w.workspace_id).collect())
}

pub fn parse_pane_cwds(json: &str) -> Result<Vec<String>, String> {
    let r: PaneListResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.panes.into_iter().map(|p| p.cwd).collect())
}

pub fn parse_created_id(json: &str) -> Result<String, String> {
    let r: CreateResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.workspace.workspace_id)
}

/// canonical dir -> workspace_id, using the first pane's cwd of each workspace.
pub fn build_cwd_map<H: Herdr>(h: &H) -> HashMap<PathBuf, String> {
    let mut map = HashMap::new();
    let Ok(ids) = h.list_workspace_ids() else { return map };
    for id in ids {
        let Ok(cwds) = h.pane_cwds(&id) else { continue };
        let Some(first) = cwds.into_iter().next() else { continue };
        if let Ok(canon) = std::fs::canonicalize(&first) {
            map.entry(canon).or_insert(id);
        }
    }
    map
}

// ---- Real implementation ----

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
            return Err(format!(
                "herdr {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

impl Herdr for CliHerdr {
    fn list_workspace_ids(&self) -> Result<Vec<String>, String> {
        parse_workspace_ids(&self.run(&["workspace", "list"])?)
    }
    fn pane_cwds(&self, workspace_id: &str) -> Result<Vec<String>, String> {
        parse_pane_cwds(&self.run(&["pane", "list", "--workspace", workspace_id])?)
    }
    fn create_workspace(&self, cwd: &str, label: &str) -> Result<String, String> {
        parse_created_id(&self.run(&[
            "workspace", "create", "--cwd", cwd, "--label", label, "--focus",
        ])?)
    }
    fn focus_workspace(&self, workspace_id: &str) -> Result<(), String> {
        self.run(&["workspace", "focus", workspace_id]).map(|_| ())
    }
    fn close_pane(&self, pane_id: &str) -> Result<(), String> {
        self.run(&["pane", "close", pane_id]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WS_LIST: &str = r#"{"id":"cli:workspace:list","result":{"type":"workspace_list","workspaces":[{"workspace_id":"w2","label":"~"},{"workspace_id":"w5","label":"/tmp"}]}}"#;
    const PANE_LIST: &str = r#"{"id":"cli:pane:list","result":{"panes":[{"cwd":"/home/x","pane_id":"w2:p1"}],"type":"pane_list"}}"#;
    const CREATE: &str = r#"{"id":"cli:workspace:create","result":{"workspace":{"workspace_id":"w9"},"root_pane":{"cwd":"/p"},"type":"workspace_created"}}"#;

    #[test]
    fn parses_workspace_ids() {
        assert_eq!(parse_workspace_ids(WS_LIST).unwrap(), vec!["w2", "w5"]);
    }

    #[test]
    fn parses_pane_cwds() {
        assert_eq!(parse_pane_cwds(PANE_LIST).unwrap(), vec!["/home/x"]);
    }

    #[test]
    fn parses_created_id() {
        assert_eq!(parse_created_id(CREATE).unwrap(), "w9");
    }

    #[test]
    fn bad_json_errors() {
        assert!(parse_workspace_ids("not json").is_err());
    }

    struct Mock;
    impl Herdr for Mock {
        fn list_workspace_ids(&self) -> Result<Vec<String>, String> {
            Ok(vec!["w2".into(), "w5".into()])
        }
        fn pane_cwds(&self, ws: &str) -> Result<Vec<String>, String> {
            // both point at the real temp dir created in the test via env override
            let dir = std::env::var("MUSTER_TEST_DIR").unwrap();
            match ws {
                "w2" => Ok(vec![dir]),
                _ => Err("boom".into()), // failing workspace is skipped
            }
        }
        fn create_workspace(&self, _c: &str, _l: &str) -> Result<String, String> { unreachable!() }
        fn focus_workspace(&self, _w: &str) -> Result<(), String> { unreachable!() }
        fn close_pane(&self, _p: &str) -> Result<(), String> { unreachable!() }
    }

    #[test]
    fn build_cwd_map_maps_first_pane_and_skips_failures() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("MUSTER_TEST_DIR", tmp.path().to_string_lossy().to_string());
        let map = build_cwd_map(&Mock);
        let canon = std::fs::canonicalize(tmp.path()).unwrap();
        assert_eq!(map.get(&canon).map(String::as_str), Some("w2"));
        assert_eq!(map.len(), 1); // w5 failed -> skipped
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test herdr`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/herdr.rs src/main.rs
git commit -m "feat: herdr CLI wrapper, JSON parsers, cwd map"
```

---

### Task 5: Selection decision

**Files:**
- Modify: `src/herdr.rs` (append `decide` + tests)

**Interfaces:**
- Consumes: the cwd map from `build_cwd_map`.
- Produces:
  - `pub enum Action { Focus(String), Create { cwd: String, label: String } }`
  - `pub fn decide(selected: &std::path::Path, cwd_map: &std::collections::HashMap<std::path::PathBuf, String>) -> Action`
    (label = the selected path as a string; cwd = same. If `selected` is a key in the map → `Focus(workspace_id)`, else `Create`.)

- [ ] **Step 1: Append the failing tests to `src/herdr.rs` `tests` module**

Add inside the existing `#[cfg(test)] mod tests { … }`:

```rust
    #[test]
    fn decide_focus_when_present() {
        let mut m = HashMap::new();
        m.insert(PathBuf::from("/a"), "w2".to_string());
        match decide(std::path::Path::new("/a"), &m) {
            Action::Focus(id) => assert_eq!(id, "w2"),
            _ => panic!("expected focus"),
        }
    }

    #[test]
    fn decide_create_when_absent() {
        let m = HashMap::new();
        match decide(std::path::Path::new("/b/proj"), &m) {
            Action::Create { cwd, label } => {
                assert_eq!(cwd, "/b/proj");
                assert_eq!(label, "/b/proj");
            }
            _ => panic!("expected create"),
        }
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test herdr::tests::decide`
Expected: FAIL — `cannot find function decide` / `cannot find type Action`.

- [ ] **Step 3: Implement `decide` + `Action`** (add above the `#[cfg(test)]` block in `src/herdr.rs`)

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Focus(String),
    Create { cwd: String, label: String },
}

pub fn decide(selected: &std::path::Path, cwd_map: &HashMap<PathBuf, String>) -> Action {
    if let Some(id) = cwd_map.get(selected) {
        return Action::Focus(id.clone());
    }
    let s = selected.to_string_lossy().to_string();
    Action::Create { cwd: s.clone(), label: s }
}
```

- [ ] **Step 4: Run to confirm pass**

Run: `cargo test herdr`
Expected: all herdr tests pass (7 total).

- [ ] **Step 5: Commit**

```bash
git add src/herdr.rs
git commit -m "feat: focus-else-create decision logic"
```

---

### Task 6: Picker TUI

**Files:**
- Create: `src/picker.rs`
- Modify: `src/main.rs` (add `mod picker;`)

**Interfaces:**
- Consumes: `Vec<sources::Candidate>` (with `live` already annotated).
- Produces:
  - `pub fn run(items: Vec<crate::sources::Candidate>) -> std::io::Result<Option<crate::sources::Candidate>>`
    (returns the chosen candidate, or `None` on quit.)
  - `fn preview(path: &std::path::Path) -> String` (git status + shallow tree; internal).

- [ ] **Step 1: Declare the module in `src/main.rs`**

```rust
mod picker;
```

- [ ] **Step 2: Write `src/picker.rs`**

```rust
use crate::sources::Candidate;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::{execute, terminal};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::io::stdout;
use std::path::Path;

/// git status + shallow directory listing for the preview pane.
fn preview(path: &Path) -> String {
    let mut out = String::new();
    if path.join(".git").exists() {
        if let Ok(o) = std::process::Command::new("git")
            .arg("-C").arg(path).arg("status").arg("-s")
            .output()
        {
            let s = String::from_utf8_lossy(&o.stdout);
            out.push_str("git status\n");
            out.push_str(if s.trim().is_empty() { "  (clean)\n" } else { &s });
            out.push('\n');
        }
    }
    out.push_str("contents\n");
    if let Ok(rd) = std::fs::read_dir(path) {
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                if e.path().is_dir() { format!("{n}/") } else { n }
            })
            .collect();
        names.sort();
        for n in names.into_iter().take(40) {
            out.push_str("  ");
            out.push_str(&n);
            out.push('\n');
        }
    }
    out
}

fn filter(items: &[Candidate], query: &str, matcher: &mut Matcher) -> Vec<usize> {
    if query.is_empty() {
        return (0..items.len()).collect();
    }
    let pat = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let hay = Utf32Str::new(&it.display, &mut buf);
            pat.score(hay, matcher).map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

pub fn run(items: Vec<Candidate>) -> std::io::Result<Option<Candidate>> {
    terminal::enable_raw_mode()?;
    execute!(stdout(), terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut term = Terminal::new(backend)?;
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);

    let mut query = String::new();
    let mut sel: usize = 0;
    let mut chosen: Option<Candidate> = None;

    loop {
        let filtered = filter(&items, &query, &mut matcher);
        if sel >= filtered.len() {
            sel = filtered.len().saturating_sub(1);
        }
        let preview_text = filtered
            .get(sel)
            .map(|&i| preview(&items[i].path))
            .unwrap_or_default();

        term.draw(|f| {
            let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(f.area());
            let left = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(cols[0]);

            let prompt = Paragraph::new(format!("muster> {query}"))
                .block(Block::default().borders(Borders::ALL).title("pick project"));
            f.render_widget(prompt, left[0]);

            let rows: Vec<ListItem> = filtered
                .iter()
                .map(|&i| {
                    let c = &items[i];
                    let mark = if c.live.is_some() { "● " } else { "  " };
                    ListItem::new(format!("{mark}{}", c.display))
                })
                .collect();
            let mut state = ListState::default();
            if !filtered.is_empty() {
                state.select(Some(sel));
            }
            let list = List::new(rows)
                .block(Block::default().borders(Borders::ALL))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, left[1], &mut state);

            let prev = Paragraph::new(preview_text)
                .block(Block::default().borders(Borders::ALL).title("preview"));
            f.render_widget(prev, cols[1]);
        })?;

        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match k.code {
                KeyCode::Esc => break,
                KeyCode::Char('c') if k.modifiers.contains(event::KeyModifiers::CONTROL) => break,
                KeyCode::Enter => {
                    if let Some(&i) = filtered.get(sel) {
                        chosen = Some(items[i].clone());
                    }
                    break;
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
                KeyCode::Char(ch) => {
                    query.push(ch);
                    sel = 0;
                }
                _ => {}
            }
        }
    }

    execute!(stdout(), terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    Ok(chosen)
}
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles clean.

- [ ] **Step 4: Manual smoke of the picker in isolation**

Temporarily add to `src/main.rs` `main()` (remove after):

```rust
// TEMP manual check
let items = vec![crate::sources::Candidate {
    path: std::env::current_dir().unwrap(),
    display: ".".into(),
    live: None,
}];
let picked = picker::run(items).unwrap();
eprintln!("picked: {:?}", picked.map(|c| c.display));
```

Run: `cargo run`
Expected: TUI opens, arrow/typing works, `Enter` prints `picked: Some(".")`, `Esc` prints `picked: None`. Then delete the TEMP block.

- [ ] **Step 5: Commit**

```bash
git add src/picker.rs src/main.rs
git commit -m "feat: nucleo fuzzy picker TUI with preview"
```

---

### Task 7: Wire `main.rs`

**Files:**
- Modify: `src/main.rs` (replace body with full orchestration)

**Interfaces:**
- Consumes: `config`, `sources`, `herdr`, `picker`.
- Produces: the finished binary behavior — load config, gather + annotate, pick, act, self-close.

- [ ] **Step 1: Write the final `src/main.rs`**

Keep the `mod` lines at the top; replace `fn main`:

```rust
mod config;
mod herdr;
mod picker;
mod sources;

use std::path::PathBuf;

fn zoxide_lines(enabled: bool) -> Vec<String> {
    if !enabled {
        return Vec::new();
    }
    let Ok(out) = std::process::Command::new("zoxide").args(["query", "-l"]).output() else {
        return Vec::new(); // zoxide absent -> skip silently
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}

fn config_path() -> PathBuf {
    match std::env::var("HERDR_PLUGIN_CONFIG_DIR") {
        Ok(dir) => PathBuf::from(dir).join("config.toml"),
        Err(_) => PathBuf::from("config.toml"),
    }
}

fn run() -> Result<(), String> {
    let bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
    let client = herdr::CliHerdr { bin };

    let cfg = config::Config::load(&config_path())?;
    let z = zoxide_lines(cfg.use_zoxide);
    let mut items = sources::gather(&cfg, &z);
    if items.is_empty() {
        return Err(format!(
            "no projects — add paths/roots to {}",
            config_path().display()
        ));
    }

    // annotate live workspaces
    let cwd_map = herdr::build_cwd_map(&client);
    for it in &mut items {
        if let Some(id) = cwd_map.get(&it.path) {
            it.live = Some(id.clone());
        }
    }

    let picked = picker::run(items).map_err(|e| e.to_string())?;
    let Some(choice) = picked else { return Ok(()) };

    use herdr::{Action, Herdr};
    match herdr::decide(&choice.path, &cwd_map) {
        Action::Focus(id) => client.focus_workspace(&id)?,
        Action::Create { cwd, label } => {
            client.create_workspace(&cwd, &label)?;
        }
    }

    // self-close the picker pane if herdr told us which one we are
    if let Ok(pane) = std::env::var("HERDR_PANE_ID") {
        let _ = client.close_pane(&pane);
    }
    Ok(())
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
Expected: compiles; all unit tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire config, sources, picker, and herdr actions"
```

---

### Task 8: Manifest, docs, and live smoke test

**Files:**
- Create: `herdr-plugin.toml`
- Create: `config.toml.example`
- Create: `README.md`

**Interfaces:**
- Produces: an installable/linkable plugin exercised end-to-end in herdr 0.7.1.

- [ ] **Step 1: Write `herdr-plugin.toml`**

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

- [ ] **Step 2: Write `config.toml.example`**

```toml
# Copy to the dir printed by:  herdr plugin config-dir kichel.muster
# then rename to config.toml
paths      = ["~/dev/api", "~/notes"]
roots      = ["~/dev"]
use_zoxide = true
```

- [ ] **Step 3: Write `README.md`**

```markdown
# muster

Fuzzy project picker for [herdr](https://herdr.dev/). One keypress opens a
picker over your project directories; pick one to jump to its workspace, or
create a workspace there if none exists. "Muster the herd."

## Install (local dev)

    herdr plugin link /home/kichelm/dev/herdr-muster

The `[[build]]` step compiles `target/release/herdr-muster` on GitHub installs;
for `link`, build once yourself:

    cargo build --release

## Configure

    herdr plugin config-dir kichel.muster   # prints the config dir
    # copy config.toml.example there as config.toml and edit

- `paths`      — directories always listed
- `roots`      — scanned one level deep for git repos
- `use_zoxide` — merge `zoxide query -l` when zoxide is installed

## Keybind

Add to your herdr `config.toml`:

    [[keys.command]]
    key = "prefix+m"
    type = "plugin_action"
    command = "kichel.muster.open"

Then `herdr server reload-config`.

## Keys (in the picker)

- type to fuzzy filter · ↑/↓ move · Enter select · Esc / Ctrl-C cancel
- `●` marks a directory that already has a live workspace
```

- [ ] **Step 4: Build release binary**

Run: `cargo build --release`
Expected: `target/release/herdr-muster` exists.

- [ ] **Step 5: Link the plugin**

Run: `herdr plugin link /home/kichelm/dev/herdr-muster`
Expected: JSON success; `herdr plugin list` shows `kichel.muster`.

- [ ] **Step 6: Verify the action opens the pane**

Run: `herdr plugin pane open --plugin kichel.muster --entrypoint picker`
Expected: a zoomed pane opens running the picker TUI (needs a config.toml with at least one valid path, or it exits with the "no projects" hint — that hint itself confirms wiring).

- [ ] **Step 7: End-to-end check**

With a valid `config.toml`: open the picker, select a directory with **no** existing workspace → confirm a new workspace is created and focused, and the picker pane closes. Open again, select the **same** directory → confirm it focuses the existing workspace (no duplicate). Run `herdr workspace list` before/after to confirm no duplicate workspace_ids for that dir.

- [ ] **Step 8: Commit**

```bash
git add herdr-plugin.toml config.toml.example README.md
git commit -m "feat: plugin manifest, config example, and README"
```

---

## Self-Review

- **Spec coverage:** name/id/binary (Task 1, 8) ✓; config paths+roots+zoxide (Task 2, 3) ✓; dedup via pane cwd (Task 4) ✓; focus-else-create (Task 5, 7) ✓; picker + preview + live marker (Task 6) ✓; self-close via `HERDR_PANE_ID` (Task 7) ✓; manifest pane+action+build, keybind docs (Task 8) ✓; error cases — zoxide absent (Task 7 `zoxide_lines`), empty list (Task 7 hint), bad config (Task 2), workspace-list failure (Task 4 `build_cwd_map` returns empty → create path) ✓.
- **Placeholder scan:** the only intentional temporary code is Task 6 Step 4's TEMP block, explicitly removed before Task 7 rewrites `main.rs`. No TBD/TODO remain.
- **Type consistency:** `Candidate { path, display, live }` used identically across sources/picker/main; `Herdr` trait method names (`list_workspace_ids`, `pane_cwds`, `create_workspace`, `focus_workspace`, `close_pane`) consistent in trait, `CliHerdr`, `Mock`, and `main`; `Action`/`decide` signatures match between Task 5 and Task 7.

## Notes / risks

- `herdr pane close $HERDR_PANE_ID` behavior is verified live in Task 8 Step 7. If it doesn't tear the pane down cleanly, fallback is to rely on process exit closing the pane (drop the `close_pane` call) — a one-line change in `main.rs`.
- nucleo-matcher 0.3 `Utf32Str::new(&str, &mut Vec<char>)` + `Pattern::score` API is used in Task 6; if a minor API drift appears, `cargo build` surfaces it immediately at that task.
