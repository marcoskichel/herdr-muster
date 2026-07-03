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
