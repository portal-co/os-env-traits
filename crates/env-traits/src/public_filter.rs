//! [`PublicFileEnv`] and [`AsyncPublicFileEnv`]: `FileEnv` / `AsyncFileEnv`
//! wrappers that expose only the subtree at the `public` path component,
//! silently hiding everything else and stripping that component from all
//! presented paths.
//!
//! # Path mapping
//!
//! | Outer path (presented to callers) | Inner path (forwarded to the wrapped env) |
//! |-----------------------------------|-------------------------------------------|
//! | `""`                              | `"public"`                                |
//! | `"foo/bar.txt"`                   | `"public/foo/bar.txt"`                    |
//! | `"apps/site/logo.png"`            | `"apps/site/public/logo.png"`             |
//!
//! The wrapper infers where to insert `public` by combining the requested
//! outer path with wrapped-env structure checks (`file_exists` / `dir_exists`).
//! `walk` strips the first `public` component from returned paths, hiding
//! non-public items. `env_var` is forwarded unchanged.

use alloc::{boxed::Box, string::String, vec, vec::Vec};
use core::future::Future;

use embedded_io::ErrorType;
use futures::Stream;

use crate::{AsyncFileEnv, FileEnv};

// ── path helpers ─────────────────────────────────────────────────────────────

fn split_components(path: &str) -> Vec<&str> {
    path.split('/').filter(|c| !c.is_empty()).collect()
}

fn join_components(parts: &[&str]) -> String {
    if parts.is_empty() {
        return String::new();
    }

    let cap = parts.iter().map(|p| p.len()).sum::<usize>() + parts.len().saturating_sub(1);
    let mut out = String::with_capacity(cap);
    for (idx, p) in parts.iter().enumerate() {
        if idx > 0 {
            out.push('/');
        }
        out.push_str(p);
    }
    out
}

fn with_public_inserted(parts: &[&str], insert_at: usize) -> String {
    let mut out_parts: Vec<&str> = Vec::with_capacity(parts.len() + 1);
    out_parts.extend_from_slice(&parts[..insert_at]);
    out_parts.push("public");
    out_parts.extend_from_slice(&parts[insert_at..]);
    join_components(&out_parts)
}

fn candidate_inner_paths(path: &str) -> Vec<(usize, String)> {
    let parts = split_components(path);
    if let Some(idx) = parts.iter().position(|c| *c == "public") {
        return vec![(idx, join_components(&parts))];
    }

    let mut out = Vec::with_capacity(parts.len() + 1);
    for idx in 0..=parts.len() {
        out.push((idx, with_public_inserted(&parts, idx)));
    }
    out
}

fn public_dir_for_insertion(path: &str, insert_at: usize) -> String {
    let parts = split_components(path);
    if parts.iter().any(|c| *c == "public") {
        return join_components(&parts[..=insert_at]);
    }

    let mut out_parts: Vec<&str> = Vec::with_capacity(insert_at + 1);
    out_parts.extend_from_slice(&parts[..insert_at]);
    out_parts.push("public");
    join_components(&out_parts)
}

fn parent_path(path: &str) -> Option<&str> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return None;
    }
    path.rsplit_once('/').map(|(p, _)| p)
}

fn to_outer(path: &str) -> Option<String> {
    let parts = split_components(path);
    let remove_at = parts.iter().position(|c| *c == "public")?;
    let mut out_parts: Vec<&str> = Vec::with_capacity(parts.len().saturating_sub(1));
    out_parts.extend_from_slice(&parts[..remove_at]);
    out_parts.extend_from_slice(&parts[remove_at + 1..]);
    Some(join_components(&out_parts))
}

fn default_inner(path: &str) -> String {
    with_public_inserted(&split_components(path), 0)
}

fn score_sync_candidate<T: FileEnv>(inner: &T, outer_path: &str, insert_at: usize, candidate: &str) -> i32 {
    let mut score = 0;

    if inner.file_exists(candidate) || inner.dir_exists(candidate) {
        score += 100;
    }

    let public_dir = public_dir_for_insertion(outer_path, insert_at);
    if !public_dir.is_empty() && inner.dir_exists(&public_dir) {
        score += 10;
    }

    if let Some(parent) = parent_path(candidate) {
        if inner.dir_exists(parent) {
            score += 5;
        }
    }

    // Prefer deeper insertions when structure checks tie.
    score + insert_at as i32
}

async fn score_async_candidate<T: AsyncFileEnv>(
    inner: &T,
    outer_path: &str,
    insert_at: usize,
    candidate: &str,
) -> i32 {
    let mut score = 0;

    if inner.file_exists(candidate).await || inner.dir_exists(candidate).await {
        score += 100;
    }

    let public_dir = public_dir_for_insertion(outer_path, insert_at);
    if !public_dir.is_empty() && inner.dir_exists(&public_dir).await {
        score += 10;
    }

    if let Some(parent) = parent_path(candidate) {
        if inner.dir_exists(parent).await {
            score += 5;
        }
    }

    score + insert_at as i32
}

fn choose_sync_path<T: FileEnv>(inner: &T, outer_path: &str) -> String {
    let candidates = candidate_inner_paths(outer_path);
    if candidates.is_empty() {
        return String::from("public");
    }

    let mut best_idx = 0usize;
    let mut best_score = i32::MIN;
    for (idx, (insert_at, candidate)) in candidates.iter().enumerate() {
        let s = score_sync_candidate(inner, outer_path, *insert_at, candidate);
        if s > best_score {
            best_score = s;
            best_idx = idx;
        }
    }

    candidates[best_idx].1.clone()
}

