# CLAUDE.md

Guidance for Claude Code when working with this repository.

## What is Rendrr?

A self-hostable HTTP service that renders Handlebars-flavored DOCX templates
with JSON data, with optional in-process PDF output, optional OAuth 2.0 bearer
auth, and optional native TLS. Single Rust binary, ships as one Docker image.

## Commands

```bash
cargo build              # Build the project
cargo test               # Run unit + integration tests
cargo test <name>        # Run a single test by name
cargo clippy --all-targets -- -D warnings  # Lint
cargo fmt --check        # Check formatting
cargo fmt                # Auto-format
cargo run                # Run the server (needs .env or env vars)
cargo run -- --version   # Print the version and exit
```

`dxpdf` links Skia, so on a fresh machine you'll need `clang`, `cmake`,
`libfontconfig1-dev`, and `libfreetype-dev` (Debian) or
`brew install cmake fontconfig freetype` (macOS) before the first build.

MSRV is 1.88 (`rust-version` in `Cargo.toml`), matching the Dockerfile's
`rust:1.88-bookworm` base. It's driven by the dependency tree — `image` and
`time` declare 1.88, and `dxpdf` pulls `clap`, which needs `edition2024` — not
by anything in this repo's own source. A CI job builds against it explicitly;
bumping a dependency can raise it, so update `rust-version`, the CI job, and
CONTRIBUTING.md together.

## Architecture

**HTTP layer** (`src/main.rs`, `src/lib.rs`, `src/api/`) — Axum router with
five routes:
- `GET    /health`                          — liveness probe, never authenticated
- `POST   /v1/templates`                    — upload DOCX
- `DELETE /v1/templates/{template_id}`      — delete a template object
- `POST   /v1/renders`                      — render a template with JSON
- `GET    /v1/renders/{render_id}/download` — stream the rendered file

The route table lives in `rendrr::build_router`, **not** in `main.rs`. Both the
binary and the integration tests call it, so the tested router is the one that
ships. Register new routes there.

`/health` is registered *after* the OAuth `route_layer` so probes work without
a token. Everything registered *before* that layer is authenticated. This
ordering matters: `oauth::AuthClaims` falls back to an unauthenticated
passthrough when the middleware hasn't run, so a route added in the wrong place
silently skips auth rather than failing.

TLS and the optional OAuth layer are both off by default — TLS turns on when
both `TLS_CERT_PATH` and `TLS_KEY_PATH` are set; OAuth when `OAUTH_ISSUER` is.

**Module layout**: DOCX-specific code lives under `src/docx/`; format-agnostic
helpers live under `src/services/`. The `services/ooxml_*` modules are written
against OOXML in general rather than Word specifically.

**Shared services** (`src/services/`):
- `template_parser.rs` — upload-time validation: `.docx` extension, 25MB size
  cap, then delegates to `docx::template_validator`.
- `handlebars_helpers.rs` — the `table` / `image` / `chunk` helpers and the
  data-structure validator (depth/size limits).
- `image_handler.rs` — fetches images by URL or base64, with a streaming size
  cap and manual redirect following.
- `url_guard.rs` — **SSRF guard**. Screens every outbound image URL: scheme
  allowlist plus a resolved-address check against loopback/private/link-local
  ranges. Read the module docs before touching it; several of the rules exist
  for specific bypasses (IPv4-mapped IPv6, bracketed literal hosts).
- `ooxml_rels.rs` — generic `.rels` XML manipulation.
- `ooxml_content_types.rs` — patches `[Content_Types].xml` to declare PNG
  and JPEG defaults.
- `ooxml_loop_normalizer.rs` — lifts `{{#each}}` / `{{#if}}` / `{{#chunk}}`
  control blocks outside an OOXML container element (parameterized on
  container name; `w:tr` for table rows).
- `storage_client.rs` — S3-compatible PUT/GET/HEAD/DELETE.

**DOCX** (`src/docx/`):
- `template_engine.rs` — `DocxTemplateEngine`. Extracts `word/document.xml`
  (and headers/footers), normalizes Word XML (merges split runs, strips
  block-tag paragraphs), runs Handlebars, embeds images, repackages the ZIP.
  Archive entries are read through `read_entry_capped` (100MB) — a `.docx` is
  a ZIP, so the 25MB *upload* limit says nothing about decompressed size.
- `template_validator.rs` — required entries `[Content_Types].xml` and
  `word/document.xml`, then syntactic check via the engine.
- `image_embedder.rs` — emits Word `<w:drawing>` XML for embedded images.
- `pdf_engine.rs` — optional in-process DOCX → PDF via `dxpdf` (Skia).

**OAuth** (`src/oauth.rs`) — optional JWT bearer-token validation against
any OIDC IdP. JWKS is fetched via discovery and cached for an hour.

**Models** (`src/models/`) — `Template`, `Render`, `OutputFormat`
(`Docx` | `Pdf`), `StorageConfig`.

**Errors** (`src/errors.rs`) — `TemplateError`, `StorageError`, `RenderError`,
`ApiError`. `ApiError` implements `IntoResponse`.

**Statelessness**: no metadata database. Templates and renders are addressed
by UUID v7 and stored in two S3-compatible buckets; object existence is the
source of truth. Templates are always `templates/{id}.docx`; renders are
`renders/{id}.docx` or `renders/{id}.pdf`, and download probes both.

**Format/output-format matrix**: a DOCX template renders to `docx` (always)
or `pdf` (when `dxpdf` is enabled). Those are the only two variants of
`OutputFormat`, so any other value is rejected by serde with a 422.

**PPTX is out of scope for now.** It was removed before the initial open-source
release because it was incomplete — no slide-cloning loops, no PDF path. The
implementation is in git history if it gets picked back up. Don't reintroduce
`TemplateKind`-style multi-format dispatch on the assumption it's coming back.

## Security invariants

These are load-bearing; changing them needs a deliberate decision, not a
drive-by refactor.

- Never fetch a caller-supplied URL without going through `url_guard`.
- Never read a ZIP entry with unbounded `read_to_string` / `read_to_end`.
- Never read an HTTP body with `Response::bytes()` — it buffers before any
  size check can run. Use the capped streaming reader.
- Register authenticated routes *before* the OAuth `route_layer` in
  `build_router`.

`tests/security_test.rs` covers all four; it should fail if one regresses.

## Code Style

Rust 2021. Formatting per `rustfmt.toml` (4 spaces, 100 char width). Unit
tests are `#[cfg(test)] mod tests` blocks in the file under test; broader
integration tests live in `tests/`.

Comments should explain *why*, not restate the code. Don't reference internal
task IDs or planning documents — nothing outside this repo is visible to a
reader.
