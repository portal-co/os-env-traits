#![no_std]

use core::fmt::Write;
#[cfg(feature = "std")]
use alloc::vec::Vec;

use alloc::{collections::btree_map::BTreeMap, string::String};
extern crate alloc;

// ── FileEnv impl ─────────────────────────────────────────────────────────────

#[cfg(feature = "std")]
mod file_env_impl {
    extern crate std;

    use alloc::{
        boxed::Box,
        format,
        string::{String, ToString},
        vec::Vec,
    };
    use std::sync::{Arc, RwLock};

    use crate::FileTree;

    // ── Error type ───────────────────────────────────────────────────────────

    /// Errors produced by the [`FileEnv`] implementation on [`FileTreeEnv`].
    ///
    /// [`FileEnv`]: env_traits::FileEnv
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum FileTreeError {
        /// No node exists at the given path.
        NotFound(String),
        /// A file was found where a directory was required (e.g. a path
        /// component in the middle of a write that names an existing file).
        NotADirectory(String),
        /// A directory was found where a file was required.
        NotAFile(String),
    }

    impl core::fmt::Display for FileTreeError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                FileTreeError::NotFound(p) => write!(f, "not found: {p}"),
                FileTreeError::NotADirectory(p) => write!(f, "not a directory: {p}"),
                FileTreeError::NotAFile(p) => write!(f, "not a file: {p}"),
            }
        }
    }

    // ── FileTreeEnv ──────────────────────────────────────────────────────────

    /// An [`env_traits::FileEnv`] implementation backed by an in-memory
    /// [`FileTree`].
    ///
    /// The tree is wrapped in an `Arc<RwLock<…>>` so that `write_file` and
    /// `create_dir_all` can mutate it through the shared `&self` reference
    /// required by the trait.
    ///
    /// `env_var` always returns `None`; `FileTree` has no concept of
    /// environment variables.
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

    impl<T: Default> Default for FileTreeEnv<T> {
        fn default() -> Self {
            FileTreeEnv::new(FileTree::Dir {
                entries: BTreeMap::new(),
            })
        }
    }

    // ── path helpers ─────────────────────────────────────────────────────────

    /// Split a path string into non-empty components, stripping leading `/`.
    fn components(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|s| !s.is_empty())
    }

    /// Navigate to the node at `path` (read-only).  Returns `None` when the
    /// path doesn't exist.
    fn get_node<'t, T>(root: &'t FileTree<T>, path: &str) -> Option<&'t FileTree<T>> {
        let mut cur = root;
        for component in components(path) {
            match cur {
                FileTree::Dir { entries } => {
                    cur = entries.get(component)?;
                }
                FileTree::File { .. } => return None,
            }
        }
        Some(cur)
    }

    /// Ensure every directory component of `path` exists, creating
    /// `FileTree::Dir` nodes as needed.  Returns `Err` if a component names
    /// an existing file node.
    fn ensure_dirs<T>(
        root: &mut FileTree<T>,
        components: &[&str],
        full_path: &str,
    ) -> Result<(), FileTreeError> {
        let mut cur = root;
        let mut so_far = String::new();
        for component in components {
            if !so_far.is_empty() {
                so_far.push('/');
            }
            so_far.push_str(component);

            match cur {
                FileTree::File { .. } => {
                    return Err(FileTreeError::NotADirectory(so_far));
                }
                FileTree::Dir { entries } => {
                    cur = entries
                        .entry((*component).to_string())
                        .or_insert_with(|| FileTree::Dir {
                            entries: BTreeMap::new(),
                        });
                }
            }
        }
        // The final node we arrived at must be a Dir (not a File that was
        // already there), unless we created it just now.
        match cur {
            FileTree::File { .. } => Err(FileTreeError::NotADirectory(full_path.to_string())),
            FileTree::Dir { .. } => Ok(()),
        }
    }

    /// Collect `(absolute_path, is_dir)` pairs for all descendants of the
    /// node at `prefix`.
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
                    let child_path = format!("{prefix}/{name}");
                    collect_walk(child, &child_path, out);
                }
            }
        }
    }

    // ── FileEnv impl ─────────────────────────────────────────────────────────

    use alloc::collections::btree_map::BTreeMap;

    impl<T> env_traits::FileEnv for FileTreeEnv<T>
    where
        T: AsRef<[u8]> + for<'a> From<&'a [u8]> + Send + Sync,
    {
        type Error = FileTreeError;

        // ── read_file ────────────────────────────────────────────────────────

        fn read_file(&self, path: &str) -> Result<Vec<u8>, FileTreeError> {
            let guard = self.root.read().unwrap();
            match get_node(&guard, path) {
                Some(FileTree::File { file }) => Ok(file.as_ref().to_vec()),
                Some(FileTree::Dir { .. }) => Err(FileTreeError::NotAFile(path.to_string())),
                None => Err(FileTreeError::NotFound(path.to_string())),
            }
        }

        // ── write_file ───────────────────────────────────────────────────────

        /// Write `contents` to `path`, creating any missing parent directories.
        ///
        /// The bound `T: for<'a> From<&'a [u8]>` is used to convert the raw
        /// bytes into the tree's storage type.
        fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), FileTreeError> {
            let comps: Vec<&str> = components(path).collect();
            if comps.is_empty() {
                // Writing to the root itself doesn't make sense for a Dir root.
                return Err(FileTreeError::NotAFile(path.to_string()));
            }
            let (dir_comps, file_name) = comps.split_at(comps.len() - 1);

            let mut guard = self.root.write().unwrap();
            ensure_dirs(&mut guard, dir_comps, path)?;

            // Navigate to the parent dir again (borrow checker requires a
            // second traversal after the mutable ensure_dirs pass).
            let parent = {
                let mut cur = &mut *guard;
                for component in dir_comps {
                    match cur {
                        FileTree::Dir { entries } => {
                            cur = entries.get_mut(*component).unwrap();
                        }
                        FileTree::File { .. } => unreachable!("ensure_dirs already checked"),
                    }
                }
                cur
            };

            match parent {
                FileTree::Dir { entries } => {
                    entries.insert(
                        file_name[0].to_string(),
                        FileTree::File {
                            file: T::from(contents),
                        },
                    );
                    Ok(())
                }
                FileTree::File { .. } => {
                    Err(FileTreeError::NotADirectory(path.to_string()))
                }
            }
        }

        // ── file_exists ──────────────────────────────────────────────────────

        fn file_exists(&self, path: &str) -> bool {
            let guard = self.root.read().unwrap();
            matches!(get_node(&guard, path), Some(FileTree::File { .. }))
        }

        // ── dir_exists ───────────────────────────────────────────────────────

        fn dir_exists(&self, path: &str) -> bool {
            let guard = self.root.read().unwrap();
            matches!(get_node(&guard, path), Some(FileTree::Dir { .. }))
        }

        // ── create_dir_all ───────────────────────────────────────────────────

        fn create_dir_all(&self, path: &str) -> Result<(), FileTreeError> {
            let comps: Vec<&str> = components(path).collect();
            let mut guard = self.root.write().unwrap();
            ensure_dirs(&mut guard, &comps, path)
        }

        // ── walk ─────────────────────────────────────────────────────────────

        /// Walk all nodes whose paths start with `root`.
        ///
        /// The root node itself is included. Yields `(path, is_dir)` pairs.
        fn walk(
            &self,
            root: &str,
        ) -> Result<
            Box<dyn Iterator<Item = Result<(String, bool), FileTreeError>> + '_>,
            FileTreeError,
        > {
            let guard = self.root.read().unwrap();
            let node = get_node(&guard, root)
                .ok_or_else(|| FileTreeError::NotFound(root.to_string()))?;

            let mut entries: Vec<Result<(String, bool), FileTreeError>> = Vec::new();
            // Normalise root path (strip leading slash, ensure no trailing slash).
            let prefix = root.trim_matches('/');
            collect_walk(node, prefix, &mut entries);
            Ok(Box::new(entries.into_iter()))
        }

        // ── env_var ──────────────────────────────────────────────────────────

        /// Always returns `None`; `FileTree` has no environment-variable store.
        fn env_var(&self, _key: &str) -> Option<String> {
            None
        }
    }
}

