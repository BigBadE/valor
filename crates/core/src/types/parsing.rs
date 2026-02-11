//! Parser trait for streaming text parsing.

use lasso::ThreadedRodeo;
use std::sync::Arc;

/// Trait for streaming parsers that emit results via callback.
pub trait Parser: 'static {
    /// The output type emitted by this parser.
    type Output: Send + 'static;

    /// The response type returned by the callback.
    type Response: Send + 'static;

    /// The callback type.
    type Callback: Fn(Self::Output) -> Self::Response + Send + 'static;

    /// Create a new parser with a callback for emitted results.
    fn new(callback: Self::Callback, interner: Arc<ThreadedRodeo>) -> Self;

    /// Process a chunk of text.
    fn process(&mut self, chunk: &str);

    /// Signal that no more input will arrive. May emit final results.
    fn finish(self);
}
