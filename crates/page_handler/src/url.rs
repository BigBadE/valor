use anyhow::{Error, anyhow};
use bytes::Bytes;
use reqwest::get as reqwest_get;
use tokio::fs::read as tokio_fs_read;
use tokio_stream::{Stream, StreamExt as _, once};
use url::Url;

use crate::embedded_chrome::get_embedded_chrome_asset;

/// Creates a byte stream from a URL.
///
/// Supported URL schemes:
/// - `http`, `https`: Fetched via `reqwest` as a streaming response
/// - `file`: Read from the local filesystem (emitted as a single chunk)
/// - `valor`: Embedded chrome resources served from the binary
///
/// # Arguments
///
/// * `url` - The URL to fetch content from
///
/// # Returns
///
/// A boxed stream of byte chunks, or an error if the URL scheme is unsupported
/// or the resource cannot be fetched.
///
/// # Errors
///
/// - Returns `Err` if the URL scheme is unsupported
/// - Returns `Err` if HTTP fetch fails or returns a non-success status
/// - Returns `Err` if the file path is invalid or the file cannot be read
/// - Returns `Err` if a `valor://` asset is not found in the embedded resources
pub async fn stream_url(
    url: &Url,
) -> Result<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>, Error> {
    Ok(match url.scheme() {
        "http" | "https" => {
            let response = reqwest_get(url.clone())
                .await
                .map_err(|err| anyhow!("Failed to fetch URL {url}: {err}"))?;

            if !response.status().is_success() {
                return Err(anyhow!(
                    "Failed to fetch URL: {} (Status: {})",
                    url,
                    response.status()
                ));
            }
            let stream = response.bytes_stream().map(|res| match res {
                Ok(bytes) => Ok::<Bytes, Error>(bytes),
                Err(err) => Err::<Bytes, Error>(anyhow!(err)),
            });
            Box::new(stream)
        }
        "file" => {
            let path = url
                .to_file_path()
                .map_err(|()| anyhow!("Invalid file path for file url: {url}"))?;
            let data = tokio_fs_read(path).await.map(Bytes::from)?;
            // Emit the entire file as a single chunk for now.
            let stream = once(Ok::<Bytes, Error>(data));
            Box::new(stream)
        }
        "valor" => {
            // We only support valor://chrome/* for now
            if url.host_str() != Some("chrome") {
                return Err(anyhow!("Unsupported valor host: {url}"));
            }
            let path = url.path();
            let Some(bytes) = get_embedded_chrome_asset(path)
                .or_else(|| get_embedded_chrome_asset(&format!("valor://chrome{path}")))
            else {
                return Err(anyhow!("Embedded asset not found for {url}"));
            };
            let data = Bytes::from_static(bytes);
            let stream = once(Ok::<Bytes, Error>(data));
            Box::new(stream)
        }
        _ => return Err(anyhow!("Unsupported url scheme {}", url.scheme())),
    })
}
