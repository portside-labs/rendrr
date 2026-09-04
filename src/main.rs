use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rendrr::docx::PdfEngine;
use rendrr::models::StorageConfig;
use rendrr::oauth::{OAuthConfig, OAuthValidator};
use rendrr::services::StorageClient;
use rendrr::{build_router, cors_layer_from_env, AppState, VERSION};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if handle_cli_args() {
        return Ok(());
    }

    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_filter_directive()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let template_config = StorageConfig::from_env("TEMPLATE")
        .map_err(|e| format!("Failed to load template storage config: {}", e))?;
    let render_config = StorageConfig::from_env("RENDER")
        .map_err(|e| format!("Failed to load render storage config: {}", e))?;

    let template_storage = StorageClient::new(template_config)?;
    let render_storage = StorageClient::new(render_config)?;

    let pdf_engine = PdfEngine::from_env();
    if pdf_engine.is_some() {
        tracing::info!("PDF engine enabled (in-process)");
    } else {
        tracing::info!("PDF engine disabled by configuration");
    }

    let oauth = OAuthConfig::from_env().map(OAuthValidator::new);
    if oauth.is_some() {
        tracing::info!("OAuth bearer-token validation enabled");
    } else {
        tracing::info!("OAuth disabled — set OAUTH_ISSUER to require Bearer tokens");
    }

    let app_state = AppState {
        template_storage,
        render_storage,
        pdf_engine,
        oauth,
    };

    let app = build_router(app_state, cors_layer_from_env());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let tls_cert = std::env::var("TLS_CERT_PATH").ok();
    let tls_key = std::env::var("TLS_KEY_PATH").ok();

    match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            tracing::info!("Starting HTTPS server on {}", addr);
            let config = RustlsConfig::from_pem_file(PathBuf::from(cert), PathBuf::from(key))
                .await
                .map_err(|e| format!("Failed to load TLS cert/key: {}", e))?;
            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service())
                .await?;
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err("TLS_CERT_PATH and TLS_KEY_PATH must both be set, or both unset".into());
        }
        (None, None) => {
            tracing::info!("Starting HTTP server on {}", addr);
            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await?;
        }
    }

    Ok(())
}

/// Handles `--version` / `--help` and returns true when the process should
/// exit without starting the server. Kept as hand-rolled matching rather than
/// pulling in an argument parser — the binary takes no other flags, since all
/// configuration is by environment variable.
fn handle_cli_args() -> bool {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("rendrr {}", VERSION);
            true
        }
        Some("--help" | "-h") => {
            println!(
                "rendrr {VERSION}
Self-hostable DOCX templating service.

USAGE:
    rendrr [FLAGS]

FLAGS:
    -V, --version    Print the version and exit
    -h, --help       Print this help and exit

All configuration is by environment variable; see .env.example for the full
list. The minimum is the TEMPLATE_BUCKET_* and RENDER_BUCKET_* settings.

Documentation: https://portside-labs.github.io/rendrr/"
            );
            true
        }
        _ => false,
    }
}

/// Translates the operator-facing `LOG_LEVEL` env var into a tracing filter
/// directive. We accept the standard set of log levels and apply them across
/// both the application and HTTP middleware. Unknown values fall back to the
/// `info` default.
fn log_filter_directive() -> String {
    let raw = std::env::var("LOG_LEVEL").unwrap_or_default();
    let level = raw.trim().to_lowercase();
    match level.as_str() {
        "trace" => "rendrr=trace,tower_http=trace".into(),
        "debug" => "rendrr=debug,tower_http=debug".into(),
        "warn" | "warning" => "rendrr=warn,tower_http=warn".into(),
        "error" => "rendrr=error,tower_http=error".into(),
        "" | "info" => "rendrr=info,tower_http=warn".into(),
        other => {
            eprintln!(
                "Unknown LOG_LEVEL value {:?}; allowed: trace, debug, info, warn, error. Falling back to 'info'.",
                other
            );
            "rendrr=info,tower_http=warn".into()
        }
    }
}
