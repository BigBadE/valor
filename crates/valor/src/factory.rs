use anyhow::Result;
use js::ChromeHostCommand;
use page_handler::{HtmlPage, ValorConfig};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use url::Url;

/// Bundle returned by `create_chrome_and_content` matching the Valor main wiring.
pub struct ChromeInit {
    pub chrome_page: HtmlPage,
    pub content_page: HtmlPage,
    pub chrome_host_rx: UnboundedReceiver<ChromeHostCommand>,
}

/// Create the chrome page (<valor://chrome/index.html>) and an initial content page,
/// and wire the privileged chromeHost channel to the chrome page.
///
/// # Errors
/// Returns an error if page creation or URL parsing fails.
pub fn create_chrome_and_content(
    runtime: &Runtime,
    initial_content_url: Url,
) -> Result<ChromeInit> {
    // Create chrome page
    let config = ValorConfig::from_env();
    let mut chrome_page = runtime.block_on(HtmlPage::new(
        runtime.handle(),
        Url::parse("valor://chrome/index.html")?,
        config.clone(),
    ))?;

    // Create content page
    let content_page =
        runtime.block_on(HtmlPage::new(runtime.handle(), initial_content_url, config))?;

    // Wire privileged chromeHost channel for the chrome page
    let (chrome_tx, chrome_rx) = unbounded_channel::<ChromeHostCommand>();
    let _attach_result: Result<(), _> = chrome_page.attach_chrome_host(chrome_tx);

    Ok(ChromeInit {
        chrome_page,
        content_page,
        chrome_host_rx: chrome_rx,
    })
}
