---
title: Deployment
description: Production deployment patterns for Rendrr — Docker, Compose, Kubernetes, plus storage, TLS, monitoring, and hardening.
---

# Deployment Guide

This guide covers everything you need to deploy Rendrr to production. Rendrr ships as a single statically-linked Rust binary in a Debian-slim container — no sidecars, no databases, no message queues. The whole runtime surface is the container itself plus an S3-compatible bucket.

If you're just trying it locally, read [Getting started](./) first. Come back here when you're ready to deploy.

## Architecture at a glance

![How Rendrr fits together](/rendrr/diagrams/context.png)

Rendrr is **stateless**. Templates and rendered documents live in your S3-compatible bucket; everything else (request handling, template rendering, PDF conversion) happens in-process and leaves no local state. You can scale horizontally by running multiple replicas behind a load balancer without any coordination.

## Prerequisites

Before deploying, you need:

- **A container host** — Docker, Kubernetes, ECS, Cloud Run, anything that runs OCI containers.
- **An S3-compatible bucket** — AWS S3, Cloudflare R2, Backblaze B2, MinIO, GCS, Wasabi, etc. Two buckets recommended (one for templates, one for renders); they can share if you prefer.
- **(Optional) An OIDC identity provider** — Auth0, Keycloak, Okta, Cognito, Google, Azure AD. Only needed if you want OAuth-protected endpoints. See [OAuth 2.0](./oauth).
- **(Optional) TLS certificates** — PEM-encoded cert and key files. Only needed if you want Rendrr to terminate TLS directly rather than via a reverse proxy.

## Image source

Tagged release images are published to GHCR on every `v*` tag, as a multi-arch manifest covering `linux/amd64` and `linux/arm64`:

```
ghcr.io/portside-labs/rendrr:latest        # newest stable release
ghcr.io/portside-labs/rendrr:0.1           # major+minor — recommended for production
ghcr.io/portside-labs/rendrr:0.1.0         # exact version pin
ghcr.io/portside-labs/rendrr:0             # major only — floats across minor releases
```

**Pin to a major+minor tag** (`:0.1`) in production. `latest` will silently move on major-version bumps that may include breaking changes. The minor tag floats forward only across patch releases (bug fixes, no behavior change).

Pre-release tags (`v1.0.0-rc1`) publish under their own version tag and deliberately do **not** move `latest`, so tracking `latest` never lands you on a release candidate.

### Verifying provenance

Every published image carries a signed build-provenance attestation, so you can confirm it was built by the release workflow from this repository rather than pushed by hand:

```bash
gh attestation verify \
  oci://ghcr.io/portside-labs/rendrr:0.1 \
  --repo portside-labs/rendrr
```

