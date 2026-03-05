//! Synchronous wrappers around the async env-traits, powered by
//! [`awaiter_trait::Awaiter`].
//!
//! # Overview
//!
//! Every `AsyncXxxEnv` trait in [`crate::async_`] returns futures that can
//! only be driven by an async runtime.  This module provides [`WithAwaiter`],
//! a thin newtype that pairs **any async env implementation** with **any
//! blocking awaiter** to produce a synchronous env implementation:
//!
//! ```text
//! AsyncFileEnv + Awaiter  ──►  FileEnv
//! AsyncGitEnv  + Awaiter  ──►  GitEnv
//! … and so on for all five trait pairs
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use env_traits::awaiter_bridge::WithAwaiter;
//! use corosensei_awaiter_trait::Stacc;
//!
//! // `my_async_file_env` implements `AsyncFileEnv`.
//! // `Stacc` (from corosensei-awaiter-trait) implements `Awaiter`.
//! let stack = || corosensei::stack::DefaultStack::new(64 * 1024).unwrap();
//! let env = WithAwaiter { awaiter: Stacc { via: &stack }, env: my_async_file_env };
//!
//! // `env` now implements `FileEnv` and can be passed to sync callers.
//! let bytes = env.read_file("README.md").unwrap();
//! ```
//!
//! # Safety note
//!
//! Each sync method stack-allocates the future returned by the async method
//! and pins it with [`core::pin::Pin::new_unchecked`].  This is sound because
//! the pinned reference is never moved after pinning; it is consumed entirely
//! inside the `r#await` call before the stack frame returns.  The same
//! technique is used by `awaiter-trait`'s own `io::Wrap` type.

use core::{future::Future, pin::Pin};

use alloc::{boxed::Box, string::String, vec::Vec};

use awaiter_trait::Awaiter;
use embedded_io::ErrorType;

use crate::{
    async_::{AsyncAiEnv, AsyncFileEnv, AsyncGitEnv, AsyncGitHubEnv, AsyncNetworkEnv},
    AiEnv, FileEnv, GitEnv, GitHubEnv, GitHubFile, NetworkEnv,
};

// ── WithAwaiter ───────────────────────────────────────────────────────────────

/// Pairs an async env implementation with a blocking awaiter to produce a
/// synchronous env implementation.
///
/// `A` must implement [`Awaiter`]; `E` must implement one (or more) of the
/// `AsyncXxxEnv` traits.  `WithAwaiter<A, E>` then implements the
/// corresponding synchronous `XxxEnv` traits.
///
/// See the [module documentation](self) for a usage example.
pub struct WithAwaiter<A, E> {
    /// The blocking awaiter used to drive futures to completion.
    pub awaiter: A,
    /// The async env implementation being wrapped.
    pub env: E,
}

// ─── helper: stack-pin a future and hand it to an Awaiter ────────────────────
//
// The macro avoids repeating the unsafe Pin::new_unchecked dance at every
// call site.  It is module-private.
//
// SAFETY: `$fut` is bound to a local variable in the enclosing scope.  We
// call `Pin::new_unchecked` on a `&mut` to that local.  The resulting `Pin`
// is only used for the duration of the `r#await` call; it never escapes and
// the local is never moved after pinning.
macro_rules! block_on {
    ($awaiter:expr, $fut:expr) => {{
        let mut fut = $fut;
        let pinned: Pin<&mut (dyn Future<Output = _> + '_)> =
            unsafe { Pin::new_unchecked(&mut fut) };
        ($awaiter).r#await(pinned)
    }};
}

// ── ErrorType passthrough ─────────────────────────────────────────────────────

impl<A, E: ErrorType> ErrorType for WithAwaiter<A, E> {
    type Error = E::Error;
}

// ── FileEnv ───────────────────────────────────────────────────────────────────

impl<A: Awaiter, E: AsyncFileEnv> FileEnv for WithAwaiter<A, E>
where
    A: Send + Sync,
{
    fn read_file(&self, path: &str) -> Result<Vec<u8>, E::Error> {
        block_on!(self.awaiter, self.env.read_file(path))
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), E::Error> {
        block_on!(self.awaiter, self.env.write_file(path, contents))
    }

    fn file_exists(&self, path: &str) -> bool {
        block_on!(self.awaiter, self.env.file_exists(path))
    }

    fn dir_exists(&self, path: &str) -> bool {
        block_on!(self.awaiter, self.env.dir_exists(path))
    }

    fn create_dir_all(&self, path: &str) -> Result<(), E::Error> {
        block_on!(self.awaiter, self.env.create_dir_all(path))
    }

    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), E::Error>> + '_>, E::Error> {
        // The async walk already collected the full listing into a Vec.
        let entries = block_on!(self.awaiter, self.env.walk(root))?;
        Ok(Box::new(core::iter::from_fn(move || {
            block_on!(self.awaiter, entries.next()).transpose()
        })))
    }

    fn env_var(&self, key: &str) -> Option<String> {
        block_on!(self.awaiter, self.env.env_var(key))
    }
}

