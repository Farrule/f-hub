mod config;
mod daemon;
mod devices;
mod model;
mod protocols;

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use model::{BatteryInfo, Capabilities, ConnectionMode, DeviceKind};
use serde::Serialize;
use config::Config;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "f-hub", version, about = "F/HUB デバイス統合管理 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 登録デバイスの接続状態と対応機能一覧を表示
    List {
        /// JSON 形式で出力
        #[arg(long)]
        json: bool,
    },
    /// 全デバイスのバッテリー残量・充電状態を一括取得
    Battery {
        /// JSON 形式で出力
        #[arg(long)]
        json: bool,
    },
    /// 指定デバイスのプロファイルを切替
    Profile {
        /// デバイスID（例: pulsar, m6-8k, hyzen）
        device: String,
        /// 切替先スロット番号
        slot: u8,
    },
    /// デーモン用 config.toml のテンプレートを生成
    InitConfig {
        /// 出力先ファイルパス（省略時は OS 標準設定パス）
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 既存ファイルがある場合に上書きする
        #[arg(long)]
        force: bool,
    },
    /// バックグラウンド常駐デーモンを起動（プロファイル自動切替・バッテリー監視）
    Daemon {
        /// 設定ファイルパス（省略時は FHUB_CONFIG -> ./config.toml -> OS 標準パス）
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// HID 送信を行わず切替判定ログのみ出力
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Serialize)]
struct ListEntry {
    id: &'static str,
    name: &'static str,
    kind: DeviceKind,
    mode: ConnectionMode,
    vid_pid: Option<String>,
    product_name: Option<String>,
    capabilities: Capabilities,
}

#[derive(Serialize)]
struct BatteryEntry {
    id: &'static str,
    name: &'static str,
    mode: ConnectionMode,
    battery: Option<BatteryInfo>,
    error: Option<String>,
}

/// 全角を幅2、半角を幅1として左寄せパディングする
fn pad_cell(s: &str, width: usize) -> String {
    let display_width: usize = s.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum();
    if display_width >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - display_width))
    }
}

fn format_profile_cap(caps: &Capabilities) -> String {
    match (caps.profile, caps.profile_slots) {
        (true, Some((min, max))) => format!("{min}..{max}"),
        (true, None) => "可".to_string(),
        (false, _) => "-".to_string(),
    }
}

fn format_remap_cap(caps: &Capabilities) -> &'static str {
    match (caps.remap, caps.wired_only_control) {
        (true, true) => "有線のみ",
        (true, false) => "可",
        (false, _) => "-",
    }
}

fn cmd_list(api: &HidApi, as_json: bool) {
    let entries: Vec<ListEntry> = devices::all_drivers()
        .into_iter()
        .map(|drv| {
            let p = drv.probe(api);
            let vid_pid = match (p.vid, p.pid) {
                (Some(v), Some(pid)) => Some(format!("{v:04x}:{pid:04x}")),
                _ => None,
            };
            ListEntry {
                id: drv.id(),
                name: drv.name(),
                kind: drv.kind(),
                mode: p.mode,
                vid_pid,
                product_name: p.product_name,
                capabilities: drv.capabilities(),
            }
        })
        .collect();

    if as_json {
        println!("{}", serde_json::to_string_pretty(&entries).unwrap());
        return;
    }

    println!(
        "{} {} {} {} {} {} {} {}",
        pad_cell("ID", 10),
        pad_cell("Device", 22),
        pad_cell("Kind", 10),
        pad_cell("Mode", 8),
        pad_cell("VID:PID", 11),
        pad_cell("Profile", 8),
        pad_cell("Remap", 10),
        "Product"
    );
    println!("{}", "-".repeat(104));
    for e in entries {
        println!(
            "{} {} {} {} {} {} {} {}",
            pad_cell(e.id, 10),
            pad_cell(e.name, 22),
            pad_cell(&e.kind.to_string(), 10),
            pad_cell(&e.mode.to_string(), 8),
            pad_cell(e.vid_pid.as_deref().unwrap_or("-"), 11),
            pad_cell(&format_profile_cap(&e.capabilities), 8),
            pad_cell(format_remap_cap(&e.capabilities), 10),
            e.product_name.as_deref().unwrap_or("-"),
        );
    }
}

