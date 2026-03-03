// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Result};
use env_traits::FileEnv;

/// In-memory filesystem + env-var store.
#[derive(Clone, Default)]
pub struct FakeFileEnv {
    files:    Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    env_vars: Arc<Mutex<HashMap<String, String>>>,
}

impl FakeFileEnv {
    /// Seed a file with given contents.
    pub fn with_file(self, path: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> Self {
        self.files.lock().unwrap().insert(path.into(), contents.into());
        self
    }

    /// Seed an environment variable.
    pub fn with_env(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.lock().unwrap().insert(key.into(), value.into());
        self
    }
}

impl FileEnv for FakeFileEnv {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("FakeFileEnv: file not found: {}", path.display()))
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn dir_exists(&self, path: &Path) -> bool {
        // A directory "exists" if any stored path has it as a prefix.
        let prefix = path.to_path_buf();
        self.files
            .lock()
            .unwrap()
            .keys()
            .any(|k| k.starts_with(&prefix) && k != &prefix)
    }

    fn create_dir_all(&self, _path: &Path) -> Result<()> {
        // No-op: directories are implicit in the in-memory map.
        Ok(())
    }

    fn walk(
        &self,
        root: &Path,
    ) -> Result<Box<dyn Iterator<Item = Result<(PathBuf, bool)>> + '_>> {
        let entries: Vec<Result<(PathBuf, bool)>> = self
            .files
            .lock()
            .unwrap()
            .keys()
            .filter(|p| p.starts_with(root))
            .map(|p| Ok((p.clone(), false)))
            .collect();
        Ok(Box::new(entries.into_iter()))
    }

    fn env_var(&self, key: &str) -> Option<String> {
        self.env_vars.lock().unwrap().get(key).cloned()
    }
}
