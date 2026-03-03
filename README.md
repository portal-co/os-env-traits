# os-env-traits

Pluggable environment traits for Rust: a set of five traits that abstract every
external operation a CLI tool or CI binary needs to perform. Production code
receives real implementations; tests receive in-memory fakes for fully hermetic
unit testing — no filesystem, no git, no network required.

## Traits

| Trait | What it covers |
|-------|----------------|
| `FileEnv` | Filesystem reads/writes, directory creation, `walk`, `env_var` |
| `GitEnv` | `rev_parse`, `show_file`, `changed_files`, `merge_base`, `fetch`, `init`, `add_and_commit` |
| `GitHubEnv` | Repo listing, Contents API, file downloads (via `gh` CLI) |
| `NetworkEnv` | Generic HTTP `GET` / `POST` (no GitHub concepts) |
| `AiEnv` | AI-content scanning: `scan(path, content) -> (bool, f64)` |

## Crates

- **`env-traits`** — the trait definitions, nothing else. Zero dependencies
  beyond `anyhow`.
- **`env-fake`** — builder-style in-memory fakes for all five traits. Import in
  `[dev-dependencies]` to write hermetic tests.
- **`env-real`** — real implementations: `OsFileEnv`, `ProcessGitEnv`,
  `GhCliGitHubEnv`, `ReqwestNetworkEnv`. Import in binaries only.

## Usage

```toml
[dependencies]
env-traits = { git = "https://github.com/portal-co/os-env-traits.git" }
env-real   = { git = "https://github.com/portal-co/os-env-traits.git" }

[dev-dependencies]
env-fake   = { git = "https://github.com/portal-co/os-env-traits.git" }
```

```rust
use env_traits::{FileEnv, GitEnv};

pub fn my_tool<F: FileEnv, G: GitEnv>(file: &F, git: &G) {
    let root = git.repo_root().unwrap();
    let data = file.read_file(&root.join("README.md")).unwrap();
    // ...
}
```

Tests use `env_fake::{FakeFileEnv, FakeGitEnv}` with `.with_*` builders; no
real filesystem or git process is needed.

## License

[MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/) — file-level copyleft.
Binary use in proprietary software is permitted.
