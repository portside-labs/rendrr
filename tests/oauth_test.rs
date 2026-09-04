//! Integration tests for the OAuth validator. Spins up a tiny mock OIDC IdP
//! (axum on an ephemeral port) that serves a discovery doc and a JWKS, then
//! signs HS256 tokens with a symmetric key and asserts the validator accepts
//! / rejects them under the right conditions.

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{response::Json, routing::get, Router};
use base64::Engine;
use jsonwebtoken::{encode, EncodingKey, Header};
use rendrr::oauth::{OAuthConfig, OAuthValidator, ScopeSeparator};
use serde::Serialize;
use serde_json::{json, Value};

const SECRET: &[u8] = b"super-secret-key-for-tests-12345678901234567890";
const KID: &str = "test-key-1";
const KID_ROTATED: &str = "test-key-2";
const SECRET_ROTATED: &[u8] = b"second-secret-key-after-rotation-12345678";

#[derive(Clone)]
struct MockIdp {
    issuer: String,
    rotated: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn jwks_value(rotated: bool) -> Value {
    if rotated {
        json!({
            "keys": [
                jwk(SECRET_ROTATED, KID_ROTATED),
            ]
        })
    } else {
        json!({
            "keys": [
                jwk(SECRET, KID),
            ]
        })
    }
}

fn jwk(secret: &[u8], kid: &str) -> Value {
    let k = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret);
    json!({
        "kty": "oct",
        "alg": "HS256",
        "use": "sig",
        "kid": kid,
        "k": k,
    })
}

async fn start_mock_idp() -> MockIdp {
    let rotated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Bind to an ephemeral port so parallel test runs don't collide.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let issuer = format!("http://{}", addr);
    let issuer_for_route = issuer.clone();
    let rotated_for_jwks = rotated.clone();

    let app = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || {
                let iss = issuer_for_route.clone();
                async move {
                    Json(json!({
                        "issuer": iss,
                        "jwks_uri": format!("{}/jwks.json", iss),
                    }))
                }
            }),
        )
        .route(
            "/jwks.json",
            get(move || {
                let rotated = rotated_for_jwks.clone();
                async move {
                    let r = rotated.load(std::sync::atomic::Ordering::SeqCst);
                    Json(jwks_value(r))
                }
            }),
        );

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to actually accept connections.
    tokio::time::sleep(Duration::from_millis(50)).await;

    MockIdp { issuer, rotated }
}

