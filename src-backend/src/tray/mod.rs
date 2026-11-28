pub mod launcher;

use crate::daemon::state::DaemonHandle;

pub struct SystemTray {
    #[cfg(target_os = "windows")]
    hwnd_raw: isize,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl SystemTray {
    /// システムトレイアイコンを専用スレッドで起動する
    pub fn start(
        ui_url: String,
        daemon: DaemonHandle,
        rt: tokio::runtime::Handle,
    ) -> Result<Self, String> {
        Self::start_platform(ui_url, daemon, rt)
    }

    /// トレイアイコンを確実に削除してスレッドを終了する
    pub fn stop(mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
            if self.hwnd_raw != 0 {
                PostMessageW(self.hwnd_raw as HWND, WM_CLOSE, 0, 0);
            }
        }

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::launcher::focus_or_open_ui_window;
    use super::SystemTray;
    use crate::daemon::state::DaemonHandle;
    use std::cell::RefCell;
    use std::ptr;
    use std::sync::mpsc as std_mpsc;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
        DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW,
        PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
        SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, IDI_APPLICATION, MF_CHECKED,
        MF_GRAYED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, TPM_BOTTOMALIGN, TPM_RETURNCMD,
        TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP, WM_NULL,
        WM_RBUTTONUP, WNDCLASSEXW,
    };

    const WM_TRAYICON: u32 = WM_APP + 1;
    const TRAY_ICON_UID: u32 = 1;

    const CMD_OPEN_UI: usize = 1001;
    const CMD_TOGGLE_AUTO: usize = 1002;
    const CMD_REFRESH_BATTERY: usize = 1003;
    const CMD_EXIT: usize = 1004;

    struct TrayContext {
        ui_url: String,
        daemon: DaemonHandle,
        rt: tokio::runtime::Handle,
        hicon: HICON,
        owns_icon: bool,
        taskbar_created_msg: u32,
        last_open: Option<Instant>,
    }

    thread_local! {
        static TRAY_CTX: RefCell<Option<TrayContext>> = const { RefCell::new(None) };
    }

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 16x16 の簡易 F/HUB バッジアイコンを生成する（失敗時はシステム標準アイコンへフォールバック）
    unsafe fn create_fhub_icon(hinstance: HINSTANCE) -> (HICON, bool) {
        const SIZE: usize = 16;
        // "F" グリフ（7x9）を中央配置
        const F_GLYPH: [&str; 9] = [
            "#######",
            "#######",
            "##.....",
            "##.....",
            "######.",
            "######.",
            "##.....",
            "##.....",
            "##.....",
        ];

        let mut xor_bgra = vec![0u8; SIZE * SIZE * 4];
        let mut and_mask = vec![0u8; SIZE * (SIZE / 8)];

        for y in 0..SIZE {
            for x in 0..SIZE {
                let is_corner = (x == 0 || x == SIZE - 1) && (y == 0 || y == SIZE - 1);
                let idx = (y * SIZE + x) * 4;

                if is_corner {
                    // 透明ピクセル: alpha=0 かつ AND マスクビット=1
                    and_mask[y * 2 + (x / 8)] |= 1 << (7 - (x % 8));
                    continue;
                }

                let is_border = x == 0 || x == SIZE - 1 || y == 0 || y == SIZE - 1;
                let is_letter = (3..12).contains(&y)
                    && (5..12).contains(&x)
                    && F_GLYPH[y - 3].as_bytes()[x - 5] == b'#';

                // BGRA 順
                let (b, g, r, a) = if is_letter {
                    (255u8, 255u8, 255u8, 255u8) // 白 "F"
                } else if is_border {
                    (248u8, 189u8, 56u8, 255u8) // スカイブルー枠 (#38bdf8)
                } else {
                    (42u8, 23u8, 15u8, 255u8) // ダークスレート背景 (#0f172a)
                };

                xor_bgra[idx] = b;
                xor_bgra[idx + 1] = g;
                xor_bgra[idx + 2] = r;
                xor_bgra[idx + 3] = a;
            }
        }

        let icon = unsafe {
            CreateIcon(
                hinstance,
                SIZE as i32,
                SIZE as i32,
                1,
                32,
                and_mask.as_ptr(),
                xor_bgra.as_ptr(),
            )
        };
        if !icon.is_null() {
            (icon, true)
        } else {
            let fallback = unsafe { LoadIconW(ptr::null_mut(), IDI_APPLICATION) };
            (fallback, false)
        }
    }

