// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use anyhow::anyhow;
use env_traits::{
    async_::{AsyncAiEnv, AsyncFileEnv, AsyncGitEnv, AsyncGitHubEnv, AsyncNetworkEnv},
    AiEnv, FileEnv, GitEnv, GitHubEnv, GitHubFile, NetworkEnv,
};

// ── FakeError ─────────────────────────────────────────────────────────────────

/// Opaque error type used by all fake env implementations.
///
/// Wraps an [`anyhow::Error`] for convenient construction in tests while
/// satisfying [`embedded_io::Error`].
#[derive(Debug)]
pub struct FakeError(anyhow::Error);

impl fmt::Display for FakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl embedded_io::Error for FakeError {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

fn fake_err(e: anyhow::Error) -> FakeError {
    FakeError(e)
}

// ── FakeFileEnv ───────────────────────────────────────────────────────────────

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

impl embedded_io::ErrorType for FakeFileEnv {
    type Error = FakeError;
}

impl FileEnv for FakeFileEnv {
    fn read_file(&self, path: &str) -> Result<Vec<u8>, FakeError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeFileEnv: file not found: {path}")))
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), FakeError> {
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
        let prefix = path.to_string();
        self.files
            .lock()
            .unwrap()
            .keys()
            .any(|k| k.starts_with(&prefix) && k != &prefix)
    }

    fn create_dir_all(&self, _path: &str) -> Result<(), FakeError> {
        Ok(())
    }

    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), FakeError>> + '_>, FakeError> {
        let prefix = root.to_string();
        let entries: Vec<Result<(String, bool), FakeError>> = self
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

impl AsyncFileEnv for FakeFileEnv {
    fn read_file(&self, path: &str) -> impl core::future::Future<Output = Result<Vec<u8>, FakeError>> + Send {
        let result = FileEnv::read_file(self, path);
        async move { result }
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> impl core::future::Future<Output = Result<(), FakeError>> + Send {
        let result = FileEnv::write_file(self, path, contents);
        async move { result }
    }

    fn file_exists(&self, path: &str) -> impl core::future::Future<Output = bool> + Send {
        let result = FileEnv::file_exists(self, path);
        async move { result }
    }

    fn dir_exists(&self, path: &str) -> impl core::future::Future<Output = bool> + Send {
        let result = FileEnv::dir_exists(self, path);
        async move { result }
    }

    fn create_dir_all(&self, path: &str) -> impl core::future::Future<Output = Result<(), FakeError>> + Send {
        let result = FileEnv::create_dir_all(self, path);
        async move { result }
    }

    fn walk(&self, root: &str) -> impl core::future::Future<Output = Result<Vec<(String, bool)>, FakeError>> + Send {
        let result = FileEnv::walk(self, root)
            .and_then(|iter| iter.collect::<Result<Vec<(String, bool)>, _>>());
        async move { result }
    }

    fn env_var(&self, key: &str) -> impl core::future::Future<Output = Option<String>> + Send {
        let result = FileEnv::env_var(self, key);
        async move { result }
    }
}

// ── FakeGitEnv ────────────────────────────────────────────────────────────────

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

impl embedded_io::ErrorType for FakeGitEnv {
    type Error = FakeError;
}

impl GitEnv for FakeGitEnv {
    fn repo_root(&self) -> Result<String, FakeError> {
        self.repo_root
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| fake_err(anyhow!("FakeGitEnv: repo_root not set")))
    }

    fn rev_parse(&self, _root: &str, rev: &str) -> Result<String, FakeError> {
        self.revs
            .lock()
            .unwrap()
            .get(rev)
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeGitEnv: rev not found: {rev}")))
    }

    fn show_file(&self, _root: &str, commit: &str, path: &str) -> Result<Vec<u8>, FakeError> {
        self.show_files
            .lock()
            .unwrap()
            .get(&(commit.to_string(), path.to_string()))
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeGitEnv: no file {path} at commit {commit}")))
    }

    fn changed_files(&self, _root: &str, _base: &str) -> Result<Vec<String>, FakeError> {
        self.changed_files
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| fake_err(anyhow!("FakeGitEnv: changed_files not set")))
    }

    fn merge_base(&self, _root: &str, branch: &str) -> Result<String, FakeError> {
        self.merge_bases
            .lock()
            .unwrap()
            .get(branch)
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeGitEnv: merge_base not set for branch {branch}")))
    }

    fn fetch(&self, _root: &str, _remote: &str, _refspec: &str) -> Result<(), FakeError> {
        Ok(())
    }

    fn init(&self, _dir: &str) -> Result<(), FakeError> {
        Ok(())
    }

    fn add_and_commit(&self, _root: &str, _message: &str) -> Result<(), FakeError> {
        Ok(())
    }
}

