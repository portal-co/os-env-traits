// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use env_traits::GitEnv;

/// `GitEnv` backed by the `git` binary on `$PATH`.
///
/// Each method runs `git <args>` in the given `repo_root` directory and
/// returns `Err` on a non-zero exit code, mirroring the Go `exec.Command`
/// approach exactly.
#[derive(Default, Clone, Copy)]
pub struct ProcessGitEnv;

impl ProcessGitEnv {
    fn run(&self, repo_root: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("git {}", args.join(" ")))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(anyhow!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

impl GitEnv for ProcessGitEnv {
    fn repo_root(&self) -> Result<PathBuf> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("git rev-parse --show-toplevel")?;
        if output.status.success() {
            Ok(PathBuf::from(
                String::from_utf8_lossy(&output.stdout).trim(),
            ))
        } else {
            Err(anyhow!(
                "git rev-parse --show-toplevel: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn rev_parse(&self, repo_root: &Path, rev: &str) -> Result<String> {
        self.run(repo_root, &["rev-parse", rev])
    }

    fn show_file(&self, repo_root: &Path, commit: &str, path: &str) -> Result<Vec<u8>> {
        let r#ref = format!("{commit}:{path}");
        let output = Command::new("git")
            .args(["show", &r#ref])
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("git show {ref}"))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(anyhow!(
                "git show {ref}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn changed_files(&self, repo_root: &Path, base: &str) -> Result<Vec<String>> {
        let out = self.run(
            repo_root,
            &["diff", "--name-only", "--diff-filter=ACMR", base, "HEAD"],
        )?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    fn merge_base(&self, repo_root: &Path, branch: &str) -> Result<String> {
        let remote_ref = format!("origin/{branch}");
        self.run(repo_root, &["merge-base", "HEAD", &remote_ref])
    }

    fn fetch(&self, repo_root: &Path, remote: &str, refspec: &str) -> Result<()> {
        self.run(repo_root, &["fetch", "--no-tags", remote, refspec])?;
        Ok(())
    }

    fn init(&self, dir: &Path) -> Result<()> {
        let output = Command::new("git")
            .arg("init")
            .current_dir(dir)
            .output()
            .context("git init")?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "git init: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn add_and_commit(&self, repo_root: &Path, message: &str) -> Result<()> {
        self.run(repo_root, &["add", "-A"])?;
        self.run(repo_root, &["commit", "-m", message])?;
        Ok(())
    }
}