// ── GitEnv ────────────────────────────────────────────────────────────────────

impl<A: Awaiter, E: AsyncGitEnv> GitEnv for WithAwaiter<A, E>
where
    A: Send + Sync,
{
    fn repo_root(&self) -> Result<String, E::Error> {
        block_on!(self.awaiter, self.env.repo_root())
    }

    fn rev_parse(&self, repo_root: &str, rev: &str) -> Result<String, E::Error> {
        block_on!(self.awaiter, self.env.rev_parse(repo_root, rev))
    }

    fn show_file(&self, repo_root: &str, commit: &str, path: &str) -> Result<Vec<u8>, E::Error> {
        block_on!(self.awaiter, self.env.show_file(repo_root, commit, path))
    }

    fn changed_files(&self, repo_root: &str, base: &str) -> Result<Vec<String>, E::Error> {
        block_on!(self.awaiter, self.env.changed_files(repo_root, base))
    }

    fn merge_base(&self, repo_root: &str, branch: &str) -> Result<String, E::Error> {
        block_on!(self.awaiter, self.env.merge_base(repo_root, branch))
    }

    fn fetch(&self, repo_root: &str, remote: &str, refspec: &str) -> Result<(), E::Error> {
        block_on!(self.awaiter, self.env.fetch(repo_root, remote, refspec))
    }

    fn init(&self, dir: &str) -> Result<(), E::Error> {
        block_on!(self.awaiter, self.env.init(dir))
    }

    fn add_and_commit(&self, repo_root: &str, message: &str) -> Result<(), E::Error> {
        block_on!(self.awaiter, self.env.add_and_commit(repo_root, message))
    }
}

// ── GitHubEnv ─────────────────────────────────────────────────────────────────

impl<A: Awaiter, E: AsyncGitHubEnv> GitHubEnv for WithAwaiter<A, E>
where
    A: Send + Sync,
{
    fn current_owner(&self) -> Result<String, E::Error> {
        block_on!(self.awaiter, self.env.current_owner())
    }

    fn list_repos(&self, org: &str, limit: usize) -> Result<Vec<String>, E::Error> {
        block_on!(self.awaiter, self.env.list_repos(org, limit))
    }

    fn list_contents(
        &self,
        org: &str,
        repo: &str,
        path: &str,
    ) -> Result<Vec<GitHubFile>, E::Error> {
        block_on!(self.awaiter, self.env.list_contents(org, repo, path))
    }

    fn download_file(&self, download_url: &str) -> Result<Vec<u8>, E::Error> {
        block_on!(self.awaiter, self.env.download_file(download_url))
    }
}

// ── NetworkEnv ────────────────────────────────────────────────────────────────

impl<A: Awaiter, E: AsyncNetworkEnv> NetworkEnv for WithAwaiter<A, E>
where
    A: Send + Sync,
{
    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>, E::Error> {
        block_on!(self.awaiter, self.env.post_json(url, body))
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, E::Error> {
        block_on!(self.awaiter, self.env.get(url))
    }
}

// ── AiEnv ─────────────────────────────────────────────────────────────────────

impl<A: Awaiter, E: AsyncAiEnv> AiEnv for WithAwaiter<A, E>
where
    A: Send + Sync,
{
    fn scan(&self, path: &str, content: &[u8]) -> Result<(bool, f64), E::Error> {
        block_on!(self.awaiter, self.env.scan(path, content))
    }
}
