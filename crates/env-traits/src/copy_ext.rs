//! Extension traits for copying a path from one env into another.
//!
//! Four combinations are provided, covering every pairing of sync
//! ([`FileEnv`]) and async ([`AsyncFileEnv`]) on each side.  Each trait
//! exposes **two methods**:
//!
//! - `copy_path_to*` — copies a single file.
//! - `copy_dir_to*`  — recursively copies an entire directory tree.
//!
//! | source \ destination   | sync [`FileEnv`]              | async [`AsyncFileEnv`]          |
//! |------------------------|-------------------------------|----------------------------------|
//! | sync [`FileEnv`]       | [`FileEnvCopyExt`]            | [`FileEnvCopyToAsyncExt`]        |
//! | async [`AsyncFileEnv`] | [`AsyncFileEnvCopyToSyncExt`] | [`AsyncFileEnvCopyExt`]          |
//!
//! # Error handling
//!
//! Because the source and destination envs carry **independent** associated
//! `Error` types, every copy method returns [`CopyError<Se, De>`], which
//! distinguishes between a failure on the read side (`CopyError::Read`) and a
//! failure on the write side (`CopyError::Write`).
//!
//! # Example
//!
//! ```ignore
//! use env_traits::copy_ext::FileEnvCopyExt;
//!
//! let src = FakeFileEnv::default().with_file("a/b.txt", b"hello");
//! let dst = FakeFileEnv::default();
//!
//! src.copy_path_to("a/b.txt", &dst, "x/y.txt").unwrap();
//! assert_eq!(dst.read_file("x/y.txt").unwrap(), b"hello");
//!
//! // Directory copy:
//! let src2 = FakeFileEnv::default()
//!     .with_file("src/foo.txt", b"foo")
//!     .with_file("src/sub/bar.txt", b"bar");
//! let dst2 = FakeFileEnv::default();
//! src2.copy_dir_to("src", &dst2, "dst").unwrap();
//! assert_eq!(dst2.read_file("dst/foo.txt").unwrap(), b"foo");
//! assert_eq!(dst2.read_file("dst/sub/bar.txt").unwrap(), b"bar");
//! ```

use alloc::{string::String, vec::Vec};
use core::{error::Error, future::Future};

use crate::{AsyncFileEnv, FileEnv};

// ── path helpers ─────────────────────────────────────────────────────────────

/// Rebase `entry_path` from `src_root` onto `dst_root`.
///
/// Strips the `src_root` prefix (with or without a trailing `/`) from
/// `entry_path` and prepends `dst_root`.
///
/// ```text
/// rebase("a/b",  "a/b/c/d.txt", "x/y")  →  "x/y/c/d.txt"
/// rebase("a/b/", "a/b/c/d.txt", "x/y")  →  "x/y/c/d.txt"
/// rebase("a/b",  "a/b",         "x/y")  →  "x/y"          (root itself)
/// ```
fn rebase(src_root: &str, entry_path: &str, dst_root: &str) -> String {
    // Normalise: treat "a/b" and "a/b/" identically.
    let prefix = src_root.trim_end_matches('/');

    let suffix = if entry_path == prefix {
        // The entry *is* the root directory itself.
        ""
    } else if let Some(rest) = entry_path.strip_prefix(prefix) {
        // `rest` starts with '/' because walk returns full paths rooted at
        // `src_root`.  E.g. "a/b" + "/c/d.txt".
        rest.trim_start_matches('/')
    } else {
        // Shouldn't happen in a well-behaved walk implementation, but fall
        // back to the entry path as-is so we don't silently lose data.
        entry_path
    };

    if suffix.is_empty() {
        String::from(dst_root)
    } else {
        let dst = dst_root.trim_end_matches('/');
        let mut out = String::with_capacity(dst.len() + 1 + suffix.len());
        out.push_str(dst);
        out.push('/');
        out.push_str(suffix);
        out
    }
}

// ── CopyError ────────────────────────────────────────────────────────────────

