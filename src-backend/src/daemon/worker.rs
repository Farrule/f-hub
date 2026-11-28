use crate::devices;
use crate::model::{BatteryInfo, ConnectionMode};
use crate::remap::{RemapAction, RemapState};
use hidapi::HidApi;
use std::collections::HashMap;
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};

pub enum WorkerCommand {
    Probe {
        reply: oneshot::Sender<ConnectionMode>,
    },
    SwitchProfile {
        slot: u8,
        reply: oneshot::Sender<Result<String, String>>,
    },
    ReadBattery {
        reply: oneshot::Sender<Result<(ConnectionMode, BatteryInfo), String>>,
    },
    ReadRemap {
        layer: u8,
        reply: oneshot::Sender<Result<RemapState, String>>,
    },
    WriteRemap {
        layer: u8,
        slot_id: String,
        action: RemapAction,
        reply: oneshot::Sender<Result<RemapState, String>>,
    },
    ResetRemap {
        layer: u8,
        slot_id: Option<String>,
        reply: oneshot::Sender<Result<RemapState, String>>,
    },
}

pub struct DeviceWorkerPool {
    workers: HashMap<String, mpsc::UnboundedSender<WorkerCommand>>,
    battery_targets: Vec<(&'static str, &'static str)>,
    probe_only_targets: Vec<(&'static str, &'static str)>,
    handles: Vec<JoinHandle<()>>,
}

impl DeviceWorkerPool {
    /// 登録済み全デバイス分の直列ワーカーを起動する
    pub fn spawn_all() -> Self {
        let mut workers = HashMap::new();
        let mut battery_targets = Vec::new();
        let mut probe_only_targets = Vec::new();
        let mut handles = Vec::new();

        for drv in devices::all_drivers() {
            let device_id = drv.id();
            let device_name = drv.name();
            if drv.capabilities().battery {
                battery_targets.push((device_id, device_name));
            } else {
                probe_only_targets.push((device_id, device_name));
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
                            WorkerCommand::Probe { reply } => {
                                let presence = drv.probe(&api);
                                let _ = reply.send(presence.mode);
                            }
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
                            WorkerCommand::ReadRemap { layer, reply } => {
                                let presence = drv.probe(&api);
                                if presence.mode == ConnectionMode::Disconnected {
                                    let _ = reply.send(Err("未接続（または電源オフ）".to_string()));
                                    continue;
                                }

                                let res = drv.read_remap(&api, layer).map_err(|e| e.to_string());
                                let _ = reply.send(res);
                            }
                            WorkerCommand::WriteRemap {
                                layer,
                                slot_id,
                                action,
                                reply,
                            } => {
                                let presence = drv.probe(&api);
                                if presence.mode == ConnectionMode::Disconnected {
                                    let _ = reply.send(Err("未接続（または電源オフ）".to_string()));
                                    continue;
                                }

                                let res = drv
                                    .write_remap(&api, layer, &slot_id, &action)
                                    .and_then(|_| drv.read_remap(&api, layer))
                                    .map_err(|e| e.to_string());
                                let _ = reply.send(res);
                            }
                            WorkerCommand::ResetRemap {
                                layer,
                                slot_id,
                                reply,
                            } => {
                                let presence = drv.probe(&api);
                                if presence.mode == ConnectionMode::Disconnected {
                                    let _ = reply.send(Err("未接続（または電源オフ）".to_string()));
                                    continue;
                                }

                                let res = match slot_id {
                                    Some(id) => drv
                                        .write_remap(&api, layer, &id, &RemapAction::Default)
                                        .and_then(|_| drv.read_remap(&api, layer))
                                        .map_err(|e| e.to_string()),
                                    None => drv
                                        .read_remap(&api, layer)
                                        .and_then(|state| {
                                            for slot in &state.slots {
                                                drv.write_remap(
                                                    &api,
                                                    layer,
                                                    &slot.slot_id,
                                                    &RemapAction::Default,
                                                )?;
                                            }
                                            drv.read_remap(&api, layer)
                                        })
                                        .map_err(|e| e.to_string()),
                                };
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
            probe_only_targets,
            handles,
        }
    }

    /// バッテリー監視対象デバイス (id, name) の一覧を返す
    pub fn battery_targets(&self) -> &[(&'static str, &'static str)] {
        &self.battery_targets
    }

    /// バッテリー非対応で接続確認のみ行うデバイス (id, name) の一覧を返す
    pub fn probe_only_targets(&self) -> &[(&'static str, &'static str)] {
        &self.probe_only_targets
    }

    /// 指定デバイスの接続状態のみ確認する
    pub fn probe(&self, device_id: &str) -> Result<oneshot::Receiver<ConnectionMode>, String> {
        let tx = self
            .workers
            .get(device_id)
            .ok_or_else(|| format!("ワーカー未登録: {device_id}"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerCommand::Probe { reply: reply_tx })
            .map_err(|_| format!("ワーカー停止済み: {device_id}"))?;

        Ok(reply_rx)
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

    /// 指定デバイスのワーカーキューへリマップ読み出しコマンドを投入する
    pub fn read_remap(
        &self,
        device_id: &str,
        layer: u8,
    ) -> Result<oneshot::Receiver<Result<RemapState, String>>, String> {
        let tx = self
            .workers
            .get(device_id)
            .ok_or_else(|| format!("ワーカー未登録: {device_id}"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerCommand::ReadRemap {
            layer,
            reply: reply_tx,
        })
        .map_err(|_| format!("ワーカー停止済み: {device_id}"))?;

        Ok(reply_rx)
    }

    /// 指定デバイスのワーカーキューへリマップ書き込みコマンドを投入する
    pub fn write_remap(
        &self,
        device_id: &str,
        layer: u8,
        slot_id: &str,
        action: &RemapAction,
    ) -> Result<oneshot::Receiver<Result<RemapState, String>>, String> {
        let tx = self
            .workers
            .get(device_id)
            .ok_or_else(|| format!("ワーカー未登録: {device_id}"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerCommand::WriteRemap {
            layer,
            slot_id: slot_id.to_string(),
            action: action.clone(),
            reply: reply_tx,
        })
        .map_err(|_| format!("ワーカー停止済み: {device_id}"))?;

        Ok(reply_rx)
    }

    /// 指定デバイスのワーカーキューへリマップ初期化コマンドを投入する
    pub fn reset_remap(
        &self,
        device_id: &str,
        layer: u8,
        slot_id: Option<&str>,
    ) -> Result<oneshot::Receiver<Result<RemapState, String>>, String> {
        let tx = self
            .workers
            .get(device_id)
            .ok_or_else(|| format!("ワーカー未登録: {device_id}"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerCommand::ResetRemap {
            layer,
            slot_id: slot_id.map(|s| s.to_string()),
            reply: reply_tx,
        })
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