    fn build_nid(hwnd: HWND, hicon: HICON) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ICON_UID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = hicon;

        let tip: Vec<u16> = "F/HUB - デバイス統合管理".encode_utf16().collect();
        let copy_len = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);
        nid
    }

    fn open_ui_debounced(ctx: &mut TrayContext) {
        let now = Instant::now();
        if let Some(prev) = ctx.last_open {
            if now.duration_since(prev) < Duration::from_millis(600) {
                return;
            }
        }
        ctx.last_open = Some(now);
        if let Err(e) = focus_or_open_ui_window(&ctx.ui_url) {
            tracing::error!(error = %e, "UI ウィンドウ起動失敗");
        }
    }

    fn show_context_menu(hwnd: HWND) {
        let snapshot = TRAY_CTX.with(|cell| {
            cell.borrow().as_ref().map(|ctx| ctx.daemon.snapshot())
        });
        let Some(snapshot) = snapshot else {
            return;
        };

        let status_label = if snapshot.auto_switch {
            match &snapshot.active_exe {
                Some(exe) => format!("状態: 自動切替中 [{}] ({exe})", snapshot.active_rule),
                None => format!("状態: 自動切替中 [{}]", snapshot.active_rule),
            }
        } else {
            "状態: 手動固定中（自動切替一時停止）".to_string()
        };

        let selected = unsafe {
            let hmenu = CreatePopupMenu();
            if hmenu.is_null() {
                return;
            }

            let status_w = to_wide_null(&status_label);
            let open_w = to_wide_null("ダッシュボードを開く");
            let auto_w = to_wide_null("自動切替を有効にする");
            let refresh_w = to_wide_null("バッテリーを今すぐ更新");
            let exit_w = to_wide_null("終了");

            AppendMenuW(hmenu, MF_STRING | MF_GRAYED, 0, status_w.as_ptr());
            AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(hmenu, MF_STRING, CMD_OPEN_UI, open_w.as_ptr());

            let auto_flags = MF_STRING
                | if snapshot.auto_switch {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
            AppendMenuW(hmenu, auto_flags, CMD_TOGGLE_AUTO, auto_w.as_ptr());
            AppendMenuW(hmenu, MF_STRING, CMD_REFRESH_BATTERY, refresh_w.as_ptr());
            AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(hmenu, MF_STRING, CMD_EXIT, exit_w.as_ptr());

            let mut pt: POINT = std::mem::zeroed();
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);

            let cmd = TrackPopupMenu(
                hmenu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                0,
                hwnd,
                ptr::null(),
            );
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(hmenu);
            cmd as usize
        };

        match selected {
            CMD_OPEN_UI => {
                TRAY_CTX.with(|cell| {
                    if let Some(ctx) = cell.borrow_mut().as_mut() {
                        open_ui_debounced(ctx);
                    }
                });
            }
            CMD_TOGGLE_AUTO => {
                let next_enabled = !snapshot.auto_switch;
                TRAY_CTX.with(|cell| {
                    if let Some(ctx) = cell.borrow().as_ref() {
                        let daemon = ctx.daemon.clone();
                        ctx.rt.spawn(async move {
                            if let Err(e) = daemon.set_auto_switch(next_enabled).await {
                                tracing::error!(error = %e, "自動切替モード変更失敗");
                            }
                        });
                    }
                });
            }
            CMD_REFRESH_BATTERY => {
                TRAY_CTX.with(|cell| {
                    if let Some(ctx) = cell.borrow().as_ref() {
                        let daemon = ctx.daemon.clone();
                        ctx.rt.spawn(async move {
                            if let Err(e) = daemon.refresh_battery_now().await {
                                tracing::error!(error = %e, "バッテリー即時更新失敗");
                            }
                        });
                    }
                });
            }
            CMD_EXIT => {
                TRAY_CTX.with(|cell| {
                    if let Some(ctx) = cell.borrow().as_ref() {
                        ctx.daemon.shutdown();
                    }
                });
            }
            _ => {}
        }
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let taskbar_created = TRAY_CTX.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|ctx| ctx.taskbar_created_msg)
                .unwrap_or(0)
        });

        if taskbar_created != 0 && msg == taskbar_created {
            // Explorer 再起動時にトレイアイコンを再登録
            TRAY_CTX.with(|cell| {
                if let Some(ctx) = cell.borrow().as_ref() {
                    let nid = build_nid(hwnd, ctx.hicon);
                    unsafe {
                        Shell_NotifyIconW(NIM_ADD, &nid);
                    }
                }
            });
            return 0;
        }

        match msg {
            WM_TRAYICON => {
                let mouse_msg = (lparam as u32) & 0xFFFF;
                match mouse_msg {
                    WM_LBUTTONUP => {
                        TRAY_CTX.with(|cell| {
                            if let Some(ctx) = cell.borrow_mut().as_mut() {
                                open_ui_debounced(ctx);
                            }
                        });
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        show_context_menu(hwnd);
                    }
                    _ => {}
                }
                0
            }
            WM_CLOSE => {
                unsafe {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_DESTROY => {
                TRAY_CTX.with(|cell| {
                    if let Some(ctx) = cell.borrow_mut().take() {
                        let nid = build_nid(hwnd, ctx.hicon);
                        unsafe {
                            Shell_NotifyIconW(NIM_DELETE, &nid);
                            if ctx.owns_icon && !ctx.hicon.is_null() {
                                DestroyIcon(ctx.hicon);
                            }
                        }
                    }
                });
                unsafe {
                    PostQuitMessage(0);
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    impl SystemTray {
        pub(super) fn start_platform(
            ui_url: String,
            daemon: DaemonHandle,
            rt: tokio::runtime::Handle,
        ) -> Result<Self, String> {
            let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<isize, String>>(1);

            let handle = std::thread::Builder::new()
                .name("f-hub-tray".to_string())
                .spawn(move || {
                    let hinstance = unsafe { GetModuleHandleW(ptr::null()) };
                    let class_name = to_wide_null("FHubTrayWindowClass");
                    let window_name = to_wide_null("F/HUB Tray");

                    let mut wc: WNDCLASSEXW = unsafe { std::mem::zeroed() };
                    wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                    wc.lpfnWndProc = Some(tray_wnd_proc);
                    wc.hInstance = hinstance;
                    wc.lpszClassName = class_name.as_ptr();

                    unsafe {
                        RegisterClassExW(&wc);
                    }

                    let hwnd = unsafe {
                        CreateWindowExW(
                            0,
                            class_name.as_ptr(),
                            window_name.as_ptr(),
                            0,
                            0,
                            0,
                            0,
                            0,
                            ptr::null_mut(),
                            ptr::null_mut(),
                            hinstance,
                            ptr::null(),
                        )
                    };

                    if hwnd.is_null() {
                        let _ = ready_tx.send(Err("トレイ用ウィンドウの作成に失敗しました".to_string()));
                        return;
                    }

                    let (hicon, owns_icon) = unsafe { create_fhub_icon(hinstance) };
                    let taskbar_msg_name = to_wide_null("TaskbarCreated");
                    let taskbar_created_msg =
                        unsafe { RegisterWindowMessageW(taskbar_msg_name.as_ptr()) };

                    TRAY_CTX.with(|cell| {
                        *cell.borrow_mut() = Some(TrayContext {
                            ui_url,
                            daemon,
                            rt,
                            hicon,
                            owns_icon,
                            taskbar_created_msg,
                            last_open: None,
                        });
                    });

                    let nid = build_nid(hwnd, hicon);
                    let added = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
                    if added == 0 {
                        let _ = ready_tx.send(Err("システムトレイへのアイコン登録に失敗しました".to_string()));
                        unsafe {
                            DestroyWindow(hwnd);
                        }
                        return;
                    }

                    let _ = ready_tx.send(Ok(hwnd as isize));

                    unsafe {
                        let mut msg: MSG = std::mem::zeroed();
                        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                })
                .map_err(|e| format!("トレイスレッド起動失敗: {e}"))?;

            let hwnd_raw = ready_rx
                .recv()
                .map_err(|_| "トレイスレッド初期化前に終了しました".to_string())??;

            Ok(Self {
                hwnd_raw,
                handle: Some(handle),
            })
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::SystemTray;
    use crate::daemon::state::DaemonHandle;

    impl SystemTray {
        pub(super) fn start_platform(
            _ui_url: String,
            _daemon: DaemonHandle,
            _rt: tokio::runtime::Handle,
        ) -> Result<Self, String> {
            tracing::warn!("この OS のシステムトレイ常駐は未実装です（--no-tray 相当で動作します）");
            Ok(Self { handle: None })
        }
    }
}