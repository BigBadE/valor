//! Parallel and incremental rendering pipeline.
//!
//! This module implements a pipelined architecture that:
//! - Parses HTML incrementally as chunks arrive
//! - Computes styles/layout for above-fold content immediately
//! - Renders partial pages before full document loads
//! - Parallelizes independent stages (DOM updates, style computation, layout)

use anyhow::{Error, Result};
use js::{DOMUpdate, NodeKey};
use rayon::prelude::*;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use tokio::sync::{Mutex, mpsc};

/// Stage of the rendering pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    /// Parsing HTML chunks
    Parse,
    /// Computing styles
    Style,
    /// Computing layout
    Layout,
    /// Building display list
    Paint,
}

/// Work item that flows through the pipeline
#[derive(Clone)]
pub enum PipelineWork {
    /// HTML chunk to parse
    HtmlChunk(Vec<u8>),
    /// DOM updates to apply
    DomUpdates(Vec<DOMUpdate>),
    /// Nodes that need style computation
    StyleDirty(Vec<NodeKey>),
    /// Nodes that need layout
    LayoutDirty(Vec<NodeKey>),
    /// Signal that HTML stream is complete
    EndOfStream,
}

/// Controls whether to wait for full document or render incrementally
#[derive(Debug, Clone, Copy)]
pub enum RenderingMode {
    /// Wait for entire document before first render (current behavior)
    Buffered,
    /// Render visible content immediately, stream rest
    Incremental {
        /// Render after this many nodes are ready
        initial_threshold: usize,
    },
}

/// Pipeline configuration
#[derive(Clone)]
pub struct PipelineConfig {
    pub mode: RenderingMode,
    /// Number of parallel workers for independent tasks
    pub parallelism: usize,
    /// Enable debug logging
    pub debug: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            mode: RenderingMode::Incremental {
                initial_threshold: 100,
            },
            parallelism: Self::default_parallelism(),
            debug: false,
        }
    }
}

impl PipelineConfig {
    const MAX_PARALLELISM: usize = 4;

    fn default_parallelism() -> usize {
        thread::available_parallelism()
            .map(NonZero::get)
            .unwrap_or(2)
            .min(Self::MAX_PARALLELISM)
    }
}

/// Statistics about pipeline execution
#[derive(Default)]
pub struct PipelineStats {
    pub nodes_parsed: AtomicUsize,
    pub nodes_styled: AtomicUsize,
    pub nodes_laid_out: AtomicUsize,
    pub first_paint_done: AtomicBool,
    pub dom_complete: AtomicBool,
}

impl PipelineStats {
    pub fn ready_for_first_paint(&self, threshold: usize) -> bool {
        !self.first_paint_done.load(Ordering::Relaxed)
            && self.nodes_laid_out.load(Ordering::Relaxed) >= threshold
    }

    pub fn mark_first_paint(&self) {
        self.first_paint_done.store(true, Ordering::Relaxed);
    }
}

/// Parallel pipeline coordinator
pub struct Pipeline {
    config: PipelineConfig,
    stats: Arc<PipelineStats>,

    // Channels between stages
    parse_tx: mpsc::UnboundedSender<PipelineWork>,
    parse_rx: Arc<Mutex<mpsc::UnboundedReceiver<PipelineWork>>>,

    style_tx: mpsc::UnboundedSender<PipelineWork>,
    style_rx: Arc<Mutex<mpsc::UnboundedReceiver<PipelineWork>>>,

    layout_tx: mpsc::UnboundedSender<PipelineWork>,
    layout_rx: Arc<Mutex<mpsc::UnboundedReceiver<PipelineWork>>>,

    paint_tx: mpsc::UnboundedSender<PipelineWork>,
    paint_rx: Arc<Mutex<mpsc::UnboundedReceiver<PipelineWork>>>,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Self {
        let (parse_tx, parse_rx) = mpsc::unbounded_channel();
        let (style_tx, style_rx) = mpsc::unbounded_channel();
        let (layout_tx, layout_rx) = mpsc::unbounded_channel();
        let (paint_tx, paint_rx) = mpsc::unbounded_channel();

        Self {
            config,
            stats: Arc::new(PipelineStats::default()),
            parse_tx,
            parse_rx: Arc::new(Mutex::new(parse_rx)),
            style_tx,
            style_rx: Arc::new(Mutex::new(style_rx)),
            layout_tx,
            layout_rx: Arc::new(Mutex::new(layout_rx)),
            paint_tx,
            paint_rx: Arc::new(Mutex::new(paint_rx)),
        }
    }

