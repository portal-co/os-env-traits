// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Result};
use env_traits::NetworkEnv;

#[derive(Clone, Default)]
pub struct FakeNetworkEnv {
    responses: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    calls:     Arc<Mutex<Vec<String>>>,
}

impl FakeNetworkEnv {
    /// Register a URL → response body mapping (used by both GET and POST).
    pub fn with_response(self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        self.responses.lock().unwrap().insert(url.into(), body.into());
        self
    }

    /// Assert that `url` was called (panics on failure — intended for tests).
    pub fn assert_called(&self, url: &str) {
        let calls = self.calls.lock().unwrap();
        assert!(
            calls.iter().any(|c| c == url),
            "FakeNetworkEnv: expected call to {url} but got: {calls:?}"
        );
    }

    fn record_and_get(&self, url: &str) -> Result<Vec<u8>> {
        self.calls.lock().unwrap().push(url.to_string());
        self.responses
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .ok_or_else(|| anyhow!("FakeNetworkEnv: no response registered for {url}"))
    }
}

impl NetworkEnv for FakeNetworkEnv {
    fn post_json(&self, url: &str, _body: &[u8]) -> Result<Vec<u8>> {
        self.record_and_get(url)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>> {
        self.record_and_get(url)
    }
}
