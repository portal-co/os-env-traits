extern crate std;

use alloc::{
    boxed::Box,
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use std::sync::{Arc, RwLock};

use crate::{FileTree, FileTreeError};

// ── FileTreeEnv ───────────────────────────────────────────────────────────────

/// An [`env_traits::FileEnv`] implementation backed by an in-memory
/// [`FileTree`].
///
/// The tree is wrapped in an `Arc<RwLock<…>>` so that `write_file` and
/// `create_dir_all` can mutate it through the shared `&self` reference
/// required by the trait.
///
/// `env_var` always returns `None`; `FileTree` has no concept of environment
/// variables.
#[derive(Clone)]
pub struct FileTreeEnv<T> {
    root: Arc<RwLock<FileTree<T>>>,
}

impl<T> FileTreeEnv<T> {
    /// Create a new `FileTreeEnv` from an existing tree.
    pub fn new(tree: FileTree<T>) -> Self {
        FileTreeEnv {
            root: Arc::new(RwLock::new(tree)),
        }
    }

    /// Consume this env and return the inner tree.
    ///
    /// Returns `Err` if other `Arc` clones still exist (same contract as
    /// [`Arc::try_unwrap`]).
    pub fn into_inner(self) -> Result<FileTree<T>, Self> {
        Arc::try_unwrap(self.root)
            .map(|lock| lock.into_inner().unwrap())
            .map_err(|arc| FileTreeEnv { root: arc })
    }
}

impl<T> Default for FileTreeEnv<T> {
    fn default() -> Self {
        FileTreeEnv::new(FileTree::Dir {
            entries: BTreeMap::new(),
        })
    }
}

// ── path helpers ─────────────────────────────────────────────────────────────

/// Split a path string into non-empty components, stripping leading `/`.
fn components(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|s| !s.is_empty())
}

/// Navigate to the node at `path` (read-only).  Returns `None` when the path
/// doesn't exist.
fn get_node<'t, T>(root: &'t FileTree<T>, path: &str) -> Option<&'t FileTree<T>> {
    let mut cur = root;
    for component in components(path) {
        match cur {
            FileTree::Dir { entries } => cur = entries.get(component)?,
            FileTree::File { .. } => return None,
        }
    }
    Some(cur)
}

/// Ensure every directory component in `comps` exists under `root`, creating
/// `FileTree::Dir` nodes as needed.  Returns `Err` if any component names an
/// existing file node.
fn ensure_dirs<T>(
    root: &mut FileTree<T>,
    comps: &[&str],
    full_path: &str,
) -> Result<(), FileTreeError> {
    let mut cur = root;
    let mut so_far = String::new();
    for component in comps {
        if !so_far.is_empty() {
            so_far.push('/');
        }
        so_far.push_str(component);

        match cur {
            FileTree::File { .. } => return Err(FileTreeError::NotADirectory(so_far)),
            FileTree::Dir { entries } => {
                cur = entries
                    .entry((*component).to_string())
                    .or_insert_with(|| FileTree::Dir { entries: BTreeMap::new() });
            }
        }
    }
    match cur {
        FileTree::File { .. } => Err(FileTreeError::NotADirectory(full_path.to_string())),
        FileTree::Dir { .. } => Ok(()),
    }
}

/// Recursively collect `(absolute_path, is_dir)` pairs for all descendants of
/// `node`, including `node` itself.
fn collect_walk<T>(
    node: &FileTree<T>,
    prefix: &str,
    out: &mut Vec<Result<(String, bool), FileTreeError>>,
) {
    match node {
        FileTree::File { .. } => {
            out.push(Ok((prefix.to_string(), false)));
        }
        FileTree::Dir { entries } => {
            out.push(Ok((prefix.to_string(), true)));
            for (name, child) in entries {
                collect_walk(child, &alloc::format!("{prefix}/{name}"), out);
            }
        }
    }
}

// ── FileEnv impl ─────────────────────────────────────────────────────────────

impl<T> env_traits::FileEnv for FileTreeEnv<T>
where
    T: AsRef<[u8]> + for<'a> From<&'a [u8]> + Send + Sync,
{
    type Error = FileTreeError;

    fn read_file(&self, path: &str) -> Result<Vec<u8>, FileTreeError> {
        let guard = self.root.read().unwrap();
        match get_node(&guard, path) {
            Some(FileTree::File { file }) => Ok(file.as_ref().to_vec()),
            Some(FileTree::Dir { .. }) => Err(FileTreeError::NotAFile(path.to_string())),
            None => Err(FileTreeError::NotFound(path.to_string())),
        }
    }

    /// Write `contents` to `path`, creating any missing parent directories.
    ///
    /// The bound `T: for<'a> From<&'a [u8]>` converts the raw bytes into the
    /// tree's storage type.
    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), FileTreeError> {
        let comps: Vec<&str> = components(path).collect();
        if comps.is_empty() {
            return Err(FileTreeError::NotAFile(path.to_string()));
        }
        let (dir_comps, file_name) = comps.split_at(comps.len() - 1);

        let mut guard = self.root.write().unwrap();
        ensure_dirs(&mut guard, dir_comps, path)?;

        // Re-navigate to the parent after the mutable ensure_dirs pass.
        let parent = {
            let mut cur = &mut *guard;
            for component in dir_comps {
                match cur {
                    FileTree::Dir { entries } => cur = entries.get_mut(*component).unwrap(),
                    FileTree::File { .. } => unreachable!("ensure_dirs already checked"),
                }
            }
            cur
        };

        match parent {
            FileTree::Dir { entries } => {
                entries.insert(
                    file_name[0].to_string(),
                    FileTree::File { file: T::from(contents) },
                );
                Ok(())
            }
            FileTree::File { .. } => Err(FileTreeError::NotADirectory(path.to_string())),
        }
    }

    fn file_exists(&self, path: &str) -> bool {
        let guard = self.root.read().unwrap();
        matches!(get_node(&guard, path), Some(FileTree::File { .. }))
    }

    fn dir_exists(&self, path: &str) -> bool {
        let guard = self.root.read().unwrap();
        matches!(get_node(&guard, path), Some(FileTree::Dir { .. }))
    }

    fn create_dir_all(&self, path: &str) -> Result<(), FileTreeError> {
        let comps: Vec<&str> = components(path).collect();
        let mut guard = self.root.write().unwrap();
        ensure_dirs(&mut guard, &comps, path)
    }

    /// Walk all nodes rooted at `root`, including `root` itself.
    ///
    /// Yields `(path, is_dir)` pairs.
    fn walk(
        &self,
        root: &str,
    ) -> Result<Box<dyn Iterator<Item = Result<(String, bool), FileTreeError>> + '_>, FileTreeError>
    {
        let guard = self.root.read().unwrap();
        let node = get_node(&guard, root)
            .ok_or_else(|| FileTreeError::NotFound(root.to_string()))?;

        let mut entries: Vec<Result<(String, bool), FileTreeError>> = Vec::new();
        let prefix = root.trim_matches('/');
        collect_walk(node, prefix, &mut entries);
        Ok(Box::new(entries.into_iter()))
    }

    /// Always returns `None`; `FileTree` has no environment-variable store.
    fn env_var(&self, _key: &str) -> Option<String> {
        None
    }
}
