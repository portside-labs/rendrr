use std::collections::HashSet;
use std::time::Duration;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::request::Parts,
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{
    decode, decode_header,
    jwk::{Jwk, JwkSet},
    DecodingKey, Validation,
};
use moka::future::Cache;
use serde::Deserialize;

use crate::{errors::ApiError, AppState};

/// Industry-standard separators allowed between scope segments. Anything else
/// in `RENDRR_SCOPE_SEPARATOR` is rejected with a warning and falls back to the
/// default (`:`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScopeSeparator {
    #[default]
    Colon,
    Dot,
    Slash,
}

impl ScopeSeparator {
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeSeparator::Colon => ":",
            ScopeSeparator::Dot => ".",
            ScopeSeparator::Slash => "/",
        }
    }

    pub fn from_env() -> Self {
        match std::env::var("RENDRR_SCOPE_SEPARATOR").ok().as_deref() {
            None | Some("") | Some(":") => Self::Colon,
            Some(".") => Self::Dot,
            Some("/") => Self::Slash,
            Some(other) => {
                tracing::warn!(
                    "Invalid RENDRR_SCOPE_SEPARATOR value {:?}; allowed: ':', '.', '/'. Falling back to ':'",
                    other
                );
                Self::Colon
            }
        }
    }
}

/// The set of scopes Rendrr enforces per route. The string a token must carry
/// is built lazily at check time from this enum + the operator-configured
/// [`ScopeSeparator`] — e.g. `Scope::TemplatesWrite` becomes `rendrr:templates:write`,
/// `rendrr.templates.write`, or `rendrr/templates/write` depending on config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    TemplatesWrite,
    TemplatesDelete,
    RendersWrite,
    RendersRead,
}

impl Scope {
    fn parts(self) -> [&'static str; 3] {
        match self {
            Scope::TemplatesWrite => ["rendrr", "templates", "write"],
            Scope::TemplatesDelete => ["rendrr", "templates", "delete"],
            Scope::RendersWrite => ["rendrr", "renders", "write"],
            Scope::RendersRead => ["rendrr", "renders", "read"],
        }
    }

    pub fn format(self, separator: ScopeSeparator) -> String {
        self.parts().join(separator.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub issuer: String,
    pub audiences: Vec<String>,
    pub jwks_url: Option<String>,
    pub allowed_client_ids: Option<HashSet<String>>,
    pub scope_separator: ScopeSeparator,
}

impl OAuthConfig {
    /// Reads OAUTH_* env vars. Returns `Some(config)` only when `OAUTH_ISSUER` is set;
    /// returning `None` is how the operator opts out of auth entirely.
    pub fn from_env() -> Option<Self> {
        let issuer = std::env::var("OAUTH_ISSUER").ok()?;
        let audiences = split_csv(std::env::var("OAUTH_AUDIENCE").ok());
        if audiences.is_empty() {
            tracing::warn!(
                "OAUTH_ISSUER is set but OAUTH_AUDIENCE is empty — tokens will be rejected without an audience match."
            );
        }
        let jwks_url = std::env::var("OAUTH_JWKS_URL")
            .ok()
            .filter(|s| !s.is_empty());
        let allowed_client_ids = std::env::var("OAUTH_ALLOWED_CLIENT_IDS")
            .ok()
            .map(|s| split_csv(Some(s)).into_iter().collect::<HashSet<_>>())
            .filter(|set| !set.is_empty());

        Some(Self {
            issuer,
            audiences,
            jwks_url,
            allowed_client_ids,
            scope_separator: ScopeSeparator::from_env(),
        })
    }
}

fn split_csv(value: Option<String>) -> Vec<String> {
    value
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Per-request auth state. Attached to request extensions by the OAuth middleware
/// when authentication is enabled. When OAuth is disabled the extractor returns
/// a passthrough value, so `require_scope` is a no-op in unauthenticated mode.
#[derive(Clone, Debug)]
pub struct AuthClaims {
    pub oauth_enabled: bool,
    pub scopes: HashSet<String>,
    pub client_id: Option<String>,
    pub scope_separator: ScopeSeparator,
}

impl AuthClaims {
    fn disabled() -> Self {
        Self {
            oauth_enabled: false,
            scopes: HashSet::new(),
            client_id: None,
            scope_separator: ScopeSeparator::Colon,
        }
    }

    /// Asserts the token carries `scope` when OAuth is enabled. No-op otherwise.
    /// The string a token must present is built from the [`Scope`] enum + the
    /// configured [`ScopeSeparator`] — never hard-coded at the call site.
    pub fn require_scope(&self, scope: Scope) -> Result<(), ApiError> {
        if !self.oauth_enabled {
            return Ok(());
        }
        let formatted = scope.format(self.scope_separator);
        if self.scopes.contains(&formatted) {
            Ok(())
        } else {
            Err(ApiError::PermissionDenied(format!(
                "Token is missing required scope '{}'",
                formatted
            )))
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthClaims
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<AuthClaims>()
            .cloned()
            .unwrap_or_else(AuthClaims::disabled))
    }
}

#[derive(Clone)]
pub struct OAuthValidator {
    config: OAuthConfig,
    http: reqwest::Client,
    jwks_cache: Cache<String, JwkSet>,
    discovery_cache: Cache<String, String>,
}

impl std::fmt::Debug for OAuthValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthValidator")
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    jwks_uri: String,
}

#[derive(Debug, Deserialize)]
struct TokenClaims {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    scp: Option<Vec<String>>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
}

impl OAuthValidator {
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client should build"),
            jwks_cache: Cache::builder()
                .max_capacity(8)
                .time_to_live(Duration::from_secs(60 * 60))
                .build(),
            discovery_cache: Cache::builder()
                .max_capacity(4)
                .time_to_live(Duration::from_secs(60 * 60))
                .build(),
        }
    }

