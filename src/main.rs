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
