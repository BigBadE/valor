use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ValorConfig {
    pub frame_budget_ms: u64,
    pub layout_debounce_ms: Option<u64>,
    pub hud_enabled: bool,
    pub telemetry_enabled: bool,
}

impl ValorConfig {
    pub fn new(frame_budget_ms: u64, layout_debounce_ms: Option<u64>, hud_enabled: bool, telemetry_enabled: bool) -> Self {
        Self { frame_budget_ms: frame_budget_ms.max(1), layout_debounce_ms, hud_enabled, telemetry_enabled }
    }

    pub fn from_env() -> Self {
        let frame_budget_ms = std::env::var("VALOR_FRAME_BUDGET_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(16)
            .max(1);
        let layout_debounce_ms = std::env::var("VALOR_LAYOUT_DEBOUNCE_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .and_then(|ms| if ms > 0 { Some(ms) } else { None });
        let hud_enabled = std::env::var("VALOR_HUD").ok().as_deref() == Some("1");
        let telemetry_enabled = std::env::var("VALOR_TELEMETRY").ok().as_deref() == Some("1");
        Self { frame_budget_ms, layout_debounce_ms, hud_enabled, telemetry_enabled }
    }

    pub fn frame_budget(&self) -> Duration { Duration::from_millis(self.frame_budget_ms) }
    pub fn layout_debounce(&self) -> Option<Duration> { self.layout_debounce_ms.map(Duration::from_millis) }
}