impl AsyncGitEnv for FakeGitEnv {
    fn repo_root(&self) -> impl core::future::Future<Output = Result<String, FakeError>> + Send {
        let r = GitEnv::repo_root(self);
        async move { r }
    }
    fn rev_parse(&self, repo_root: &str, rev: &str) -> impl core::future::Future<Output = Result<String, FakeError>> + Send {
        let r = GitEnv::rev_parse(self, repo_root, rev);
        async move { r }
    }
    fn show_file(&self, repo_root: &str, commit: &str, path: &str) -> impl core::future::Future<Output = Result<Vec<u8>, FakeError>> + Send {
        let r = GitEnv::show_file(self, repo_root, commit, path);
        async move { r }
    }
    fn changed_files(&self, repo_root: &str, base: &str) -> impl core::future::Future<Output = Result<Vec<String>, FakeError>> + Send {
        let r = GitEnv::changed_files(self, repo_root, base);
        async move { r }
    }
    fn merge_base(&self, repo_root: &str, branch: &str) -> impl core::future::Future<Output = Result<String, FakeError>> + Send {
        let r = GitEnv::merge_base(self, repo_root, branch);
        async move { r }
    }
    fn fetch(&self, repo_root: &str, remote: &str, refspec: &str) -> impl core::future::Future<Output = Result<(), FakeError>> + Send {
        let r = GitEnv::fetch(self, repo_root, remote, refspec);
        async move { r }
    }
    fn init(&self, dir: &str) -> impl core::future::Future<Output = Result<(), FakeError>> + Send {
        let r = GitEnv::init(self, dir);
        async move { r }
    }
    fn add_and_commit(&self, repo_root: &str, message: &str) -> impl core::future::Future<Output = Result<(), FakeError>> + Send {
        let r = GitEnv::add_and_commit(self, repo_root, message);
        async move { r }
    }
}

// ── FakeGitHubEnv ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct FakeGitHubEnv {
    owner:     Arc<Mutex<Option<String>>>,
    repos:     Arc<Mutex<HashMap<String, Vec<String>>>>,
    contents:  Arc<Mutex<HashMap<(String, String, String), Vec<GitHubFile>>>>,
    downloads: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl FakeGitHubEnv {
    pub fn with_owner(self, owner: impl Into<String>) -> Self {
        *self.owner.lock().unwrap() = Some(owner.into());
        self
    }

    /// Register the list of repo names for an org.
    pub fn with_repos(self, org: impl Into<String>, repos: Vec<impl Into<String>>) -> Self {
        self.repos
            .lock()
            .unwrap()
            .insert(org.into(), repos.into_iter().map(Into::into).collect());
        self
    }

    /// Register files returned for a (org, repo, path) listing.
    pub fn with_contents(
        self,
        org: impl Into<String>,
        repo: impl Into<String>,
        path: impl Into<String>,
        files: Vec<GitHubFile>,
    ) -> Self {
        self.contents
            .lock()
            .unwrap()
            .insert((org.into(), repo.into(), path.into()), files);
        self
    }

    /// Register a download URL → bytes mapping.
    pub fn with_download(self, url: impl Into<String>, content: impl Into<Vec<u8>>) -> Self {
        self.downloads.lock().unwrap().insert(url.into(), content.into());
        self
    }
}

impl embedded_io::ErrorType for FakeGitHubEnv {
    type Error = FakeError;
}

