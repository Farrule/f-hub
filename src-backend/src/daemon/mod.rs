pub mod notifier;
pub mod state;
pub mod watcher;
pub mod worker;

use crate::config::Config;
use crate::devices;
use crate::model::{BatteryInfo, ConnectionMode};
use crate::server;
use notifier::{send_os_notification, BatteryMonitor};
use state::{AppSnapshot, DaemonCommand, DaemonHandle};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use watcher::{ActiveAppWatcher, AppEvent};
use worker::DeviceWorkerPool;

enum PollItem {
    Battery {
        device_id: &'static str,
        device_name: &'static str,
        result: Result<(ConnectionMode, BatteryInfo), String>,
    },
    ProbeOnly {
        device_id: &'static str,
        mode: ConnectionMode,
    },
}

/// 実行ファイル名に対して適用すべき目標プロファイル一覧とルール名を算出する
fn resolve_target_profiles<'a>(cfg: &'a Config, exe_name: &str) -> (&'a str, BTreeMap<String, u8>) {
    let mut target = cfg.default.clone();

    for rule in &cfg.rules {
        let matched = rule
            .match_exe
            .iter()
            .any(|pattern| pattern.eq_ignore_ascii_case(exe_name));
        if matched {
            for (dev_id, &slot) in &rule.profiles {
                target.insert(dev_id.clone(), slot);
            }
            return (rule.name.as_str(), target);
        }
    }

    ("default", target)
}

fn trigger_poll(
    pool: &DeviceWorkerPool,
    poll_tx: &mpsc::UnboundedSender<Vec<PollItem>>,
) {
    let mut battery_inflight = Vec::new();
    for &(dev_id, dev_name) in pool.battery_targets() {
        match pool.read_battery(dev_id) {
            Ok(reply_rx) => battery_inflight.push((dev_id, dev_name, reply_rx)),
            Err(e) => {
                tracing::error!(device = dev_id, error = %e, "バッテリー取得要求の送信失敗");
            }
        }
    }

    let mut probe_inflight = Vec::new();
    for &(dev_id, _dev_name) in pool.probe_only_targets() {
        match pool.probe(dev_id) {
            Ok(reply_rx) => probe_inflight.push((dev_id, reply_rx)),
            Err(e) => {
                tracing::error!(device = dev_id, error = %e, "接続確認要求の送信失敗");
            }
        }
    }

    let tx_done = poll_tx.clone();
    tokio::spawn(async move {
        let mut results = Vec::with_capacity(battery_inflight.len() + probe_inflight.len());
        for (device_id, device_name, reply_rx) in battery_inflight {
            let result = match reply_rx.await {
                Ok(res) => res,
                Err(_) => Err("ワーカー応答チャネルが切断されました".to_string()),
            };
            results.push(PollItem::Battery {
                device_id,
                device_name,
                result,
            });
        }
        for (device_id, reply_rx) in probe_inflight {
            let mode = reply_rx.await.unwrap_or(ConnectionMode::Disconnected);
            results.push(PollItem::ProbeOnly { device_id, mode });
        }
        let _ = tx_done.send(results);
    });
}

