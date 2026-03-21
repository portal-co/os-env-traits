# os-env-traits

A Rust workspace that defines pluggable traits for every external operation a
CLI tool or CI binary typically needs: filesystem access, git commands, GitHub
API calls, generic HTTP, and AI-content detection. Each trait has a real
(production) implementation and an in-memory fake suitable for hermetic unit
tests.

The trait crate is `no_std` (requires `alloc`). Paths are `&str`/`String`
rather than `std::path::Path` so the definitions remain usable outside a
standard library environment.

## Status

Early-stage / internal library. Not published to crates.io. Consumed via git
dependency. The `crates/` and `packages/` directories are currently empty
placeholders; all four crates live under `crates/`.

## Workspace layout

```
crates/
  env-traits/   trait definitions only (no_std + alloc)
  env-real/     production implementations
  env-fake/     in-memory test fakes
  ftree/        FileTree data structure + FileTreeEnv (no_std + alloc)
```

## Traits

Each trait is defined in `env-traits` in both a synchronous and asynchronous
form. All traits extend `embedded_io::ErrorType` (a single associated `Error`
type, no `std::error::Error` required at the trait level).

### Sync traits

| Trait | Purpose |
|-------|---------|
| `FileEnv` | `read_file`, `write_file`, `file_exists`, `dir_exists`, `create_dir_all`, `walk`, `env_var` |
| `GitEnv` | `repo_root`, `rev_parse`, `show_file`, `changed_files`, `merge_base`, `fetch`, `init`, `add_and_commit` |
| `GitHubEnv` | `current_owner`, `list_repos`, `list_contents`, `download_file` (via `gh` CLI) |
| `NetworkEnv` | `get`, `post_json` (generic HTTP, no GitHub-specific concepts) |
| `AiEnv` | `scan(path, content) -> (bool, f64)` — AI-content detection |

### Async traits

`AsyncFileEnv`, `AsyncGitEnv`, `AsyncGitHubEnv`, `AsyncNetworkEnv`, and
`AsyncAiEnv` mirror the sync traits. Methods return
`impl Future<Output = …> + Send` (the explicit desugared form, not bare
`async fn`) so the traits are compatible with multi-threaded runtimes without
extra bounds at call sites. `AsyncFileEnv::walk` returns a `Stream` because
async iterators are not yet stable.

`Box<dyn XxxEnv>` implements each trait via blanket impls in `env-traits`.

## Crates

### `env-traits`

Trait definitions only. Dependencies: `embedded-io`, `embedded-io-async`,
`futures`. Optional feature `awaiter` pulls in `awaiter-trait` and enables the
`WithAwaiter` adapter (see below).

Also provides extension traits in `copy_ext`:

- `FileEnvCopyExt` — sync-to-sync file/directory copy
- `FileEnvCopyToAsyncExt` — sync-to-async copy
- `AsyncFileEnvCopyExt` — async-to-async copy
- `AsyncFileEnvCopyToSyncExt` — async-to-sync copy

Each variant exposes `copy_path_to*` (single file) and `copy_dir_to*`
(recursive directory). Errors are `CopyError<SourceErr, DestErr>` to
distinguish which side failed.

The optional `awaiter_bridge` module (`feature = "awaiter"`) provides
`WithAwaiter<A, E>`, a newtype that pairs any `AsyncXxxEnv` with an
`Awaiter` (from the `awaiter-trait` crate) to produce a synchronous
`XxxEnv` implementation. Each sync method stack-allocates the async future
and drives it with `Pin::new_unchecked` inside the `r#await` call.

### `env-real`

Production implementations. All types implement both the sync and async
variants of their respective traits; the async methods are thin wrappers
that call the sync implementation and wrap the result in `async move { … }`.

| Type | Implements | Mechanism |
|------|-----------|-----------|
| `OsFileEnv` | `FileEnv`, `AsyncFileEnv` | `std::fs`, `walkdir`, `std::env::var` |
| `ProcessGitEnv` | `GitEnv`, `AsyncGitEnv` | spawns `git` via `std::process::Command` |
| `GhCliGitHubEnv` | `GitHubEnv`, `AsyncGitHubEnv` | spawns `gh` via `std::process::Command`; auth is managed by `gh auth` |
| `ReqwestNetworkEnv` | `NetworkEnv`, `AsyncNetworkEnv` | `reqwest` blocking client |
| `NoopAiEnv` | `AiEnv`, `AsyncAiEnv` | always returns `(false, 0.0)`; a real AI implementation is expected to live in a higher-level crate |

