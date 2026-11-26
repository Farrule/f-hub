mod devices;
mod model;
mod protocols;

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use model::{BatteryInfo, Capabilities, ConnectionMode, DeviceKind};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "f-hub", version, about = "F/HUB デバイス統合管理 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 登録デバイスの接続状態一覧を表示
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
    /// 指定デバイスのプロファイル / レイヤーを切替
    Profile {
        /// デバイスID（例: pulsar, m6-8k, nape-pro, luminkey, hyzen）
        device: String,
        /// 切替先スロット番号
        slot: u8,
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
        "{:<10} {:<22} {:<10} {:<8} {:<11} {}",
        "ID", "Device", "Kind", "Mode", "VID:PID", "Product"
    );
    println!("{}", "-".repeat(86));
    for e in entries {
        println!(
            "{:<10} {:<22} {:<10} {:<8} {:<11} {}",
            e.id,
            e.name,
            e.kind,
            e.mode,
            e.vid_pid.as_deref().unwrap_or("-"),
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
        "{:<10} {:<22} {:<8} {:<8} {:<14} {}",
        "ID", "Device", "Mode", "Level", "State", "Note"
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
                    "{:<10} {:<22} {:<8} {:<8} {:<14} {}",
                    e.id,
                    e.name,
                    e.mode,
                    level,
                    state,
                    b.note.as_deref().unwrap_or("-"),
                );
            }
            (None, Some(err)) => {
                println!(
                    "{:<10} {:<22} {:<8} {:<8} {:<14} {}",
                    e.id, e.name, e.mode, "-", "-", err
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

    match drv.switch_profile(api, slot) {
        Ok(res) => println!("{} -> slot {}: {}", drv.name(), res.slot, res.message),
        Err(e) => {
            eprintln!("{} のプロファイル切替に失敗: {e}", drv.name());
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let api = HidApi::new().unwrap_or_else(|e| {
        eprintln!("hidapi 初期化エラー: {e}");
        std::process::exit(1);
    });

    match cli.command {
        Commands::List { json } => cmd_list(&api, json),
        Commands::Battery { json } => cmd_battery(&api, json),
        Commands::Profile { device, slot } => cmd_profile(&api, &device, slot),
    }
}