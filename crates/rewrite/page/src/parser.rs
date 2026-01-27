//! Async parser infrastructure for streaming HTML and CSS parsing.
//!
//! This module provides utilities to bridge async streams with blocking parsers
//! that are not `Send`. It handles the plumbing of forwarding stream chunks to
//! blocking tasks where CPU-bound parsing happens.

use futures::stream::Stream;
use std::sync::mpsc;

/// Spawns a streaming parser that processes chunks from an async stream on a blocking thread.
///
/// This function handles the common pattern of:
/// 1. Receiving chunks from an async stream (network/disk I/O)
/// 2. Forwarding them to a blocking task via channel
/// 3. Processing chunks with a CPU-bound, non-Send parser
///
/// # Type Parameters
///
/// - `S`: The stream type that yields chunks
/// - `C`: The chunk type (e.g., `String` for HTML/CSS)
/// - `F`: The parser function that processes chunks
///
/// # Arguments
///
/// - `runtime`: The Tokio runtime to spawn tasks on
/// - `stream`: The async stream of chunks to parse
/// - `parser_fn`: A function that receives chunks via channel and does the parsing
///
/// # Example
///
/// ```ignore
/// spawn_streaming_parser(
///     &runtime,
///     html_stream,
///     |chunk_rx| {
///         let mut parser = create_parser();
///         while let Ok(chunk) = chunk_rx.recv() {
///             parser.process(chunk);
///         }
///         parser.finish();
///     }
/// );
/// ```
pub fn spawn_streaming_parser<S, C, F>(runtime: &tokio::runtime::Runtime, stream: S, parser_fn: F)
where
    S: Stream<Item = C> + Send + 'static,
    C: Send + 'static,
    F: FnOnce(mpsc::Receiver<C>) + Send + 'static,
{
    // Create a channel to forward stream chunks to the blocking parser
    let (chunk_tx, chunk_rx) = mpsc::channel::<C>();

    // Spawn async task to forward stream chunks to channel
    runtime.spawn(async move {
        use futures::StreamExt;

        let mut stream = Box::pin(stream);

        while let Some(chunk) = stream.next().await {
            if chunk_tx.send(chunk).is_err() {
                break; // Parser has terminated
            }
        }
    });

    // Spawn blocking task to parse chunks
    runtime.spawn(async move {
        tokio::task::spawn_blocking(move || {
            parser_fn(chunk_rx);
        })
        .await
        .expect("Parser task panicked");
    });
}
