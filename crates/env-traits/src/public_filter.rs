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
//!
//! `walk` results from the inner env that do *not* begin with the `"public/"`
//! component are silently filtered out.  `env_var` is forwarded unchanged.

use alloc::{boxed::Box, string::String, vec::Vec};
use core::future::Future;

use embedded_io::ErrorType;
use futures::Stream;

use crate::{AsyncFileEnv, FileEnv};

// ── path helpers ─────────────────────────────────────────────────────────────

/// Map an outer path (no `public` component) to its inner counterpart.
///
/// ```text
/// ""            → "public"
/// "foo"         → "public/foo"
/// "foo/bar.txt" → "public/foo/bar.txt"
/// ```
fn to_inner(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        String::from("public")
    } else {
        let mut out = String::with_capacity("public/".len() + path.len());
        out.push_str("public/");
        out.push_str(path);
        out
    }
}

/// Strip the leading `public` component from an inner path, returning the
/// outer path.  Returns `None` when the path does not start with `public`.
///
/// ```text
/// "public"             → Some("")
/// "public/foo"         → Some("foo")
/// "public/foo/bar.txt" → Some("foo/bar.txt")
/// "other/foo.txt"      → None
/// ```
fn to_outer(path: &str) -> Option<String> {
    if path == "public" {
        return Some(String::new());
    }
    path.strip_prefix("public/").map(String::from)
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
        self.inner.read_file(&to_inner(path))
    }

    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), Self::Error> {
        self.inner.write_file(&to_inner(path), contents)
    }

    fn file_exists(&self, path: &str) -> bool {
        self.inner.file_exists(&to_inner(path))
    }

    fn dir_exists(&self, path: &str) -> bool {
        self.inner.dir_exists(&to_inner(path))
    }

    fn create_dir_all(&self, path: &str) -> Result<(), Self::Error> {
        self.inner.create_dir_all(&to_inner(path))
    }

    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), Self::Error>> + Send + '_>, Self::Error>
    {
        let inner_root = to_inner(root);
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
        let inner_path = to_inner(path);
        async move { self.inner.read_file(&inner_path).await }
    }

    fn write_file(
        &self,
        path: &str,
        contents: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let inner_path = to_inner(path);
        // Clone so the owned bytes can be held across the await point without
        // tying the future's lifetime to the caller's `contents` slice.
        let contents = contents.to_vec();
        async move { self.inner.write_file(&inner_path, &contents).await }
    }

    fn file_exists(&self, path: &str) -> impl Future<Output = bool> + Send {
        let inner_path = to_inner(path);
        async move { self.inner.file_exists(&inner_path).await }
    }

    fn dir_exists(&self, path: &str) -> impl Future<Output = bool> + Send {
        let inner_path = to_inner(path);
        async move { self.inner.dir_exists(&inner_path).await }
    }

    fn create_dir_all(&self, path: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let inner_path = to_inner(path);
        async move { self.inner.create_dir_all(&inner_path).await }
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
        let inner_root = to_inner(root);
        async move {
            use futures::StreamExt as _;
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
