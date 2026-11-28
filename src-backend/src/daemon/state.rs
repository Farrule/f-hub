use crate::config::Config;
use crate::devices;
use crate::model::{Capabilities, ConnectionMode, DeviceKind};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStateSnapshot {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub capabilities: Capabilities,
    pub mode: ConnectionMode,
    pub battery_percent: Option<u8>,
    pub charging_label: Option<String>,
    pub is_charging: bool,
    pub battery_stale: bool,
    pub active_slot: Option<u8>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub dry_run: bool,
    pub auto_switch: bool,
    pub active_exe: Option<String>,
    pub active_rule: String,
    pub config_path: String,
    pub updated_at_ms: u64,
    pub devices: Vec<DeviceStateSnapshot>,
}

impl AppSnapshot {
    pub fn initial(config_path: &Path, dry_run: bool) -> Self {
        let device_list = devices::all_drivers()
            .into_iter()
            .map(|drv| DeviceStateSnapshot {
                id: drv.id().to_string(),
                name: drv.name().to_string(),
                kind: drv.kind(),
                capabilities: drv.capabilities(),
                mode: ConnectionMode::Disconnected,
                battery_percent: None,
                charging_label: None,
                is_charging: false,
                battery_stale: false,
                active_slot: None,
                last_error: None,
            })
            .collect();

        Self {
            dry_run,
            auto_switch: true,
            active_exe: None,
            active_rule: "default".to_string(),
            config_path: config_path.display().to_string(),
            updated_at_ms: now_epoch_ms(),
            devices: device_list,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at_ms = now_epoch_ms();
    }

    pub fn device_mut(&mut self, device_id: &str) -> Option<&mut DeviceStateSnapshot> {
        self.devices.iter_mut().find(|d| d.id == device_id)
    }
}

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub enum DaemonCommand {
    ManualSwitchProfile {
        device_id: String,
        slot: u8,
        lock_auto: bool,
        reply: oneshot::Sender<Result<String, String>>,
    },
    SetAutoSwitch {
        enabled: bool,
        reply: oneshot::Sender<bool>,
    },
    RefreshBatteryNow {
        reply: oneshot::Sender<()>,
    },
    GetConfig {
        reply: oneshot::Sender<Config>,
    },
    UpdateConfig {
        config: Config,
        reply: oneshot::Sender<Result<(), Vec<String>>>,
    },
    #[allow(dead_code)] // Step 3A-2 のトレイ終了メニューから使用
    Shutdown,
}

#[derive(Clone)]
pub struct DaemonHandle {
    state_rx: watch::Receiver<AppSnapshot>,
    cmd_tx: mpsc::UnboundedSender<DaemonCommand>,
    ui_url: String,
}

impl DaemonHandle {
    pub fn new(
        state_rx: watch::Receiver<AppSnapshot>,
        cmd_tx: mpsc::UnboundedSender<DaemonCommand>,
        ui_url: String,
    ) -> Self {
        Self {
            state_rx,
            cmd_tx,
            ui_url,
        }
    }

    pub fn ui_url(&self) -> &str {
        &self.ui_url
    }

    pub fn snapshot(&self) -> AppSnapshot {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<AppSnapshot> {
        self.state_rx.clone()
    }

    pub async fn switch_profile(
        &self,
        device_id: String,
        slot: u8,
        lock_auto: bool,
    ) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DaemonCommand::ManualSwitchProfile {
                device_id,
                slot,
                lock_auto,
                reply: tx,
            })
            .map_err(|_| "デーモンは停止しています".to_string())?;
        rx.await
            .map_err(|_| "デーモンからの応答を受信できませんでした".to_string())?
    }

    pub async fn set_auto_switch(&self, enabled: bool) -> Result<bool, String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DaemonCommand::SetAutoSwitch { enabled, reply: tx })
            .map_err(|_| "デーモンは停止しています".to_string())?;
        rx.await
            .map_err(|_| "デーモンからの応答を受信できませんでした".to_string())
    }

    pub async fn refresh_battery_now(&self) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DaemonCommand::RefreshBatteryNow { reply: tx })
            .map_err(|_| "デーモンは停止しています".to_string())?;
        rx.await
            .map_err(|_| "デーモンからの応答を受信できませんでした".to_string())
    }

    pub async fn get_config(&self) -> Result<Config, String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DaemonCommand::GetConfig { reply: tx })
            .map_err(|_| "デーモンは停止しています".to_string())?;
        rx.await
            .map_err(|_| "デーモンからの応答を受信できませんでした".to_string())
    }

    pub async fn update_config(&self, config: Config) -> Result<(), Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(DaemonCommand::UpdateConfig { config, reply: tx })
            .map_err(|_| vec!["デーモンは停止しています".to_string()])?;
        rx.await
            .map_err(|_| vec!["デーモンからの応答を受信できませんでした".to_string()])?
    }

    #[allow(dead_code)] // Step 3A-2 のトレイ終了メニューから使用
    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(DaemonCommand::Shutdown);
    }
}