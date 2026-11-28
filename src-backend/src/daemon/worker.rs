use crate::devices;
use crate::model::{BatteryInfo, ConnectionMode};
use hidapi::HidApi;
use std::collections::HashMap;
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};

pub enum WorkerCommand {
    SwitchProfile {
        slot: u8,
        reply: oneshot::Sender<Result<String, String>>,
    },
    ReadBattery {
        reply: oneshot::Sender<Result<(ConnectionMode, BatteryInfo), String>>,
    },
}

pub struct DeviceWorkerPool {
    workers: HashMap<String, mpsc::UnboundedSender<WorkerCommand>>,
    battery_targets: Vec<(&'static str, &'static str)>,
    handles: Vec<JoinHandle<()>>,
}

impl DeviceWorkerPool {
    /// 登録済み全デバイス分の直列ワーカーを起動する
    pub fn spawn_all() -> Self {
        let mut workers = HashMap::new();
        let mut battery_targets = Vec::new();
        let mut handles = Vec::new();

        for drv in devices::all_drivers() {
            let device_id = drv.id();
            let device_name = drv.name();
            if drv.capabilities().battery {
                battery_targets.push((device_id, device_name));
            }

            let (tx, mut rx) = mpsc::unbounded_channel::<WorkerCommand>();

            let handle = std::thread::Builder::new()
                .name(format!("f-hub-worker-{device_id}"))
                .spawn(move || {
                    let Some(drv) = devices::find_driver(device_id) else {
                        tracing::error!(device = device_id, "ワーカー用ドライバが見つかりません");
                        return;
                    };

                    let mut api = match HidApi::new() {
                        Ok(api) => api,
                        Err(e) => {
                            tracing::error!(
                                device = device_id,
                                error = %e,
                                "ワーカー用 HidApi 初期化失敗"
                            );
                            return;
                        }
                    };

                    tracing::debug!(device = device_id, "デバイスワーカー起動");

                    while let Some(cmd) = rx.blocking_recv() {
                        if let Err(e) = api.refresh_devices() {
                            tracing::warn!(
                                device = device_id,
                                error = %e,
                                "HID デバイス一覧の再取得に失敗"
                            );
                        }

                        match cmd {
                            WorkerCommand::SwitchProfile { slot, reply } => {
                                let presence = drv.probe(&api);
                                if presence.mode == ConnectionMode::Disconnected {
                                    let _ = reply.send(Err("未接続（または電源オフ）".to_string()));
                                    continue;
                                }

                                let res = drv
                                    .switch_profile(&api, slot)
                                    .map(|r| r.message)
                                    .map_err(|e| e.to_string());
                                let _ = reply.send(res);
                            }
                            WorkerCommand::ReadBattery { reply } => {
                                let presence = drv.probe(&api);
                                if presence.mode == ConnectionMode::Disconnected {
                                    let _ = reply.send(Err("未接続（または電源オフ）".to_string()));
                                    continue;
                                }

                                let res = drv
                                    .read_battery(&api)
                                    .map(|info| (presence.mode, info))
                                    .map_err(|e| e.to_string());
                                let _ = reply.send(res);
                            }
                        }
                    }

                    tracing::debug!(device = device_id, "デバイスワーカー停止");
                })
                .expect("デバイスワーカースレッド起動失敗");

            workers.insert(device_id.to_string(), tx);
            handles.push(handle);
        }

        Self {
            workers,
            battery_targets,
            handles,
        }
    }

    /// バッテリー監視対象デバイス (id, name) の一覧を返す
    pub fn battery_targets(&self) -> &[(&'static str, &'static str)] {
        &self.battery_targets
    }

    /// 指定デバイスのワーカーキューへプロファイル切替コマンドを投入する
    pub fn switch_profile(
        &self,
        device_id: &str,
        slot: u8,
    ) -> Result<oneshot::Receiver<Result<String, String>>, String> {
        let tx = self
            .workers
            .get(device_id)
            .ok_or_else(|| format!("ワーカー未登録: {device_id}"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerCommand::SwitchProfile {
            slot,
            reply: reply_tx,
        })
        .map_err(|_| format!("ワーカー停止済み: {device_id}"))?;

        Ok(reply_rx)
    }

    /// 指定デバイスのワーカーキューへバッテリー取得コマンドを投入する
    pub fn read_battery(
        &self,
        device_id: &str,
    ) -> Result<oneshot::Receiver<Result<(ConnectionMode, BatteryInfo), String>>, String> {
        let tx = self
            .workers
            .get(device_id)
            .ok_or_else(|| format!("ワーカー未登録: {device_id}"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerCommand::ReadBattery { reply: reply_tx })
            .map_err(|_| format!("ワーカー停止済み: {device_id}"))?;

        Ok(reply_rx)
    }

    /// 全ワーカーへ終了を通知してスレッド終了を待機する
    pub fn shutdown(mut self) {
        self.workers.clear();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}