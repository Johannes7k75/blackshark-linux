#[derive(Clone, Debug, Default)]
pub struct SharedState {
    pub connected: bool,
    pub battery_pct: u8,
    pub charging: bool,
    pub sidetone: u8,
    pub thx_enabled: bool,
    pub anc_enabled: bool,
    pub anc_level: u8,
    pub power_savings_minutes: u8,
}
