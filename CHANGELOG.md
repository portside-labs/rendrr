# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `GET /health` — an unauthenticated liveness probe reporting version, PDF
  availability, and whether OAuth is enforced. Wired up as a Docker
  `HEALTHCHECK`.
- `--version` and `--help` flags on the binary.
- `CORS_ALLOWED_ORIGINS` to restrict `Access-Control-Allow-Origin` to a
  specific list. Unset keeps the previous permissive behavior.
- `IMAGE_FETCH_ALLOW_PRIVATE_NETWORKS` to opt out of the new SSRF guard for
  deployments that legitimately serve template images from a private network.
- `rust-version` (1.88) in `Cargo.toml`, matching the Dockerfile base image,
  with a CI job that builds against it.
- Dependabot configuration and a scheduled `cargo audit` workflow.
- A `Docker` workflow that builds the container image and smoke-tests it on
  any change to the Dockerfile or the dependency manifests. The image is what
  ships, but nothing validated it until a release tag fired.
- Integration tests that render the shipped sample templates in
  `docs/public/samples/` with their documented payloads, covering real Word
  documents rather than hand-built fixtures.

### Changed

- Dependency upgrades: `handlebars` 5 → 6, `image` 0.24 → 0.25, `tower`
  0.4 → 0.5, `object_store` 0.11 → 0.14, `dxpdf` 0.2 → 0.5, `reqwest`
  0.11 → 0.12, Astro 5 → 7 and Shiki 1 → 4 for the docs site, and every
  GitHub Action to its current major.
- The docs workflow now runs Node 22; Astro 7 requires >= 22.12.

- The project moved to the `portside-labs` organization. The container image
  is now `ghcr.io/portside-labs/rendrr` and the docs are at
  https://portside-labs.github.io/rendrr/.
- The release workflow now runs fmt, clippy, and the test suite before
  publishing an image to GHCR, and refuses to publish when the tag disagrees
  with the version in `Cargo.toml`.
- Release images are built on native `amd64` and `arm64` runners and merged
  into one manifest list, rather than cross-building `arm64` under QEMU.
- Pre-release tags (`v1.0.0-rc1`) no longer move the `latest` tag.
- Published images carry a signed build-provenance attestation, verifiable
  with `gh attestation verify`.
- `main.rs` and the integration tests now share a single `build_router`, so the
  tested route table is the one that ships.

### Fixed

- The container image now builds. `dxpdf` enables skia-safe's `embed-freetype`
  feature, and rust-skia publishes no prebuilt binary for that combination, so
  `skia-bindings` compiles Skia from source — which needs `ninja` and `python3`
  (absent from the builder stage) and a toolchain newer than Debian bookworm's
  GCC 12, whose libstdc++ headers fail Skia's C++20 build on arm64. The image
  is now based on Debian trixie and installs both tools; `ninja-build` and
  `python3` were added to the CI dependency lists for the same reason.
- The service user in the container now has a home directory and a font cache
  built at image-build time. Without a writable cache, fontconfig logged
  "No writable cache directories" and rescanned the font set on every process
  start.
- Dropped `quick-xml` as a direct dependency — it was declared but never
  referenced anywhere in the source.
- Removed the unused `multipart` feature from `reqwest`; inbound uploads are
  handled by axum.

### Security

- **SSRF guard on image fetching.** The `{{image}}` helper takes its URL from
  caller-supplied render data. Rendrr now rejects non-`http(s)` schemes and
  refuses to connect to any host resolving to a non-public address —
  loopback, RFC1918, CGNAT, and link-local, which is where cloud metadata
  endpoints live. Redirects are followed manually, capped at 3, with every hop
  re-checked so a public URL can't bounce to a private one.
- **Image download size cap is now enforced while streaming** rather than
  after the whole body is buffered, so a hostile URL can't exhaust memory by
  ignoring its advertised `Content-Length`.
- Resolved every advisory reported by `cargo audit`. `reqwest` 0.11 → 0.12
  drops `hyper` 0.14 and `h2` 0.3 (RUSTSEC-2026-0258); `dxpdf` 0.2 → 0.5 and
  `object_store` 0.11 → 0.14 bring every copy of `quick-xml` to 0.41
  (RUSTSEC-2026-0194/0195); a lockfile refresh clears `crossbeam-epoch`,
  `quinn-proto`, and `rustls-webpki`.
- **Decompression limit on template archives.** A `.docx` is a ZIP, so a
  template within the 25MB upload limit could previously expand to gigabytes
  and exhaust memory. Individual archive entries are now capped at 100MB while
  being read.

### Removed

- **PPTX support**, which was incomplete and is tabled for a future release.
  `OutputFormat::Pptx` is gone, uploading a `.pptx` returns 400, and requesting
  `"output_format": "pptx"` returns 422. DOCX templates rendering to DOCX or
  PDF are unaffected.
- The unused `LimitExceeded` error variant. Rendrr has no built-in rate
  limiting; see SECURITY.md for the recommended approach.