If you'd rather build from source — for security review, custom features, or to lock down the exact build provenance — the project's `Dockerfile` is the single source of truth. `docker build -t my-org/rendrr:custom .` from the repo root produces an image equivalent to the published one (the build takes ~5 minutes the first time because of dxpdf's Skia compile).

## Storage configuration

The minimum required configuration is the eight `*_BUCKET_*` variables:

```bash
TEMPLATE_BUCKET_NAME=acme-rendrr-templates
TEMPLATE_BUCKET_REGION=us-west-2
TEMPLATE_BUCKET_ACCESS_KEY_ID=AKIA...
TEMPLATE_BUCKET_SECRET_ACCESS_KEY=...

RENDER_BUCKET_NAME=acme-rendrr-renders
RENDER_BUCKET_REGION=us-west-2
RENDER_BUCKET_ACCESS_KEY_ID=AKIA...
RENDER_BUCKET_SECRET_ACCESS_KEY=...
```

For non-AWS providers, also set `TEMPLATE_BUCKET_ENDPOINT` and `RENDER_BUCKET_ENDPOINT`. The IAM/policy permissions Rendrr needs are minimal: `PutObject`, `GetObject`, `HeadObject`, `DeleteObject` on each bucket. No bucket-level listing or admin permissions.

> **IAM advice:** Use scoped credentials with bucket-level grants only. Never give Rendrr an account-wide admin role. If your container platform supports workload identity (EKS IRSA, GKE Workload Identity, ECS task roles), prefer that over static keys.

## Production environment template

A production `rendrr.env` typically looks like:

```ini
# Listener
PORT=8443

# Optional TLS termination
TLS_CERT_PATH=/etc/rendrr/tls/fullchain.pem
TLS_KEY_PATH=/etc/rendrr/tls/privkey.pem

# Optional OAuth bearer-token validation
OAUTH_ISSUER=https://auth.example.com/
OAUTH_AUDIENCE=rendrr-api
# OAUTH_ALLOWED_CLIENT_IDS=order-service,invoice-worker

# Template storage
TEMPLATE_BUCKET_NAME=acme-rendrr-templates
TEMPLATE_BUCKET_REGION=us-west-2
TEMPLATE_BUCKET_ACCESS_KEY_ID=AKIA...
TEMPLATE_BUCKET_SECRET_ACCESS_KEY=...

# Render storage
RENDER_BUCKET_NAME=acme-rendrr-renders
RENDER_BUCKET_REGION=us-west-2
RENDER_BUCKET_ACCESS_KEY_ID=AKIA...
RENDER_BUCKET_SECRET_ACCESS_KEY=...

# Log verbosity: trace, debug, info, warn, error
LOG_LEVEL=info
```

Never commit this file. Store the secrets in your platform's secret manager (AWS Secrets Manager, GCP Secret Manager, Vault, Kubernetes Secret, etc.) and mount or inject them at container start.

## TLS termination

Two options:

### Option A — Rendrr terminates TLS directly

Mount PEM-encoded cert and key files into the container and set the paths:

```yaml
environment:
  TLS_CERT_PATH: /certs/fullchain.pem
  TLS_KEY_PATH: /certs/privkey.pem
volumes:
  - /etc/letsencrypt/live/rendrr.example.com:/certs:ro
```

Rendrr will bind HTTPS on `PORT` (rustls under the hood). Simple, one fewer moving part. Cert rotation requires a container restart — Rendrr doesn't watch the cert files for changes.

### Option B — Reverse proxy in front

Run Rendrr in plain HTTP mode and put nginx/Caddy/Traefik/an ALB in front. Better when you also need:

- Automatic Let's Encrypt issuance with HTTP-01 challenges
- HTTP-level rate limiting or WAF
- Multi-tenant routing (multiple services on one host)
- HTTP/2 or HTTP/3 (Rendrr terminates HTTP/1.1 only)

Caddy example:

```caddy
rendrr.example.com {
    reverse_proxy rendrr:8080
}
```

That's the entire config. Caddy handles cert issuance and renewal automatically.

## PDF rendering

PDF output is available out of the box — clients request it per-render via `"output_format": "pdf"`. Rendering happens **in-process**, no extra container or sidecar. The Rendrr image ships with `libfontconfig`, `libfreetype`, and `fonts-dejavu-core` already installed.

To use specific fonts in templates (e.g., your corporate typeface), mount them into the container's fonts directory:

```yaml
volumes:
  - ./fonts:/usr/share/fonts/local:ro
```

The fontconfig cache rebuilds automatically when the container starts.

Most invoice/letter/report templates render faithfully, but some advanced Word features (justify alignment, footnotes, tracked changes, charts, SmartArt) may differ from Word's own output — evaluate against your specific templates before committing to PDF mode.

## Resource sizing

Sizing depends almost entirely on whether you're producing PDFs:

| Workload                     | CPU             | Memory          | Notes                                              |
| ---------------------------- | --------------- | --------------- | -------------------------------------------------- |
| DOCX-only                    | 0.25–0.5 vCPU   | 128–256 MB      | The template engine is fast and footprint-light.   |
| PDF, light load              | 0.5–1 vCPU      | 256–512 MB      | Each PDF render is ~150ms of CPU per page.         |
| PDF, peak load               | 1–2 vCPU        | 512 MB–1 GB     | Memory scales with concurrent renders, not RPS.    |

The dxpdf engine runs each conversion on a blocking-thread pool slot (`tokio::task::spawn_blocking`) so it doesn't stall the async runtime. Practical guideline: **budget ~150 ms of CPU per page and 50–150 MB of working memory per concurrent PDF render**. If you expect bursts, scale CPU first (more replicas or larger ones) before memory.

## Health checks and monitoring

Rendrr doesn't expose a dedicated `/health` endpoint today. For load balancers and orchestrators:

- **TCP probe** on the listener port — sufficient for "is the process up." Recommended as a readiness check.
- **HTTP probe** at any path returning `404` (e.g., `GET /`) — proves the HTTP stack is live.

Logs go to stdout in human-readable format. Verbosity is controlled by `LOG_LEVEL`:

```bash
LOG_LEVEL=info        # production default — app at info, HTTP middleware at warn
LOG_LEVEL=debug       # verbose troubleshooting
LOG_LEVEL=warn        # quiet — warnings and errors only
```

Accepted values: `trace`, `debug`, `info` (default), `warn`, `error`. Unknown values fall back to `info`.

For structured log aggregation (Datadog, Loki, CloudWatch), capture stdout from the container — there's nothing to configure inside Rendrr.

## Deployment recipes

### Docker run (single host)

The simplest production-grade invocation:

```bash
docker run -d \
  --name rendrr \
  --restart unless-stopped \
  -p 443:8443 \
  -e PORT=8443 \
  -e TLS_CERT_PATH=/certs/fullchain.pem \
  -e TLS_KEY_PATH=/certs/privkey.pem \
  -e OAUTH_ISSUER=https://auth.example.com/ \
  -e OAUTH_AUDIENCE=rendrr-api \
  --env-file ./rendrr.env \
  -v /etc/letsencrypt/live/rendrr.example.com:/certs:ro \
  ghcr.io/portside-labs/rendrr:0.1
```

### Docker Compose

For a small dedicated host (single instance, easy rollouts):

```yaml
services:
  rendrr:
    image: ghcr.io/portside-labs/rendrr:0.1
    restart: unless-stopped
    ports:
      - "443:8443"
    environment:
      PORT: "8443"
      TLS_CERT_PATH: /certs/fullchain.pem
      TLS_KEY_PATH: /certs/privkey.pem
      OAUTH_ISSUER: https://auth.example.com/
      OAUTH_AUDIENCE: rendrr-api
    env_file: ./rendrr.env
    volumes:
      - /etc/letsencrypt/live/rendrr.example.com:/certs:ro
    healthcheck:
      test: ["CMD", "nc", "-z", "localhost", "8443"]
      interval: 30s
      timeout: 5s
      retries: 3
```

Bring it up with `docker compose up -d`. Roll forward with `docker compose pull && docker compose up -d`.

### Kubernetes

Production-shaped manifest:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rendrr
  labels: { app: rendrr }
spec:
  replicas: 2
  selector:
    matchLabels: { app: rendrr }
  template:
    metadata:
      labels: { app: rendrr }
    spec:
      containers:
        - name: rendrr
          image: ghcr.io/portside-labs/rendrr:0.1
          ports:
            - name: http
              containerPort: 8080
          env:
            - name: LOG_LEVEL
              value: "info"
          envFrom:
            - secretRef: { name: rendrr-storage }
            - secretRef: { name: rendrr-oauth }
          resources:
            requests:
              cpu: 250m
              memory: 256Mi
            limits:
              cpu: 1000m
              memory: 1Gi
          readinessProbe:
            tcpSocket: { port: 8080 }
            initialDelaySeconds: 2
            periodSeconds: 5
          livenessProbe:
            tcpSocket: { port: 8080 }
            initialDelaySeconds: 10
            periodSeconds: 15
---
apiVersion: v1
kind: Service
metadata:
  name: rendrr
spec:
  selector: { app: rendrr }
  ports:
    - port: 80
      targetPort: 8080
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: rendrr
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
spec:
  ingressClassName: nginx
  tls:
    - hosts: [rendrr.example.com]
      secretName: rendrr-tls
  rules:
    - host: rendrr.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: rendrr
                port: { number: 80 }
```

This setup terminates TLS at the ingress (cert-manager + Let's Encrypt), so Rendrr itself runs plain HTTP on port 8080. For horizontal scaling, increase `replicas` — Rendrr is fully stateless and shares no in-process state across instances.

### Cloud Run / Fargate / App Runner

Any "containers as a service" platform works. Configure:

- **Container image**: `ghcr.io/portside-labs/rendrr:0.1`
- **Port**: 8080 (or whatever you set `PORT` to)
- **Concurrency**: 80–250 depending on workload (PDF-heavy = lower)
- **Min instances**: ≥1 if you can afford it (cold-start dxpdf init is ~200 ms)
- **Memory**: 512 MB minimum with PDF, 256 MB without
- **Secrets**: inject via the platform's secret manager → env vars

Workload identity is strongly preferred over static storage credentials when the platform supports it.

### Bare-metal / VM (systemd)

If you'd rather not deploy via container, `cargo build --release` produces a self-contained binary. A minimal systemd unit:

```ini
# /etc/systemd/system/rendrr.service
[Unit]
Description=Rendrr document rendering service
After=network.target

[Service]
Type=simple
User=rendrr
EnvironmentFile=/etc/rendrr/rendrr.env
ExecStart=/usr/local/bin/rendrr
Restart=on-failure
RestartSec=5s

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/log/rendrr

[Install]
WantedBy=multi-user.target
```

Build with the same system deps as the Docker builder stage: `clang`, `cmake`, `libfontconfig1-dev`, `libfreetype-dev`, `pkg-config`. See [Contributing](https://github.com/portside-labs/rendrr/blob/main/CONTRIBUTING.md) for the build-from-source recipe.

## Rolling out updates

Rendrr is stateless and there's no schema migration step, so updates are straightforward:

1. Pull the new image (`docker compose pull`, `kubectl set image`, etc.)
2. Restart / rolling-restart the container(s)
3. Verify the new version answers requests

Object keys (`templates/<uuid>.docx`, `renders/<uuid>.{docx,pdf}`) are stable across all versions, so previously-uploaded templates and rendered files keep working without migration.

For zero-downtime updates, run at least two replicas behind a load balancer and configure your orchestrator's rolling-update strategy (Kubernetes default `maxSurge: 1, maxUnavailable: 0` works fine).

## Backup considerations

There's no Rendrr database to back up — the entire persistent state is in your S3 buckets. Standard bucket-level versioning + cross-region replication covers most needs:

- **Templates bucket** — back this up. Customers can't re-upload templates they no longer have locally.
- **Renders bucket** — backup is optional. Rendered documents can be re-rendered from the template + data if the original data is preserved by the client.

If you want point-in-time recovery, enable S3 versioning on both buckets. The storage cost is negligible (templates are small, renders are short-lived for most workflows).

## Hardening checklist

Before going to production with real user data:

- [ ] **OAuth enabled.** Set `OAUTH_ISSUER` and `OAUTH_AUDIENCE`. Do not run unauthenticated on a public IP. See [OAuth 2.0](./oauth).
- [ ] **TLS in place.** Either Rendrr-terminated (`TLS_CERT_PATH` + `TLS_KEY_PATH`) or via a reverse proxy / ingress.
- [ ] **Scoped storage credentials.** IAM/policy grants are restricted to the two bucket ARNs and to `Put/Get/Head/Delete Object` only.
- [ ] **Secrets injected from a secret manager**, not committed to repo or baked into images.
- [ ] **Non-root container user.** The published image already runs as the `rendrr` UID 10001 — don't override with `USER root`.
- [ ] **Body size limit reviewed.** Default 50 MB max request body. Lower it at the reverse proxy if you don't accept multi-megabyte templates.
- [ ] **Logs forwarded** to your aggregation pipeline; alert on 5xx rates and on `Shiki`/`dxpdf` warnings.
- [ ] **Image pinned to a major+minor tag**, not `:latest`.
- [ ] **Disaster recovery plan**: bucket versioning enabled, cross-region replication for templates if your RTO requires it.

## What's next

- Lock down endpoints behind your IdP → [OAuth 2.0](./oauth)
- Browse the request shapes → [API reference](./api-reference)
- Learn the template language → [Template syntax](./template-syntax)
