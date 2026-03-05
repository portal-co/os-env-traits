// AIKEY-l4qkxonqry2b4gj7bsrkqpryiy
use std::fmt;

/// Opaque error type used by all real env implementations.
///
/// Wraps an [`anyhow::Error`] so that `anyhow`'s ergonomic context-building
/// can be used internally while satisfying [`embedded_io::Error`].
#[derive(Debug)]
pub struct RealError(pub(crate) anyhow::Error);

impl fmt::Display for RealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RealError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl embedded_io::Error for RealError {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

/// Convenience: wrap an `anyhow::Error` into a `RealError`.
pub(crate) fn real_err(e: anyhow::Error) -> RealError {
    RealError(e)
}