/// Error returned by all `copy_path_to*` and `copy_dir_to*` methods.
///
/// `Se` is the source env's error type; `De` is the destination env's error
/// type.
#[derive(Debug)]
pub enum CopyError<Se, De> {
    /// The read from the source env failed.
    Read(Se),
    /// The write to the destination env failed.
    Write(De),
}

impl<Se: core::fmt::Display, De: core::fmt::Display> core::fmt::Display for CopyError<Se, De> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CopyError::Read(e) => write!(f, "copy failed on read: {e}"),
            CopyError::Write(e) => write!(f, "copy failed on write: {e}"),
        }
    }
}
impl<Se: Error + 'static, De: Error + 'static> Error for CopyError<Se, De> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CopyError::Read(e) => Some(e),
            CopyError::Write(e) => Some(e),
        }
    }
}
impl<Se: embedded_io::Error + 'static, De: embedded_io::Error + 'static> embedded_io::Error
    for CopyError<Se, De>
{
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            CopyError::Read(e) => e.kind(),
            CopyError::Write(e) => e.kind(),
        }
    }
}

// ── 1. FileEnv → FileEnv (sync → sync) ───────────────────────────────────────

/// Extension methods on [`FileEnv`] that copy a file or directory tree into
/// another [`FileEnv`].
pub trait FileEnvCopyExt: FileEnv {
    /// Read `src_path` from `self` and write its contents to `dst_path` in
    /// `dst`.
    ///
    /// Returns [`CopyError::Read`] if the source read fails, or
    /// [`CopyError::Write`] if the destination write fails.
    fn copy_path_to<D: FileEnv>(
        &self,
        src_path: &str,
        dst: &D,
        dst_path: &str,
    ) -> Result<(), CopyError<Self::Error, D::Error>>;

    /// Recursively copy the directory tree rooted at `src_root` in `self`
    /// into `dst_root` in `dst`.
    ///
    /// For each entry produced by [`FileEnv::walk`]:
    /// - Directories are created in `dst` via [`FileEnv::create_dir_all`].
    /// - Files are read from `self` and written to `dst`.
    ///
    /// The destination path for each entry is computed by replacing the
    /// `src_root` prefix with `dst_root`.
    ///
    /// Returns [`CopyError::Read`] if walking or reading from `self` fails,
    /// or [`CopyError::Write`] if creating a directory or writing a file in
    /// `dst` fails.
    fn copy_dir_to<D: FileEnv>(
        &self,
        src_root: &str,
        dst: &D,
        dst_root: &str,
    ) -> Result<(), CopyError<Self::Error, D::Error>>;
}

impl<S: FileEnv + ?Sized> FileEnvCopyExt for S {
    fn copy_path_to<D: FileEnv>(
        &self,
        src_path: &str,
        dst: &D,
        dst_path: &str,
    ) -> Result<(), CopyError<Self::Error, D::Error>> {
        let contents = self.read_file(src_path).map_err(CopyError::Read)?;
        dst.write_file(dst_path, &contents)
            .map_err(CopyError::Write)
    }

    fn copy_dir_to<D: FileEnv>(
        &self,
        src_root: &str,
        dst: &D,
        dst_root: &str,
    ) -> Result<(), CopyError<Self::Error, D::Error>> {
        let iter = self.walk(src_root).map_err(CopyError::Read)?;
        for entry in iter {
            let (entry_path, is_dir) = entry.map_err(CopyError::Read)?;
            let dst_path = rebase(src_root, &entry_path, dst_root);
            if is_dir {
                dst.create_dir_all(&dst_path).map_err(CopyError::Write)?;
            } else {
                let contents = self.read_file(&entry_path).map_err(CopyError::Read)?;
                dst.write_file(&dst_path, &contents)
                    .map_err(CopyError::Write)?;
            }
        }
        Ok(())
    }
}

// ── 2. FileEnv → AsyncFileEnv (sync → async) ─────────────────────────────────

