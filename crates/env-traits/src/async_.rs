//! Async versions of all five env-traits.
//!
//! Every method is declared as `fn … -> impl Future<Output = …> + Send`,
//! which is the desugared form of `async fn` in traits.  This spelling is
//! preferred over bare `async fn` in public traits because it explicitly
//! constrains the returned future to be `Send`, making the traits usable
//! with multi-threaded executors such as Tokio without any additional
//! `where` bounds at call sites.
//!
//! The error contract is identical to the sync equivalents: every trait
//! extends [`embedded_io::ErrorType`] so that a single associated `Error`
//! type is shared across all methods.
//!
//! # `walk` return type
//!
//! The sync [`FileEnv::walk`] returns a lazy `Box<dyn Iterator<…>>`.
//! Async iterators (`Stream`) are not yet stable, so [`AsyncFileEnv::walk`]
//! instead returns a `Stream` of `(String, bool)` pairs.  This keeps the API
//! simple and avoids a dependency on any particular async-stream crate.
//!
//! [`FileEnv::walk`]: crate::FileEnv::walk

use core::future::Future;

use alloc::{string::String, vec::Vec};

use embedded_io::ErrorType;
use futures::Stream;

use crate::GitHubFile;

// ── AsyncFileEnv ─────────────────────────────────────────────────────────────

/// Async version of [`FileEnv`](crate::FileEnv).
pub trait AsyncFileEnv: ErrorType + Send + Sync {
    /// Read the full contents of a file.
    fn read_file(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;

    /// Write (create or overwrite) a file, creating parent directories as
    /// needed.
    fn write_file(
        &self,
        path: &str,
        contents: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Return `true` if `path` exists and is a regular file.
    fn file_exists(&self, path: &str) -> impl Future<Output = bool> + Send;

    /// Return `true` if `path` exists and is a directory.
    fn dir_exists(&self, path: &str) -> impl Future<Output = bool> + Send;

    /// Create `path` and all parent directories (like `mkdir -p`).
    fn create_dir_all(&self, path: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Walk the directory tree rooted at `root`.
    ///
    /// Returns all `(path, is_dir)` pairs as a `Stream`.  Entries are
    /// yielded in an unspecified order.
    fn walk(
        &self,
        root: &str,
    ) -> impl Future<
        Output = Result<impl Stream<Item = Result<(String, bool), Self::Error>> + Unpin + Send, Self::Error>,
    > + Send;

    /// Read a single environment variable.  Returns `None` when the variable
    /// is absent or not valid UTF-8.
    fn env_var(&self, key: &str) -> impl Future<Output = Option<String>> + Send;
}

// ── AsyncGitEnv ──────────────────────────────────────────────────────────────

/// Async version of [`GitEnv`](crate::GitEnv).
pub trait AsyncGitEnv: ErrorType + Send + Sync {
    /// Return the absolute path to the repository root
    /// (`git rev-parse --show-toplevel`).
    fn repo_root(&self) -> impl Future<Output = Result<String, Self::Error>> + Send;

    /// Resolve a revision string to a full commit SHA
    /// (`git rev-parse <rev>`).
    fn rev_parse(
        &self,
        repo_root: &str,
        rev: &str,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;

    /// Return the raw bytes of a file at a specific commit
    /// (`git show <commit>:<path>`).
    fn show_file(
        &self,
        repo_root: &str,
        commit: &str,
        path: &str,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;

    /// Return the list of files that differ between `base` and `HEAD`,
    /// filtered to added/copied/modified/renamed regular files.
    fn changed_files(
        &self,
        repo_root: &str,
        base: &str,
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;

    /// Return the merge-base commit SHA between `HEAD` and `origin/<branch>`.
    fn merge_base(
        &self,
        repo_root: &str,
        branch: &str,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;

    /// Fetch a single refspec from a remote.
    fn fetch(
        &self,
        repo_root: &str,
        remote: &str,
        refspec: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Initialise a new git repository in `dir`.
    fn init(&self, dir: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Stage all changes and create a commit with `message`.
    fn add_and_commit(
        &self,
        repo_root: &str,
        message: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Create a new branch and switch to it (`git checkout -b <branch>`).
    fn create_branch(
        &self,
        repo_root: &str,
        branch: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Push a local branch to a remote (`git push <remote> <branch>`).
    fn push(
        &self,
        repo_root: &str,
        remote: &str,
        branch: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

// ── AsyncGitHubEnv ───────────────────────────────────────────────────────────

/// Async version of [`GitHubEnv`](crate::GitHubEnv).
pub trait AsyncGitHubEnv: ErrorType + Send + Sync {
    /// Return the owner login of the repository in the current working
    /// directory.
    fn current_owner(&self) -> impl Future<Output = Result<String, Self::Error>> + Send;

    /// List all repository names inside `org`, up to `limit` results.
    fn list_repos(
        &self,
        org: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;

    /// Recursively list files inside a repository at the given `path` prefix.
    fn list_contents(
        &self,
        org: &str,
        repo: &str,
        path: &str,
    ) -> impl Future<Output = Result<Vec<GitHubFile>, Self::Error>> + Send;

    /// Download the raw bytes of a file by its GitHub download URL.
    fn download_file(
        &self,
        download_url: &str,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;

    /// Create a pull request on GitHub and return its metadata.
    ///
    /// Runs `gh pr create` from within `repo_root`.
    fn create_pr(
        &self,
        repo_root: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> impl Future<Output = Result<crate::PullRequest, Self::Error>> + Send;
}

// ── AsyncNetworkEnv ──────────────────────────────────────────────────────────

/// Async version of [`NetworkEnv`](crate::NetworkEnv).
pub trait AsyncNetworkEnv: ErrorType + Send + Sync {
    /// POST `body` (JSON bytes) to `url` and return the response body.
    ///
    /// Non-2xx responses must be surfaced as `Err`.
    fn post_json(
        &self,
        url: &str,
        body: &[u8],
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;

    /// GET `url` and return the response body.
    ///
    /// Non-2xx responses must be surfaced as `Err`.
    fn get(&self, url: &str) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;
}

// ── AsyncAiEnv ───────────────────────────────────────────────────────────────

/// Async version of [`AiEnv`](crate::AiEnv).
pub trait AsyncAiEnv: ErrorType + Send + Sync {
    /// Inspect file content and return `(likely_ai, confidence)`.
    fn scan(
        &self,
        path: &str,
        content: &[u8],
    ) -> impl Future<Output = Result<(bool, f64), Self::Error>> + Send;
}
