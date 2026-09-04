---
title: OAuth 2.0
description: Configure Rendrr to validate OAuth 2.0 / OIDC bearer tokens against your identity provider.
---

# OAuth 2.0 Configuration

By default Rendrr runs unauthenticated — every endpoint accepts requests with no `Authorization` header. That's fine on a trusted network, but in any deployment where the service is reachable from the open internet you'll want to require bearer tokens issued by your identity provider.

Rendrr acts as a standard **OAuth 2.0 resource server**. It validates incoming JWTs against your IdP's public keys (JWKS), enforces audience and issuer claims, and gates each endpoint behind a specific scope. There is no built-in user database, no signup flow, no token issuance — Rendrr trusts whichever IdP you point it at.

> **When you should turn this on:** any production deployment, any shared internal deployment, anything that holds customer templates or rendered documents. Don't run Rendrr unauthenticated on a public IP.

## Quick setup

Two environment variables are enough to enable OAuth:

```bash
OAUTH_ISSUER=https://auth.example.com/
OAUTH_AUDIENCE=rendrr-api
```

When `OAUTH_ISSUER` is set, Rendrr:

1. Fetches `${OAUTH_ISSUER}/.well-known/openid-configuration` on first request to discover the JWKS endpoint
2. Caches the discovery doc and the JWKS for one hour
3. Requires every request to include `Authorization: Bearer <jwt>`
4. Validates the token's signature, `iss`, `aud`, and `exp` claims
5. Enforces the per-endpoint scope (see [Scopes](#scopes) below)

If the IdP rotates its signing keys, Rendrr transparently refreshes the JWKS the first time a token presents an unknown `kid`. You don't need to restart the service.

## Configuration reference

| Variable                    | Required | Effect                                                              |
| --------------------------- | -------- | ------------------------------------------------------------------- |
| `OAUTH_ISSUER`              | yes¹     | The IdP's issuer URL. Toggles auth on.                              |
| `OAUTH_AUDIENCE`            | yes      | Required token `aud` claim. Comma-separated for multiple audiences. |
| `OAUTH_JWKS_URL`            | no       | Override OIDC discovery for the JWKS endpoint.                      |
| `OAUTH_ALLOWED_CLIENT_IDS`  | no       | Comma-separated allowlist for the `azp` / `client_id` claim.        |
| `RENDRR_SCOPE_SEPARATOR`    | no       | Separator between segments of scope names. `:` (default), `.`, or `/`. |

¹ Required to *enable* OAuth. Omit it to keep Rendrr open.

### `OAUTH_ISSUER`

The exact value the IdP places in the `iss` claim. Rendrr will reject tokens whose `iss` doesn't match this string exactly — trailing slash matters.

Find it in your IdP's docs as the "issuer URL" or "authority":

| IdP                  | Issuer format                                                           |
| -------------------- | ----------------------------------------------------------------------- |
| Auth0                | `https://<your-tenant>.auth0.com/`                                      |
| Keycloak             | `https://<host>/realms/<realm-name>`                                    |
| Okta                 | `https://<your-tenant>.okta.com/oauth2/default`                         |
| Microsoft Entra ID   | `https://login.microsoftonline.com/<tenant-id>/v2.0`                    |
| AWS Cognito          | `https://cognito-idp.<region>.amazonaws.com/<user-pool-id>`             |
| Google               | `https://accounts.google.com`                                           |

### `OAUTH_AUDIENCE`

The value the IdP stamps into the `aud` claim of tokens issued for Rendrr. This is how the IdP signals "this token is meant for Rendrr specifically, not some other API on the same tenant."

You define this when registering Rendrr as a resource/API in your IdP. Pick a value and configure both sides — typically a stable identifier like `rendrr-api`, `https://api.example.com/rendrr`, or your Rendrr deployment's URL.

Multiple audiences are accepted comma-separated. A token whose `aud` matches *any* of them passes:

```bash
OAUTH_AUDIENCE=rendrr-api,https://api.example.com/rendrr
```

### `OAUTH_JWKS_URL` (optional)

Override the OIDC discovery process and point at the JWKS endpoint directly. Useful when:

- Your IdP doesn't expose `/.well-known/openid-configuration`
- You want to pin a specific JWKS endpoint (testing, alternate region)
- Network policy blocks the discovery URL but allows the JWKS URL

```bash
OAUTH_JWKS_URL=https://auth.example.com/.well-known/jwks.json
```

### `OAUTH_ALLOWED_CLIENT_IDS` (optional)

A defense-in-depth allowlist on top of audience matching. When set, Rendrr additionally rejects tokens whose `azp` (or `client_id`) claim isn't in the list:

```bash
OAUTH_ALLOWED_CLIENT_IDS=order-service,invoice-worker,reporting-pipeline
```

This is useful when many clients in your IdP share the same audience but you only want a specific subset to reach Rendrr. Without this set, any token with the right issuer + audience + scope passes — which is normally what you want.

## Scopes

Every endpoint requires a specific scope in the token's `scope` (space-separated string) or `scp` (JSON array) claim. Rendrr's scopes are **always prefixed with `rendrr`** so they can't collide with existing scopes already defined in your IdP (e.g., your own `templates:write` for a CMS) — the prefix is not configurable.

The separator between segments **is** configurable via `RENDRR_SCOPE_SEPARATOR`. Pick whichever fits your IdP's convention; allowed values are `:` (default, GitHub-style), `.` (Google-style), or `/` (URL-path-style).

| Endpoint                                       | `:` (default)               | `.`                          | `/`                          |
| ---------------------------------------------- | --------------------------- | ---------------------------- | ---------------------------- |
| `POST /v1/templates`                           | `rendrr:templates:write`    | `rendrr.templates.write`     | `rendrr/templates/write`     |
| `DELETE /v1/templates/{template_id}`           | `rendrr:templates:delete`   | `rendrr.templates.delete`    | `rendrr/templates/delete`    |
| `POST /v1/renders`                             | `rendrr:renders:write`      | `rendrr.renders.write`       | `rendrr/renders/write`       |
| `GET /v1/renders/{render_id}/download`         | `rendrr:renders:read`       | `rendrr.renders.read`        | `rendrr/renders/read`        |

Any value of `RENDRR_SCOPE_SEPARATOR` other than the three above is rejected with a warning at startup, and Rendrr falls back to the default `:`.

A token missing the required scope returns:

```json
{
  "error": "PermissionDenied",
  "message": "Token is missing required scope 'rendrr:templates:write'"
}
```

…with a `403` status. The exact scope string in the error matches whatever separator you configured.

### Designing your scope grants

Issue tokens with only the scopes a given client actually needs. Some common patterns:

- **Render worker** — `rendrr:templates:write rendrr:renders:write rendrr:renders:read`. Uploads new templates, renders documents, downloads results.
- **Read-only consumer** — `rendrr:renders:read`. Can fetch previously rendered documents but can't render new ones or touch templates.
- **Template manager** — `rendrr:templates:write rendrr:templates:delete`. Manages the template catalog but doesn't render.
- **Full access** — all four scopes. Equivalent to admin.

There is no wildcard scope. You either grant the four individual scopes or pick a subset.

## Common provider recipes

The exact UI varies, but the conceptual setup is the same for every OIDC IdP: create an API/resource representing Rendrr, define its identifier (audience) and scopes, then create one or more clients that can request tokens for it.

### Auth0

1. **Dashboard → APIs → Create API**
   - Name: `Rendrr API`
   - Identifier: `rendrr-api` (this becomes your `OAUTH_AUDIENCE`)
   - Signing Algorithm: `RS256`
2. **Permissions** tab → add the four scopes: `rendrr:templates:write`, `rendrr:templates:delete`, `rendrr:renders:write`, `rendrr:renders:read`.
3. **Applications → Create Application** (Machine to Machine).
4. Authorize the application against your Rendrr API and grant the needed scopes.
5. Rendrr config:
   ```bash
   OAUTH_ISSUER=https://<your-tenant>.auth0.com/
   OAUTH_AUDIENCE=rendrr-api
   ```

### Keycloak

1. **Realm Settings → Clients → Create client**
   - Client ID: `rendrr-worker`
   - Client authentication: ON
   - Service accounts roles: ON (for client-credentials flow)
2. **Client scopes → Create scope** for each of the four `rendrr:*` scopes. Set type to "Default" or "Optional" and assign to the client.
3. Rendrr config:
   ```bash
   OAUTH_ISSUER=https://<host>/realms/<realm-name>
   OAUTH_AUDIENCE=<your-resource-identifier>
   ```

Note: Keycloak doesn't always populate `aud` automatically for client-credentials tokens. You may need an "Audience" client-scope mapper that adds `aud: rendrr-api` to issued tokens.

### Okta

1. **Security → API → Add Authorization Server** (or reuse the default).
2. **Scopes tab** → add the four scopes (`rendrr:templates:write`, `rendrr:templates:delete`, `rendrr:renders:write`, `rendrr:renders:read`).
3. **Applications → Create App Integration** (API Services).
4. Grant the scopes to the new app.
5. Rendrr config:
   ```bash
   OAUTH_ISSUER=https://<tenant>.okta.com/oauth2/<auth-server-id>
   OAUTH_AUDIENCE=api://rendrr
   ```

### AWS Cognito

1. **User Pool → App integration → Resource servers → Create resource server**
   - Identifier: `rendrr-api` (this is the `OAUTH_AUDIENCE`)
   - Add the four scopes — they appear in tokens as `rendrr-api/rendrr:templates:write`, etc.
2. Create an **App client** with `Client credentials` flow enabled and grant the scopes.
3. Rendrr config:
   ```bash
   OAUTH_ISSUER=https://cognito-idp.<region>.amazonaws.com/<user-pool-id>
   OAUTH_AUDIENCE=rendrr-api
   ```

Note: Cognito prefixes scopes with the resource server identifier, producing values like `rendrr-api/rendrr:templates:write`. Rendrr's check expects `rendrr:templates:write` exactly. For Cognito-fronted deployments, either set `RENDRR_SCOPE_SEPARATOR=/` (so Rendrr expects `rendrr/templates/write` and tokens carry `rendrr-api/rendrr/templates/write` — close but still mismatched) or put an API gateway in front that strips Cognito's resource-server prefix before forwarding to Rendrr.

### Microsoft Entra ID (Azure AD)

1. **App registrations → New registration** for Rendrr (this represents Rendrr as an API).
2. **Expose an API → Set application ID URI** to e.g. `api://rendrr` (your `OAUTH_AUDIENCE`).
3. **Expose an API → Add scopes** for each of the four.
4. **App registrations → New registration** for the *client* application that will call Rendrr.
5. **API permissions** on the client → add the Rendrr scopes.
6. Rendrr config:
   ```bash
   OAUTH_ISSUER=https://login.microsoftonline.com/<tenant-id>/v2.0
   OAUTH_AUDIENCE=api://rendrr
   ```

## Testing locally without a real IdP

The fastest way to smoke-test OAuth is [`oauth2-mock-server`](https://www.npmjs.com/package/oauth2-mock-server), a tiny mock OIDC provider that spins up on demand:

```bash
npx oauth2-mock-server -p 8080
```

It exposes everything Rendrr needs — discovery doc, JWKS, token endpoint — at `http://localhost:8080`. Point Rendrr at it:

```bash
OAUTH_ISSUER=http://localhost:8080
OAUTH_AUDIENCE=rendrr-api
```

Mint a token with the scopes you want and call Rendrr:

```bash
# Missing scope → 403
TOKEN=$(curl -s -X POST http://localhost:8080/token \
  -d "grant_type=client_credentials&aud=rendrr-api&scope=rendrr:renders:read" \
  | jq -r .access_token)

curl -i http://localhost:3000/v1/templates \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@template.docx"
# → 403 Permission denied: Token is missing required scope 'rendrr:templates:write'

# Correct scope → 201
TOKEN=$(curl -s -X POST http://localhost:8080/token \
  -d "grant_type=client_credentials&aud=rendrr-api&scope=rendrr:templates:write" \
  | jq -r .access_token)

curl -i http://localhost:3000/v1/templates \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@template.docx"
# → 201 Created
```

The mock server also supports rotating keys on demand (re-fetch `/jwks` after issuing a new token), so you can verify that Rendrr's JWKS cache refresh works.

## Token claim requirements

Tokens reaching Rendrr must include:

| Claim   | Required | Notes                                                                       |
| ------- | -------- | --------------------------------------------------------------------------- |
| `iss`   | yes      | Must equal `OAUTH_ISSUER` exactly.                                          |
| `aud`   | yes¹     | Must match one of the `OAUTH_AUDIENCE` values.                              |
| `exp`   | yes      | Standard expiration. Clocks are validated with a 60-second leeway.          |
| `kid`   | yes      | In the JWT *header*. Used to find the right key in the JWKS.                |
| `scope` | yes      | Space-separated scope list. `scp` (JSON array) is also accepted.            |
| `azp`   | no       | Only checked when `OAUTH_ALLOWED_CLIENT_IDS` is set. Falls back to `client_id`. |

¹ When `OAUTH_AUDIENCE` is unset, the audience check is skipped. Not recommended in production.

Algorithm support follows the JWT header's `alg` field — Rendrr trusts whatever algorithm your IdP advertises in its JWKS, including RSA (RS256, RS384, RS512), ECDSA (ES256, ES384), and EdDSA (Ed25519).

## Troubleshooting

**`401 Token validation failed: ExpiredSignature`** — the token's `exp` is in the past. Check clock sync between issuer and Rendrr host.

**`401 Token validation failed: InvalidAudience`** — `aud` claim doesn't match `OAUTH_AUDIENCE`. Inspect the token at [jwt.io](https://jwt.io) and verify the `aud` value. Note that some IdPs emit `aud` as a single string and others as an array — Rendrr handles both.

**`401 Token validation failed: InvalidIssuer`** — `iss` doesn't match `OAUTH_ISSUER`. Common cause: trailing slash mismatch (`https://auth.example.com/` vs `https://auth.example.com`). Make them match exactly.

**`401 Token signed with unknown key id`** — the token's `kid` isn't in the JWKS. Rendrr automatically refreshes the JWKS once on cache miss; if that still doesn't find it, the IdP may be issuing tokens signed with a key it isn't publishing. Check the IdP's signing key configuration.

**`401 Missing Authorization header`** — request didn't include `Authorization: Bearer ...`. Verify your client is attaching the header (and that no intermediate proxy is stripping it).

**`403 Token is missing required scope '...'`** — token is valid but lacks the scope for that endpoint. See [Scopes](#scopes).

**`500 Failed to fetch OIDC discovery doc`** — Rendrr can't reach the IdP's `/.well-known/openid-configuration` URL. Check network egress from the Rendrr container; set `OAUTH_JWKS_URL` to bypass discovery if needed.

## Disabling OAuth

To turn auth off, unset `OAUTH_ISSUER` and restart the container. All endpoints become open again. Useful in development and behind a trusted reverse proxy.
