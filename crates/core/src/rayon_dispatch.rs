//! Helper for dispatching work to rayon and awaiting results from tokio.

use tokio::sync::oneshot;

/// Dispatch a closure to rayon and await the result from tokio.
///
/// This bridges async tokio code with rayon's thread pool:
/// - The closure runs on rayon's thread pool
/// - The caller awaits completion without blocking the tokio runtime
///
/// # Example
///
/// ```ignore
/// let result = rayon_dispatch(|| {
///     // CPU-intensive work here
///     expensive_computation()
/// }).await;
/// ```
pub async fn rayon_dispatch<T, F>(f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = oneshot::channel();

    rayon::spawn(move || {
        let result = f();
        let _ = tx.send(result);
    });

    rx.await.expect("rayon task panicked")
}
