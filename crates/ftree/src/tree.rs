use alloc::{collections::btree_map::BTreeMap, string::String};
use core::fmt::Write;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum FileTree<T> {
    File { file: T },
    Dir { entries: BTreeMap<String, FileTree<T>> },
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
