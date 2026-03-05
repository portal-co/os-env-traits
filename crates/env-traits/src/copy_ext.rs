//! Extension traits for copying a path from one env into another.
//!
//! Four combinations are provided, covering every pairing of sync
//! ([`FileEnv`]) and async ([`AsyncFileEnv`]) on each side:
//!
//! | source \ destination | sync [`FileEnv`]              | async [`AsyncFileEnv`]             |
//! |----------------------|-------------------------------|-------------------------------------|
//! | sync [`FileEnv`]     | [`FileEnvCopyExt`]            | [`FileEnvCopyToAsyncExt`]           |
//! | async [`AsyncFileEnv`] | [`AsyncFileEnvCopyToSyncExt`] | [`AsyncFileEnvCopyExt`]            |
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
//! ```

use core::future::Future;

use crate::{AsyncFileEnv, FileEnv};

// ── CopyError ────────────────────────────────────────────────────────────────

/// Error returned by all `copy_path_to*` methods.
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
            CopyError::Read(e)  => write!(f, "copy failed on read: {e}"),
            CopyError::Write(e) => write!(f, "copy failed on write: {e}"),
        }
    }
}

// ── 1. FileEnv → FileEnv (sync → sync) ───────────────────────────────────────

/// Extension method on [`FileEnv`] that copies a path into another
/// [`FileEnv`].
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
}

impl<S: FileEnv + ?Sized> FileEnvCopyExt for S {
    fn copy_path_to<D: FileEnv>(
        &self,
        src_path: &str,
        dst: &D,
        dst_path: &str,
    ) -> Result<(), CopyError<Self::Error, D::Error>> {
        let contents = self.read_file(src_path).map_err(CopyError::Read)?;
        dst.write_file(dst_path, &contents).map_err(CopyError::Write)
    }
}

// ── 2. FileEnv → AsyncFileEnv (sync → async) ─────────────────────────────────

/// Extension method on [`FileEnv`] that copies a path into an
/// [`AsyncFileEnv`].
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
            dst.write_file(dst_path, &contents).await.map_err(CopyError::Write)
        }
    }
}

// ── 3. AsyncFileEnv → AsyncFileEnv (async → async) ───────────────────────────

/// Extension method on [`AsyncFileEnv`] that copies a path into another
/// [`AsyncFileEnv`].
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
            dst.write_file(dst_path, &contents).await.map_err(CopyError::Write)
        }
    }
}

// ── 4. AsyncFileEnv → FileEnv (async → sync) ─────────────────────────────────

/// Extension method on [`AsyncFileEnv`] that copies a path into a sync
/// [`FileEnv`].
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
            dst.write_file(dst_path, &contents).map_err(CopyError::Write)
        }
    }
}
