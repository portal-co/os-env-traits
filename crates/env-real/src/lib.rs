// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
//! Real (production) implementations of the five env-traits.

use std::{fmt, fs, path::Path, process::Command};

use anyhow::{anyhow, Context};
use env_traits::{
    async_::{AsyncAiEnv, AsyncFileEnv, AsyncGitEnv, AsyncGitHubEnv, AsyncNetworkEnv},
    AiEnv, FileEnv, GitEnv, GitHubEnv, GitHubFile, NetworkEnv,
};
use futures::{stream, Stream};
use serde::Deserialize;
use walkdir::WalkDir;

// ── RealError ─────────────────────────────────────────────────────────────────

/// Opaque error type used by all real env implementations.
///
/// Wraps an [`anyhow::Error`] so that `anyhow`'s ergonomic context-building
/// can be used internally while satisfying [`embedded_io::Error`].
#[derive(Debug)]
pub struct RealError(anyhow::Error);

impl fmt::Display for RealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RealError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl embedded_io::Error for RealError {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

fn real_err(e: anyhow::Error) -> RealError {
    RealError(e)
}

// ── OsFileEnv ─────────────────────────────────────────────────────────────────

/// [`FileEnv`] backed by the real OS filesystem and `std::env`.
#[derive(Default, Clone, Copy)]
pub struct OsFileEnv;

impl embedded_io::ErrorType for OsFileEnv {
    type Error = RealError;
}

impl FileEnv for OsFileEnv {
    fn read_file(&self, path: &str) -> Result<Vec<u8>, RealError> {
        fs::read(path)
            .with_context(|| format!("read_file: {path}"))
            .map_err(real_err)
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), RealError> {
        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("write_file: create_dir_all {}", parent.display()))
                .map_err(real_err)?;
        }
        fs::write(p, contents)
            .with_context(|| format!("write_file: {path}"))
            .map_err(real_err)
    }

    fn file_exists(&self, path: &str) -> bool {
        Path::new(path).is_file()
    }

    fn dir_exists(&self, path: &str) -> bool {
        Path::new(path).is_dir()
    }

    fn create_dir_all(&self, path: &str) -> Result<(), RealError> {
        fs::create_dir_all(path)
            .with_context(|| format!("create_dir_all: {path}"))
            .map_err(real_err)
    }

    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), RealError>> + Send + '_>, RealError> {
        let iter = WalkDir::new(root).min_depth(1).into_iter().map(|entry| {
            let e = entry
                .with_context(|| "walkdir entry error")
                .map_err(real_err)?;
            let is_dir = e.file_type().is_dir();
            let path = e.path().to_string_lossy().into_owned();
            Ok((path, is_dir))
        });
        Ok(Box::new(iter))
    }

    fn env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

impl AsyncFileEnv for OsFileEnv {
    fn read_file(
        &self,
        path: &str,
    ) -> impl core::future::Future<Output = Result<Vec<u8>, RealError>> + Send {
        let r = FileEnv::read_file(self, path);
        async move { r }
    }
    fn write_file(
        &self,
        path: &str,
        contents: &[u8],
    ) -> impl core::future::Future<Output = Result<(), RealError>> + Send {
        let r = FileEnv::write_file(self, path, contents);
        async move { r }
    }
    fn file_exists(&self, path: &str) -> impl core::future::Future<Output = bool> + Send {
        let r = FileEnv::file_exists(self, path);
        async move { r }
    }
    fn dir_exists(&self, path: &str) -> impl core::future::Future<Output = bool> + Send {
        let r = FileEnv::dir_exists(self, path);
        async move { r }
    }
    fn create_dir_all(
        &self,
        path: &str,
    ) -> impl core::future::Future<Output = Result<(), RealError>> + Send {
        let r = FileEnv::create_dir_all(self, path);
        async move { r }
    }
    fn walk(
        &self,
        root: &str,
    ) -> impl core::future::Future<
        Output = Result<
            impl Stream<Item = Result<(String, bool), RealError>> + Unpin + Send,
            RealError,
        >,
    > + Send {
        let r = FileEnv::walk(self, root).and_then(|iter| Ok(stream::iter(iter)));
        async move { r }
    }
    fn env_var(&self, key: &str) -> impl core::future::Future<Output = Option<String>> + Send {
        let r = FileEnv::env_var(self, key);
        async move { r }
    }
}

// ── ProcessGitEnv ─────────────────────────────────────────────────────────────

/// [`GitEnv`] backed by the `git` binary on `$PATH`.
///
/// Each method runs `git <args>` in the given `repo_root` directory and
/// returns `Err` on a non-zero exit code.
#[derive(Default, Clone, Copy)]
pub struct ProcessGitEnv;

