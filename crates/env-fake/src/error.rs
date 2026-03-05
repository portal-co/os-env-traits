// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::fmt;

/// Opaque error type used by all fake env implementations.
///
/// Wraps an [`anyhow::Error`] for convenient construction in tests while
/// satisfying [`embedded_io::Error`].
#[derive(Debug)]
pub struct FakeError(pub(crate) anyhow::Error);

impl fmt::Display for FakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl embedded_io::Error for FakeError {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

/// Convenience: wrap an `anyhow::Error` into a `FakeError`.
pub(crate) fn fake_err(e: anyhow::Error) -> FakeError {
    FakeError(e)
}
