pub mod api;
pub mod docx;
pub mod errors;
pub mod models;
pub mod oauth;
pub mod services;

use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, StatusCode},
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use docx::PdfEngine;
use services::StorageClient;

/// Version reported by `--version` and the `/health` endpoint.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Largest request body the server will accept, in bytes. Templates are
/// capped lower (25MB) by `TemplateParser`; this is the outer HTTP guard.
pub const MAX_BODY_BYTES: usize = 50 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub template_storage: StorageClient,
    pub render_storage: StorageClient,
    pub pdf_engine: Option<PdfEngine>,
    pub oauth: Option<oauth::OAuthValidator>,
}

/// Build the application router.
///
/// This is the single source of truth for the route table and middleware
/// stack — `main.rs` and the integration tests both call it, so the two can't
/// drift. In particular, the OAuth layer is attached here rather than at the
/// call site: [`oauth::AuthClaims`] falls back to an unauthenticated
/// passthrough when the middleware hasn't run, so a route registered outside
/// this function would silently skip authentication.
///
/// `/health` is deliberately registered *after* the OAuth `route_layer` so it
/// stays reachable without a token — probes shouldn't need credentials.
pub fn build_router(state: AppState, cors: CorsLayer) -> Router {
    let mut app = Router::new()
        .route("/v1/templates", post(api::templates::upload_template))
        .route(
            "/v1/templates/:template_id",
            delete(api::templates::delete_template),
        )
        .route("/v1/renders", post(api::renders::render_document))
        .route(
            "/v1/renders/:render_id/download",
            get(api::renders::download_render),
        );

    if state.oauth.is_some() {
        app = app.route_layer(middleware::from_fn_with_state(
            state.clone(),
            oauth::middleware,
        ));
    }

    app.route("/health", get(api::health::health))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state)
}

/// Build the CORS layer from `CORS_ALLOWED_ORIGINS`.
///
/// Unset (or `*`) keeps the permissive default, which is appropriate for a
/// service sitting behind an authenticating proxy on a trusted network. A
/// comma-separated list restricts `Access-Control-Allow-Origin` to those
/// exact origins. Unparseable entries are dropped with a warning rather than
/// failing startup.
pub fn cors_layer_from_env() -> CorsLayer {
    let raw = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
    match parse_cors_origins(&raw) {
        Some(origins) => CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
            .allow_headers(Any),
        None => permissive_cors(),
    }
}

/// The permissive default: any origin, method, and header.
pub fn permissive_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}

/// Parse a `CORS_ALLOWED_ORIGINS` value into an explicit origin list.
///
/// `None` means "no restriction" — the caller should fall back to
/// [`permissive_cors`]. Unparseable entries are dropped with a warning rather
/// than aborting startup, but a value that contained *only* junk also yields
/// `None`: silently allowing nothing would break every browser client with no
/// obvious cause, and is more likely a typo than an intent to block all of them.
fn parse_cors_origins(raw: &str) -> Option<Vec<HeaderValue>> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "*" {
        return None;
    }

    let origins: Vec<HeaderValue> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s.parse::<HeaderValue>() {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!("Ignoring invalid CORS_ALLOWED_ORIGINS entry {:?}", s);
                None
            }
        })
        .collect();

    if origins.is_empty() {
        tracing::warn!(
            "CORS_ALLOWED_ORIGINS contained no valid origins; falling back to permissive CORS"
        );
        return None;
    }

    Some(origins)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(raw: &str) -> Option<Vec<String>> {
        parse_cors_origins(raw).map(|v| v.iter().map(|h| h.to_str().unwrap().to_string()).collect())
    }

    #[test]
    fn unset_value_means_permissive() {
        assert!(parsed("").is_none());
        assert!(parsed("   ").is_none());
    }

    #[test]
    fn star_means_permissive() {
        assert!(parsed("*").is_none());
        assert!(parsed("  *  ").is_none());
    }

    #[test]
    fn single_origin_is_restricted() {
        assert_eq!(
            parsed("https://app.example.com"),
            Some(vec!["https://app.example.com".to_string()])
        );
    }

    #[test]
    fn multiple_origins_are_split_and_trimmed() {
        assert_eq!(
            parsed(" https://a.example.com , https://b.example.com "),
            Some(vec![
                "https://a.example.com".to_string(),
                "https://b.example.com".to_string(),
            ])
        );
    }

    #[test]
    fn empty_entries_are_skipped() {
        assert_eq!(
            parsed("https://a.example.com,,https://b.example.com,"),
            Some(vec![
                "https://a.example.com".to_string(),
                "https://b.example.com".to_string(),
            ])
        );
    }

    #[test]
    fn invalid_entries_are_dropped_but_valid_ones_survive() {
        // A header value can't contain control characters.
        assert_eq!(
            parsed("https://good.example.com,bad\u{7f}value"),
            Some(vec!["https://good.example.com".to_string()])
        );
    }

    #[test]
    fn all_invalid_entries_fall_back_to_permissive() {
        assert!(parsed("bad\u{7f}value").is_none());
    }
}
