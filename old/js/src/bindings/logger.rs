use super::values::LogLevel;

/// Cross-runtime logger used by bindings like `console.*`.
pub trait HostLogger: Send + Sync {
    /// Log a message with a given level.
    fn log(&self, level: LogLevel, message: &str);
}
