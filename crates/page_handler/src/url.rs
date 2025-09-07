use anyhow::{anyhow, Error};
use bytes::Bytes;
use tokio::fs;
use tokio_stream::{once, Stream, StreamExt};
use url::Url;

/// Create a byte stream from a URL. Supports http(s) and file schemes.
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
        _ => return Err(anyhow!("Unsupported url scheme {}", url.scheme())),
    })
}
