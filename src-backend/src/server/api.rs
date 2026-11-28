use crate::config::Config;
use crate::daemon::state::DaemonHandle;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;

#[derive(Deserialize)]
pub struct SwitchProfileRequest {
    pub slot: u8,
    /// 手動切替時に自動切替を一時停止するか（省略時は true）
    #[serde(default = "default_true")]
    pub lock_auto: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct SetModeRequest {
    pub auto_switch: bool,
}

#[derive(Serialize)]
struct MessageResponse {
    message: String,
}

#[derive(Serialize)]
struct ModeResponse {
    auto_switch: bool,
}

#[derive(Serialize)]
struct ErrorListResponse {
    errors: Vec<String>,
}

#[derive(Serialize)]
struct ProcessesResponse {
    processes: Vec<String>,
}

pub async fn get_state(State(handle): State<DaemonHandle>) -> impl IntoResponse {
    Json(handle.snapshot())
}

pub async fn events(
    State(handle): State<DaemonHandle>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = WatchStream::new(handle.subscribe()).map(|snapshot| {
        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
        Ok(Event::default().event("state").data(json))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn switch_device_profile(
    State(handle): State<DaemonHandle>,
    Path(device_id): Path<String>,
    Json(req): Json<SwitchProfileRequest>,
) -> impl IntoResponse {
    match handle
        .switch_profile(device_id, req.slot, req.lock_auto)
        .await
    {
        Ok(message) => (StatusCode::OK, Json(MessageResponse { message })).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorListResponse { errors: vec![err] }),
        )
            .into_response(),
    }
}

pub async fn set_mode(
    State(handle): State<DaemonHandle>,
    Json(req): Json<SetModeRequest>,
) -> impl IntoResponse {
    match handle.set_auto_switch(req.auto_switch).await {
        Ok(auto_switch) => (StatusCode::OK, Json(ModeResponse { auto_switch })).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorListResponse { errors: vec![err] }),
        )
            .into_response(),
    }
}

pub async fn refresh_battery(State(handle): State<DaemonHandle>) -> impl IntoResponse {
    match handle.refresh_battery_now().await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(MessageResponse {
                message: "バッテリー更新要求を受け付けました".to_string(),
            }),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorListResponse { errors: vec![err] }),
        )
            .into_response(),
    }
}

pub async fn get_config(State(handle): State<DaemonHandle>) -> impl IntoResponse {
    match handle.get_config().await {
        Ok(cfg) => (StatusCode::OK, Json(cfg)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorListResponse { errors: vec![err] }),
        )
            .into_response(),
    }
}

pub async fn put_config(
    State(handle): State<DaemonHandle>,
    Json(new_cfg): Json<Config>,
) -> impl IntoResponse {
    match handle.update_config(new_cfg).await {
        Ok(()) => (
            StatusCode::OK,
            Json(MessageResponse {
                message: "設定を保存し即時反映しました".to_string(),
            }),
        )
            .into_response(),
        Err(errors) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorListResponse { errors }),
        )
            .into_response(),
    }
}

pub async fn list_processes() -> impl IntoResponse {
    let processes = tokio::task::spawn_blocking(list_visible_processes)
        .await
        .unwrap_or_default();
    Json(ProcessesResponse { processes })
}

#[cfg(target_os = "windows")]
fn list_visible_processes() -> Vec<String> {
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) == 0 || GetWindowTextLengthW(hwnd) == 0 {
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
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(handle);
            if ok != 0 && len > 0 {
                let full_path = OsString::from_wide(&buf[..len as usize]);
                if let Some(name) = Path::new(&full_path).file_name() {
                    let set = &mut *(lparam as *mut BTreeSet<String>);
                    set.insert(name.to_string_lossy().into_owned());
                }
            }
        }
        1
    }

    let mut names: BTreeSet<String> = BTreeSet::new();
    unsafe {
        EnumWindows(
            Some(enum_proc),
            (&mut names as *mut BTreeSet<String>) as isize,
        );
    }
    names.into_iter().collect()
}

#[cfg(not(target_os = "windows"))]
fn list_visible_processes() -> Vec<String> {
    Vec::new()
}