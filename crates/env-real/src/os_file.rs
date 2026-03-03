// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use env_traits::FileEnv;
use walkdir::WalkDir;

/// `FileEnv` backed by the real OS filesystem and `std::env`.
#[derive(Default, Clone, Copy)]
pub struct OsFileEnv;

impl FileEnv for OsFileEnv {
    fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        fs::read(path).with_context(|| format!("read_file: {}", path.display()))
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("write_file: create_dir_all {}", parent.display()))?;
        }
        fs::write(path, contents).with_context(|| format!("write_file: {}", path.display()))
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn dir_exists(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path)
            .with_context(|| format!("create_dir_all: {}", path.display()))
    }

    fn walk(
        &self,
        root: &Path,
    ) -> Result<Box<dyn Iterator<Item = Result<(PathBuf, bool)>> + '_>> {
        let iter = WalkDir::new(root)
            .min_depth(1)
            .into_iter()
            .map(|entry| {
                let e = entry.with_context(|| "walkdir entry error")?;
                Ok((e.path().to_path_buf(), e.file_type().is_dir()))
            });
        Ok(Box::new(iter))
    }

    fn env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}
