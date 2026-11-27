use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Trackball,
}

impl fmt::Display for DeviceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse",
            Self::Trackball => "Trackball",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    Wired,
    #[serde(rename = "wireless_24g")]
    Wireless24G,
    Disconnected,
}

impl fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Self::Wired => "有線",
            Self::Wireless24G => "2.4G",
            Self::Disconnected => "未接続",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargingState {
    Charging,
    Discharging,
    FullOrIdle,
    Unknown(u8),
}

impl fmt::Display for ChargingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Charging => f.pad("充電中"),
            Self::Discharging => f.pad("放電中"),
            Self::FullOrIdle => f.pad("満充電/待機"),
            Self::Unknown(v) => f.pad(&format!("不明(0x{:02x})", v)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Capabilities {
    pub battery: bool,
    pub profile: bool,
    /// プロファイル切替対応時の有効スロット範囲 (min, max)
    pub profile_slots: Option<(u8, u8)>,
    /// Phase 3-B: キー／ボタンリマップ対応可否
    pub remap: bool,
    /// 制御・リマップに有線接続が必要な場合 true（例: LUMINKEY NOVA KINE）
    pub wired_only_control: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DevicePresence {
    pub mode: ConnectionMode,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub product_name: Option<String>,
}

impl DevicePresence {
    pub fn disconnected() -> Self {
        Self {
            mode: ConnectionMode::Disconnected,
            vid: None,
            pid: None,
            product_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BatteryInfo {
    pub percent: Option<u8>,
    pub charging: Option<ChargingState>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileResult {
    pub slot: u8,
    pub message: String,
}

#[derive(Debug)]
pub enum DeviceError {
    Disconnected,
    Unsupported,
    Communication(String),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "未接続"),
            Self::Unsupported => write!(f, "非対応"),
            Self::Communication(msg) => write!(f, "{msg}"),
        }
    }
}