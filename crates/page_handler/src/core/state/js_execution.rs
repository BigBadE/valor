//! JavaScript execution helpers.

use crate::internal::embedded_chrome::get_embedded_chrome_asset;
use crate::internal::url::stream_url;
use anyhow::{Error, anyhow};
use html::parser::{ScriptJob, ScriptKind};
use js::{HostContext, JsEngine, ModuleResolver};
use log::info;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::StreamExt as _;
use url::Url;

/// Parameters for executing pending scripts.
pub(super) struct ExecuteScriptsParams<'exec, E: JsEngine> {
    pub script_rx: &'exec mut UnboundedReceiver<ScriptJob>,
    pub script_counter: &'exec mut u64,
    pub js_engine: &'exec mut E,
    pub module_resolver: &'exec mut Box<dyn ModuleResolver>,
    pub url: &'exec Url,
    pub host_context: &'exec HostContext,
}

/// Execute any pending inline scripts from the parser.
pub(super) fn execute_pending_scripts<E: JsEngine>(params: &mut ExecuteScriptsParams<'_, E>) {
    while let Ok(job) = params.script_rx.try_recv() {
        let script_url = if job.url.is_empty() {
            let kind = match job.kind {
                ScriptKind::Module => "module",
                ScriptKind::Classic => "script",
            };
            let inline_url = format!("inline:{kind}-{}", *params.script_counter);
            *params.script_counter = (*params.script_counter).wrapping_add(1);
            inline_url
        } else {
            job.url.clone()
        };
        info!(
            "HtmlPage: executing {} (length={} bytes)",
            script_url,
            job.source.len()
        );
        match job.kind {
            ScriptKind::Classic => {
                let code = classic_script_source(&job, &script_url, params.host_context);
                let _unused = params.js_engine.eval_script(&code, &script_url);
                let _unused2 = params.js_engine.run_jobs();
            }
            ScriptKind::Module => {
                eval_module_job(
                    &job,
                    &script_url,
                    params.js_engine,
                    params.module_resolver,
                    params.url,
                );
            }
        }
    }
}

/// Helper: obtain classic script source given a job and resolved `script_url`.
fn classic_script_source(job: &ScriptJob, script_url: &str, host_context: &HostContext) -> String {
    // Inline or provided source: return immediately
    if !job.source.is_empty() || script_url.starts_with("inline:") {
        return job.source.clone();
    }
    // Parse URL or bail
    let Ok(url) = Url::parse(script_url) else {
        return String::new();
    };
    // Embedded chrome asset
    if url.scheme() == "valor" {
        let path = url.path();
        if let Some(bytes) = get_embedded_chrome_asset(path)
            .or_else(|| get_embedded_chrome_asset(&format!("valor://chrome{path}")))
        {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        return String::new();
    }
    // Fetch text via stream_url for network/file schemes
    fetch_url_text(&url, host_context).unwrap_or_default()
}

/// Fetch URL text content.
///
/// # Errors
///
/// Returns an error if fetching fails.
fn fetch_url_text(url: &Url, host_context: &HostContext) -> Result<String, Error> {
    let fut = async {
        let mut buffer: Vec<u8> = Vec::new();
        let mut stream = stream_url(url).await?;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|err| anyhow!("{err}"))?;
            buffer.extend_from_slice(&bytes);
        }
        Ok::<String, Error>(String::from_utf8_lossy(&buffer).into_owned())
    };
    host_context.tokio_handle.block_on(fut)
}

/// Helper: evaluate a module job using the resolver and engine, handling inline roots.
fn eval_module_job<E: JsEngine>(
    job: &ScriptJob,
    script_url: &str,
    js_engine: &mut E,
    resolver: &mut Box<dyn ModuleResolver>,
    url: &Url,
) {
    let inline_source = script_url
        .starts_with("inline:")
        .then_some(job.source.as_str());
    if let Ok(bundle) = resolver.bundle_root(script_url, url, inline_source) {
        let _unused = js_engine.eval_module(&bundle, script_url);
        let _unused2 = js_engine.run_jobs();
    } else {
        let _unused = js_engine.eval_module(&job.source, script_url);
        let _unused2 = js_engine.run_jobs();
    }
}

/// Execute at most one due JavaScript timer callback.
///
/// This evaluates the runtime prelude hook `__valorTickTimersOnce(nowMs)` and then
/// flushes engine microtasks to approximate browser ordering (microtasks after each task).
pub(super) fn tick_js_timers_once<E: JsEngine>(js_engine: &mut E) {
    // Use engine-provided clock (Date.now()/performance.now) by omitting the argument.
    // This keeps the runtime timer origin consistent with scheduling inside JS.
    let script = String::from(
        "(function(){ try { var f = globalThis.__valorTickTimersOnce; if (typeof f === 'function') f(); } catch(_){} })();",
    );
    let _unused = js_engine.eval_script(&script, "valor://timers_tick");
    let _unused2 = js_engine.run_jobs();
}
