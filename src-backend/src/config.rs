use crate::devices;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// アクティブウィンドウ切替後の確定待ち時間 (ms)
    pub debounce_ms: u64,
    /// バッテリー定期取得の間隔 (秒)
    pub battery_poll_sec: u64,
    /// 低バッテリー OS 通知を出す残量閾値 (%)
    pub low_battery_threshold: u8,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 250,
            battery_poll_sec: 120,
            low_battery_threshold: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// ルール表示名（ログ・将来の Web UI 表示用）
    pub name: String,
    /// 対象プロセスの実行ファイル名一覧（大文字小文字は評価時に無視）
    pub match_exe: Vec<String>,
    /// デバイスID -> 切替先スロット番号
    #[serde(default)]
    pub profiles: BTreeMap<String, u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    /// ルール未一致時に維持する通常プロファイル（デバイスID -> スロット番号）
    pub default: BTreeMap<String, u8>,
    /// 上から順に評価し、最初に一致したルールを適用
    pub rules: Vec<RuleConfig>,
}

impl Config {
    /// OS 標準の設定ファイルパスを返す
    /// - Windows: %APPDATA%\f-hub\config.toml
    /// - macOS / Linux: $XDG_CONFIG_HOME/f-hub/config.toml または ~/.config/f-hub/config.toml
    pub fn default_config_path() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                if !appdata.trim().is_empty() {
                    return PathBuf::from(appdata).join("f-hub").join("config.toml");
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                if !xdg.trim().is_empty() {
                    return PathBuf::from(xdg).join("f-hub").join("config.toml");
                }
            }
            if let Ok(home) = std::env::var("HOME") {
                if !home.trim().is_empty() {
                    return PathBuf::from(home)
                        .join(".config")
                        .join("f-hub")
                        .join("config.toml");
                }
            }
        }

        PathBuf::from("config.toml")
    }

    /// 読み込む設定ファイルパスを優先順位に従って決定する
    /// 1. CLI 引数 (--config)
    /// 2. 環境変数 FHUB_CONFIG
    /// 3. カレントディレクトリの ./config.toml（存在する場合）
    /// 4. OS 標準設定パス
    pub fn resolve_path(explicit: Option<&Path>) -> PathBuf {
        if let Some(p) = explicit {
            return p.to_path_buf();
        }
        if let Ok(env_path) = std::env::var("FHUB_CONFIG") {
            if !env_path.trim().is_empty() {
                return PathBuf::from(env_path);
            }
        }
        let local = PathBuf::from("config.toml");
        if local.exists() {
            return local;
        }
        Self::default_config_path()
    }

    /// 初期テンプレート（コメント付き TOML）を生成する
    pub fn default_template() -> String {
        r#"# F/HUB 設定ファイル (config.toml)
# プロファイル切替対応デバイスと有効スロット範囲:
#   - pulsar : 1..=6 (Pulsar Feinmann F01)
#   - m6-8k  : 1..=5 (Keychron M6 8K)
#   - hyzen  : 0..=6 (Lofree Hyzen)
# ※ nape-pro / luminkey はプロファイル切替非対応のため指定不可

[general]
debounce_ms = 250
battery_poll_sec = 120
low_battery_threshold = 20

# ルール未一致アプリがアクティブなときに維持する通常プロファイル
[default]
pulsar = 1
m6-8k = 1
hyzen = 0

# 上から順に評価し、最初に一致したルールを適用
[[rules]]
name = "開発作業"
match_exe = ["Code.exe", "WindowsTerminal.exe"]
[rules.profiles]
m6-8k = 2
hyzen = 1

[[rules]]
name = "FPSゲーム"
match_exe = ["VALORANT-Win64-Shipping.exe", "r5apex.exe"]
[rules.profiles]
pulsar = 2
hyzen = 2
"#
        .to_string()
    }

    /// テンプレートをファイルへ書き出す
    pub fn write_template(path: &Path, force: bool) -> Result<(), String> {
        if path.exists() && !force {
            return Err(format!(
                "設定ファイルが既に存在します: {}（上書きする場合は --force を指定）",
                path.display()
            ));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("ディレクトリ作成失敗 ({}): {e}", parent.display())
                })?;
            }
        }
        fs::write(path, Self::default_template())
            .map_err(|e| format!("ファイル書込失敗 ({}): {e}", path.display()))
    }

    /// 設定ファイルを読み込み、Capabilities に基づくバリデーションを実行する
    pub fn load_and_validate(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|e| {
            format!(
                "設定ファイルを読み込めません ({}): {e}\nヒント: `f-hub init-config` でテンプレートを生成できます",
                path.display()
            )
        })?;

        let cfg: Config = toml::from_str(&raw)
            .map_err(|e| format!("TOML 構文エラー ({}):\n{e}", path.display()))?;

        if let Err(errors) = cfg.validate() {
            let joined = errors
                .into_iter()
                .map(|msg| format!("  - {msg}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "設定内容の検証エラー ({}):\n{joined}",
                path.display()
            ));
        }

        Ok(cfg)
    }

    /// 登録ドライバの Capabilities を参照して設定値の妥当性を検証する
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if !(50..=5000).contains(&self.general.debounce_ms) {
            errors.push(format!(
                "[general].debounce_ms は 50〜5000 の範囲で指定してください (現在値: {})",
                self.general.debounce_ms
            ));
        }
        if !(10..=3600).contains(&self.general.battery_poll_sec) {
            errors.push(format!(
                "[general].battery_poll_sec は 10〜3600 の範囲で指定してください (現在値: {})",
                self.general.battery_poll_sec
            ));
        }
        if !(1..=99).contains(&self.general.low_battery_threshold) {
            errors.push(format!(
                "[general].low_battery_threshold は 1〜99 の範囲で指定してください (現在値: {})",
                self.general.low_battery_threshold
            ));
        }

        Self::validate_profile_map("[default]", &self.default, &mut errors);

        let mut seen_names = HashSet::new();
        for (idx, rule) in self.rules.iter().enumerate() {
            let section = format!("[[rules]][{idx}] ({})", rule.name);
            if rule.name.trim().is_empty() {
                errors.push(format!("[[rules]][{idx}].name が空です"));
            } else if !seen_names.insert(rule.name.clone()) {
                errors.push(format!("{section}: ルール名が重複しています"));
            }

            if rule.match_exe.is_empty() {
                errors.push(format!("{section}: match_exe を 1 件以上指定してください"));
            } else if rule.match_exe.iter().any(|s| s.trim().is_empty()) {
                errors.push(format!("{section}: match_exe に空文字列が含まれています"));
            }

            if rule.profiles.is_empty() {
                errors.push(format!(
                    "{section}: [rules.profiles] を 1 デバイス以上指定してください"
                ));
            } else {
                Self::validate_profile_map(&section, &rule.profiles, &mut errors);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_profile_map(
        section: &str,
        profiles: &BTreeMap<String, u8>,
        errors: &mut Vec<String>,
    ) {
        let supported_ids: Vec<&'static str> = devices::all_drivers()
            .into_iter()
            .filter(|d| d.capabilities().profile)
            .map(|d| d.id())
            .collect();

        for (device_id, &slot) in profiles {
            let Some(drv) = devices::find_driver(device_id) else {
                errors.push(format!(
                    "{section}: 未知のデバイスID '{device_id}'（プロファイル対応ID: {}）",
                    supported_ids.join(", ")
                ));
                continue;
            };

            let caps = drv.capabilities();
            if !caps.profile {
                errors.push(format!(
                    "{section}: '{}' ({}) はプロファイル切替非対応です（対応ID: {}）",
                    device_id,
                    drv.name(),
                    supported_ids.join(", ")
                ));
                continue;
            }

            if let Some((min, max)) = caps.profile_slots {
                if !(min..=max).contains(&slot) {
                    errors.push(format!(
                        "{section}: '{}' のスロット番号 {slot} は範囲外です（有効範囲: {min}〜{max}）",
                        device_id
                    ));
                }
            }
        }
    }
}