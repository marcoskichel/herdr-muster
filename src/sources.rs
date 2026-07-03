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