    /// Submit HTML chunk for parsing.
    ///
    /// # Errors
    ///
    /// Returns an error if the pipeline channel fails to send.
    pub fn submit_html(&self, chunk: Vec<u8>) -> Result<()> {
        self.parse_tx
            .send(PipelineWork::HtmlChunk(chunk))
            .map_err(|err| anyhow::anyhow!("Failed to submit HTML chunk: {err}"))
    }

    /// Signal end of HTML stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the pipeline channel fails to send.
    pub fn finish_html(&self) -> Result<()> {
        self.stats.dom_complete.store(true, Ordering::Relaxed);
        self.parse_tx
            .send(PipelineWork::EndOfStream)
            .map_err(|err| anyhow::anyhow!("Failed to signal end of stream: {err}"))
    }

    /// Submit DOM updates to style stage.
    ///
    /// # Errors
    ///
    /// Returns an error if the pipeline channel fails to send.
    pub fn submit_dom_updates(&self, updates: Vec<DOMUpdate>) -> Result<()> {
        if !updates.is_empty() {
            self.style_tx
                .send(PipelineWork::DomUpdates(updates))
                .map_err(|err| anyhow::anyhow!("Failed to submit DOM updates: {err}"))?;
        }
        Ok(())
    }

    /// Submit nodes that need styling.
    ///
    /// # Errors
    ///
    /// Returns an error if the pipeline channel fails to send.
    pub fn submit_style_dirty(&self, nodes: Vec<NodeKey>) -> Result<()> {
        if !nodes.is_empty() {
            self.layout_tx
                .send(PipelineWork::StyleDirty(nodes))
                .map_err(|err| anyhow::anyhow!("Failed to submit style dirty: {err}"))?;
        }
        Ok(())
    }

    /// Submit nodes that need layout.
    ///
    /// # Errors
    ///
    /// Returns an error if the pipeline channel fails to send.
    pub fn submit_layout_dirty(&self, nodes: Vec<NodeKey>) -> Result<()> {
        if !nodes.is_empty() {
            self.paint_tx
                .send(PipelineWork::LayoutDirty(nodes))
                .map_err(|err| anyhow::anyhow!("Failed to submit layout dirty: {err}"))?;
        }
        Ok(())
    }

    /// Check if ready for first paint (incremental mode)
    pub fn ready_for_first_paint(&self) -> bool {
        match self.config.mode {
            RenderingMode::Buffered => self.stats.dom_complete.load(Ordering::Relaxed),
            RenderingMode::Incremental { initial_threshold } => {
                self.stats.ready_for_first_paint(initial_threshold)
            }
        }
    }

    /// Mark that first paint has occurred
    pub fn mark_first_paint(&self) {
        self.stats.mark_first_paint();
    }

    /// Get reference to stats
    pub fn stats(&self) -> &Arc<PipelineStats> {
        &self.stats
    }

    /// Try to receive work from parse stage (non-blocking)
    pub async fn try_recv_parse(&self) -> Option<PipelineWork> {
        self.parse_rx.lock().await.recv().await
    }

    /// Try to receive work from style stage (non-blocking)
    pub async fn try_recv_style(&self) -> Option<PipelineWork> {
        self.style_rx.lock().await.recv().await
    }

    /// Try to receive work from layout stage (non-blocking)
    pub async fn try_recv_layout(&self) -> Option<PipelineWork> {
        self.layout_rx.lock().await.recv().await
    }

    /// Try to receive work from paint stage (non-blocking)
    pub async fn try_recv_paint(&self) -> Option<PipelineWork> {
        self.paint_rx.lock().await.recv().await
    }
}

/// Batch processor for parallel work
pub struct BatchProcessor {
    batch_size: usize,
    _parallelism: usize,
}

impl BatchProcessor {
    pub fn new(parallelism: usize) -> Self {
        Self {
            batch_size: 64,
            _parallelism: parallelism,
        }
    }

    /// Process nodes in parallel batches.
    ///
    /// # Errors
    ///
    /// Returns an error if any processing function fails.
    pub fn process_parallel<T, F>(&self, items: Vec<T>, process_fn: F) -> Result<()>
    where
        T: Send + 'static,
        F: Fn(T) -> Result<()> + Send + Sync + 'static,
    {
        if items.is_empty() {
            return Ok(());
        }

        items
            .into_par_iter()
            .chunks(self.batch_size)
            .try_for_each(|batch| {
                for item in batch {
                    process_fn(item)?;
                }
                Ok::<(), Error>(())
            })?;

        Ok(())
    }
}