/// Extension methods on [`FileEnv`] that copy a file or directory tree into
/// an [`AsyncFileEnv`].
pub trait FileEnvCopyToAsyncExt: FileEnv {
    /// Read `src_path` synchronously from `self` and write its contents
    /// asynchronously to `dst_path` in `dst`.
    ///
    /// The future resolves to [`CopyError::Read`] if the synchronous source
    /// read fails, or [`CopyError::Write`] if the asynchronous destination
    /// write fails.
    fn copy_path_to_async<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_path: &'s str,
        dst: &'d D,
        dst_path: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
        Self::Error: Send;

    /// Recursively copy the directory tree rooted at `src_root` in `self`
    /// (synchronous walk + reads) into `dst_root` in `dst` (async writes).
    ///
    /// The entire walk is performed synchronously up-front, then each
    /// destination write is awaited in turn.
    ///
    /// Returns [`CopyError::Read`] if walking or reading from `self` fails,
    /// or [`CopyError::Write`] if creating a directory or writing a file in
    /// `dst` fails.
    fn copy_dir_to_async<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_root: &'s str,
        dst: &'d D,
        dst_root: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
        Self::Error: Send,
        D::Error: Send;
}

impl<S: FileEnv + ?Sized> FileEnvCopyToAsyncExt for S {
    fn copy_path_to_async<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_path: &'s str,
        dst: &'d D,
        dst_path: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
        S::Error: Send,
    {
        let read_result = self.read_file(src_path);
        async move {
            let contents = read_result.map_err(CopyError::Read)?;
            dst.write_file(dst_path, &contents)
                .await
                .map_err(CopyError::Write)
        }
    }

    fn copy_dir_to_async<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_root: &'s str,
        dst: &'d D,
        dst_root: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
        S::Error: Send,
        D::Error: Send,
    {
        // Collect the entire walk synchronously before entering the async
        // block; this avoids holding the iterator (which borrows `self`)
        // across await points.
        let walked: Result<Vec<(String, bool)>, _> = (|| {
            let iter = self.walk(src_root).map_err(CopyError::Read)?;
            iter.map(|r| r.map_err(CopyError::Read))
                .collect::<Result<Vec<_>, _>>()
        })();
        // Also collect the file contents synchronously, keyed by rebased dst
        // path, so nothing from `self` is held across awaits.
        let entries: Result<Vec<(String, Option<Vec<u8>>)>, CopyError<S::Error, D::Error>> =
            walked.and_then(|entries| {
                entries
                    .into_iter()
                    .map(|(entry_path, is_dir)| {
                        let dst_path = rebase(src_root, &entry_path, dst_root);
                        if is_dir {
                            Ok((dst_path, None))
                        } else {
                            let contents =
                                self.read_file(&entry_path).map_err(CopyError::Read)?;
                            Ok((dst_path, Some(contents)))
                        }
                    })
                    .collect()
            });
        async move {
            for (dst_path, contents_opt) in entries? {
                match contents_opt {
                    None => {
                        dst.create_dir_all(&dst_path)
                            .await
                            .map_err(CopyError::Write)?;
                    }
                    Some(contents) => {
                        dst.write_file(&dst_path, &contents)
                            .await
                            .map_err(CopyError::Write)?;
                    }
                }
            }
            Ok(())
        }
    }
}

// ── 3. AsyncFileEnv → AsyncFileEnv (async → async) ───────────────────────────

/// Extension methods on [`AsyncFileEnv`] that copy a file or directory tree
/// into another [`AsyncFileEnv`].
pub trait AsyncFileEnvCopyExt: AsyncFileEnv {
    /// Read `src_path` asynchronously from `self` and write its contents
    /// asynchronously to `dst_path` in `dst`.
    ///
    /// Returns [`CopyError::Read`] if the source read fails, or
    /// [`CopyError::Write`] if the destination write fails.
    fn copy_path_to<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_path: &'s str,
        dst: &'d D,
        dst_path: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's;

    /// Recursively copy the directory tree rooted at `src_root` in `self`
    /// into `dst_root` in `dst`, fully async on both sides.
    ///
    /// The walk result is awaited first to obtain the full listing, then each
    /// entry is read (if a file) and written in turn.
    ///
    /// Returns [`CopyError::Read`] if walking or reading from `self` fails,
    /// or [`CopyError::Write`] if creating a directory or writing a file in
    /// `dst` fails.
    fn copy_dir_to<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_root: &'s str,
        dst: &'d D,
        dst_root: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's;
}

