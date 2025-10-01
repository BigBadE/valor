//! Configuration settings for the Valor browser engine.
//!
//! This module defines runtime configuration for frame budgeting, layout debouncing,
//! HUD display, and telemetry. Configuration can be loaded from environment variables
//! or constructed programmatically.

use core::time::Duration;
use std::env;

/// Runtime configuration for the Valor browser engine.
///
/// Controls frame budget timing, layout debouncing, and feature flags for debugging
/// and performance monitoring.
#[derive(Clone, Debug)]
pub struct ValorConfig {
    /// Frame budget in milliseconds for layout computation throttling
    pub frame_budget_ms: u64,
    /// Optional layout debounce period in milliseconds
    pub layout_debounce_ms: Option<u64>,
    /// Whether to display the performance HUD overlay
    pub hud_enabled: bool,
    /// Whether to emit telemetry data to stdout
    pub telemetry_enabled: bool,
}

impl ValorConfig {
    /// Construct a new `ValorConfig` with explicit values.
    ///
    /// # Arguments
    ///
    /// * `frame_budget_ms` - Frame budget in milliseconds (minimum 1ms)
    /// * `layout_debounce_ms` - Optional layout debounce period in milliseconds
    /// * `hud_enabled` - Whether to show the performance HUD
    /// * `telemetry_enabled` - Whether to emit telemetry logs
    ///
    /// # Returns
    ///
    /// A new `ValorConfig` instance with the specified settings
    #[inline]
    #[must_use]
    pub const fn new(
        frame_budget_ms: u64,
        layout_debounce_ms: Option<u64>,
        hud_enabled: bool,
        telemetry_enabled: bool,
    ) -> Self {
        let budget = if frame_budget_ms < 1 {
            1
        } else {
            frame_budget_ms
        };
        Self {
            frame_budget_ms: budget,
            layout_debounce_ms,
            hud_enabled,
            telemetry_enabled,
        }
    }

    /// Load configuration from environment variables.
    ///
    /// Reads the following environment variables:
    /// - `VALOR_FRAME_BUDGET_MS`: Frame budget in milliseconds (default: 16)
    /// - `VALOR_LAYOUT_DEBOUNCE_MS`: Layout debounce period in milliseconds
    /// - `VALOR_HUD`: Set to "1" to enable HUD (default: disabled)
    /// - `VALOR_TELEMETRY`: Set to "1" to enable telemetry (default: disabled)
    ///
    /// # Returns
    ///
    /// A new `ValorConfig` instance populated from environment variables
    #[inline]
    #[must_use]
    pub fn from_env() -> Self {
        let frame_budget_ms = env::var("VALOR_FRAME_BUDGET_MS")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .unwrap_or(16)
            .max(1);
        let layout_debounce_ms = env::var("VALOR_LAYOUT_DEBOUNCE_MS")
            .ok()
            .and_then(|val| val.parse::<u64>().ok())
            .and_then(|millis| (millis > 0).then_some(millis));
        let hud_enabled = env::var("VALOR_HUD").ok().as_deref() == Some("1");
        let telemetry_enabled = env::var("VALOR_TELEMETRY").ok().as_deref() == Some("1");
        Self {
            frame_budget_ms,
            layout_debounce_ms,
            hud_enabled,
            telemetry_enabled,
        }
    }

    /// Get the frame budget as a `Duration`.
    ///
    /// # Returns
    ///
    /// The frame budget duration
    #[inline]
    #[must_use]
    pub const fn frame_budget(&self) -> Duration {
        Duration::from_millis(self.frame_budget_ms)
    }

    /// Get the layout debounce period as an optional `Duration`.
    ///
    /// # Returns
    ///
    /// The layout debounce duration if configured, otherwise `None`
    #[inline]
    #[must_use]
    pub const fn layout_debounce(&self) -> Option<Duration> {
        if let Some(millis) = self.layout_debounce_ms {
            Some(Duration::from_millis(millis))
        } else {
            None
        }
    }
}
