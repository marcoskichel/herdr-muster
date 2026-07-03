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