async fn apply_rule_for_exe(
    cfg: &Config,
    exe_name: &str,
    pool: Option<&DeviceWorkerPool>,
    applied_slots: &mut BTreeMap<String, u8>,
    snapshot: &mut AppSnapshot,
) {
    let (rule_name, target) = resolve_target_profiles(cfg, exe_name);
    snapshot.active_exe = Some(exe_name.to_string());
    snapshot.active_rule = rule_name.to_string();

    let diffs: Vec<(String, u8)> = target
        .into_iter()
        .filter(|(dev_id, slot)| applied_slots.get(dev_id) != Some(slot))
        .collect();

    if diffs.is_empty() {
        return;
    }

    let diff_summary = diffs
        .iter()
        .map(|(id, slot)| format!("{id}={slot}"))
        .collect::<Vec<_>>()
        .join(", ");

    if let Some(pool) = pool {
        tracing::info!(
            exe = %exe_name,
            rule = rule_name,
            changes = %diff_summary,
            "プロファイル切替を実行します"
        );

        let mut inflight = Vec::with_capacity(diffs.len());
        for (dev_id, slot) in diffs {
            let rx_res = pool.switch_profile(&dev_id, slot);
            inflight.push((dev_id, slot, rx_res));
        }

        for (dev_id, slot, rx_res) in inflight {
            match rx_res {
                Ok(reply_rx) => match reply_rx.await {
                    Ok(Ok(msg)) => {
                        tracing::info!(
                            device = %dev_id,
                            slot,
                            message = %msg,
                            "プロファイル切替完了"
                        );
                        applied_slots.insert(dev_id.clone(), slot);
                        if let Some(dev) = snapshot.device_mut(&dev_id) {
                            dev.active_slot = Some(slot);
                            dev.last_error = None;
                        }
                    }
                    Ok(Err(err)) => {
                        tracing::warn!(
                            device = %dev_id,
                            slot,
                            error = %err,
                            "プロファイル切替失敗（次回切替時に再試行します）"
                        );
                        if let Some(dev) = snapshot.device_mut(&dev_id) {
                            dev.last_error = Some(err);
                        }
                    }
                    Err(_) => {
                        tracing::error!(
                            device = %dev_id,
                            "ワーカー応答チャネルが切断されました"
                        );
                    }
                },
                Err(err) => {
                    tracing::error!(
                        device = %dev_id,
                        error = %err,
                        "ワーカーへの送信失敗"
                    );
                }
            }
        }
    } else {
        tracing::info!(
            exe = %exe_name,
            rule = rule_name,
            changes = %diff_summary,
            "[DRY-RUN] プロファイル切替対象を検出"
        );
        for (dev_id, slot) in diffs {
            applied_slots.insert(dev_id.clone(), slot);
            if let Some(dev) = snapshot.device_mut(&dev_id) {
                dev.active_slot = Some(slot);
                dev.last_error = None;
            }
        }
    }
}