async fn choose_async_path<T: AsyncFileEnv>(inner: &T, outer_path: &str) -> String {
    let candidates = candidate_inner_paths(outer_path);
    if candidates.is_empty() {
        return String::from("public");
    }

    let mut best_idx = 0usize;
    let mut best_score = i32::MIN;
    for (idx, (insert_at, candidate)) in candidates.iter().enumerate() {
        let s = score_async_candidate(inner, outer_path, *insert_at, candidate).await;
        if s > best_score {
            best_score = s;
            best_idx = idx;
        }
    }

    candidates[best_idx].1.clone()
}

// ── PublicFileEnv ─────────────────────────────────────────────────────────────

/// A [`FileEnv`] wrapper that exposes only the `public/` subtree of the inner
/// env, stripping `public/` from every path.
///
/// See the [module documentation](self) for path-mapping details.
pub struct PublicFileEnv<T> {
    inner: T,
}

impl<T> PublicFileEnv<T> {
    /// Wrap `inner`, exposing only its `public/` subtree.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Unwrap and return the inner env.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: FileEnv> ErrorType for PublicFileEnv<T> {
    type Error = T::Error;
}

impl<T: FileEnv> FileEnv for PublicFileEnv<T> {
    fn read_file(&self, path: &str) -> Result<Vec<u8>, Self::Error> {
        let inner_path = choose_sync_path(&self.inner, path);
        self.inner.read_file(&inner_path)
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), Self::Error> {
        let inner_path = choose_sync_path(&self.inner, path);
        self.inner.write_file(&inner_path, contents)
    }

    fn file_exists(&self, path: &str) -> bool {
        candidate_inner_paths(path)
            .iter()
            .any(|(_, c)| self.inner.file_exists(c))
    }

    fn dir_exists(&self, path: &str) -> bool {
        candidate_inner_paths(path)
            .iter()
            .any(|(_, c)| self.inner.dir_exists(c))
    }

    fn create_dir_all(&self, path: &str) -> Result<(), Self::Error> {
        let inner_path = choose_sync_path(&self.inner, path);
        self.inner.create_dir_all(&inner_path)
    }

    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), Self::Error>> + Send + '_>, Self::Error>
    {
        let inner_root = choose_sync_path(&self.inner, root);
        let iter = self.inner.walk(&inner_root)?;
        Ok(Box::new(iter.filter_map(|item| match item {
            Ok((path, is_dir)) => to_outer(&path).map(|outer| Ok((outer, is_dir))),
            Err(e) => Some(Err(e)),
        })))
    }

    fn env_var(&self, key: &str) -> Option<String> {
        self.inner.env_var(key)
    }
}

// ── AsyncPublicFileEnv ────────────────────────────────────────────────────────

/// An [`AsyncFileEnv`] wrapper that exposes only the `public/` subtree of the
/// inner env, stripping `public/` from every path.
///
/// See the [module documentation](self) for path-mapping details.
pub struct AsyncPublicFileEnv<T> {
    inner: T,
}

impl<T> AsyncPublicFileEnv<T> {
    /// Wrap `inner`, exposing only its `public/` subtree.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Unwrap and return the inner env.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: AsyncFileEnv> ErrorType for AsyncPublicFileEnv<T> {
    type Error = T::Error;
}

impl<T: AsyncFileEnv> AsyncFileEnv for AsyncPublicFileEnv<T>
where
    T::Error: Send,
{
    fn read_file(&self, path: &str) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let path = String::from(path);
        async move {
            let inner_path = choose_async_path(&self.inner, &path).await;
            self.inner.read_file(&inner_path).await
        }
    }

    fn write_file(
        &self,
        path: &str,
        contents: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let path = String::from(path);
        // Clone so the owned bytes can be held across the await point without
        // tying the future's lifetime to the caller's `contents` slice.
        let contents = contents.to_vec();
        async move {
            let inner_path = choose_async_path(&self.inner, &path).await;
            self.inner.write_file(&inner_path, &contents).await
        }
    }

    fn file_exists(&self, path: &str) -> impl Future<Output = bool> + Send {
        let path = String::from(path);
        async move {
            for (_, candidate) in candidate_inner_paths(&path) {
                if self.inner.file_exists(&candidate).await {
                    return true;
                }
            }
            false
        }
    }

    fn dir_exists(&self, path: &str) -> impl Future<Output = bool> + Send {
        let path = String::from(path);
        async move {
            for (_, candidate) in candidate_inner_paths(&path) {
                if self.inner.dir_exists(&candidate).await {
                    return true;
                }
            }
            false
        }
    }

    fn create_dir_all(&self, path: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let path = String::from(path);
        async move {
            let inner_path = choose_async_path(&self.inner, &path).await;
            self.inner.create_dir_all(&inner_path).await
        }
    }

    fn walk(
        &self,
        root: &str,
    ) -> impl Future<
        Output = Result<
            impl Stream<Item = Result<(String, bool), Self::Error>> + Unpin + Send,
            Self::Error,
        >,
    > + Send {
        let root = String::from(root);
        async move {
            use futures::StreamExt as _;
            let inner_root = choose_async_path(&self.inner, &root).await;
            let mut stream = self.inner.walk(&inner_root).await?;
            // Collect eagerly so the returned stream has no lifetime dependency
            // on `inner_root` (which is local to this async block).
            let mut items: Vec<Result<(String, bool), T::Error>> = Vec::new();
            while let Some(item) = stream.next().await {
                match item {
                    Ok((path, is_dir)) => {
                        if let Some(outer) = to_outer(&path) {
                            items.push(Ok((outer, is_dir)));
                        }
                    }
                    Err(e) => items.push(Err(e)),
                }
            }
            Ok(futures::stream::iter(items))
        }
    }

    fn env_var(&self, key: &str) -> impl Future<Output = Option<String>> + Send {
        let key = String::from(key);
        async move { self.inner.env_var(&key).await }
    }
}
