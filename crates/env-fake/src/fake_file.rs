// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Error};
use env_traits::FileEnv;

/// In-memory filesystem + env-var store.
#[derive(Clone, Default)]
pub struct FakeFileEnv {
    files:    Arc<Mutex<HashMap<String, Vec<u8>>>>,
    env_vars: Arc<Mutex<HashMap<String, String>>>,
}

impl FakeFileEnv {
    /// Seed a file with given contents.
    pub fn with_file(self, path: impl Into<String>, contents: impl Into<Vec<u8>>) -> Self {
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
    type Error = Error;

    fn read_file(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("FakeFileEnv: file not found: {path}"))
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), Error> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), contents.to_vec());
        Ok(())
    }

    fn file_exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn dir_exists(&self, path: &str) -> bool {
        // A directory "exists" if any stored path has it as a prefix.
        let prefix = path.to_string();
        self.files
            .lock()
            .unwrap()
            .keys()
            .any(|k| k.starts_with(&prefix) && k != &prefix)
    }

    fn create_dir_all(&self, _path: &str) -> Result<(), Error> {
        // No-op: directories are implicit in the in-memory map.
        Ok(())
    }

    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), Error>> + '_>, Error> {
        let prefix = root.to_string();
        let entries: Vec<Result<(String, bool), Error>> = self
            .files
            .lock()
            .unwrap()
            .keys()
            .filter(|p| p.starts_with(&prefix))
            .map(|p| Ok((p.clone(), false)))
            .collect();
        Ok(Box::new(entries.into_iter()))
    }

    fn env_var(&self, key: &str) -> Option<String> {
        self.env_vars.lock().unwrap().get(key).cloned()
    }
}
