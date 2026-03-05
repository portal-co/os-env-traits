// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Error};
use env_traits::GitEnv;

#[derive(Clone, Default)]
pub struct FakeGitEnv {
    repo_root:     Arc<Mutex<Option<String>>>,
    revs:          Arc<Mutex<HashMap<String, String>>>,
    show_files:    Arc<Mutex<HashMap<(String, String), Vec<u8>>>>,
    changed_files: Arc<Mutex<Option<Vec<String>>>>,
    merge_bases:   Arc<Mutex<HashMap<String, String>>>,
}

impl FakeGitEnv {
    pub fn with_repo_root(self, path: impl Into<String>) -> Self {
        *self.repo_root.lock().unwrap() = Some(path.into());
        self
    }

    /// Register a rev → SHA mapping (e.g. `"HEAD^"` → `"abc123"`).
    pub fn with_rev(self, rev: impl Into<String>, sha: impl Into<String>) -> Self {
        self.revs.lock().unwrap().insert(rev.into(), sha.into());
        self
    }

    /// Register file content visible at a given commit.
    pub fn with_show_file(
        self,
        commit: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<Vec<u8>>,
    ) -> Self {
        self.show_files
            .lock()
            .unwrap()
            .insert((commit.into(), path.into()), content.into());
        self
    }

    /// Set the list returned by `changed_files` (applies to any base).
    pub fn with_changed_files(self, files: Vec<String>) -> Self {
        *self.changed_files.lock().unwrap() = Some(files);
        self
    }

    /// Register a branch → merge-base SHA mapping.
    pub fn with_merge_base(self, branch: impl Into<String>, sha: impl Into<String>) -> Self {
        self.merge_bases
            .lock()
            .unwrap()
            .insert(branch.into(), sha.into());
        self
    }
}

impl GitEnv for FakeGitEnv {
    type Error = Error;

    fn repo_root(&self) -> Result<String, Error> {
        self.repo_root
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("FakeGitEnv: repo_root not set"))
    }

    fn rev_parse(&self, _root: &str, rev: &str) -> Result<String, Error> {
        self.revs
            .lock()
            .unwrap()
            .get(rev)
            .cloned()
            .ok_or_else(|| anyhow!("FakeGitEnv: rev not found: {rev}"))
    }

    fn show_file(&self, _root: &str, commit: &str, path: &str) -> Result<Vec<u8>, Error> {
        self.show_files
            .lock()
            .unwrap()
            .get(&(commit.to_string(), path.to_string()))
            .cloned()
            .ok_or_else(|| anyhow!("FakeGitEnv: no file {path} at commit {commit}"))
    }

    fn changed_files(&self, _root: &str, _base: &str) -> Result<Vec<String>, Error> {
        self.changed_files
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("FakeGitEnv: changed_files not set"))
    }

    fn merge_base(&self, _root: &str, branch: &str) -> Result<String, Error> {
        self.merge_bases
            .lock()
            .unwrap()
            .get(branch)
            .cloned()
            .ok_or_else(|| anyhow!("FakeGitEnv: merge_base not set for branch {branch}"))
    }

    fn fetch(&self, _root: &str, _remote: &str, _refspec: &str) -> Result<(), Error> {
        Ok(()) // no-op
    }

    fn init(&self, _dir: &str) -> Result<(), Error> {
        Ok(()) // no-op
    }

    fn add_and_commit(&self, _root: &str, _message: &str) -> Result<(), Error> {
        Ok(()) // no-op
    }
}