All real implementations share a single `RealError` type that wraps
`anyhow::Error`.

`GhCliGitHubEnv::list_contents` calls the GitHub Contents API recursively via
`gh api --paginate` and returns a flat list of all files (not directories)
under the requested path.

### `env-fake`

In-memory test fakes for all five traits. Every fake also implements the
corresponding `Async*` trait. Builder-style API using `.with_*` methods:

```rust
let file = FakeFileEnv::default()
    .with_file("config.toml", b"[foo]\nbar = 1")
    .with_env("HOME", "/home/ci");

let git = FakeGitEnv::default()
    .with_repo_root("/repo")
    .with_rev("HEAD^", "abc123")
    .with_show_file("abc123", "src/lib.rs", b"fn main(){}")
    .with_changed_files(vec!["src/lib.rs".into()])
    .with_merge_base("main", "deadbeef");

let github = FakeGitHubEnv::default()
    .with_owner("my-org")
    .with_repos("my-org", vec!["repo-a", "repo-b"])
    .with_download("https://…/file.txt", b"content");

let net = FakeNetworkEnv::default()
    .with_response("https://api.example.com/check", b"{\"ok\":true}");
// After exercising code under test:
net.assert_called("https://api.example.com/check");

let ai = FakeAiEnv::default()
    .always(false, 0.0)
    .with_result("suspicious.rs", true, 0.95);
```

`FakeGitEnv::fetch`, `init`, and `add_and_commit` are no-ops.
`FakeFileEnv::create_dir_all` is a no-op.
`FakeGitHubEnv::list_contents` returns an empty vec when no contents are
registered (rather than an error).

All fakes use `Arc<Mutex<…>>` internally so they are `Clone + Send + Sync`.

### `ftree`

A `no_std + alloc` crate providing `FileTree<T>`, an in-memory recursive
enum representing a directory tree:

```rust
pub enum FileTree<T> {
    File { file: T },
    Dir  { entries: BTreeMap<String, FileTree<T>> },
}
```

Key methods:
- `bash(&self, path, writer)` — serialises the tree as a shell script that
  recreates it using `echo` and `mkdir`.
- `read(&self, root, env)` — replaces every `File` node with bytes read from
  a `FileEnv`, returning a `FileTree<Vec<u8>>`.
- `map`, `as_ref`, `as_mut` — standard structure-preserving transforms.

Optional features:
- `serde` — derives `Serialize`/`Deserialize` (untagged enum form).
- `env-traits` — enables `FileTree::read` (depends on `env-traits` +
  `embedded-io`).
- `std` — enables `FileTreeEnv<T>`, a `FileEnv` implementation backed by an
  in-memory `FileTree` wrapped in `Arc<RwLock<…>>`. Supports all `FileEnv`
  methods including `walk`; `env_var` always returns `None`.

## Usage

```toml
[dependencies]
env-traits = { git = "https://github.com/portal-co/os-env-traits.git" }
env-real   = { git = "https://github.com/portal-co/os-env-traits.git" }

[dev-dependencies]
env-fake   = { git = "https://github.com/portal-co/os-env-traits.git" }
```

Write generic functions against the traits:

```rust
use env_traits::{FileEnv, GitEnv};

pub fn process<F: FileEnv, G: GitEnv>(file: &F, git: &G) -> anyhow::Result<()> {
    let root = git.repo_root()?;
    let data = file.read_file(&format!("{root}/config.toml"))?;
    // ...
    Ok(())
}
```

Test without touching the filesystem or running git:

```rust
use env_fake::{FakeFileEnv, FakeGitEnv};

#[test]
fn test_process() {
    let file = FakeFileEnv::default().with_file("/repo/config.toml", b"[x]");
    let git  = FakeGitEnv::default().with_repo_root("/repo");
    process(&file, &git).unwrap();
}
```

## Technical notes

- `env-traits` is `no_std`. It uses `alloc` for `String`, `Vec`, and `Box`.
- All traits require `Send + Sync` on the implementor.
- `GitEnv::changed_files` is filtered to added/copied/modified/renamed files
  (`--diff-filter=ACMR`); deletes are excluded.
- `GitHubEnv::download_file` lives on `GitHubEnv` rather than `NetworkEnv`
  because GitHub downloads require authentication; fakes key responses by URL.
- The workspace uses Rust edition 2021, except `ftree` which uses 2024.

## License

[MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/) — file-level copyleft.
Proprietary binaries may link against this library without being subject to
the copyleft requirement, provided the source of modified files is made
available.
