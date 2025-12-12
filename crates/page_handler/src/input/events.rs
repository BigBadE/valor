/// Keyboard modifier flags for key events.
///
/// This structure tracks the state of common keyboard modifiers
/// during key press and key release events.
#[derive(Copy, Clone, Debug, Default)]
pub struct KeyMods {
    /// Whether the Control (Ctrl) key is pressed.
    pub ctrl: bool,
    /// Whether the Alt key is pressed.
    pub alt: bool,
    /// Whether the Shift key is pressed.
    pub shift: bool,
}
