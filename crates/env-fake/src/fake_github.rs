// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Error};
use env_traits::{GitHubEnv, GitHubFile};

#[derive(Clone, Default)]
pub struct FakeGitHubEnv {
    owner:     Arc<Mutex<Option<String>>>,
    repos:     Arc<Mutex<HashMap<String, Vec<String>>>>,
    contents:  Arc<Mutex<HashMap<(String, String, String), Vec<GitHubFile>>>>,
    downloads: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl FakeGitHubEnv {
    pub fn with_owner(self, owner: impl Into<String>) -> Self {
        *self.owner.lock().unwrap() = Some(owner.into());
        self
    }

    /// Register the list of repo names for an org.
    pub fn with_repos(self, org: impl Into<String>, repos: Vec<impl Into<String>>) -> Self {
        self.repos
            .lock()
            .unwrap()
            .insert(org.into(), repos.into_iter().map(Into::into).collect());
        self
    }

    /// Register files returned for a (org, repo, path) listing.
    pub fn with_contents(
        self,
        org: impl Into<String>,
        repo: impl Into<String>,
        path: impl Into<String>,
        files: Vec<GitHubFile>,
    ) -> Self {
        self.contents
            .lock()
            .unwrap()
            .insert((org.into(), repo.into(), path.into()), files);
        self
    }

    /// Register a download URL → bytes mapping.
    pub fn with_download(self, url: impl Into<String>, content: impl Into<Vec<u8>>) -> Self {
        self.downloads.lock().unwrap().insert(url.into(), content.into());
        self
    }
}

impl GitHubEnv for FakeGitHubEnv {
    type Error = Error;

    fn current_owner(&self) -> Result<String, Error> {
        self.owner
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("FakeGitHubEnv: owner not set"))
    }

    fn list_repos(&self, org: &str, _limit: usize) -> Result<Vec<String>, Error> {
        self.repos
            .lock()
            .unwrap()
            .get(org)
            .cloned()
            .ok_or_else(|| anyhow!("FakeGitHubEnv: no repos registered for org {org}"))
    }

    fn list_contents(&self, org: &str, repo: &str, path: &str) -> Result<Vec<GitHubFile>, Error> {
        Ok(self
            .contents
            .lock()
            .unwrap()
            .get(&(org.to_string(), repo.to_string(), path.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    fn download_file(&self, download_url: &str) -> Result<Vec<u8>, Error> {
        self.downloads
            .lock()
            .unwrap()
            .get(download_url)
            .cloned()
            .ok_or_else(|| anyhow!("FakeGitHubEnv: no download registered for {download_url}"))
    }
}
