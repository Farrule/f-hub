use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEvent {
    pub exe_name: String,
    pub pid: u32,
}

pub struct ActiveAppWatcher {
    #[cfg(target_os = "windows")]
    thread_id: u32,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ActiveAppWatcher {
    /// アクティブウィンドウ監視を開始し、イベント受信用チャネルを返す
    pub fn start() -> Result<(Self, mpsc::UnboundedReceiver<AppEvent>), String> {
        let (tx, rx) = mpsc::unbounded_channel();
        Self::start_platform(tx).map(|watcher| (watcher, rx))
    }

    /// 監視スレッドへ終了通知を送り、クリーンに停止する
    pub fn stop(mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            if self.thread_id != 0 {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
        }

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{ActiveAppWatcher, AppEvent};
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use std::ptr;
    use std::sync::mpsc as std_mpsc;
    use tokio::sync::mpsc;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND};
    use windows_sys::Win32::System::Threading::{
        GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Accessibility::{
        SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
        TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG, WINEVENT_OUTOFCONTEXT,
        WINEVENT_SKIPOWNPROCESS,
    };

    thread_local! {
        static EVENT_TX: RefCell<Option<mpsc::UnboundedSender<AppEvent>>> = const { RefCell::new(None) };
    }

    impl ActiveAppWatcher {
        pub(super) fn start_platform(
            tx: mpsc::UnboundedSender<AppEvent>,
        ) -> Result<Self, String> {
            let (ready_tx, ready_rx) = std_mpsc::sync_channel::<Result<u32, String>>(1);

            let handle = std::thread::Builder::new()
                .name("f-hub-app-watcher".to_string())
                .spawn(move || {
                    EVENT_TX.with(|cell| *cell.borrow_mut() = Some(tx));

                    let tid = unsafe { GetCurrentThreadId() };
                    let hook = unsafe {
                        SetWinEventHook(
                            EVENT_SYSTEM_FOREGROUND,
                            EVENT_SYSTEM_FOREGROUND,
                            ptr::null_mut(),
                            Some(win_event_proc),
                            0,
                            0,
                            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                        )
                    };

                    if hook.is_null() {
                        let _ = ready_tx.send(Err("SetWinEventHook の登録に失敗しました".to_string()));
                        EVENT_TX.with(|cell| *cell.borrow_mut() = None);
                        return;
                    }

                    let _ = ready_tx.send(Ok(tid));

                    // 起動直後の最前面ウィンドウも初期イベントとして送信
                    let initial_hwnd = unsafe { GetForegroundWindow() };
                    emit_hwnd(initial_hwnd);

                    // イベント駆動の Win32 メッセージループ（WM_QUIT で抜ける）
                    unsafe {
                        let mut msg: MSG = std::mem::zeroed();
                        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                        UnhookWinEvent(hook);
                    }

                    EVENT_TX.with(|cell| *cell.borrow_mut() = None);
                })
                .map_err(|e| format!("監視スレッド起動失敗: {e}"))?;

            let thread_id = ready_rx
                .recv()
                .map_err(|_| "監視スレッド初期化前に終了しました".to_string())??;

            Ok(Self {
                thread_id,
                handle: Some(handle),
            })
        }
    }

    unsafe extern "system" fn win_event_proc(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _id_event_thread: u32,
        _event_time: u32,
    ) {
        if event == EVENT_SYSTEM_FOREGROUND {
            emit_hwnd(hwnd);
        }
    }

    fn emit_hwnd(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        let Some(ev) = query_app_event(hwnd) else {
            return;
        };
        EVENT_TX.with(|cell| {
            if let Some(tx) = cell.borrow().as_ref() {
                let _ = tx.send(ev);
            }
        });
    }

    fn query_app_event(hwnd: HWND) -> Option<AppEvent> {
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 {
            return None;
        }

        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            // 管理者権限プロセス等はアクセス不可のため安全にスキップ
            tracing::debug!(pid, "プロセス情報の取得をスキップしました（権限不足等）");
            return None;
        }

        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) };
        unsafe {
            CloseHandle(handle);
        }

        if ok == 0 || len == 0 {
            return None;
        }

        let full_path = OsString::from_wide(&buf[..len as usize]);
        let exe_name = Path::new(&full_path)
            .file_name()?
            .to_string_lossy()
            .into_owned();

        Some(AppEvent { exe_name, pid })
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{ActiveAppWatcher, AppEvent};
    use tokio::sync::mpsc;

    impl ActiveAppWatcher {
        pub(super) fn start_platform(
            _tx: mpsc::UnboundedSender<AppEvent>,
        ) -> Result<Self, String> {
            tracing::warn!("この OS の ActiveAppWatcher は未実装です（macOS 対応時に追加予定）");
            Ok(Self { handle: None })
        }
    }
}