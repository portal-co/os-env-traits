// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{path::Path, process::Command};

use anyhow::{anyhow, Context};
use env_traits::GitEnv;

use crate::error::{real_err, RealError};

/// `GitEnv` backed by the `git` binary on `$PATH`.
///
/// Each method runs `git <args>` in the given `repo_root` directory and
/// returns `Err` on a non-zero exit code, mirroring the Go `exec.Command`
/// approach exactly.
#[derive(Default, Clone, Copy)]
pub struct ProcessGitEnv;

impl ProcessGitEnv {
    fn run(&self, repo_root: &str, args: &[&str]) -> Result<String, RealError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("git {}", args.join(" ")))
            .map_err(real_err)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(real_err(anyhow!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

impl embedded_io::ErrorType for ProcessGitEnv {
    type Error = RealError;
}

impl GitEnv for ProcessGitEnv {
    fn repo_root(&self) -> Result<String, RealError> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("git rev-parse --show-toplevel")
            .map_err(real_err)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(real_err(anyhow!(
                "git rev-parse --show-toplevel: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn rev_parse(&self, repo_root: &str, rev: &str) -> Result<String, RealError> {
        self.run(repo_root, &["rev-parse", rev])
    }

    fn show_file(&self, repo_root: &str, commit: &str, path: &str) -> Result<Vec<u8>, RealError> {
        let r#ref = format!("{commit}:{path}");
        let output = Command::new("git")
            .args(["show", &r#ref])
            .current_dir(repo_root)
            .output()
            .with_context(|| format!("git show {ref}"))
            .map_err(real_err)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(real_err(anyhow!(
                "git show {ref}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn changed_files(&self, repo_root: &str, base: &str) -> Result<Vec<String>, RealError> {
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

    fn merge_base(&self, repo_root: &str, branch: &str) -> Result<String, RealError> {
        let remote_ref = format!("origin/{branch}");
        self.run(repo_root, &["merge-base", "HEAD", &remote_ref])
    }

    fn fetch(&self, repo_root: &str, remote: &str, refspec: &str) -> Result<(), RealError> {
        self.run(repo_root, &["fetch", "--no-tags", remote, refspec])?;
        Ok(())
    }

    fn init(&self, dir: &str) -> Result<(), RealError> {
        let output = Command::new("git")
            .arg("init")
            .current_dir(Path::new(dir))
            .output()
            .context("git init")
            .map_err(real_err)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(real_err(anyhow!(
                "git init: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn add_and_commit(&self, repo_root: &str, message: &str) -> Result<(), RealError> {
        self.run(repo_root, &["add", "-A"])?;
        self.run(repo_root, &["commit", "-m", message])?;
        Ok(())
    }
}
