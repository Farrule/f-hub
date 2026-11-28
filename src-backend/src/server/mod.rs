mod api;

use crate::daemon::state::DaemonHandle;
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::watch;

/// 127.0.0.1:<port> へバインドする（ポート競合時は即座にエラー返却）
pub async fn bind(port: u16) -> Result<TcpListener, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpListener::bind(addr)
        .await
        .map_err(|e| format!("内蔵サーバーの待受開始失敗 ({addr}): {e}"))
}

/// バインド済みリスナーで axum HTTP / SSE サーバーを起動する
pub async fn serve(
    listener: TcpListener,
    handle: DaemonHandle,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let app = Router::new()
        .route("/", get(api::index_placeholder))
        .route("/api/state", get(api::get_state))
        .route("/api/events", get(api::events))
        .route(
            "/api/devices/{id}/profile",
            post(api::switch_device_profile),
        )
        .route("/api/mode", post(api::set_mode))
        .route("/api/battery/refresh", post(api::refresh_battery))
        .route("/api/config", get(api::get_config).put(api::put_config))
        .route("/api/processes", get(api::list_processes))
        .with_state(handle);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            while !*shutdown_rx.borrow() {
                if shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .map_err(|e| format!("内蔵サーバー実行エラー: {e}"))
}