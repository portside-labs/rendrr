---
title: Getting started
description: Run Rendrr locally and render your first document.
---

# Getting started

Rendrr is a self-hostable HTTP service that renders Handlebars-flavored Word
templates (`.docx`) with JSON data, as DOCX or PDF. The fastest path from
zero to a rendered document. No Rust toolchain required — just Docker.

## Prerequisites

- Docker (with Compose v2)

## 1. Save a compose file

Create a `docker-compose.yml` anywhere on your machine. It defines Rendrr and
a local MinIO bucket on a private network:

```yaml
services:
  rendrr:
    image: ghcr.io/portside-labs/rendrr:latest
    restart: unless-stopped
    ports:
      - "3000:8080"
    environment:
      PORT: "8080"
      TEMPLATE_BUCKET_NAME: rendrr-templates
      TEMPLATE_BUCKET_ENDPOINT: http://minio:9000
      TEMPLATE_BUCKET_ACCESS_KEY_ID: minioadmin
      TEMPLATE_BUCKET_SECRET_ACCESS_KEY: minioadmin
      TEMPLATE_BUCKET_REGION: us-east-1
      RENDER_BUCKET_NAME: rendrr-renders
      RENDER_BUCKET_ENDPOINT: http://minio:9000
      RENDER_BUCKET_ACCESS_KEY_ID: minioadmin
      RENDER_BUCKET_SECRET_ACCESS_KEY: minioadmin
      RENDER_BUCKET_REGION: us-east-1
    depends_on:
      minio-init:
        condition: service_completed_successfully

  minio:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    volumes:
      - minio-data:/data

  minio-init:
    image: minio/mc:latest
    depends_on:
      - minio
    entrypoint: >
      /bin/sh -c "
      sleep 5;
      /usr/bin/mc alias set m http://minio:9000 minioadmin minioadmin;
      /usr/bin/mc mb m/rendrr-templates --ignore-existing;
      /usr/bin/mc mb m/rendrr-renders --ignore-existing;
      "

volumes:
  minio-data:
```

## 2. Bring it up

```bash
docker compose up -d
```

Rendrr is now listening on `http://localhost:3000`.

## 3. Render your first document

```bash
# Upload a template (any .docx with Handlebars placeholders)
TEMPLATE_ID=$(curl -s -X POST http://localhost:3000/v1/templates \
  -F "file=@template.docx" \
  | jq -r .template_id)

# Render with data
RENDER=$(curl -s -X POST http://localhost:3000/v1/renders \
  -H "Content-Type: application/json" \
  -d "{
    \"template_id\": \"$TEMPLATE_ID\",
    \"data\": { \"name\": \"World\" },
    \"output_format\": \"docx\"
  }")

RENDER_ID=$(echo "$RENDER" | jq -r .render_id)

# Download the rendered file
curl -OJ "http://localhost:3000/v1/renders/$RENDER_ID/download"
```

You should now have a populated document on disk. To get a PDF instead, set
`output_format` to `"pdf"` — no other change needed. Need a sample template
to try this with? Grab one from
[`docs/public/samples/`](https://github.com/portside-labs/rendrr/tree/main/docs/public/samples)
in the repo.

## 4. Point Rendrr at your own storage

For real deployments, run the container against your own S3-compatible
storage (AWS, R2, B2, GCS, MinIO — anything that speaks the S3 API):

```bash
docker run -d \
  --name rendrr \
  --restart unless-stopped \
  -p 3000:8080 \
  --env-file ./rendrr.env \
  ghcr.io/portside-labs/rendrr:latest
```

A starter `rendrr.env` needs the eight `*_BUCKET_*` variables, plus optional
config for PDF, OAuth, and TLS:

```ini
# Storage
TEMPLATE_BUCKET_NAME=acme-rendrr-templates
TEMPLATE_BUCKET_REGION=us-west-2
TEMPLATE_BUCKET_ACCESS_KEY_ID=AKIA...
TEMPLATE_BUCKET_SECRET_ACCESS_KEY=...

RENDER_BUCKET_NAME=acme-rendrr-renders
RENDER_BUCKET_REGION=us-west-2
RENDER_BUCKET_ACCESS_KEY_ID=AKIA...
RENDER_BUCKET_SECRET_ACCESS_KEY=...

# Optional: OAuth 2.0 bearer-token auth
OAUTH_ISSUER=https://auth.example.com/
OAUTH_AUDIENCE=rendrr-api

# Optional: native TLS
TLS_CERT_PATH=/etc/rendrr/tls/fullchain.pem
TLS_KEY_PATH=/etc/rendrr/tls/privkey.pem
```

## What next

- Learn how to write templates → [Template syntax](./template-syntax)
- Lock the API behind your IdP → [OAuth 2.0](./oauth)
- Take it to production → [Deployment](./deployment)
- Generate templates with Claude → [AI template skill](./ai-template-skill)
- Browse the API → [API reference](./api-reference)
