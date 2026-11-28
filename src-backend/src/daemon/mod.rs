pub mod notifier;
pub mod watcher;
pub mod worker;

use crate::config::Config;
use crate::model::{BatteryInfo, ConnectionMode};
use notifier::{send_os_notification, BatteryMonitor};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep_until, Instant, MissedTickBehavior};
use watcher::{ActiveAppWatcher, AppEvent};
use worker::DeviceWorkerPool;

struct BatteryPollItem {
    device_id: &'static str,
    device_name: &'static str,
    result: Result<(ConnectionMode, BatteryInfo), String>,
}

/// 実行ファイル名に対して適用すべき目標プロファイル一覧とルール名を算出する
fn resolve_target_profiles<'a>(cfg: &'a Config, exe_name: &str) -> (&'a str, BTreeMap<String, u8>) {
    // ルール未一致時は [default] を維持
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

pub async fn run(cfg: Config, dry_run: bool) -> Result<(), String> {
    let pool = if dry_run {
        None
    } else {
        Some(DeviceWorkerPool::spawn_all())
    };

    let (watcher, mut rx) = ActiveAppWatcher::start()?;
    let debounce = Duration::from_millis(cfg.general.debounce_ms);

    tracing::info!(
        dry_run,
        debounce_ms = cfg.general.debounce_ms,
        battery_poll_sec = cfg.general.battery_poll_sec,
        low_battery_threshold = cfg.general.low_battery_threshold,
        "常駐デーモンを起動しました（終了: Ctrl+C）"
    );

    let mut pending: Option<AppEvent> = None;
    let mut applied_slots: BTreeMap<String, u8> = BTreeMap::new();
    let mut last_rule: Option<String> = None;

    let timer = sleep_until(Instant::now() + Duration::from_secs(86_400));
    tokio::pin!(timer);
    let mut timer_active = false;

    // バッテリー定期監視（起動直後に初回取得し、以降は battery_poll_sec 間隔）
    let mut battery_interval = interval(Duration::from_secs(cfg.general.battery_poll_sec));
    battery_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let (battery_tx, mut battery_rx) = mpsc::unbounded_channel::<Vec<BatteryPollItem>>();
    let mut battery_poll_in_flight = false;
    let mut battery_monitor = BatteryMonitor::new(cfg.general.low_battery_threshold);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl+C を受信しました。デーモンを終了します...");
                break;
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

                let (rule_name, target) = resolve_target_profiles(&cfg, &ev.exe_name);

                // 差分のあるデバイスだけ抽出
                let diffs: Vec<(String, u8)> = target
                    .into_iter()
                    .filter(|(dev_id, slot)| applied_slots.get(dev_id) != Some(slot))
                    .collect();

                if diffs.is_empty() {
                    if last_rule.as_deref() != Some(rule_name) {
                        tracing::info!(
                            exe = %ev.exe_name,
                            rule = rule_name,
                            "ルール確定（変更なし・維持）"
                        );
                        last_rule = Some(rule_name.to_string());
                    }
                    continue;
                }

                let diff_summary = diffs
                    .iter()
                    .map(|(id, slot)| format!("{id}={slot}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                if let Some(pool) = &pool {
                    tracing::info!(
                        exe = %ev.exe_name,
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
                                    applied_slots.insert(dev_id, slot);
                                }
                                Ok(Err(err)) => {
                                    tracing::warn!(
                                        device = %dev_id,
                                        slot,
                                        error = %err,
                                        "プロファイル切替失敗（次回切替時に再試行します）"
                                    );
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
                        exe = %ev.exe_name,
                        rule = rule_name,
                        changes = %diff_summary,
                        "[DRY-RUN] プロファイル切替対象を検出"
                    );
                    for (dev_id, slot) in diffs {
                        applied_slots.insert(dev_id, slot);
                    }
                }

                last_rule = Some(rule_name.to_string());
            }
            _ = battery_interval.tick(), if pool.is_some() && !battery_poll_in_flight => {
                if let Some(pool) = &pool {
                    battery_poll_in_flight = true;
                    let mut inflight = Vec::new();
                    for &(dev_id, dev_name) in pool.battery_targets() {
                        match pool.read_battery(dev_id) {
                            Ok(reply_rx) => inflight.push((dev_id, dev_name, reply_rx)),
                            Err(e) => {
                                tracing::error!(device = dev_id, error = %e, "バッテリー取得要求の送信失敗");
                            }
                        }
                    }

                    let tx_done = battery_tx.clone();
                    tokio::spawn(async move {
                        let mut results = Vec::with_capacity(inflight.len());
                        for (device_id, device_name, reply_rx) in inflight {
                            let result = match reply_rx.await {
                                Ok(res) => res,
                                Err(_) => Err("ワーカー応答チャネルが切断されました".to_string()),
                            };
                            results.push(BatteryPollItem {
                                device_id,
                                device_name,
                                result,
                            });
                        }
                        let _ = tx_done.send(results);
                    });
                }
            }
            Some(items) = battery_rx.recv() => {
                battery_poll_in_flight = false;
                for item in items {
                    match item.result {
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
                                device = item.device_id,
                                mode = %mode,
                                level = %level,
                                state = %state,
                                "バッテリー取得"
                            );

                            if let Some((title, body)) = battery_monitor.record_success(
                                item.device_id,
                                item.device_name,
                                mode,
                                &info,
                            ) {
                                tracing::warn!(
                                    device = item.device_id,
                                    level = %level,
                                    "低バッテリー OS 通知を送信します"
                                );
                                tokio::spawn(async move {
                                    if let Err(e) = send_os_notification(title, body).await {
                                        tracing::error!(error = %e, "OS 通知送信エラー");
                                    }
                                });
                            }
                        }
                        Err(err) => {
                            if let Some(cached) = battery_monitor.record_failure(item.device_id) {
                                tracing::warn!(
                                    device = item.device_id,
                                    last_percent = ?cached.percent,
                                    stale = cached.stale,
                                    error = %err,
                                    "バッテリー取得失敗（前回値を保持してスキップ）"
                                );
                            } else {
                                tracing::warn!(
                                    device = item.device_id,
                                    error = %err,
                                    "バッテリー取得失敗（未接続またはスリープ）"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    watcher.stop();
    if let Some(pool) = pool {
        pool.shutdown();
    }
    tracing::info!("デーモンを正常終了しました");
    Ok(())
}