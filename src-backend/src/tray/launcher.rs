use std::path::PathBuf;
use std::process::Command;

/// 内蔵 Web UI を独立ウィンドウ（--app= モード）で開く
pub fn open_ui_window(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        open_windows(url)
    }

    #[cfg(target_os = "macos")]
    {
        open_macos(url)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("ブラウザ起動失敗: {e}"))
    }
}

#[cfg(target_os = "windows")]
fn open_windows(url: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if let Some(browser) = find_windows_app_browser() {
        tracing::info!(browser = %browser.display(), url, "独立ウィンドウ (--app=) で UI を起動します");
        Command::new(&browser)
            .arg(format!("--app={url}"))
            .arg("--window-size=1080,760")
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("ブラウザ起動失敗 ({}): {e}", browser.display()))
    } else {
        tracing::info!(url, "Edge/Chrome が見つからないため既定ブラウザで UI を開きます");
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("既定ブラウザ起動失敗: {e}"))
    }
}

#[cfg(target_os = "windows")]
fn find_windows_app_browser() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    for env_key in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(env_key) {
            candidates.push(
                PathBuf::from(&base)
                    .join("Microsoft")
                    .join("Edge")
                    .join("Application")
                    .join("msedge.exe"),
            );
        }
    }

    for env_key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(env_key) {
            candidates.push(
                PathBuf::from(&base)
                    .join("Google")
                    .join("Chrome")
                    .join("Application")
                    .join("chrome.exe"),
            );
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(target_os = "macos")]
fn open_macos(url: &str) -> Result<(), String> {
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    ];

    for path in candidates {
        if PathBuf::from(path).is_file() {
            return Command::new(path)
                .arg(format!("--app={url}"))
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("ブラウザ起動失敗 ({path}): {e}"));
        }
    }

    Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("ブラウザ起動失敗: {e}"))
}