    /// Validates the token signature, issuer, audience, and optional client-id allowlist.
    /// Returns the extracted `AuthClaims` (with scopes) on success.
    /// Per-route scope enforcement is the caller's responsibility (see [`AuthClaims::require_scope`]).
    pub async fn validate(&self, bearer: &str) -> Result<AuthClaims, ApiError> {
        let header = decode_header(bearer)
            .map_err(|e| ApiError::Unauthenticated(format!("Invalid token header: {}", e)))?;

        let jwks_url = self.resolve_jwks_url().await?;
        let mut jwks = self.fetch_jwks(&jwks_url, false).await?;

        let kid = header
            .kid
            .ok_or_else(|| ApiError::Unauthenticated("Token header missing 'kid'".to_string()))?;

        let jwk = match jwks.find(&kid) {
            Some(j) => j.clone(),
            None => {
                // Cache miss for this kid — force a refresh in case the IdP rotated keys.
                jwks = self.fetch_jwks(&jwks_url, true).await?;
                jwks.find(&kid).cloned().ok_or_else(|| {
                    ApiError::Unauthenticated(format!("Token signed with unknown key id '{}'", kid))
                })?
            }
        };

        let decoding_key = decoding_key_from_jwk(&jwk)?;
        let algorithm = header.alg;
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[self.config.issuer.as_str()]);
        if !self.config.audiences.is_empty() {
            validation.set_audience(&self.config.audiences);
        } else {
            validation.validate_aud = false;
        }

        let data = decode::<serde_json::Value>(bearer, &decoding_key, &validation)
            .map_err(|e| ApiError::Unauthenticated(format!("Token validation failed: {}", e)))?;

        let claims: TokenClaims = serde_json::from_value(data.claims)
            .map_err(|e| ApiError::Unauthenticated(format!("Token claims parse failed: {}", e)))?;

        let client_id = claims.azp.clone().or_else(|| claims.client_id.clone());

        if let Some(allowed) = &self.config.allowed_client_ids {
            let cid = client_id.as_deref().ok_or_else(|| {
                ApiError::PermissionDenied(
                    "Token is missing 'azp'/'client_id' claim required by OAUTH_ALLOWED_CLIENT_IDS"
                        .to_string(),
                )
            })?;
            if !allowed.contains(cid) {
                return Err(ApiError::PermissionDenied(format!(
                    "Client '{}' is not in OAUTH_ALLOWED_CLIENT_IDS",
                    cid
                )));
            }
        }

        let scopes: HashSet<String> = match (claims.scope, claims.scp) {
            (Some(s), _) => s.split_whitespace().map(|s| s.to_string()).collect(),
            (None, Some(list)) => list.into_iter().collect(),
            (None, None) => HashSet::new(),
        };

