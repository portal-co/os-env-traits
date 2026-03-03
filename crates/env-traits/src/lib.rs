// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
//! Pluggable environment traits.
//!
//! Every external operation performed by the tools in this workspace is
//! expressed as one of five traits defined here.  Production code receives
//! real implementations from `env-real`; tests receive in-memory fakes from
//! `env-fake`.  No crate below this one is allowed to call `std::fs`,
//! `std::process::Command`, or `reqwest` directly.

use std::path::{Path, PathBuf};

pub use anyhow::{Error, Result};

// ── FileEnv ──────────────────────────────────────────────────────────────────

/// All local filesystem and environment-variable operations.
pub trait FileEnv: Send + Sync {
    /// Read the full contents of a file.
    fn read_file(&self, path: &Path) -> Result<Vec<u8>>;

    /// Write (create or overwrite) a file, creating parent directories as
    /// needed.
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()>;

    /// Return `true` if `path` exists and is a regular file.
    fn file_exists(&self, path: &Path) -> bool;

    /// Return `true` if `path` exists and is a directory.
    fn dir_exists(&self, path: &Path) -> bool;

    /// Create `path` and all parent directories (like `mkdir -p`).
    fn create_dir_all(&self, path: &Path) -> Result<()>;

    /// Walk the directory tree rooted at `root`.
    ///
    /// Yields `(absolute_path, is_dir)` pairs in an unspecified order.
    /// Returning an `Err` from the iterator aborts the walk.
    fn walk(
        &self,
        root: &Path,
    ) -> Result<Box<dyn Iterator<Item = Result<(PathBuf, bool)>> + '_>>;

    /// Read a single environment variable.  Returns `None` when the variable
    /// is absent or not valid UTF-8 (mirrors `std::env::var` semantics).
    fn env_var(&self, key: &str) -> Option<String>;
}

// ── GitEnv ───────────────────────────────────────────────────────────────────

/// Pure-git operations (no GitHub API implied).
pub trait GitEnv: Send + Sync {
    /// Return the absolute path to the repository root
    /// (`git rev-parse --show-toplevel`).
    fn repo_root(&self) -> Result<PathBuf>;

    /// Resolve a revision string to a full commit SHA
    /// (`git rev-parse <rev>`).
    fn rev_parse(&self, repo_root: &Path, rev: &str) -> Result<String>;

    /// Return the raw bytes of a file at a specific commit
    /// (`git show <commit>:<path>`).
    ///
    /// Returns `Err` when the path does not exist in that commit tree.
    fn show_file(&self, repo_root: &Path, commit: &str, path: &str)
        -> Result<Vec<u8>>;

    /// Return the list of files that differ between `base` and `HEAD`,
    /// filtered to added/copied/modified/renamed regular files
    /// (`git diff --name-only --diff-filter=ACMR <base> HEAD`).
    fn changed_files(&self, repo_root: &Path, base: &str) -> Result<Vec<String>>;

    /// Return the merge-base commit SHA between `HEAD` and
    /// `origin/<branch>` (`git merge-base HEAD origin/<branch>`).
    fn merge_base(&self, repo_root: &Path, branch: &str) -> Result<String>;

    /// Fetch a single refspec from a remote
    /// (`git fetch --no-tags <remote> <refspec>`).
    fn fetch(&self, repo_root: &Path, remote: &str, refspec: &str) -> Result<()>;

    /// Initialise a new git repository in `dir` (`git init`).
    fn init(&self, dir: &Path) -> Result<()>;

    /// Stage all changes and create a commit with `message`
    /// (`git add -A && git commit -m <message>`).
    fn add_and_commit(&self, repo_root: &Path, message: &str) -> Result<()>;
}

// ── GitHubEnv ────────────────────────────────────────────────────────────────

/// GitHub API / `gh` CLI operations.
pub trait GitHubEnv: Send + Sync {
    /// Return the owner (org or user) login of the repository in the current
    /// working directory (`gh repo view --json owner --jq .owner.login`).
    fn current_owner(&self) -> Result<String>;

    /// List all repository names inside `org`, up to `limit` results
    /// (`gh repo list <org> --limit <N> --json name --jq .[].name`).
    fn list_repos(&self, org: &str, limit: usize) -> Result<Vec<String>>;

    /// Recursively list files inside a repository at the given `path` prefix.
    ///
    /// Uses the GitHub Contents API (via `gh api`) and recurses into
    /// sub-directories automatically.  Returns a flat list of all files
    /// (not directories) found beneath `path`.
    fn list_contents(
        &self,
        org: &str,
        repo: &str,
        path: &str,
    ) -> Result<Vec<GitHubFile>>;

    /// Download the raw bytes of a file by its GitHub download URL.
    ///
    /// This method lives on `GitHubEnv` (rather than `NetworkEnv`) because
    /// GitHub downloads require authentication headers and may be proxied
    /// through the `gh` CLI in restricted environments.  Fakes return
    /// pre-seeded content keyed by path, not by arbitrary URL.
    fn download_file(&self, download_url: &str) -> Result<Vec<u8>>;
}

/// A file (or directory entry) returned by the GitHub Contents API.
#[derive(Debug, Clone)]
pub struct GitHubFile {
    pub name: String,
    pub path: String,
    /// `"file"` or `"dir"`.
    pub kind: String,
    /// Present for `"file"` entries; absent for `"dir"`.
    pub download_url: Option<String>,
}

// ── NetworkEnv ───────────────────────────────────────────────────────────────

/// Generic HTTP operations.
///
/// Kept separate from `GitHubEnv` so that the AI scanner can use it without
/// depending on any GitHub concepts.
pub trait NetworkEnv: Send + Sync {
    /// POST `body` (JSON bytes) to `url` and return the response body.
    ///
    /// Non-2xx responses must be surfaced as `Err`.
    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>>;

    /// GET `url` and return the response body.
    ///
    /// Non-2xx responses must be surfaced as `Err`.
    fn get(&self, url: &str) -> Result<Vec<u8>>;
}

// ── AiEnv ────────────────────────────────────────────────────────────────────

/// AI-content detection.
///
/// Wraps the whole scanning concern so that `check-ai-key` treats AI detection
/// as a single swappable dependency rather than wiring `NetworkEnv` itself.
pub trait AiEnv: Send + Sync {
    /// Inspect file content and return `(likely_ai, confidence)`.
    ///
    /// `confidence` is in `[0.0, 1.0]`.  An `Err` means the scan itself
    /// failed; a `(false, 0.0)` result means the file was scanned and not flagged.
    fn scan(&self, path: &Path, content: &[u8]) -> Result<(bool, f64)>;
}

/// Blanket impl so `Box<dyn AiEnv>` satisfies `AiEnv`.
impl AiEnv for Box<dyn AiEnv> {
    fn scan(&self, path: &Path, content: &[u8]) -> Result<(bool, f64)> {
        (**self).scan(path, content)
    }
}

