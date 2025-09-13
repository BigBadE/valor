use anyhow::{anyhow, Error};
use bytes::Bytes;
use tokio::fs;
use tokio_stream::{once, Stream, StreamExt};
use url::Url;

use crate::embedded_chrome::get_embedded_chrome_asset;

/// Create a byte stream from a URL.
///
/// Supported schemes:
/// - http, https: fetched via reqwest as a streaming response
/// - file: read from local filesystem (emitted as a single chunk)
/// - valor: embedded chrome resources served from the binary
pub async fn stream_url(
    url: &Url,
) -> Result<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + Unpin>, Error> {
    Ok(match url.scheme() {
        "http" | "https" => {
            let response = reqwest::get(url.clone())
                .await
                .map_err(|e| anyhow!("Failed to fetch URL {}: {}", url, e))?;

            if !response.status().is_success() {
                return Err(anyhow!(
                    "Failed to fetch URL: {} (Status: {})",
                    url,
                    response.status()
                ));
            }
            let s = response
                .bytes_stream()
                .map(|res| match res {
                    Ok(b) => Ok::<Bytes, Error>(b),
                    Err(e) => Err::<Bytes, Error>(anyhow!(e)),
                });
            Box::new(s)
        }
        "file" => {
            let path = url
                .to_file_path()
                .map_err(|_| anyhow!("Invalid file path for file url: {url}"))?;
            let data = fs::read(path).await.map(Bytes::from)?;
            // Emit the entire file as a single chunk for now.
            let s = once(Ok::<Bytes, Error>(data));
            Box::new(s)
        }
        "valor" => {
            // We only support valor://chrome/* for now
            if url.host_str() != Some("chrome") {
                return Err(anyhow!("Unsupported valor host: {}", url));
            }
            let path = url.path();
            let Some(bytes) = get_embedded_chrome_asset(path).or_else(|| get_embedded_chrome_asset(&format!("valor://chrome{}", path))) else {
                return Err(anyhow!("Embedded asset not found for {}", url));
            };
            let data = Bytes::from_static(bytes);
            let s = once(Ok::<Bytes, Error>(data));
            Box::new(s)
        }
        _ => return Err(anyhow!("Unsupported url scheme {}", url.scheme())),
    })
}
