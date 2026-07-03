use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    pub agent_status: String,
}

/// A live pane. Carries the directory identity for its workspace when muster
/// did not create it (root-pane cwd), plus any detected agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub pane_id: String,
    pub workspace_id: String,
    pub cwd: String,
    pub agent: Option<String>,
}

impl Pane {
    /// Numeric suffix of `wX:pN`; used to pick a workspace's root (lowest) pane.
    pub fn number(&self) -> u32 {
        self.pane_id
            .rsplit(':')
            .next()
            .and_then(|s| s.strip_prefix('p'))
            .and_then(|n| n.parse().ok())
            .unwrap_or(u32::MAX)
    }
}

pub trait Herdr {
    fn list_workspaces(&self) -> Result<Vec<Workspace>, String>;
    fn list_panes(&self) -> Result<Vec<Pane>, String>;
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
struct PnResp { result: PnResult }
#[derive(Deserialize)]
struct PnResult { panes: Vec<PnItem> }
#[derive(Deserialize)]
struct PnItem {
    pane_id: String,
    workspace_id: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    agent: Option<String>,
}

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

pub fn parse_created_id(json: &str) -> Result<String, String> {
    let r: CrResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.workspace.workspace_id)
}

pub fn parse_panes(json: &str) -> Result<Vec<Pane>, String> {
    let r: PnResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.result.panes.into_iter().map(|p| Pane {
        pane_id: p.pane_id,
        workspace_id: p.workspace_id,
        cwd: p.cwd,
        agent: p.agent.filter(|a| !a.is_empty()),
    }).collect())
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
    fn list_panes(&self) -> Result<Vec<Pane>, String> {
        parse_panes(&self.run(&["pane", "list"])?)
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
    const CR: &str = r#"{"result":{"workspace":{"workspace_id":"w9"},"root_pane":{"cwd":"/p"},"type":"workspace_created"}}"#;
    const PN: &str = r#"{"result":{"type":"pane_list","panes":[{"pane_id":"wE:p1","workspace_id":"wE","cwd":"/home/x/dev/api","agent":"claude","agent_status":"working"},{"pane_id":"wE:p2","workspace_id":"wE","cwd":"/tmp"},{"pane_id":"wB:p1","workspace_id":"wB","cwd":"/home/x"}]}}"#;

    #[test]
    fn parses_workspaces_with_status_default() {
        let ws = parse_workspaces(WS).unwrap();
        assert_eq!(ws[0].workspace_id, "w5");
        assert_eq!(ws[0].agent_status, "working");
        assert_eq!(ws[1].agent_status, "unknown"); // empty -> unknown
    }

    #[test]
    fn parses_created_id() {
        assert_eq!(parse_created_id(CR).unwrap(), "w9");
    }

    #[test]
    fn parses_panes_with_cwd_and_optional_agent() {
        let pn = parse_panes(PN).unwrap();
        assert_eq!(pn[0].workspace_id, "wE");
        assert_eq!(pn[0].cwd, "/home/x/dev/api");
        assert_eq!(pn[0].agent.as_deref(), Some("claude"));
        assert_eq!(pn[1].agent, None); // missing agent -> None
        assert_eq!(pn[0].number(), 1);
        assert_eq!(pn[1].number(), 2);
    }

    #[test]
    fn bad_json_errors() {
        assert!(parse_workspaces("nope").is_err());
    }
}
