//! `apps/mobile-web` 构建产物的 `rust-embed` 内嵌伺服：静态资产路由、MIME 映射与 SPA 回退。

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_embed::RustEmbed;
use soxu_protocol::{ActionableError, ErrorCode};

/// 内嵌的移动端 PWA 静态资产（构建期从 `apps/mobile-web/dist` 读取；dist 缺失时嵌入空集）。
#[derive(RustEmbed)]
#[folder = "../../apps/mobile-web/dist"]
#[allow_missing = true]
struct MobileWebAssets;

/// 伺服已构建的移动端 PWA：`GET /` 与 `GET /assets/*` 等静态路径。
///
/// 安全边界：静态资产只含前端产物，不含守护进程凭据；配对令牌仅由
/// `/api/rpc` 的 `ClaimPairing` 流程下发，静态路由不读取也不回显任何令牌。
pub fn build_web_assets_router() -> Router<()> {
    Router::new()
        .route("/", get(serve_index))
        .fallback(serve_asset)
}

fn lookup_asset(path: &str) -> Option<rust_embed::EmbeddedFile> {
    let normalized = path.trim_start_matches('/');
    if normalized.is_empty() {
        return MobileWebAssets::get("index.html");
    }
    // 防路径穿越：嵌入键由构建期文件列表生成，任何含父目录段的请求都查不到，
    // 这里再显式拒绝一次，保证语义而非依赖键集合的巧合缺失。
    if normalized
        .split('/')
        .any(|seg| seg == ".." || seg.is_empty())
        && normalized != "index.html"
    {
        return None;
    }
    MobileWebAssets::get(normalized)
        .or_else(|| MobileWebAssets::get(&format!("{normalized}/index.html")))
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

fn respond_asset(file: rust_embed::EmbeddedFile, path: &str) -> Response {
    let immutable = path.starts_with("assets/");
    // Vite 产物带内容哈希文件名：assets/* 长缓存，入口与 manifest 每次协商校验。
    let cache_control = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=0, must-revalidate"
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime_for(path)),
            (header::CACHE_CONTROL, cache_control),
        ],
        file.data,
    )
        .into_response()
}

async fn serve_index() -> Response {
    match MobileWebAssets::get("index.html") {
        Some(file) => respond_asset(file, "index.html"),
        None => missing_dist_error(),
    }
}

async fn serve_asset(uri: axum::http::Uri) -> Response {
    // API 与 WS 前缀的未知路径返回 JSON 404，不被 SPA 回退吞掉；
    // 其余带扩展名的路径必须真实命中嵌入资产，未知资产返回 404；
    // 无扩展路径视为前端路由，回 index.html。
    let raw_path = uri.path();
    if raw_path.starts_with("/api/") || raw_path.starts_with("/ws") {
        return api_not_found();
    }
    let path = raw_path.trim_start_matches('/');
    let has_extension = path.rsplit('.').next().is_some_and(|ext| {
        ext != path
            && !ext.contains('/')
            && ext.len() <= 5
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
    });
    let resolved = if has_extension {
        lookup_asset(path).map(|file| (file, path.to_owned()))
    } else {
        lookup_asset("index.html").map(|file| (file, "index.html".to_owned()))
    };
    match resolved {
        Some((file, served_path)) => respond_asset(file, &served_path),
        None if has_extension => not_found_error(path),
        None => missing_dist_error(),
    }
}

/// `/api` 与 `/ws` 前缀下未注册的协议路径：JSON 结构化 404。
fn api_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ActionableError::new(
            ErrorCode::NotFound,
            "请求的 API 路径不存在",
            "该请求未送达任何守护进程能力",
            "请核对客户端协议路径（/api/health、/api/snapshot、/api/events、/api/rpc、/ws）",
        )),
    )
        .into_response()
}

/// 请求的静态资产在嵌入集中不存在（含 dist 缺失的扩展路径场景）。
fn not_found_error(path: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        format!("静态资产不存在: /{path}"),
    )
        .into_response()
}