pub async fn run(
    mut cfg: Config,
    config_path: PathBuf,
    dry_run: bool,
    port: u16,
    no_tray: bool,
    open_ui: bool,
) -> Result<(), String> {
    let listener = server::bind(port).await?;
    let bound_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("127.0.0.1:{port}"));
    let ui_url = format!("http://{bound_addr}");

    let pool = if dry_run {
        None
    } else {
        Some(DeviceWorkerPool::spawn_all())
    };

    let mut snapshot = AppSnapshot::initial(&config_path, dry_run);
    let (state_tx, state_rx) = watch::channel(snapshot.clone());
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<DaemonCommand>();
    let handle = DaemonHandle::new(state_rx, cmd_tx, ui_url.clone());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server::serve(listener, handle.clone(), shutdown_rx));

    let tray = if no_tray {
        None
    } else {
        match crate::tray::SystemTray::start(
            ui_url.clone(),
            handle.clone(),
            tokio::runtime::Handle::current(),
        ) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::error!(error = %e, "システムトレイ起動失敗（--no-tray 相当で継続します）");
                None
            }
        }
    };

    if open_ui {
        if let Err(e) = crate::tray::launcher::focus_or_open_ui_window(&ui_url) {
            tracing::warn!(error = %e, "UI ウィンドウの自動起動に失敗しました");
        }
    }

    let (watcher, mut rx) = ActiveAppWatcher::start()?;
    let mut debounce = Duration::from_millis(cfg.general.debounce_ms);

    tracing::info!(
        url = %ui_url,
        dry_run,
        tray = tray.is_some(),
        debounce_ms = cfg.general.debounce_ms,
        battery_poll_sec = cfg.general.battery_poll_sec,
        low_battery_threshold = cfg.general.low_battery_threshold,
        "常駐デーモン＋内蔵サーバーを起動しました（終了: Ctrl+C またはトレイの終了）"
    );

    let mut pending: Option<AppEvent> = None;
    let mut applied_slots: BTreeMap<String, u8> = BTreeMap::new();
    let mut last_rule: Option<String> = None;

    let timer = sleep_until(Instant::now() + Duration::from_secs(86_400));
    tokio::pin!(timer);
    let mut timer_active = false;

    let mut battery_interval = interval(Duration::from_secs(cfg.general.battery_poll_sec));
    battery_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let (poll_tx, mut poll_rx) = mpsc::unbounded_channel::<Vec<PollItem>>();
    let mut poll_in_flight = false;
    let mut battery_monitor = BatteryMonitor::new(cfg.general.low_battery_threshold);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl+C を受信しました。デーモンを終了します...");
                break;
            }
            maybe_cmd = cmd_rx.recv() => {
                let Some(cmd) = maybe_cmd else {
                    break;
                };
                match cmd {
                    DaemonCommand::ManualSwitchProfile { device_id, slot, lock_auto, reply } => {
                        let res = handle_manual_switch(
                            &device_id,
                            slot,
                            lock_auto,
                            pool.as_ref(),
                            &mut applied_slots,
                            &mut snapshot,
                        ).await;
                        snapshot.touch();
                        let _ = state_tx.send(snapshot.clone());
                        let _ = reply.send(res);
                    }
                    DaemonCommand::SetAutoSwitch { enabled, reply } => {
                        snapshot.auto_switch = enabled;
                        tracing::info!(auto_switch = enabled, "自動切替モードを変更しました");
                        if enabled {
                            if let Some(exe) = snapshot.active_exe.clone() {
                                apply_rule_for_exe(
                                    &cfg,
                                    &exe,
                                    pool.as_ref(),
                                    &mut applied_slots,
                                    &mut snapshot,
                                ).await;
                                last_rule = Some(snapshot.active_rule.clone());
                            }
                        }
                        snapshot.touch();
                        let _ = state_tx.send(snapshot.clone());
                        let _ = reply.send(enabled);
                    }
                    DaemonCommand::RefreshBatteryNow { reply } => {
                        if let Some(pool) = &pool {
                            if !poll_in_flight {
                                poll_in_flight = true;
                                trigger_poll(pool, &poll_tx);
                            }
                        }
                        let _ = reply.send(());
                    }
                    DaemonCommand::GetConfig { reply } => {
                        let _ = reply.send(cfg.clone());
                    }
                    DaemonCommand::UpdateConfig { config: new_cfg, reply } => {
                        if let Err(errors) = new_cfg.validate() {
                            let _ = reply.send(Err(errors));
                            continue;
                        }
                        if let Err(err) = new_cfg.save(&config_path) {
                            let _ = reply.send(Err(vec![err]));
                            continue;
                        }

                        cfg = new_cfg;
                        debounce = Duration::from_millis(cfg.general.debounce_ms);
                        battery_monitor.set_threshold(cfg.general.low_battery_threshold);
                        battery_interval = interval(Duration::from_secs(cfg.general.battery_poll_sec));
                        battery_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
                        battery_interval.reset();

                        tracing::info!(
                            path = %config_path.display(),
                            rules = cfg.rules.len(),
                            "config.toml を保存・再読込しました"
                        );

                        if snapshot.auto_switch {
                            if let Some(exe) = snapshot.active_exe.clone() {
                                apply_rule_for_exe(
                                    &cfg,
                                    &exe,
                                    pool.as_ref(),
                                    &mut applied_slots,
                                    &mut snapshot,
                                ).await;
                                last_rule = Some(snapshot.active_rule.clone());
                            }
                        }
                        snapshot.touch();
                        let _ = state_tx.send(snapshot.clone());
                        let _ = reply.send(Ok(()));
                    }
                    DaemonCommand::ReadRemap {
                        device_id,
                        layer,
                        reply,
                    } => match &pool {
                        Some(p) => match p.read_remap(&device_id, layer) {
                            Ok(rx) => {
                                tokio::spawn(async move {
                                    let res = rx
                                        .await
                                        .unwrap_or_else(|_| Err("ワーカー応答の受信に失敗しました".to_string()));
                                    let _ = reply.send(res);
                                });
                            }
                            Err(e) => {
                                let _ = reply.send(Err(e));
                            }
                        },
                        None => {
                            let _ = reply.send(Err("HID ワーカースレッドが初期化されていません".to_string()));
                        }
                    },
                    DaemonCommand::WriteRemap {
                        device_id,
                        layer,
                        slot_id,
                        action,
                        reply,
                    } => match &pool {
                        Some(p) => match p.write_remap(&device_id, layer, &slot_id, &action) {
                            Ok(rx) => {
                                tokio::spawn(async move {
                                    let res = rx
                                        .await
                                        .unwrap_or_else(|_| Err("ワーカー応答の受信に失敗しました".to_string()));
                                    let _ = reply.send(res);
                                });
                            }
                            Err(e) => {
                                let _ = reply.send(Err(e));
                            }
                        },
                        None => {
                            let _ = reply.send(Err("HID ワーカースレッドが初期化されていません".to_string()));
                        }
                    },
                    DaemonCommand::ResetRemap {
                        device_id,
                        layer,
                        slot_id,
                        reply,
                    } => match &pool {
                        Some(p) => match p.reset_remap(&device_id, layer, slot_id.as_deref()) {
                            Ok(rx) => {
                                tokio::spawn(async move {
                                    let res = rx
                                        .await
                                        .unwrap_or_else(|_| Err("ワーカー応答の受信に失敗しました".to_string()));
                                    let _ = reply.send(res);
                                });
                            }
                            Err(e) => {
                                let _ = reply.send(Err(e));
                            }
                        },
                        None => {
                            let _ = reply.send(Err("HID ワーカースレッドが初期化されていません".to_string()));
                        }
                    },
                    DaemonCommand::Shutdown => {
                        tracing::info!("終了コマンドを受信しました。デーモンを終了します...");
                        break;
                    }
                }
            }
            maybe_ev = rx.recv() => {
                let Some(ev) = maybe_ev else {
                    tracing::warn!("ActiveAppWatcher チャネルが閉じられました");
                    break;
                };
                tracing::debug!(exe = %ev.exe_name, pid = ev.pid, "前面ウィンドウ変化を検知");
                pending = Some(ev);
                timer.as_mut().reset(Instant::now() + debounce);
                timer_active = true;
            }
            _ = &mut timer, if timer_active => {
                timer_active = false;
                let Some(ev) = pending.take() else {
                    continue;
                };

                if !snapshot.auto_switch {
                    snapshot.active_exe = Some(ev.exe_name.clone());
                    snapshot.touch();
                    let _ = state_tx.send(snapshot.clone());
                    tracing::debug!(exe = %ev.exe_name, "手動固定モードのため自動切替をスキップ");
                    continue;
                }

                let prev_rule = last_rule.clone();
                apply_rule_for_exe(
                    &cfg,
                    &ev.exe_name,
                    pool.as_ref(),
                    &mut applied_slots,
                    &mut snapshot,
                ).await;

                if prev_rule.as_deref() != Some(snapshot.active_rule.as_str()) {
                    tracing::info!(
                        exe = %ev.exe_name,
                        rule = %snapshot.active_rule,
                        "アクティブアプリ・ルール更新"
                    );
                    last_rule = Some(snapshot.active_rule.clone());
                }

                snapshot.touch();
                let _ = state_tx.send(snapshot.clone());
            }
            _ = battery_interval.tick(), if pool.is_some() && !poll_in_flight => {
                if let Some(pool) = &pool {
                    poll_in_flight = true;
                    trigger_poll(pool, &poll_tx);
                }
            }
            Some(items) = poll_rx.recv() => {
                poll_in_flight = false;
                for item in items {
                    match item {
                        PollItem::Battery { device_id, device_name, result } => {
                            match result {
                                Ok((mode, info)) => {
                                    let level = info
                                        .percent
                                        .map(|p| format!("{p}%"))
                                        .unwrap_or_else(|| "-".to_string());
                                    let state = info
                                        .charging
                                        .as_ref()
                                        .map(|c| c.to_string())
                                        .unwrap_or_else(|| "-".to_string());

                                    tracing::info!(
                                        device = device_id,
                                        mode = %mode,
                                        level = %level,
                                        state = %state,
                                        "バッテリー取得"
                                    );

                                    if let Some((title, body)) = battery_monitor.record_success(
                                        device_id,
                                        device_name,
                                        mode,
                                        &info,
                                    ) {
                                        tracing::warn!(
                                            device = device_id,
                                            level = %level,
                                            "低バッテリー OS 通知を送信します"
                                        );
                                        tokio::spawn(async move {
                                            if let Err(e) = send_os_notification(title, body).await {
                                                tracing::error!(error = %e, "OS 通知送信エラー");
                                            }
                                        });
                                    }

                                    if let (Some(cached), Some(dev)) = (
                                        battery_monitor.get(device_id),
                                        snapshot.device_mut(device_id),
                                    ) {
                                        dev.mode = cached.mode;
                                        dev.battery_percent = cached.percent;
                                        dev.charging_label = cached.charging_label.clone();
                                        dev.is_charging = cached.is_charging;
                                        dev.battery_stale = cached.stale;
                                        dev.last_error = None;
                                    }
                                }
                                Err(err) => {
                                    if let Some(cached) = battery_monitor.record_failure(device_id) {
                                        tracing::warn!(
                                            device = device_id,
                                            last_percent = ?cached.percent,
                                            stale = cached.stale,
                                            error = %err,
                                            "バッテリー取得失敗（前回値を保持してスキップ）"
                                        );
                                        if let Some(dev) = snapshot.device_mut(device_id) {
                                            dev.battery_stale = true;
                                            dev.last_error = Some(err);
                                        }
                                    } else {
                                        tracing::warn!(
                                            device = device_id,
                                            error = %err,
                                            "バッテリー取得失敗（未接続またはスリープ）"
                                        );
                                        if let Some(dev) = snapshot.device_mut(device_id) {
                                            dev.mode = ConnectionMode::Disconnected;
                                            dev.last_error = Some(err);
                                        }
                                    }
                                }
                            }
                        }
                        PollItem::ProbeOnly { device_id, mode } => {
                            if let Some(dev) = snapshot.device_mut(device_id) {
                                dev.mode = mode;
                            }
                        }
                    }
                }
                snapshot.touch();
                let _ = state_tx.send(snapshot.clone());
            }
        }
    }

    crate::tray::launcher::close_ui_windows();
    if let Some(tray) = tray {
        tray.stop();
    }
    // SSE ストリーム (/api/events) を即座に終端させてからサーバー停止を待つ
    drop(state_tx);
    let _ = shutdown_tx.send(true);
    let mut server_task = server_task;
    if tokio::time::timeout(Duration::from_secs(2), &mut server_task)
        .await
        .is_err()
    {
        tracing::warn!("内蔵サーバーの終了待機がタイムアウトしたため強制停止します");
        server_task.abort();
    }
    watcher.stop();
    if let Some(pool) = pool {
        pool.shutdown();
    }
    tracing::info!("デーモンを正常終了しました");
    Ok(())
}

