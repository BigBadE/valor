use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// An engine-agnostic representation of JavaScript values.
/// This is intentionally small for now; more variants can be added as needed.
#[derive(Clone, Debug)]
pub enum JSValue {
    /// The `undefined` value.
    Undefined,
    /// The `null` value.
    Null,
    /// A boolean primitive.
    Boolean(bool),
    /// A number (IEEE 754 double precision).
    Number(f64),
    /// A string value (UTF-8).
    String(String),
}

/// Error type used by host callbacks.
#[derive(Debug)]
pub enum JSError {
    /// A type error (for example, wrong argument types).
    TypeError(String),
    /// An internal error not exposed to user code in detail.
    InternalError(String),
}

impl Display for JSError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            JSError::TypeError(message) => write!(f, "TypeError: {}", message),
            JSError::InternalError(message) => write!(f, "InternalError: {}", message),
        }
    }
}

impl Error for JSError {}

/// Log severity levels understood by the host logger.
#[derive(Copy, Clone, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