fn cmd_battery(api: &HidApi, as_json: bool) {
    let entries: Vec<BatteryEntry> = devices::all_drivers()
        .into_iter()
        .map(|drv| {
            let presence = drv.probe(api);
            if !drv.capabilities().battery {
                return BatteryEntry {
                    id: drv.id(),
                    name: drv.name(),
                    mode: presence.mode,
                    battery: None,
                    error: Some("非対応".to_string()),
                };
            }
            if presence.mode == ConnectionMode::Disconnected {
                return BatteryEntry {
                    id: drv.id(),
                    name: drv.name(),
                    mode: presence.mode,
                    battery: None,
                    error: Some("未接続".to_string()),
                };
            }
            match drv.read_battery(api) {
                Ok(info) => BatteryEntry {
                    id: drv.id(),
                    name: drv.name(),
                    mode: presence.mode,
                    battery: Some(info),
                    error: None,
                },
                Err(e) => BatteryEntry {
                    id: drv.id(),
                    name: drv.name(),
                    mode: presence.mode,
                    battery: None,
                    error: Some(e.to_string()),
                },
            }
        })
        .collect();

    if as_json {
        println!("{}", serde_json::to_string_pretty(&entries).unwrap());
        return;
    }

    println!(
        "{} {} {} {} {} {}",
        pad_cell("ID", 10),
        pad_cell("Device", 22),
        pad_cell("Mode", 8),
        pad_cell("Level", 8),
        pad_cell("State", 14),
        "Note"
    );
    println!("{}", "-".repeat(82));
    for e in entries {
        match (e.battery, e.error) {
            (Some(b), _) => {
                let level = b
                    .percent
                    .map(|p| format!("{p}%"))
                    .unwrap_or_else(|| "-".to_string());
                let state = b
                    .charging
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{} {} {} {} {} {}",
                    pad_cell(e.id, 10),
                    pad_cell(e.name, 22),
                    pad_cell(&e.mode.to_string(), 8),
                    pad_cell(&level, 8),
                    pad_cell(&state, 14),
                    b.note.as_deref().unwrap_or("-"),
                );
            }
            (None, Some(err)) => {
                println!(
                    "{} {} {} {} {} {}",
                    pad_cell(e.id, 10),
                    pad_cell(e.name, 22),
                    pad_cell(&e.mode.to_string(), 8),
                    pad_cell("-", 8),
                    pad_cell("-", 14),
                    err
                );
            }
            _ => {}
        }
    }
}

fn cmd_profile(api: &HidApi, device_id: &str, slot: u8) {
    let Some(drv) = devices::find_driver(device_id) else {
        let ids: Vec<_> = devices::all_drivers().into_iter().map(|d| d.id()).collect();
        eprintln!(
            "未知のデバイスID: '{device_id}'（有効なID: {}）",
            ids.join(", ")
        );
        std::process::exit(1);
    };

    let caps = drv.capabilities();
    if !caps.profile {
        let supported: Vec<_> = devices::all_drivers()
            .into_iter()
            .filter(|d| d.capabilities().profile)
            .map(|d| d.id())
            .collect();
        eprintln!(
            "{} はプロファイル切替に非対応です（対応デバイス: {}）",
            drv.name(),
            supported.join(", ")
        );
        std::process::exit(1);
    }

    if let Some((min, max)) = caps.profile_slots {
        if !(min..=max).contains(&slot) {
            eprintln!(
                "{} のプロファイル番号は {min}〜{max} の範囲で指定してください",
                drv.name()
            );
            std::process::exit(1);
        }
    }

    match drv.switch_profile(api, slot) {
        Ok(res) => println!("{} -> slot {}: {}", drv.name(), res.slot, res.message),
        Err(e) => {
            eprintln!("{} のプロファイル切替に失敗: {e}", drv.name());
            std::process::exit(1);
        }
    }
}

fn cmd_init_config(output: Option<PathBuf>, force: bool) {
    let target = output.unwrap_or_else(Config::default_config_path);
    match Config::write_template(&target, force) {
        Ok(()) => {
            println!("テンプレートを生成しました: {}", target.display());
            // 生成直後に自身のバリデーションも通ることを確認
            if let Err(e) = Config::load_and_validate(&target) {
                eprintln!("内部警告: 生成テンプレートの検証に失敗しました:\n{e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn cmd_daemon(config_path: Option<PathBuf>, dry_run: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let resolved = Config::resolve_path(config_path.as_deref());
    let cfg = match Config::load_and_validate(&resolved) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };

    tracing::info!(
        path = %resolved.display(),
        dry_run,
        debounce_ms = cfg.general.debounce_ms,
        battery_poll_sec = cfg.general.battery_poll_sec,
        low_battery_threshold = cfg.general.low_battery_threshold,
        rules = cfg.rules.len(),
        "設定ファイルを読み込みました"
    );
    tracing::info!(default_profiles = ?cfg.default, "デフォルトプロファイル");
    for rule in &cfg.rules {
        tracing::info!(
            rule = %rule.name,
            match_exe = ?rule.match_exe,
            profiles = ?rule.profiles,
            "ルール登録"
        );
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            tracing::error!("Tokio ランタイム起動失敗: {e}");
            std::process::exit(1);
        });

    if let Err(e) = rt.block_on(daemon::run(cfg, dry_run)) {
        tracing::error!("デーモン異常終了: {e}");
        std::process::exit(1);
    }

    tracing::info!("Step 2-1: 設定検証完了（Step 2-2 で ActiveAppWatcher を接続します）");
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::InitConfig { output, force } => {
            cmd_init_config(output, force);
            return;
        }
        Commands::Daemon { config, dry_run } => {
            cmd_daemon(config, dry_run);
            return;
        }
        other => {
            let api = HidApi::new().unwrap_or_else(|e| {
                eprintln!("hidapi 初期化エラー: {e}");
                std::process::exit(1);
            });
            match other {
                Commands::List { json } => cmd_list(&api, json),
                Commands::Battery { json } => cmd_battery(&api, json),
                Commands::Profile { device, slot } => cmd_profile(&api, &device, slot),
                Commands::InitConfig { .. } | Commands::Daemon { .. } => unreachable!(),
            }
        }
    }
}