async fn handle_manual_switch(
    device_id: &str,
    slot: u8,
    lock_auto: bool,
    pool: Option<&DeviceWorkerPool>,
    applied_slots: &mut BTreeMap<String, u8>,
    snapshot: &mut AppSnapshot,
) -> Result<String, String> {
    let drv = devices::find_driver(device_id)
        .ok_or_else(|| format!("未知のデバイスID: '{device_id}'"))?;
    let caps = drv.capabilities();
    if !caps.profile {
        return Err(format!("{} はプロファイル切替非対応です", drv.name()));
    }
    if let Some((min, max)) = caps.profile_slots {
        if !(min..=max).contains(&slot) {
            return Err(format!(
                "{} のスロット番号は {min}〜{max} の範囲で指定してください",
                drv.name()
            ));
        }
    }

    if lock_auto {
        snapshot.auto_switch = false;
    }

    if let Some(pool) = pool {
        let reply_rx = pool.switch_profile(device_id, slot)?;
        let msg = reply_rx
            .await
            .map_err(|_| "ワーカー応答チャネルが切断されました".to_string())??;
        applied_slots.insert(device_id.to_string(), slot);
        if let Some(dev) = snapshot.device_mut(device_id) {
            dev.active_slot = Some(slot);
            dev.last_error = None;
        }
        tracing::info!(
            device = device_id,
            slot,
            lock_auto,
            "手動プロファイル切替完了"
        );
        Ok(msg)
    } else {
        applied_slots.insert(device_id.to_string(), slot);
        if let Some(dev) = snapshot.device_mut(device_id) {
            dev.active_slot = Some(slot);
            dev.last_error = None;
        }
        Ok(format!("[DRY-RUN] {device_id} -> slot {slot}"))
    }
}