        Ok(AuthClaims {
            oauth_enabled: true,
            scopes,
            client_id,
            scope_separator: self.config.scope_separator,
        })
    }

    async fn resolve_jwks_url(&self) -> Result<String, ApiError> {
        if let Some(url) = &self.config.jwks_url {
            return Ok(url.clone());
        }
        if let Some(url) = self.discovery_cache.get(&self.config.issuer).await {
            return Ok(url);
        }
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer.trim_end_matches('/')
        );
        let response = self.http.get(&discovery_url).send().await.map_err(|e| {
            ApiError::Internal(format!("Failed to fetch OIDC discovery doc: {}", e))
        })?;
        if !response.status().is_success() {
            return Err(ApiError::Internal(format!(
                "OIDC discovery returned {}",
                response.status()
            )));
        }
        let doc: OidcDiscovery = response.json().await.map_err(|e| {
            ApiError::Internal(format!("Failed to parse OIDC discovery doc: {}", e))
        })?;
        self.discovery_cache
            .insert(self.config.issuer.clone(), doc.jwks_uri.clone())
            .await;
        Ok(doc.jwks_uri)
    }

    async fn fetch_jwks(&self, url: &str, force: bool) -> Result<JwkSet, ApiError> {
        if !force {
            if let Some(cached) = self.jwks_cache.get(url).await {
                return Ok(cached);
            }
        }
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to fetch JWKS: {}", e)))?;
        if !response.status().is_success() {
            return Err(ApiError::Internal(format!(
                "JWKS endpoint returned {}",
                response.status()
            )));
        }
        let set: JwkSet = response
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to parse JWKS: {}", e)))?;
        self.jwks_cache.insert(url.to_string(), set.clone()).await;
        Ok(set)
    }
}

fn decoding_key_from_jwk(jwk: &Jwk) -> Result<DecodingKey, ApiError> {
    DecodingKey::from_jwk(jwk)
        .map_err(|e| ApiError::Unauthenticated(format!("Unsupported JWK: {}", e)))
}

