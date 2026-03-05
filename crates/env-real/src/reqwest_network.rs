// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use anyhow::{anyhow, Context};
use env_traits::NetworkEnv;

use crate::error::{real_err, RealError};

/// `NetworkEnv` backed by `reqwest` blocking client.
#[derive(Default, Clone)]
pub struct ReqwestNetworkEnv;

impl embedded_io::ErrorType for ReqwestNetworkEnv {
    type Error = RealError;
}

impl NetworkEnv for ReqwestNetworkEnv {
    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>, RealError> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_vec())
            .send()
            .with_context(|| format!("POST {url}"))
            .map_err(real_err)?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .with_context(|| format!("POST {url}: read body"))
            .map_err(real_err)?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(real_err(anyhow!("POST {url}: server returned {status}")))
        }
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, RealError> {
        let resp = reqwest::blocking::get(url)
            .with_context(|| format!("GET {url}"))
            .map_err(real_err)?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .with_context(|| format!("GET {url}: read body"))
            .map_err(real_err)?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(real_err(anyhow!("GET {url}: server returned {status}")))
        }
    }
}
