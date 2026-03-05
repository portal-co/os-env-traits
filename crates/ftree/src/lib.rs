#![no_std]

extern crate alloc;

mod tree;

#[cfg(feature = "env-traits")]
mod error;

#[cfg(feature = "env-traits")]
mod read;

#[cfg(feature = "std")]
mod env;

pub use tree::FileTree;

#[cfg(feature = "env-traits")]
pub use error::FileTreeError;

#[cfg(feature = "std")]
pub use env::FileTreeEnv;
