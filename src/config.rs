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
