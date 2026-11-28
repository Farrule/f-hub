pub mod watcher;
pub mod worker;

use crate::config::Config;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::{sleep_until, Instant};
use watcher::{ActiveAppWatcher, AppEvent};
use worker::DeviceWorkerPool;

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
        "ActiveAppWatcher を起動しました（終了: Ctrl+C）"
    );

    let mut pending: Option<AppEvent> = None;
    let mut applied_slots: BTreeMap<String, u8> = BTreeMap::new();
    let mut last_rule: Option<String> = None;

    let timer = sleep_until(Instant::now() + Duration::from_secs(86_400));
    tokio::pin!(timer);
    let mut timer_active = false;

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

                    // 1. 全対象デバイスのワーカーへ一斉投入（デバイス単位で並列実行）
                    let mut inflight = Vec::with_capacity(diffs.len());
                    for (dev_id, slot) in diffs {
                        let rx_res = pool.switch_profile(&dev_id, slot);
                        inflight.push((dev_id, slot, rx_res));
                    }

                    // 2. 結果を回収し、成功したデバイスのみ適用済みキャッシュを更新
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
        }
    }

    watcher.stop();
    if let Some(pool) = pool {
        pool.shutdown();
    }
    tracing::info!("デーモンを正常終了しました");
    Ok(())
}