impl GitHubEnv for FakeGitHubEnv {
    fn current_owner(&self) -> Result<String, FakeError> {
        self.owner
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| fake_err(anyhow!("FakeGitHubEnv: owner not set")))
    }

    fn list_repos(&self, org: &str, _limit: usize) -> Result<Vec<String>, FakeError> {
        self.repos
            .lock()
            .unwrap()
            .get(org)
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeGitHubEnv: no repos registered for org {org}")))
    }

    fn list_contents(&self, org: &str, repo: &str, path: &str) -> Result<Vec<GitHubFile>, FakeError> {
        Ok(self
            .contents
            .lock()
            .unwrap()
            .get(&(org.to_string(), repo.to_string(), path.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    fn download_file(&self, download_url: &str) -> Result<Vec<u8>, FakeError> {
        self.downloads
            .lock()
            .unwrap()
            .get(download_url)
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeGitHubEnv: no download registered for {download_url}")))
    }
}

impl AsyncGitHubEnv for FakeGitHubEnv {
    fn current_owner(&self) -> impl core::future::Future<Output = Result<String, FakeError>> + Send {
        let r = GitHubEnv::current_owner(self);
        async move { r }
    }
    fn list_repos(&self, org: &str, limit: usize) -> impl core::future::Future<Output = Result<Vec<String>, FakeError>> + Send {
        let r = GitHubEnv::list_repos(self, org, limit);
        async move { r }
    }
    fn list_contents(&self, org: &str, repo: &str, path: &str) -> impl core::future::Future<Output = Result<Vec<GitHubFile>, FakeError>> + Send {
        let r = GitHubEnv::list_contents(self, org, repo, path);
        async move { r }
    }
    fn download_file(&self, download_url: &str) -> impl core::future::Future<Output = Result<Vec<u8>, FakeError>> + Send {
        let r = GitHubEnv::download_file(self, download_url);
        async move { r }
    }
}

// ── FakeNetworkEnv ────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct FakeNetworkEnv {
    responses: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    calls:     Arc<Mutex<Vec<String>>>,
}

impl FakeNetworkEnv {
    /// Register a URL → response body mapping (used by both GET and POST).
    pub fn with_response(self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        self.responses.lock().unwrap().insert(url.into(), body.into());
        self
    }

    /// Assert that `url` was called (panics on failure — intended for tests).
    pub fn assert_called(&self, url: &str) {
        let calls = self.calls.lock().unwrap();
        assert!(
            calls.iter().any(|c| c == url),
            "FakeNetworkEnv: expected call to {url} but got: {calls:?}"
        );
    }

    fn record_and_get(&self, url: &str) -> Result<Vec<u8>, FakeError> {
        self.calls.lock().unwrap().push(url.to_string());
        self.responses
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .ok_or_else(|| fake_err(anyhow!("FakeNetworkEnv: no response registered for {url}")))
    }
}

impl embedded_io::ErrorType for FakeNetworkEnv {
    type Error = FakeError;
}

impl NetworkEnv for FakeNetworkEnv {
    fn post_json(&self, url: &str, _body: &[u8]) -> Result<Vec<u8>, FakeError> {
        self.record_and_get(url)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, FakeError> {
        self.record_and_get(url)
    }
}

impl AsyncNetworkEnv for FakeNetworkEnv {
    fn post_json(&self, url: &str, body: &[u8]) -> impl core::future::Future<Output = Result<Vec<u8>, FakeError>> + Send {
        let r = NetworkEnv::post_json(self, url, body);
        async move { r }
    }
    fn get(&self, url: &str) -> impl core::future::Future<Output = Result<Vec<u8>, FakeError>> + Send {
        let r = NetworkEnv::get(self, url);
        async move { r }
    }
}

// ── FakeAiEnv ─────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct FakeAiEnv {
    default:   Arc<Mutex<(bool, f64)>>,
    overrides: Arc<Mutex<HashMap<String, (bool, f64)>>>,
}

impl FakeAiEnv {
    /// Set the result returned for every path not explicitly overridden.
    pub fn always(self, likely: bool, confidence: f64) -> Self {
        *self.default.lock().unwrap() = (likely, confidence);
        self
    }

    /// Override the result for a specific path.
    pub fn with_result(
        self,
        path: impl Into<String>,
        likely: bool,
        confidence: f64,
    ) -> Self {
        self.overrides
            .lock()
            .unwrap()
            .insert(path.into(), (likely, confidence));
        self
    }
}

impl embedded_io::ErrorType for FakeAiEnv {
    type Error = FakeError;
}

impl AiEnv for FakeAiEnv {
    fn scan(&self, path: &str, _content: &[u8]) -> Result<(bool, f64), FakeError> {
        Ok(self
            .overrides
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(*self.default.lock().unwrap()))
    }
}

impl AsyncAiEnv for FakeAiEnv {
    fn scan(&self, path: &str, content: &[u8]) -> impl core::future::Future<Output = Result<(bool, f64), FakeError>> + Send {
        let r = AiEnv::scan(self, path, content);
        async move { r }
    }
}