pub async fn middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let validator = match &state.oauth {
        Some(v) => v,
        None => {
            return Err(ApiError::Internal(
                "OAuth middleware invoked without a validator".to_string(),
            ));
        }
    };

    let header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::Unauthenticated("Missing Authorization header".to_string()))?;

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or_else(|| {
            ApiError::Unauthenticated("Authorization header must use Bearer scheme".to_string())
        })?
        .trim();

    if token.is_empty() {
        return Err(ApiError::Unauthenticated(
            "Bearer token must not be empty".to_string(),
        ));
    }

    let claims = validator.validate(token).await?;
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_env<F: FnOnce()>(pairs: &[(&str, Option<&str>)], f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved: Vec<(String, Option<String>)> = pairs
            .iter()
            .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in pairs {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        f();
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn split_csv_handles_empty_none() {
        assert!(split_csv(None).is_empty());
    }

    #[test]
    fn split_csv_handles_empty_string() {
        assert!(split_csv(Some(String::new())).is_empty());
    }

    #[test]
    fn split_csv_trims_and_filters_blanks() {
        let out = split_csv(Some("a, b,  ,c".into()));
        assert_eq!(out, vec!["a", "b", "c"]);
    }

    #[test]
    fn from_env_returns_none_without_issuer() {
        with_env(&[("OAUTH_ISSUER", None)], || {
            assert!(OAuthConfig::from_env().is_none());
        });
    }

    #[test]
    fn from_env_minimal() {
        with_env(
            &[
                ("OAUTH_ISSUER", Some("https://idp.example.com/")),
                ("OAUTH_AUDIENCE", None),
                ("OAUTH_JWKS_URL", None),
                ("OAUTH_ALLOWED_CLIENT_IDS", None),
            ],
            || {
                let cfg = OAuthConfig::from_env().expect("issuer is set");
                assert_eq!(cfg.issuer, "https://idp.example.com/");
                assert!(cfg.audiences.is_empty());
                assert!(cfg.jwks_url.is_none());
                assert!(cfg.allowed_client_ids.is_none());
            },
        );
    }

    #[test]
    fn from_env_full() {
        with_env(
            &[
                ("OAUTH_ISSUER", Some("https://idp.example.com/")),
                ("OAUTH_AUDIENCE", Some("aud-a, aud-b")),
                ("OAUTH_JWKS_URL", Some("https://idp.example.com/jwks")),
                ("OAUTH_ALLOWED_CLIENT_IDS", Some("client-a,client-b")),
            ],
            || {
                let cfg = OAuthConfig::from_env().unwrap();
                assert_eq!(cfg.audiences, vec!["aud-a", "aud-b"]);
                assert_eq!(
                    cfg.jwks_url.as_deref(),
                    Some("https://idp.example.com/jwks")
                );
                let allowed = cfg.allowed_client_ids.unwrap();
                assert!(allowed.contains("client-a"));
                assert!(allowed.contains("client-b"));
            },
        );
    }

    #[test]
    fn from_env_empty_jwks_url_treated_as_none() {
        with_env(
            &[
                ("OAUTH_ISSUER", Some("https://idp.example.com/")),
                ("OAUTH_JWKS_URL", Some("")),
            ],
            || {
                let cfg = OAuthConfig::from_env().unwrap();
                assert!(cfg.jwks_url.is_none());
            },
        );
    }

    #[test]
    fn auth_claims_disabled_passes_any_scope() {
        let claims = AuthClaims::disabled();
        assert!(!claims.oauth_enabled);
        assert!(claims.require_scope(Scope::TemplatesWrite).is_ok());
        assert!(claims.require_scope(Scope::RendersRead).is_ok());
    }

    #[test]
    fn auth_claims_enabled_requires_scope_present_colon() {
        let claims = AuthClaims {
            oauth_enabled: true,
            scopes: ["rendrr:templates:write".to_string()].into_iter().collect(),
            client_id: None,
            scope_separator: ScopeSeparator::Colon,
        };
        assert!(claims.require_scope(Scope::TemplatesWrite).is_ok());
    }

    #[test]
    fn auth_claims_enabled_requires_scope_present_dot() {
        let claims = AuthClaims {
            oauth_enabled: true,
            scopes: ["rendrr.renders.read".to_string()].into_iter().collect(),
            client_id: None,
            scope_separator: ScopeSeparator::Dot,
        };
        assert!(claims.require_scope(Scope::RendersRead).is_ok());
    }

    #[test]
    fn auth_claims_enabled_requires_scope_present_slash() {
        let claims = AuthClaims {
            oauth_enabled: true,
            scopes: ["rendrr/templates/delete".to_string()]
                .into_iter()
                .collect(),
            client_id: None,
            scope_separator: ScopeSeparator::Slash,
        };
        assert!(claims.require_scope(Scope::TemplatesDelete).is_ok());
    }

    #[test]
    fn auth_claims_enabled_rejects_missing_scope() {
        let claims = AuthClaims {
            oauth_enabled: true,
            scopes: ["rendrr:renders:read".to_string()].into_iter().collect(),
            client_id: None,
            scope_separator: ScopeSeparator::Colon,
        };
        let err = claims.require_scope(Scope::TemplatesWrite).unwrap_err();
        match err {
            ApiError::PermissionDenied(msg) => {
                assert!(msg.contains("rendrr:templates:write"));
            }
            other => panic!("expected PermissionDenied, got {:?}", other),
        }
    }

    #[test]
    fn auth_claims_with_wrong_separator_rejects_scope() {
        // Token carries dot-separated scope, but validator was configured for colon.
        let claims = AuthClaims {
            oauth_enabled: true,
            scopes: ["rendrr.templates.write".to_string()].into_iter().collect(),
            client_id: None,
            scope_separator: ScopeSeparator::Colon,
        };
        assert!(claims.require_scope(Scope::TemplatesWrite).is_err());
    }

    #[test]
    fn scope_format_uses_configured_separator() {
        assert_eq!(
            Scope::TemplatesWrite.format(ScopeSeparator::Colon),
            "rendrr:templates:write"
        );
        assert_eq!(
            Scope::TemplatesDelete.format(ScopeSeparator::Dot),
            "rendrr.templates.delete"
        );
        assert_eq!(
            Scope::RendersWrite.format(ScopeSeparator::Slash),
            "rendrr/renders/write"
        );
        assert_eq!(
            Scope::RendersRead.format(ScopeSeparator::Colon),
            "rendrr:renders:read"
        );
    }

    #[test]
    fn scope_separator_from_env_accepts_industry_standard() {
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some(":"))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Colon);
        });
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some("."))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Dot);
        });
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some("/"))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Slash);
        });
    }

    #[test]
    fn scope_separator_from_env_defaults_to_colon() {
        with_env(&[("RENDRR_SCOPE_SEPARATOR", None)], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Colon);
        });
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some(""))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Colon);
        });
    }

    #[test]
    fn scope_separator_from_env_rejects_invalid() {
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some("|"))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Colon);
        });
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some("::"))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Colon);
        });
        with_env(&[("RENDRR_SCOPE_SEPARATOR", Some("-"))], || {
            assert_eq!(ScopeSeparator::from_env(), ScopeSeparator::Colon);
        });
    }

    #[test]
    fn validator_constructs_without_panicking() {
        let cfg = OAuthConfig {
            issuer: "https://idp.example.com/".into(),
            audiences: vec!["aud".into()],
            jwks_url: Some("https://idp.example.com/jwks".into()),
            allowed_client_ids: None,
            scope_separator: ScopeSeparator::Colon,
        };
        let _ = OAuthValidator::new(cfg);
    }

    #[test]
    fn validator_debug_does_not_expose_internals() {
        let cfg = OAuthConfig {
            issuer: "https://idp.example.com/".into(),
            audiences: vec!["aud".into()],
            jwks_url: None,
            allowed_client_ids: None,
            scope_separator: ScopeSeparator::Colon,
        };
        let validator = OAuthValidator::new(cfg);
        let debug = format!("{:?}", validator);
        assert!(debug.contains("OAuthValidator"));
        assert!(debug.contains("issuer"));
    }
}
