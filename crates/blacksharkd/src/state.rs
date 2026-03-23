#[derive(Clone, Debug, Default)]
pub struct SharedState {
    pub connected: bool,
    pub battery_pct: u8,
    pub charging: bool,
    pub sidetone: u8,
}
