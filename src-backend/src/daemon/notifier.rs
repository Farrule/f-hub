use crate::model::{BatteryInfo, ConnectionMode};
use std::collections::HashMap;
use std::process::Command;

/// 通知解除に必要なヒステリシス幅 (%)
const HYSTERESIS_RECOVERY_MARGIN: u8 = 5;

#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 3-A の Web UI / API スナップショット配信で参照予定
pub struct CachedBattery {
    pub mode: ConnectionMode,
    pub percent: Option<u8>,
    pub charging_label: Option<String>,
    pub is_charging: bool,
    /// 直近の取得が失敗して前回値を保持している場合 true
    pub stale: bool,
}

#[derive(Debug, Default)]
struct DeviceAlertState {
    notified: bool,
    last_known: Option<CachedBattery>,
}

/// バッテリー状態キャッシュと低残量通知のヒステリシスを管理する
pub struct BatteryMonitor {
    threshold: u8,
    states: HashMap<String, DeviceAlertState>,
}

impl BatteryMonitor {
    pub fn new(threshold: u8) -> Self {
        Self {
            threshold,
            states: HashMap::new(),
        }
    }

    /// バッテリー取得結果を反映し、通知が必要な場合のみ (title, body) を返す
    pub fn record_success(
        &mut self,
        device_id: &str,
        device_name: &str,
        mode: ConnectionMode,
        info: &BatteryInfo,
    ) -> Option<(String, String)> {
        let is_charging = is_charging_or_full(info);
        let charging_label = info.charging.as_ref().map(|c| c.to_string());

        let state = self.states.entry(device_id.to_string()).or_default();
        state.last_known = Some(CachedBattery {
            mode,
            percent: info.percent,
            charging_label,
            is_charging,
            stale: false,
        });

        let percent = info.percent?;
        let recovery_threshold = self.threshold.saturating_add(HYSTERESIS_RECOVERY_MARGIN).min(100);

        if is_charging || percent >= recovery_threshold {
            if state.notified {
                tracing::info!(
                    device = device_id,
                    percent,
                    is_charging,
                    "低バッテリー通知状態をリセットしました"
                );
                state.notified = false;
            }
            return None;
        }

        if percent <= self.threshold && !state.notified {
            state.notified = true;
            let title = format!("F/HUB バッテリー低下: {device_name}");
            let body = format!(
                "残量が {percent}% になりました（閾値: {}%）。充電をおすすめします。",
                self.threshold
            );
            return Some((title, body));
        }

        None
    }

    /// 取得失敗（スリープ・未接続等）を記録する。前回値は保持し stale=true にする
    pub fn record_failure(&mut self, device_id: &str) -> Option<&CachedBattery> {
        let state = self.states.entry(device_id.to_string()).or_default();
        if let Some(cached) = state.last_known.as_mut() {
            cached.stale = true;
        }
        state.last_known.as_ref()
    }
}

fn is_charging_or_full(info: &BatteryInfo) -> bool {
    let Some(state) = info.charging.as_ref() else {
        return false;
    };
    let text = state.to_string();
    let lower = text.to_ascii_lowercase();
    text.contains("充電中")
        || text.contains("満充電")
        || (lower.contains("charging") && !lower.contains("discharging"))
        || lower.contains("full")
}

/// OS 標準のデスクトップ通知を非同期（ブロッキング隔離）で送信する
pub async fn send_os_notification(title: String, body: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || send_os_notification_blocking(&title, &body))
        .await
        .map_err(|e| format!("通知タスク実行エラー: {e}"))?
}

#[cfg(target_os = "windows")]
fn send_os_notification_blocking(title: &str, body: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let title_xml = escape_xml(title);
    let body_xml = escape_xml(body);

    // Windows 11 標準 powershell.exe の WinRT ToastNotificationManager を利用
    let script = format!(
        r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml('<toast><visual><binding template="ToastText02"><text id="1">{title_xml}</text><text id="2">{body_xml}</text></binding></visual></toast>')
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
$appId = '{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\WindowsPowerShell\v1.0\powershell.exe'
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($appId).Show($toast)
"#
    );

    let utf16le: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    let encoded = encode_base64(&utf16le);

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("powershell.exe 起動失敗: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("トースト通知失敗: {}", stderr.trim()))
    }
}

#[cfg(target_os = "macos")]
fn send_os_notification_blocking(title: &str, body: &str) -> Result<(), String> {
    let title_esc = title.replace('\\', "\\\\").replace('"', "\\\"");
    let body_esc = body.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(r#"display notification "{body_esc}" with title "{title_esc}""#);

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript 起動失敗: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("macOS 通知失敗: {}", stderr.trim()))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn send_os_notification_blocking(title: &str, body: &str) -> Result<(), String> {
    tracing::warn!(title, body, "この OS のネイティブ通知は未実装です");
    Ok(())
}

#[cfg(target_os = "windows")]
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info(percent: u8) -> BatteryInfo {
        BatteryInfo {
            percent: Some(percent),
            charging: None,
            note: None,
        }
    }

    #[test]
    fn hysteresis_notifies_once_and_resets_after_margin() {
        let mut mon = BatteryMonitor::new(20);

        // 21% は通知しない
        assert!(mon
            .record_success("m6-8k", "Keychron M6 8K", ConnectionMode::Wireless24G, &sample_info(21))
            .is_none());

        // 20% 到達で初回通知
        assert!(mon
            .record_success("m6-8k", "Keychron M6 8K", ConnectionMode::Wireless24G, &sample_info(20))
            .is_some());

        // 18%・24% では再通知しない（ヒステリシス範囲内）
        assert!(mon
            .record_success("m6-8k", "Keychron M6 8K", ConnectionMode::Wireless24G, &sample_info(18))
            .is_none());
        assert!(mon
            .record_success("m6-8k", "Keychron M6 8K", ConnectionMode::Wireless24G, &sample_info(24))
            .is_none());

        // 25% (閾値+5%) でリセット
        assert!(mon
            .record_success("m6-8k", "Keychron M6 8K", ConnectionMode::Wireless24G, &sample_info(25))
            .is_none());

        // 再び 20% へ下がったら再通知
        assert!(mon
            .record_success("m6-8k", "Keychron M6 8K", ConnectionMode::Wireless24G, &sample_info(20))
            .is_some());
    }
}