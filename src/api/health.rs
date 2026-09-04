use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{AppState, VERSION};

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    /// Whether in-process DOCX→PDF conversion is available on this instance.
    pub pdf_enabled: bool,
    /// Whether bearer-token authentication is being enforced.
    pub oauth_enabled: bool,
}

/// GET /health — liveness probe.
///
/// Deliberately unauthenticated and dependency-free: it reports that the
/// process is up and how it is configured, without touching object storage.
/// A storage round-trip would make orchestrators restart a healthy container
/// during a transient S3 outage, which is the opposite of what a liveness
/// probe is for.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: VERSION.to_string(),
        pdf_enabled: state.pdf_engine.is_some(),
        oauth_enabled: state.oauth.is_some(),
    })
}