/// dist 缺失时返回明确的 500 语义错误而非 panic。
fn missing_dist_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CACHE_CONTROL,
                "no-store",
            ),
        ],
        "移动端 PWA 静态资产未嵌入：请先在 apps/mobile-web 下执行 pnpm run build 后重新编译守护进程",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_mapping_covers_pwa_assets() {
        assert_eq!(mime_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            mime_for("assets/index-abc123.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_for("manifest.webmanifest"), "application/json");
        assert_eq!(mime_for("icon-192.png"), "image/png");
    }

    #[test]
    fn lookup_rejects_parent_traversal() {
        assert!(lookup_asset("../Cargo.toml").is_none());
        assert!(lookup_asset("a/../../b.txt").is_none());
    }

    #[test]
    fn missing_dist_yields_error_response_not_panic() {
        // 本地 dist 可能存在（开发机已构建）也可能缺失（CI rust-check）。
        // 两种情况下 serve_index 都不得 panic：有 dist 返回 200，无 dist 返回 500 明确错误。
        let has_dist = MobileWebAssets::get("index.html").is_some();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async {
            let resp = serve_index().await;
            assert_eq!(
                resp.status(),
                if has_dist {
                    StatusCode::OK
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            );
            let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                .await
                .expect("body");
            if !has_dist {
                let msg = String::from_utf8_lossy(&bytes);
                assert!(
                    msg.contains("移动端 PWA 静态资产未嵌入"),
                    "未嵌入时应返回明确中文错误: {msg}"
                );
            }
        });
    }

    #[test]
    fn cache_policy_separates_hashed_and_entry() {
        let hashed = "assets/index-Da1b2c3.js";
        assert!(hashed.starts_with("assets/"));
    }

    /// Router 层端到端：未知带扩展资产 404、命中资产 200、
    /// 无扩展 SPA 路由回 index、/api 未知路径返回 JSON 404 而非 SPA 回退。
    #[test]
    fn router_serves_404_200_and_spa_fallback_correctly() {
        use axum::body::to_bytes;
        use tower::ServiceExt;

        fn request(path: &str) -> axum::http::Request<String> {
            axum::http::Request::get(path)
                .body(String::new())
                .expect("test request")
        }

        let has_dist = MobileWebAssets::get("index.html").is_some();
        let router = build_web_assets_router();
        let rt = tokio::runtime::Runtime::new().expect("runtime");

        async fn read(resp: Response) -> (StatusCode, String, String) {
            let content_type = resp
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let status = resp.status();
            let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
            (
                status,
                content_type,
                String::from_utf8_lossy(&bytes).to_string(),
            )
        }

        rt.block_on(async {
            // 1. 未知带扩展资产：404，且绝不回退 index.html
            let (status, _, body) = read(
                router
                    .clone()
                    .oneshot(request("/missing-bundle-xyz.js"))
                    .await
                    .expect("request"),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "未知 JS 资产应 404: {body}");
            assert!(!body.contains("<html"), "404 不得回退 SPA index");

            // 2. 无扩展 SPA 路由：回 index.html（有 dist 时 200）或 dist 缺失 500
            let (status, ct, _) = read(
                router
                    .clone()
                    .oneshot(request("/some/spa/route"))
                    .await
                    .expect("request"),
            )
            .await;
            if has_dist {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(ct, "text/html; charset=utf-8");
            } else {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            }

            // 3. 根路径：与 SPA 路由同语义
            let (status, _, _) =
                read(router.clone().oneshot(request("/")).await.expect("request")).await;
            assert_eq!(
                status,
                if has_dist {
                    StatusCode::OK
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            );

            // 4. 命中的真实嵌入资产（有 dist 时取首个真实键）：200 + 正确 MIME
            if let Some(real_key) = MobileWebAssets::iter().next() {
                let (status, ct, _) = read(
                    router
                        .clone()
                        .oneshot(request(&format!("/{real_key}")))
                        .await
                        .expect("request"),
                )
                .await;
                assert_eq!(status, StatusCode::OK, "真实资产 {real_key} 应 200");
                assert_eq!(ct, mime_for(&real_key));
            }

            // 5. /api 未知子路径：JSON 结构化 404，不被 SPA 回退吞掉
            let (status, ct, body) = read(
                router
                    .oneshot(request("/api/unknown-endpoint"))
                    .await
                    .expect("request"),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert!(
                ct.starts_with("application/json"),
                "API 404 应为 JSON: {ct}"
            );
            assert!(
                body.contains("not_found"),
                "API 404 应携带协议错误码: {body}"
            );
        });
    }
}
