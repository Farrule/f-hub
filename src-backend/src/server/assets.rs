use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::Response;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../src-ui/dist/"]
struct UiAssets;

pub async fn static_handler(uri: Uri) -> Response {
    let raw_path = uri.path().trim_start_matches('/');

    // 存在しない API パスは index.html へフォールバックせず 404 を返す
    if raw_path == "api" || raw_path.starts_with("api/") {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"errors":["API エンドポイントが見つかりません"]}"#))
            .unwrap();
    }

    let path = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

    if let Some(resp) = serve_embedded(path) {
        return resp;
    }

    // 拡張子付きアセット（.js / .css / .png 等）の欠落は 404、画面ルートは index.html へフォールバック
    if path.contains('.') {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap();
    }

    serve_embedded("index.html").unwrap_or_else(|| {
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from(
                "Web UI アセットが見つかりません。src-ui で `pnpm build` を実行してから再ビルドしてください。",
            ))
            .unwrap()
    })
}

fn serve_embedded(path: &str) -> Option<Response> {
    let file = UiAssets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache_control = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(file.data.into_owned()))
        .ok()
}