impl<S: AsyncFileEnv + ?Sized> AsyncFileEnvCopyExt for S {
    fn copy_path_to<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_path: &'s str,
        dst: &'d D,
        dst_path: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
    {
        async move {
            let contents = self.read_file(src_path).await.map_err(CopyError::Read)?;
            dst.write_file(dst_path, &contents)
                .await
                .map_err(CopyError::Write)
        }
    }

    fn copy_dir_to<'s, 'd, D: AsyncFileEnv>(
        &'s self,
        src_root: &'s str,
        dst: &'d D,
        dst_root: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
    {
        async move {
            let entries = self.walk(src_root).await.map_err(CopyError::Read)?;
            for (entry_path, is_dir) in entries {
                let dst_path = rebase(src_root, &entry_path, dst_root);
                if is_dir {
                    dst.create_dir_all(&dst_path)
                        .await
                        .map_err(CopyError::Write)?;
                } else {
                    let contents = self
                        .read_file(&entry_path)
                        .await
                        .map_err(CopyError::Read)?;
                    dst.write_file(&dst_path, &contents)
                        .await
                        .map_err(CopyError::Write)?;
                }
            }
            Ok(())
        }
    }
}

// ── 4. AsyncFileEnv → FileEnv (async → sync) ─────────────────────────────────

/// Extension methods on [`AsyncFileEnv`] that copy a file or directory tree
/// into a sync [`FileEnv`].
pub trait AsyncFileEnvCopyToSyncExt: AsyncFileEnv {
    /// Read `src_path` asynchronously from `self` and write its contents
    /// synchronously to `dst_path` in `dst`.
    ///
    /// The future resolves to [`CopyError::Read`] if the asynchronous source
    /// read fails, or [`CopyError::Write`] if the synchronous destination
    /// write fails.
    fn copy_path_to_sync<'s, 'd, D: FileEnv>(
        &'s self,
        src_path: &'s str,
        dst: &'d D,
        dst_path: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's;

    /// Recursively copy the directory tree rooted at `src_root` in `self`
    /// (async walk + reads) into `dst_root` in `dst` (synchronous writes).
    ///
    /// Returns [`CopyError::Read`] if walking or reading from `self` fails,
    /// or [`CopyError::Write`] if creating a directory or writing a file in
    /// `dst` fails.
    fn copy_dir_to_sync<'s, 'd, D: FileEnv>(
        &'s self,
        src_root: &'s str,
        dst: &'d D,
        dst_root: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's;
}

impl<S: AsyncFileEnv + ?Sized> AsyncFileEnvCopyToSyncExt for S {
    fn copy_path_to_sync<'s, 'd, D: FileEnv>(
        &'s self,
        src_path: &'s str,
        dst: &'d D,
        dst_path: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
    {
        async move {
            let contents = self.read_file(src_path).await.map_err(CopyError::Read)?;
            dst.write_file(dst_path, &contents)
                .map_err(CopyError::Write)
        }
    }

    fn copy_dir_to_sync<'s, 'd, D: FileEnv>(
        &'s self,
        src_root: &'s str,
        dst: &'d D,
        dst_root: &'d str,
    ) -> impl Future<Output = Result<(), CopyError<Self::Error, D::Error>>> + Send + 's
    where
        'd: 's,
    {
        async move {
            let entries = self.walk(src_root).await.map_err(CopyError::Read)?;
            for (entry_path, is_dir) in entries {
                let dst_path = rebase(src_root, &entry_path, dst_root);
                if is_dir {
                    dst.create_dir_all(&dst_path).map_err(CopyError::Write)?;
                } else {
                    let contents = self
                        .read_file(&entry_path)
                        .await
                        .map_err(CopyError::Read)?;
                    dst.write_file(&dst_path, &contents)
                        .map_err(CopyError::Write)?;
                }
            }
            Ok(())
        }
    }
}
