#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const DEFAULT_PORT: u16 = 17890;

fn log_file_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    #[cfg(not(target_os = "windows"))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state"))?;

    let dir = base.join("F-HUB");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("daemon.log"))
}

/// 既に稼働中のデーモンへ接続を試み、必要なら UI を開くよう要求する
fn notify_running_instance(port: u16, background: bool) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(250)) else {
        return false;
    };

    if background {
        // ログイン自動起動の重複実行時は静かに終了
        return true;
    }

    #[cfg(target_os = "windows")]
    unsafe {
        // デーモン側が既存 UI ウィンドウを最前面へ出せるようフォーカス権限を委譲
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX);
    }

    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let req = format!(
        "POST /api/ui/open HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(req.as_bytes());
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
    true
}

fn main() {
    let mut background = false;
    let mut port = DEFAULT_PORT;
    let mut pass_args: Vec<OsString> = Vec::new();
    let mut has_explicit_open = false;

    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let s = arg.to_string_lossy();
        if s == "--background" || s == "-b" {
            background = true;
            continue;
        }
        if s == "--open" {
            has_explicit_open = true;
        }
        if s == "--port" {
            pass_args.push(arg);
            if let Some(val) = args.next() {
                if let Ok(p) = val.to_string_lossy().parse::<u16>() {
                    port = p;
                }
                pass_args.push(val);
            }
            continue;
        }
        if let Some(val) = s.strip_prefix("--port=") {
            if let Ok(p) = val.parse::<u16>() {
                port = p;
            }
        }
        pass_args.push(arg);
    }

    // 既に常駐済みなら 2 重起動せず UI を前面表示して終了
    if notify_running_instance(port, background) {
        return;
    }

    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    else {
        return;
    };

    #[cfg(target_os = "windows")]
    let target_exe = exe_dir.join("F-HUB.exe");
    #[cfg(not(target_os = "windows"))]
    let target_exe = exe_dir.join("F-HUB");

    let mut cmd = Command::new(&target_exe);
    cmd.arg("daemon");
    if !background && !has_explicit_open {
        cmd.arg("--open");
    }
    for arg in pass_args {
        cmd.arg(arg);
    }

    if let Some(log_path) = log_file_path() {
        if let Ok(out_file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
        {
            if let Ok(err_file) = out_file.try_clone() {
                cmd.stdout(Stdio::from(out_file));
                cmd.stderr(Stdio::from(err_file));
            }
        }
    } else {
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
    }
    cmd.stdin(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let _ = cmd.spawn();
}