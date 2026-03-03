// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use env_traits::{GitHubEnv, GitHubFile};
use serde::Deserialize;

/// `GitHubEnv` backed by the `gh` CLI on `$PATH`.
///
/// All GitHub API calls are routed through `gh api …` so that authentication
/// tokens are managed by `gh auth` — no token handling in this crate.
#[derive(Default, Clone, Copy)]
pub struct GhCliGitHubEnv;

impl GhCliGitHubEnv {
    fn gh(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new("gh")
            .args(args)
            .output()
            .with_context(|| format!("gh {}", args.join(" ")))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(anyhow!(
                "gh {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

#[derive(Deserialize)]
struct GhContentsEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    kind: String,
    download_url: Option<String>,
}

impl GitHubEnv for GhCliGitHubEnv {
    fn current_owner(&self) -> Result<String> {
        let raw = self.gh(&[
            "repo", "view", "--json", "owner", "--jq", ".owner.login",
        ])?;
        Ok(String::from_utf8_lossy(&raw).trim().to_string())
    }

    fn list_repos(&self, org: &str, limit: usize) -> Result<Vec<String>> {
        let limit_s = limit.to_string();
        let raw = self.gh(&[
            "repo", "list", org,
            "--limit", &limit_s,
            "--json", "name",
            "--jq", ".[].name",
        ])?;
        Ok(String::from_utf8_lossy(&raw)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    fn list_contents(&self, org: &str, repo: &str, path: &str) -> Result<Vec<GitHubFile>> {
        let url = format!(
            "https://api.github.com/repos/{org}/{repo}/contents/{path}"
        );
        let raw = self.gh(&["api", &url, "--paginate"])?;
        let entries: Vec<GhContentsEntry> =
            serde_json::from_slice(&raw).with_context(|| {
                format!("list_contents: parse JSON for {org}/{repo}/{path}")
            })?;

        let mut result = Vec::new();
        for entry in entries {
            if entry.kind == "dir" {
                let sub = self.list_contents(org, repo, &entry.path)?;
                result.extend(sub);
            } else {
                result.push(GitHubFile {
                    name: entry.name,
                    path: entry.path,
                    kind: entry.kind,
                    download_url: entry.download_url,
                });
            }
        }
        Ok(result)
    }

    fn download_file(&self, download_url: &str) -> Result<Vec<u8>> {
        // Use `gh api` with the raw URL so auth headers are injected.
        self.gh(&["api", download_url])
    }
}
