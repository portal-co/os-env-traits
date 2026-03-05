// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{fs, path::Path};

use anyhow::Context;
use env_traits::FileEnv;
use walkdir::WalkDir;

use crate::error::{real_err, RealError};

/// `FileEnv` backed by the real OS filesystem and `std::env`.
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
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), RealError>> + '_>, RealError> {
        let iter = WalkDir::new(root)
            .min_depth(1)
            .into_iter()
            .map(|entry| {
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
