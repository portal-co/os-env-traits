// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use env_traits::AiEnv;

use crate::error::FakeError;

#[derive(Clone, Default)]
pub struct FakeAiEnv {
    default:   Arc<Mutex<(bool, f64)>>,
    overrides: Arc<Mutex<HashMap<String, (bool, f64)>>>,
}

impl FakeAiEnv {
    /// Set the result returned for every path not explicitly overridden.
    pub fn always(self, likely: bool, confidence: f64) -> Self {
        *self.default.lock().unwrap() = (likely, confidence);
        self
    }

    /// Override the result for a specific path.
    pub fn with_result(
        self,
        path: impl Into<String>,
        likely: bool,
        confidence: f64,
    ) -> Self {
        self.overrides
            .lock()
            .unwrap()
            .insert(path.into(), (likely, confidence));
        self
    }
}

impl embedded_io::ErrorType for FakeAiEnv {
    type Error = FakeError;
}

impl AiEnv for FakeAiEnv {
    fn scan(&self, path: &str, _content: &[u8]) -> Result<(bool, f64), FakeError> {
        Ok(self
            .overrides
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(*self.default.lock().unwrap()))
    }
}
