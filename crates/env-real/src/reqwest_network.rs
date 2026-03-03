// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use anyhow::{anyhow, Context, Result};
use env_traits::NetworkEnv;

/// `NetworkEnv` backed by `reqwest` blocking client.
#[derive(Default, Clone)]
pub struct ReqwestNetworkEnv;

impl NetworkEnv for ReqwestNetworkEnv {
    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_vec())
            .send()
            .with_context(|| format!("POST {url}"))?;

        let status = resp.status();
        let bytes = resp.bytes().with_context(|| format!("POST {url}: read body"))?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(anyhow!("POST {url}: server returned {status}"))
        }
    }

    fn get(&self, url: &str) -> Result<Vec<u8>> {
        let resp = reqwest::blocking::get(url)
            .with_context(|| format!("GET {url}"))?;

        let status = resp.status();
        let bytes = resp.bytes().with_context(|| format!("GET {url}: read body"))?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(anyhow!("GET {url}: server returned {status}"))
        }
    }
}
