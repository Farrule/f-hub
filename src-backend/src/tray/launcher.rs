use std::path::PathBuf;
use std::process::Command;

/// 既存の F/HUB 独立ウィンドウがあれば最前面へフォーカスし、無ければ新規起動する
pub fn focus_or_open_ui_window(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if focus_existing_window() {
            return Ok(());
        }
    }

    open_ui_window(url)
}

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

/// 起動中の F/HUB 独立ウィンドウ (--app=) を閉じる
pub fn close_ui_windows() {
    #[cfg(target_os = "windows")]
    {
        close_windows();
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

#[cfg(target_os = "windows")]
fn find_ui_windows() -> Vec<windows_sys::Win32::Foundation::HWND> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }

            let title_len = GetWindowTextLengthW(hwnd);
            if title_len <= 0 {
                return 1;
            }
            let mut title_buf = vec![0u16; (title_len as usize) + 1];
            let copied = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
            if copied <= 0 {
                return 1;
            }
            let title = String::from_utf16_lossy(&title_buf[..copied as usize]);
            if title.trim() != "F/HUB" {
                return 1;
            }

            let mut class_buf = [0u16; 128];
            let class_len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
            if class_len <= 0 {
                return 1;
            }
            let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
            if class_name != "Chrome_WidgetWin_1" {
                return 1;
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return 1;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return 1;
            }
            let mut exe_buf = vec![0u16; 1024];
            let mut exe_len = exe_buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, exe_buf.as_mut_ptr(), &mut exe_len);
            CloseHandle(handle);
            if ok == 0 || exe_len == 0 {
                return 1;
            }

            let full_path = OsString::from_wide(&exe_buf[..exe_len as usize]);
            let is_browser = Path::new(&full_path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| {
                    n.eq_ignore_ascii_case("msedge.exe") || n.eq_ignore_ascii_case("chrome.exe")
                })
                .unwrap_or(false);

            if is_browser {
                let list = &mut *(lparam as *mut Vec<HWND>);
                list.push(hwnd);
            }
        }
        1
    }

    let mut found: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(enum_proc), (&mut found as *mut Vec<HWND>) as isize);
    }
    found
}

#[cfg(target_os = "windows")]
fn focus_existing_window() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let windows = find_ui_windows();
    let Some(&hwnd) = windows.first() else {
        return false;
    };

    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
    }
    tracing::info!("既存の独立 UI ウィンドウを最前面へ表示しました");
    true
}

#[cfg(target_os = "windows")]
fn close_windows() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    let windows = find_ui_windows();
    for &hwnd in &windows {
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
    if !windows.is_empty() {
        tracing::info!(count = windows.len(), "独立 UI ウィンドウを閉じました");
    }
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