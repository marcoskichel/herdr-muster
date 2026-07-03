use crate::herdr::{Pane, Workspace};
use crate::sources::{basename, collapse_home, Candidate};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
            AgentState::Blocked => "●",
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

/// Canonicalize a workspace cwd for identity/dedup; fall back to the raw path
/// when it no longer exists on disk.
fn canon(dir: &str) -> PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| PathBuf::from(dir))
}

/// One OPEN row per live workspace. Directory identity: the registry binding
/// (muster-created, survives `cd`) wins; otherwise the workspace's root-pane
/// cwd. Dormant projects that resolve to an open dir are dropped.
pub fn assemble(
    bound: &HashMap<PathBuf, String>,
    workspaces: &[Workspace],
    panes: &[Pane],
    dormant: &[Candidate],
) -> Vec<Row> {
    // ws -> bound dir (invert the registry map).
    let ws_bound: HashMap<&str, &Path> =
        bound.iter().map(|(d, w)| (w.as_str(), d.as_path())).collect();

    // ws -> its root (lowest-numbered) pane.
    let mut root_pane: HashMap<&str, &Pane> = HashMap::new();
    for p in panes {
        root_pane
            .entry(p.workspace_id.as_str())
            .and_modify(|cur| {
                if p.number() < cur.number() {
                    *cur = p;
                }
            })
            .or_insert(p);
    }

    let mut rows = Vec::new();
    let mut open_dirs = HashSet::new();
    for w in workspaces {
        let id = w.workspace_id.as_str();
        let root = root_pane.get(id);
        // identity: registry binding first, then root-pane cwd.
        let dir: Option<PathBuf> = ws_bound
            .get(id)
            .map(|p| p.to_path_buf())
            .or_else(|| root.map(|p| canon(&p.cwd)));
        let agent = root.and_then(|p| p.agent.clone());
        let state = AgentState::from_str(&w.agent_status);

        let (name, display, path) = match &dir {
            Some(d) => (basename(d), collapse_home(d), d.clone()),
            // no dir known — show the workspace label so it's still reachable.
            None => (w.label.clone(), w.label.clone(), PathBuf::from(&w.label)),
        };
        open_dirs.insert(path.clone());
        rows.push(Row {
            name,
            display,
            path,
            kind: Kind::Open { workspace_id: w.workspace_id.clone(), state, agent },
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

    fn ws(id: &str, label: &str, st: &str) -> Workspace {
        Workspace { workspace_id: id.into(), label: label.into(), agent_status: st.into() }
    }
    fn cand(p: &str) -> Candidate {
        Candidate { path: PathBuf::from(p), display: p.into() }
    }
    fn pane(id: &str, ws: &str, cwd: &str, agent: Option<&str>) -> Pane {
        Pane { pane_id: id.into(), workspace_id: ws.into(), cwd: cwd.into(), agent: agent.map(Into::into) }
    }

    #[test]
    fn open_from_registry_and_from_pane_cwd_with_sort() {
        // w1 muster-created: binding /dev/web. Its pane has cd'd to /tmp/moved,
        // but the stored identity must win.
        let mut bound = HashMap::new();
        bound.insert(PathBuf::from("/dev/web"), "w1".to_string());
        let workspaces = vec![ws("w1", "web", "working"), ws("w2", "api", "blocked")];
        // w2 not bound -> dir inferred from its root (lowest) pane cwd.
        let panes = vec![
            pane("w1:p1", "w1", "/tmp/moved", Some("codex")),
            pane("w2:p2", "w2", "/other", None),
            pane("w2:p1", "w2", "/dev/api", None), // root pane (p1 < p2)
        ];
        // /dev/api is open (w2), so must NOT appear as dormant.
        let dormant = vec![cand("/dev/api"), cand("/dev/zeta"), cand("/dev/alpha")];

        let rows = assemble(&bound, &workspaces, &panes, &dormant);

        // order: blocked(api), working(web), then dormant alpha, zeta
        assert_eq!(rows[0].name, "api");
        assert!(matches!(rows[0].kind, Kind::Open { state: AgentState::Blocked, .. }));
        // binding wins over the pane's moved cwd
        assert_eq!(rows[1].name, "web");
        assert_eq!(rows[1].path, PathBuf::from("/dev/web"));
        match &rows[1].kind {
            Kind::Open { agent, .. } => assert_eq!(agent.as_deref(), Some("codex")),
            _ => panic!(),
        }
        assert_eq!(rows[2].name, "alpha");
        assert!(matches!(rows[2].kind, Kind::Dormant));
        assert_eq!(rows[3].name, "zeta");
        assert_eq!(rows.len(), 4); // api open, not duplicated as dormant
    }
}