impl ProcessGitEnv {
    fn run(&self, repo_root: &str, args: &[&str]) -> Result<String, RealError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("git {}", args.join(" ")))
            .map_err(real_err)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(real_err(anyhow!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

impl embedded_io::ErrorType for ProcessGitEnv {
    type Error = RealError;
}

impl GitEnv for ProcessGitEnv {
    fn repo_root(&self) -> Result<String, RealError> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("git rev-parse --show-toplevel")
            .map_err(real_err)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(real_err(anyhow!(
                "git rev-parse --show-toplevel: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn rev_parse(&self, repo_root: &str, rev: &str) -> Result<String, RealError> {
        self.run(repo_root, &["rev-parse", rev])
    }

    fn show_file(&self, repo_root: &str, commit: &str, path: &str) -> Result<Vec<u8>, RealError> {
        let r#ref = format!("{commit}:{path}");
        let output = Command::new("git")
            .args(["show", &r#ref])
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("git show {ref}"))
            .map_err(real_err)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(real_err(anyhow!(
                "git show {ref}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn changed_files(&self, repo_root: &str, base: &str) -> Result<Vec<String>, RealError> {
        let out = self.run(
            repo_root,
            &["diff", "--name-only", "--diff-filter=ACMR", base, "HEAD"],
        )?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    fn merge_base(&self, repo_root: &str, branch: &str) -> Result<String, RealError> {
        let remote_ref = format!("origin/{branch}");
        self.run(repo_root, &["merge-base", "HEAD", &remote_ref])
    }

    fn fetch(&self, repo_root: &str, remote: &str, refspec: &str) -> Result<(), RealError> {
        self.run(repo_root, &["fetch", "--no-tags", remote, refspec])?;
        Ok(())
    }

    fn init(&self, dir: &str) -> Result<(), RealError> {
        let output = Command::new("git")
            .arg("init")
            .current_dir(Path::new(dir))
            .output()
            .context("git init")
            .map_err(real_err)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(real_err(anyhow!(
                "git init: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn add_and_commit(&self, repo_root: &str, message: &str) -> Result<(), RealError> {
        self.run(repo_root, &["add", "-A"])?;
        self.run(repo_root, &["commit", "-m", message])?;
        Ok(())
    }
}

impl AsyncGitEnv for ProcessGitEnv {
    fn repo_root(&self) -> impl core::future::Future<Output = Result<String, RealError>> + Send {
        let r = GitEnv::repo_root(self);
        async move { r }
    }
    fn rev_parse(
        &self,
        repo_root: &str,
        rev: &str,
    ) -> impl core::future::Future<Output = Result<String, RealError>> + Send {
        let r = GitEnv::rev_parse(self, repo_root, rev);
        async move { r }
    }
    fn show_file(
        &self,
        repo_root: &str,
        commit: &str,
        path: &str,
    ) -> impl core::future::Future<Output = Result<Vec<u8>, RealError>> + Send {
        let r = GitEnv::show_file(self, repo_root, commit, path);
        async move { r }
    }
    fn changed_files(
        &self,
        repo_root: &str,
        base: &str,
    ) -> impl core::future::Future<Output = Result<Vec<String>, RealError>> + Send {
        let r = GitEnv::changed_files(self, repo_root, base);
        async move { r }
    }
    fn merge_base(
        &self,
        repo_root: &str,
        branch: &str,
    ) -> impl core::future::Future<Output = Result<String, RealError>> + Send {
        let r = GitEnv::merge_base(self, repo_root, branch);
        async move { r }
    }
    fn fetch(
        &self,
        repo_root: &str,
        remote: &str,
        refspec: &str,
    ) -> impl core::future::Future<Output = Result<(), RealError>> + Send {
        let r = GitEnv::fetch(self, repo_root, remote, refspec);
        async move { r }
    }
    fn init(&self, dir: &str) -> impl core::future::Future<Output = Result<(), RealError>> + Send {
        let r = GitEnv::init(self, dir);
        async move { r }
    }
    fn add_and_commit(
        &self,
        repo_root: &str,
        message: &str,
    ) -> impl core::future::Future<Output = Result<(), RealError>> + Send {
        let r = GitEnv::add_and_commit(self, repo_root, message);
        async move { r }
    }
}

// ── GhCliGitHubEnv ────────────────────────────────────────────────────────────

/// [`GitHubEnv`] backed by the `gh` CLI on `$PATH`.
///
/// All GitHub API calls are routed through `gh api …` so that authentication
/// tokens are managed by `gh auth` — no token handling in this crate.
#[derive(Default, Clone, Copy)]
pub struct GhCliGitHubEnv;

#[derive(Deserialize)]
struct GhContentsEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    kind: String,
    download_url: Option<String>,
}

impl GhCliGitHubEnv {
    fn gh(&self, args: &[&str]) -> Result<Vec<u8>, RealError> {
        let output = Command::new("gh")
            .args(args)
            .output()
            .with_context(|| format!("gh {}", args.join(" ")))
            .map_err(real_err)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(real_err(anyhow!(
                "gh {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

impl embedded_io::ErrorType for GhCliGitHubEnv {
    type Error = RealError;
}

impl GitHubEnv for GhCliGitHubEnv {
    fn current_owner(&self) -> Result<String, RealError> {
        let raw = self.gh(&["repo", "view", "--json", "owner", "--jq", ".owner.login"])?;
        Ok(String::from_utf8_lossy(&raw).trim().to_string())
    }

    fn list_repos(&self, org: &str, limit: usize) -> Result<Vec<String>, RealError> {
        let limit_s = limit.to_string();
        let raw = self.gh(&[
            "repo", "list", org, "--limit", &limit_s, "--json", "name", "--jq", ".[].name",
        ])?;
        Ok(String::from_utf8_lossy(&raw)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    fn list_contents(
        &self,
        org: &str,
        repo: &str,
        path: &str,
    ) -> Result<Vec<GitHubFile>, RealError> {
        let url = format!("https://api.github.com/repos/{org}/{repo}/contents/{path}");
        let raw = self.gh(&["api", &url, "--paginate"])?;
        let entries: Vec<GhContentsEntry> = serde_json::from_slice(&raw)
            .with_context(|| format!("list_contents: parse JSON for {org}/{repo}/{path}"))
            .map_err(real_err)?;

        let mut result = Vec::new();
        for entry in entries {
            if entry.kind == "dir" {
                let sub = GitHubEnv::list_contents(self, org, repo, &entry.path)?;
                result.extend(sub);
            } else {
                result.push(GitHubFile {
                    name: entry.name,
                    path: entry.path,
                    kind: entry.kind,
                    download_url: entry.download_url,
                });
            }
        }
        Ok(result)
    }

    fn download_file(&self, download_url: &str) -> Result<Vec<u8>, RealError> {
        self.gh(&["api", download_url])
    }
}

impl AsyncGitHubEnv for GhCliGitHubEnv {
    fn current_owner(
        &self,
    ) -> impl core::future::Future<Output = Result<String, RealError>> + Send {
        let r = GitHubEnv::current_owner(self);
        async move { r }
    }
    fn list_repos(
        &self,
        org: &str,
        limit: usize,
    ) -> impl core::future::Future<Output = Result<Vec<String>, RealError>> + Send {
        let r = GitHubEnv::list_repos(self, org, limit);
        async move { r }
    }
    fn list_contents(
        &self,
        org: &str,
        repo: &str,
        path: &str,
    ) -> impl core::future::Future<Output = Result<Vec<GitHubFile>, RealError>> + Send {
        let r = GitHubEnv::list_contents(self, org, repo, path);
        async move { r }
    }
    fn download_file(
        &self,
        download_url: &str,
    ) -> impl core::future::Future<Output = Result<Vec<u8>, RealError>> + Send {
        let r = GitHubEnv::download_file(self, download_url);
        async move { r }
    }
}

// ── ReqwestNetworkEnv ─────────────────────────────────────────────────────────

/// [`NetworkEnv`] backed by `reqwest` blocking client.
#[derive(Default, Clone)]
pub struct ReqwestNetworkEnv;

impl embedded_io::ErrorType for ReqwestNetworkEnv {
    type Error = RealError;
}

impl NetworkEnv for ReqwestNetworkEnv {
    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>, RealError> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_vec())
            .send()
            .with_context(|| format!("POST {url}"))
            .map_err(real_err)?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .with_context(|| format!("POST {url}: read body"))
            .map_err(real_err)?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(real_err(anyhow!("POST {url}: server returned {status}")))
        }
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, RealError> {
        let resp = reqwest::blocking::get(url)
            .with_context(|| format!("GET {url}"))
            .map_err(real_err)?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .with_context(|| format!("GET {url}: read body"))
            .map_err(real_err)?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(real_err(anyhow!("GET {url}: server returned {status}")))
        }
    }
}

impl AsyncNetworkEnv for ReqwestNetworkEnv {
    fn post_json(
        &self,
        url: &str,
        body: &[u8],
    ) -> impl core::future::Future<Output = Result<Vec<u8>, RealError>> + Send {
        let r = NetworkEnv::post_json(self, url, body);
        async move { r }
    }
    fn get(
        &self,
        url: &str,
    ) -> impl core::future::Future<Output = Result<Vec<u8>, RealError>> + Send {
        let r = NetworkEnv::get(self, url);
        async move { r }
    }
}

// ── OsAiEnv (placeholder) ─────────────────────────────────────────────────────
// There is no real AI implementation in this crate (it lives in a higher-level
// crate that wires NetworkEnv to an AI service).  The type is provided so that
// the module graph is complete; it always returns `(false, 0.0)`.

/// Placeholder [`AiEnv`] that always reports content as not AI-generated.
///
/// A real implementation would delegate to an AI detection service via
/// [`NetworkEnv`].
#[derive(Default, Clone, Copy)]
pub struct NoopAiEnv;

impl embedded_io::ErrorType for NoopAiEnv {
    type Error = RealError;
}

impl AiEnv for NoopAiEnv {
    fn scan(&self, _path: &str, _content: &[u8]) -> Result<(bool, f64), RealError> {
        Ok((false, 0.0))
    }
}

impl AsyncAiEnv for NoopAiEnv {
    fn scan(
        &self,
        path: &str,
        content: &[u8],
    ) -> impl core::future::Future<Output = Result<(bool, f64), RealError>> + Send {
        let r = AiEnv::scan(self, path, content);
        async move { r }
    }
}