#[derive(Serialize)]
struct Claims {
    iss: String,
    aud: String,
    exp: u64,
    iat: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    azp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn mint_token(
    issuer: &str,
    aud: &str,
    secret: &[u8],
    kid: &str,
    scope: Option<&str>,
    azp: Option<&str>,
) -> String {
    let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
    header.kid = Some(kid.to_string());
    let claims = Claims {
        iss: issuer.to_string(),
        aud: aud.to_string(),
        exp: now() + 300,
        iat: now(),
        scope: scope.map(|s| s.to_string()),
        azp: azp.map(|s| s.to_string()),
        client_id: None,
    };
    encode(&header, &claims, &EncodingKey::from_secret(secret)).unwrap()
}

fn validator_for(idp: &MockIdp, audience: &str) -> OAuthValidator {
    OAuthValidator::new(OAuthConfig {
        issuer: idp.issuer.clone(),
        audiences: vec![audience.to_string()],
        jwks_url: Some(format!("{}/jwks.json", idp.issuer)),
        allowed_client_ids: None,
        scope_separator: ScopeSeparator::Colon,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn valid_token_passes() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");
    let token = mint_token(&idp.issuer, "rendrr", SECRET, KID, Some("a b"), None);
    let claims = validator.validate(&token).await.unwrap();
    assert!(claims.oauth_enabled);
    assert!(claims.scopes.contains("a"));
    assert!(claims.scopes.contains("b"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_audience_rejected() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");
    let token = mint_token(&idp.issuer, "other-aud", SECRET, KID, None, None);
    let err = validator.validate(&token).await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("Unauthenticated"), "got: {}", msg);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_signature_rejected() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");
    let token = mint_token(
        &idp.issuer,
        "rendrr",
        b"completely-different-secret-12345678901234",
        KID,
        None,
        None,
    );
    assert!(validator.validate(&token).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_kid_after_refresh_rejected() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");
    let token = mint_token(&idp.issuer, "rendrr", SECRET, "nonexistent-kid", None, None);
    let err = validator.validate(&token).await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("unknown key id") || msg.contains("nonexistent-kid"),
        "got: {}",
        msg
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn key_rotation_triggers_jwks_refresh() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");

    // First call populates the JWKS cache with the original key.
    let token_v1 = mint_token(&idp.issuer, "rendrr", SECRET, KID, None, None);
    validator.validate(&token_v1).await.unwrap();

    // IdP rotates the key and starts serving the new one only.
    idp.rotated.store(true, std::sync::atomic::Ordering::SeqCst);

    // A token signed with the new key has a kid the cache doesn't know;
    // the validator must refresh the JWKS and find it.
    let token_v2 = mint_token(
        &idp.issuer,
        "rendrr",
        SECRET_ROTATED,
        KID_ROTATED,
        None,
        None,
    );
    let claims = validator.validate(&token_v2).await.unwrap();
    assert!(claims.oauth_enabled);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn allowed_client_id_passes() {
    let idp = start_mock_idp().await;
    let mut cfg = OAuthConfig {
        issuer: idp.issuer.clone(),
        audiences: vec!["rendrr".into()],
        jwks_url: Some(format!("{}/jwks.json", idp.issuer)),
        allowed_client_ids: Some(["client-a".into()].into_iter().collect()),
        scope_separator: ScopeSeparator::Colon,
    };
    cfg.allowed_client_ids = Some(["client-a".into()].into_iter().collect());
    let validator = OAuthValidator::new(cfg);
    let token = mint_token(&idp.issuer, "rendrr", SECRET, KID, None, Some("client-a"));
    let claims = validator.validate(&token).await.unwrap();
    assert_eq!(claims.client_id.as_deref(), Some("client-a"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disallowed_client_id_rejected() {
    let idp = start_mock_idp().await;
    let cfg = OAuthConfig {
        issuer: idp.issuer.clone(),
        audiences: vec!["rendrr".into()],
        jwks_url: Some(format!("{}/jwks.json", idp.issuer)),
        allowed_client_ids: Some(["client-allowed".into()].into_iter().collect()),
        scope_separator: ScopeSeparator::Colon,
    };
    let validator = OAuthValidator::new(cfg);
    let token = mint_token(
        &idp.issuer,
        "rendrr",
        SECRET,
        KID,
        None,
        Some("client-other"),
    );
    let err = validator.validate(&token).await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("PermissionDenied"), "got: {}", msg);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_client_id_when_allowlist_set_is_rejected() {
    let idp = start_mock_idp().await;
    let cfg = OAuthConfig {
        issuer: idp.issuer.clone(),
        audiences: vec!["rendrr".into()],
        jwks_url: Some(format!("{}/jwks.json", idp.issuer)),
        allowed_client_ids: Some(["client-a".into()].into_iter().collect()),
        scope_separator: ScopeSeparator::Colon,
    };
    let validator = OAuthValidator::new(cfg);
    let token = mint_token(&idp.issuer, "rendrr", SECRET, KID, None, None);
    let err = validator.validate(&token).await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("PermissionDenied"), "got: {}", msg);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_without_kid_rejected() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");
    // Encode without setting header.kid.
    let claims = Claims {
        iss: idp.issuer.clone(),
        aud: "rendrr".into(),
        exp: now() + 300,
        iat: now(),
        scope: None,
        azp: None,
        client_id: None,
    };
    let token = encode(
        &Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET),
    )
    .unwrap();
    let err = validator.validate(&token).await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("kid"), "got: {}", msg);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_audience_configured_skips_audience_check() {
    let idp = start_mock_idp().await;
    let cfg = OAuthConfig {
        issuer: idp.issuer.clone(),
        audiences: vec![],
        jwks_url: Some(format!("{}/jwks.json", idp.issuer)),
        allowed_client_ids: None,
        scope_separator: ScopeSeparator::Colon,
    };
    let validator = OAuthValidator::new(cfg);
    let token = mint_token(&idp.issuer, "any-audience", SECRET, KID, None, None);
    validator.validate(&token).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovery_url_resolves_jwks_when_jwks_url_absent() {
    let idp = start_mock_idp().await;
    // Omit OAUTH_JWKS_URL — the validator should hit the discovery doc.
    let cfg = OAuthConfig {
        issuer: idp.issuer.clone(),
        audiences: vec!["rendrr".into()],
        jwks_url: None,
        allowed_client_ids: None,
        scope_separator: ScopeSeparator::Colon,
    };
    let validator = OAuthValidator::new(cfg);
    let token = mint_token(&idp.issuer, "rendrr", SECRET, KID, None, None);
    validator.validate(&token).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_token_rejected() {
    let idp = start_mock_idp().await;
    let validator = validator_for(&idp, "rendrr");
    let err = validator.validate("not.a.jwt").await.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("Unauthenticated"), "got: {}", msg);
}

// ----- Full middleware path through the router -----

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get as get_route;
use bytes::Bytes;
use object_store::memory::InMemory;
use rendrr::services::StorageClient;
use rendrr::AppState;
use std::sync::Arc;
use tower::ServiceExt;

fn protected_router(state: AppState) -> Router {
    Router::new()
        .route("/protected", get_route(|| async { "ok" }))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rendrr::oauth::middleware,
        ))
        .with_state(state)
}

fn state_with_validator(validator: OAuthValidator) -> AppState {
    AppState {
        template_storage: StorageClient::from_store(Arc::new(InMemory::new())),
        render_storage: StorageClient::from_store(Arc::new(InMemory::new())),
        pdf_engine: None,
        oauth: Some(validator),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn middleware_rejects_missing_authorization() {
    let idp = start_mock_idp().await;
    let app = protected_router(state_with_validator(validator_for(&idp, "rendrr")));
    let req = Request::builder()
        .uri("/protected")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn middleware_rejects_non_bearer_authorization() {
    let idp = start_mock_idp().await;
    let app = protected_router(state_with_validator(validator_for(&idp, "rendrr")));
    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn middleware_rejects_empty_bearer_token() {
    let idp = start_mock_idp().await;
    let app = protected_router(state_with_validator(validator_for(&idp, "rendrr")));
    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", "Bearer ")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn middleware_accepts_valid_bearer() {
    let idp = start_mock_idp().await;
    let app = protected_router(state_with_validator(validator_for(&idp, "rendrr")));
    let token = mint_token(&idp.issuer, "rendrr", SECRET, KID, None, None);
    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64).await.unwrap();
    assert_eq!(body, Bytes::from_static(b"ok"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn middleware_accepts_lowercase_bearer_scheme() {
    let idp = start_mock_idp().await;
    let app = protected_router(state_with_validator(validator_for(&idp, "rendrr")));
    let token = mint_token(&idp.issuer, "rendrr", SECRET, KID, None, None);
    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", format!("bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ----- Scope enforcement through the real router -----
//
// The tests above prove the middleware validates tokens; these prove the
// route table actually *applies* it. That distinction matters because
// `AuthClaims` falls back to an unauthenticated passthrough when the
// middleware hasn't run, so a wiring mistake in `build_router` would disable
// auth silently rather than failing loudly. Each case therefore goes through
// `rendrr::build_router` — the same function `main.rs` calls.

fn secured_app(idp: &MockIdp) -> Router {
    let state = state_with_validator(validator_for(idp, "rendrr"));
    rendrr::build_router(state, tower_http::cors::CorsLayer::permissive())
}

fn minimal_docx_bytes() -> Bytes {
    use std::io::{Cursor, Write};
    use zip::{write::FileOptions, ZipWriter};

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("[Content_Types].xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#).unwrap();
    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{{name}}</w:t></w:r></w:p></w:body></w:document>"#).unwrap();

    Bytes::from(zip.finish().unwrap().into_inner())
}

fn multipart_docx() -> (String, Body) {
    const BOUNDARY: &str = "oauthboundary123";
    let docx = minimal_docx_bytes();
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", BOUNDARY).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"t.docx\"\r\n\r\n",
    );
    body.extend_from_slice(&docx);
    body.extend_from_slice(format!("\r\n--{}--\r\n", BOUNDARY).as_bytes());
    (
        format!("multipart/form-data; boundary={}", BOUNDARY),
        Body::from(body),
    )
}

/// A token carrying every scope except the one named, so each test proves the
/// *specific* scope is what unlocks the route rather than "any valid token".
fn token_missing(idp: &MockIdp, missing: &str) -> String {
    let all = [
        "rendrr:templates:write",
        "rendrr:templates:delete",
        "rendrr:renders:write",
        "rendrr:renders:read",
    ];
    let scope = all
        .iter()
        .filter(|s| **s != missing)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    mint_token(&idp.issuer, "rendrr", SECRET, KID, Some(&scope), None)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_without_token_is_401() {
    let idp = start_mock_idp().await;
    let (ct, body) = multipart_docx();
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header("Content-Type", ct)
        .body(body)
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_without_templates_write_scope_is_403() {
    let idp = start_mock_idp().await;
    let token = token_missing(&idp, "rendrr:templates:write");
    let (ct, body) = multipart_docx();
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header("Content-Type", ct)
        .header("Authorization", format!("Bearer {}", token))
        .body(body)
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_with_templates_write_scope_succeeds() {
    let idp = start_mock_idp().await;
    let token = mint_token(
        &idp.issuer,
        "rendrr",
        SECRET,
        KID,
        Some("rendrr:templates:write"),
        None,
    );
    let (ct, body) = multipart_docx();
    let req = Request::builder()
        .uri("/v1/templates")
        .method("POST")
        .header("Content-Type", ct)
        .header("Authorization", format!("Bearer {}", token))
        .body(body)
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_without_templates_delete_scope_is_403() {
    let idp = start_mock_idp().await;
    let token = token_missing(&idp, "rendrr:templates:delete");
    let id = uuid::Uuid::now_v7();
    let req = Request::builder()
        .uri(format!("/v1/templates/{}", id))
        .method("DELETE")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn render_without_renders_write_scope_is_403() {
    let idp = start_mock_idp().await;
    let token = token_missing(&idp, "rendrr:renders:write");
    let req = Request::builder()
        .uri("/v1/renders")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(
            json!({"template_id": uuid::Uuid::now_v7().to_string(), "data": {}, "output_format": "docx"})
                .to_string(),
        ))
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_without_renders_read_scope_is_403() {
    let idp = start_mock_idp().await;
    let token = token_missing(&idp, "rendrr:renders:read");
    let id = uuid::Uuid::now_v7();
    let req = Request::builder()
        .uri(format!("/v1/renders/{}/download", id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_is_reachable_without_a_token_when_oauth_is_enabled() {
    // Probes must not need credentials — /health is registered outside the
    // OAuth route_layer precisely so orchestrators can reach it.
    let idp = start_mock_idp().await;
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_reports_oauth_enabled() {
    let idp = start_mock_idp().await;
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = secured_app(&idp).oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["oauth_enabled"], true);
}
