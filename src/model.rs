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
