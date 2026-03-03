// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
//! In-memory fake implementations of every env-trait, for use in unit tests.
//!
//! All fakes use builder-style `with_*` methods so tests can set up only the
//! state they care about.  Missing entries return `Err` by default, making
//! unintended calls loud.

pub mod fake_file;
pub mod fake_git;
pub mod fake_github;
pub mod fake_network;
pub mod fake_ai;

pub use fake_file::FakeFileEnv;
pub use fake_git::FakeGitEnv;
pub use fake_github::FakeGitHubEnv;
pub use fake_network::FakeNetworkEnv;
pub use fake_ai::FakeAiEnv;