#[cfg(feature = "std")]
pub use file_env_impl::{FileTreeEnv, FileTreeError};

// ── FileTree::read ────────────────────────────────────────────────────────────

#[cfg(feature = "std")]
impl<T> FileTree<T> {
    /// Read the tree from the filesystem using a [`FileEnv`] implementation.
    ///
    /// Each `FileTree::File` node is replaced by the bytes returned by
    /// [`FileEnv::read_file`]; directory structure is preserved.  The path
    /// `p` is the filesystem path that corresponds to the root of `self`.
    ///
    /// [`FileEnv`]: env_traits::FileEnv
    pub fn read<E>(
        &self,
        p: &str,
        env: &impl env_traits::FileEnv<Error = E>,
    ) -> Result<FileTree<Vec<u8>>, E>
    where
        E: core::fmt::Debug + core::fmt::Display,
    {
        Ok(match self {
            FileTree::File { .. } => FileTree::File {
                file: env.read_file(p)?,
            },
            FileTree::Dir { entries } => FileTree::Dir {
                entries: entries
                    .iter()
                    .map(|(a, b)| {
                        Ok((
                            a.clone(),
                            b.read(&alloc::format!("{p}/{a}"), env)?,
                        ))
                    })
                    .collect::<Result<_, E>>()?,
            },
        })
    }
}

