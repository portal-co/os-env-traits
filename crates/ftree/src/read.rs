use alloc::vec::Vec;

use crate::FileTree;

impl<T> FileTree<T> {
    /// Read the tree from an environment using a [`FileEnv`] implementation.
    ///
    /// Each `FileTree::File` node is replaced by the bytes returned by
    /// [`FileEnv::read_file`]; directory structure is preserved.  The path
    /// `p` is the path that corresponds to the root of `self` within the
    /// given environment.
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
                    .map(|(a, b)| Ok((a.clone(), b.read(&alloc::format!("{p}/{a}"), env)?)))
                    .collect::<Result<_, E>>()?,
            },
        })
    }
}
