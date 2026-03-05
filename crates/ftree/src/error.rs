use alloc::string::String;

/// Errors produced by the [`FileEnv`] implementation on [`FileTreeEnv`].
///
/// [`FileEnv`]: env_traits::FileEnv
/// [`FileTreeEnv`]: crate::FileTreeEnv
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTreeError {
    /// No node exists at the given path.
    NotFound(String),
    /// A file was found where a directory was required (e.g. a path component
    /// in the middle of a write that names an existing file).
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

impl core::error::Error for FileTreeError {}

impl embedded_io::Error for FileTreeError {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            FileTreeError::NotFound(_) => embedded_io::ErrorKind::NotFound,
            FileTreeError::NotADirectory(_) | FileTreeError::NotAFile(_) => {
                embedded_io::ErrorKind::InvalidInput
            }
        }
    }
}