// ── FileTree ──────────────────────────────────────────────────────────────────

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum FileTree<T> {
    File {
        file: T,
    },
    Dir {
        entries: BTreeMap<String, FileTree<T>>,
    },
}
impl<T: AsRef<[u8]>> FileTree<T> {
    pub fn bash(&self, path: &str, w: &mut (dyn Write + '_)) -> core::fmt::Result {
        match self {
            FileTree::File { file } => match core::str::from_utf8(file.as_ref()) {
                Ok(a) => write!(w, "echo -n '{}'>'{path}';", a.replace("'", "'\"'\"'")),
                Err(_) => {
                    write!(w, "echo -en '")?;
                    for f in file.as_ref() {
                        write!(w, "\\x{f:x}")?;
                    }
                    write!(w, "'>'{path}';")
                }
            },
            FileTree::Dir { entries } => {
                write!(w, "mkdir '{path}';")?;
                for (a, b) in entries {
                    b.bash(&alloc::format!("{path}/{a}"), w)?;
                }
                Ok(())
            }
        }
    }
}
impl<T> FileTree<T> {
    pub fn as_ref(&self) -> FileTree<&T> {
        match self {
            FileTree::File { file } => FileTree::File { file },
            FileTree::Dir { entries } => FileTree::Dir {
                entries: entries
                    .iter()
                    .map(|(a, b)| (a.clone(), b.as_ref()))
                    .collect(),
            },
        }
    }
    pub fn as_mut(&mut self) -> FileTree<&mut T> {
        match self {
            FileTree::File { file } => FileTree::File { file },
            FileTree::Dir { entries } => FileTree::Dir {
                entries: entries
                    .iter_mut()
                    .map(|(a, b)| (a.clone(), b.as_mut()))
                    .collect(),
            },
        }
    }
    pub fn map<NewT, E>(
        self,
        f: &mut (dyn FnMut(T) -> Result<NewT, E> + '_),
    ) -> Result<FileTree<NewT>, E> {
        Ok(match self {
            FileTree::File { file } => FileTree::File { file: f(file)? },
            FileTree::Dir { entries } => FileTree::Dir {
                entries: entries
                    .into_iter()
                    .map(|(a, b)| Ok((a, b.map(f)?)))
                    .collect::<Result<_, E>>()?,
            },
        })
    }
}
